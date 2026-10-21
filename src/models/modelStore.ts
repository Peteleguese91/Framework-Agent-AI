import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { modelApi } from "./modelApi";
import { localRuntimeApi, STOPPED_RUNTIME, type HardwareProfile, type LocalRuntimeConfig, type RuntimeDiscovery, type RuntimeStatus } from "./localRuntime";
import { providerRegistry } from "./provider";
import {
  DEFAULT_SETTINGS, SQUARE_NUMBER_TOOL, normalizeModelError, providerSettingsSchema,
  type ChatMessage, type ModelInfo, type ProviderSettings, type TokenUsage, type ToolCall,
} from "./types";

const STORAGE_KEY = "gravityforge.models.v1";
const LOCAL_RUNTIME_KEY = "gravityforge.local-runtime.v1";
const FIRST_RUN_KEY = "gravityforge.first-run.v1";
export type ConnectionStatus = "disconnected" | "connecting" | "connected" | "error";
export interface ConversationMessage extends ChatMessage { id: string }
interface DeltaEvent { requestId: string; content: string }
interface CompleteEvent { requestId: string; content: string; toolCalls: ToolCall[]; usage?: TokenUsage }
interface ErrorEvent { requestId: string; error: unknown }

interface ModelState {
  settings: ProviderSettings; models: ModelInfo[]; connection: ConnectionStatus; settingsOpen: boolean;
  localConfig: LocalRuntimeConfig; runtime: RuntimeStatus; hardware: HardwareProfile | null; discovery: RuntimeDiscovery | null;
  remoteEndpoint: boolean; messages: ConversationMessage[]; usage: TokenUsage | null;
  activeRequestId: string | null; generating: boolean; error: string | null;
  initialize: () => Promise<void>; openSettings: () => void; closeSettings: () => void;
  saveSettings: (settings: ProviderSettings, apiKey?: string) => Promise<void>;
  saveLocalConfig: (config: LocalRuntimeConfig) => void; loadRuntime: () => Promise<void>;
  unloadRuntime: () => Promise<void>; restartRuntime: () => Promise<void>; testCapabilities: () => Promise<void>;
  refreshModels: () => Promise<void>; testConnection: () => Promise<void>; selectModel: (model: string) => void;
  send: (content: string) => Promise<void>; cancel: () => Promise<void>; clearError: () => void;
}

let listening: Promise<void> | null = null;
let nextMessage = 1;

const DEFAULT_LOCAL_CONFIG: LocalRuntimeConfig = {
  modelPath: "", useCustomRuntime: false, backend: "auto", contextLength: 32_768, cpuThreads: Math.max(1, Math.floor((globalThis.navigator?.hardwareConcurrency ?? 4) / 2)),
  parallelism: 1, startupTimeoutMs: 120_000,
};

function loadLocalConfig(storage: Pick<Storage, "getItem">): LocalRuntimeConfig {
  try {
    const value = JSON.parse(storage.getItem(LOCAL_RUNTIME_KEY) ?? "null") as Partial<LocalRuntimeConfig> | null;
    return value ? { ...DEFAULT_LOCAL_CONFIG, ...value } : DEFAULT_LOCAL_CONFIG;
  } catch { return DEFAULT_LOCAL_CONFIG; }
}

export function isRemoteEndpoint(baseUrl: string): boolean {
  try { return !["localhost", "127.0.0.1", "::1"].includes(new URL(baseUrl).hostname.toLowerCase()); }
  catch { return false; }
}
export function loadProviderSettings(storage: Pick<Storage, "getItem">): ProviderSettings {
  const raw = storage.getItem(STORAGE_KEY);
  if (!raw) return DEFAULT_SETTINGS;
  try { return providerSettingsSchema.parse(JSON.parse(raw)); } catch { return DEFAULT_SETTINGS; }
}
function persist(settings: ProviderSettings): void { localStorage.setItem(STORAGE_KEY, JSON.stringify(settings)); }
function text(error: unknown): string { return normalizeModelError(error).message; }
function toApiMessage(message: ConversationMessage): ChatMessage {
  return { role: message.role, content: message.content, name: message.name, toolCallId: message.toolCallId, toolCalls: message.toolCalls };
}

function appendDelta(state: ModelState, requestId: string, content: string): Partial<ModelState> {
  if (!state.generating || state.activeRequestId && state.activeRequestId !== requestId) return {};
  const messages = state.messages.map((message, index) => index === state.messages.length - 1 && message.role === "assistant"
    ? { ...message, content: (message.content ?? "") + content } : message);
  return { messages, activeRequestId: requestId };
}

export const useModelStore = create<ModelState>((set, get) => ({
  settings: DEFAULT_SETTINGS, models: [], connection: "disconnected", settingsOpen: false, remoteEndpoint: false,
  localConfig: DEFAULT_LOCAL_CONFIG, runtime: STOPPED_RUNTIME, hardware: null, discovery: null,
  messages: [], usage: null, activeRequestId: null, generating: false, error: null,
  initialize: async () => {
    const settings = loadProviderSettings(localStorage);
    const localConfig = loadLocalConfig(localStorage);
    const firstRun = localStorage.getItem(FIRST_RUN_KEY) !== "complete";
    set({ settings, localConfig, settingsOpen: firstRun, remoteEndpoint: settings.provider !== "gravityforge-local" && isRemoteEndpoint(settings.baseUrl) });
    if (!listening) {
      listening = Promise.all([
        listen<RuntimeStatus>("local-runtime-status", ({ payload }) => set({ runtime: payload })),
        listen<DeltaEvent>("model-stream-delta", ({ payload }) => set((state) => appendDelta(state, payload.requestId, payload.content))),
        listen<CompleteEvent>("model-stream-complete", ({ payload }) => set((state) => {
          if (state.activeRequestId && state.activeRequestId !== payload.requestId) return {};
          const messages = state.messages.map((message, index) => index === state.messages.length - 1 && message.role === "assistant"
            ? { ...message, content: payload.content, toolCalls: payload.toolCalls } : message);
          return { messages, usage: payload.usage ?? null, generating: false, activeRequestId: null, error: null };
        })),
        listen<ErrorEvent>("model-stream-error", ({ payload }) => set((state) => {
          if (state.activeRequestId && state.activeRequestId !== payload.requestId) return {};
          const normalized = normalizeModelError(payload.error);
          return { generating: false, activeRequestId: null, error: normalized.code === "CANCELLED" ? null : normalized.message };
        })),
      ]).then(() => undefined).catch((error: unknown) => { listening = null; set({ error: text(error) }); });
    }
    await listening;
    try {
      const [runtime, hardware, discovery] = await Promise.all([localRuntimeApi.status(), localRuntimeApi.hardware(), localRuntimeApi.discovery()]);
      set({ runtime, hardware, discovery, localConfig });
    } catch (error) { set({ error: text(error) }); }
  },
  openSettings: () => set({ settingsOpen: true }), closeSettings: () => set({ settingsOpen: false }),
  saveSettings: async (candidate, apiKey) => {
    const settings = providerSettingsSchema.parse(candidate);
    await modelApi.setApiKey(apiKey);
    persist(settings);
    set({ settings, models: [], connection: "disconnected", remoteEndpoint: settings.provider !== "gravityforge-local" && isRemoteEndpoint(settings.baseUrl), error: null });
  },
  saveLocalConfig: (config) => { localStorage.setItem(LOCAL_RUNTIME_KEY, JSON.stringify(config)); localStorage.setItem(FIRST_RUN_KEY, "complete"); set({ localConfig: config }); },
  loadRuntime: async () => {
    set({ connection: "connecting", error: null });
    try {
      get().saveLocalConfig(get().localConfig);
      const runtime = await localRuntimeApi.start(get().localConfig);
      set({ runtime, connection: "connected" });
      await get().refreshModels();
    } catch (error) { set({ connection: "error", error: text(error) }); }
  },
  unloadRuntime: async () => {
    try {
      const runtime = await localRuntimeApi.stop();
      const settings = { ...get().settings, model: "", capabilities: { ...get().settings.capabilities, supportsTools: false, supportsStreaming: false } };
      persist(settings); set({ runtime, settings, models: [], connection: "disconnected", error: null });
    } catch (error) { set({ error: text(error) }); }
  },
  restartRuntime: async () => {
    set({ connection: "connecting", error: null });
    try { const runtime = await localRuntimeApi.restart(); set({ runtime, connection: "connected" }); await get().refreshModels(); }
    catch (error) { set({ connection: "error", error: text(error) }); }
  },
  testCapabilities: async () => {
    try {
      const capabilities = await localRuntimeApi.testCapabilities();
      const runtime = { ...get().runtime, capabilities };
      const settings = { ...get().settings, capabilities: { ...get().settings.capabilities, supportsTools: capabilities.toolCalling && capabilities.toolChoiceRequired, supportsStreaming: capabilities.streaming } };
      persist(settings); set({ runtime, settings, error: capabilities.agentMode ? null : capabilities.detail ?? "Agent Mode unavailable" });
    } catch (error) { set({ error: text(error) }); }
  },
  refreshModels: async () => {
    set({ connection: "connecting", error: null });
    try {
      const models = await providerRegistry.get(get().settings.provider).listModels(get().settings);
      const selected = models.some((model) => model.id === get().settings.model) ? get().settings.model : models[0]?.id ?? "";
      const settings = { ...get().settings, model: selected };
      persist(settings); set({ models, settings, connection: "connected" });
    } catch (error) { set({ connection: "error", error: text(error) }); }
  },
  testConnection: async () => {
    set({ connection: "connecting", error: null });
    try { await providerRegistry.get(get().settings.provider).healthCheck(get().settings); set({ connection: "connected" }); }
    catch (error) { set({ connection: "error", error: text(error) }); }
  },
  selectModel: (model) => { const settings = { ...get().settings, model }; persist(settings); set({ settings }); },
  send: async (content) => {
    const prompt = content.trim();
    if (!prompt || get().generating) return;
    if (!get().settings.model) { set({ error: "Select a model before sending a message" }); return; }
    await get().initialize();
    const user: ConversationMessage = { id: String(nextMessage++), role: "user", content: prompt, toolCalls: [] };
    const assistant: ConversationMessage = { id: String(nextMessage++), role: "assistant", content: "", toolCalls: [] };
    const history = [...get().messages, user];
    set({ messages: [...history, assistant], generating: true, activeRequestId: null, usage: null, error: null });
    try {
      const provider = providerRegistry.get(get().settings.provider);
      const tools = provider.supportsTools(get().settings) ? [SQUARE_NUMBER_TOOL] : [];
      const options = { messages: history.map(toApiMessage), tools, toolChoice: tools.length ? "auto" as const : undefined };
      if (provider.supportsStreaming(get().settings)) {
        const requestId = await provider.streamChat(get().settings, options);
        if (get().generating) set({ activeRequestId: requestId });
      } else {
        const response = await provider.chat(get().settings, options);
        set((state) => ({
          messages: state.messages.map((message, index) => index === state.messages.length - 1
            ? { ...message, content: response.message.content, toolCalls: response.message.toolCalls } : message),
          usage: response.usage ?? null,
          generating: false,
          activeRequestId: null,
        }));
      }
    } catch (error) { set({ generating: false, activeRequestId: null, error: text(error) }); }
  },
  cancel: async () => {
    const requestId = get().activeRequestId;
    if (!requestId) return;
    try { await modelApi.cancel(requestId); } catch (error) { set({ error: text(error) }); }
  },
  clearError: () => set({ error: null }),
}));
