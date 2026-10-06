import { invoke } from "@tauri-apps/api/core";
import type { ChatOptions, ChatResponse, ModelInfo, ProviderSettings } from "./types";
import { chatMessageSchema, normalizeModelError, toolDefinitionSchema } from "./types";

interface ProviderConfig { provider: string; baseUrl: string; model?: string; timeoutMs: number; retryOnce: boolean }

function config(settings: ProviderSettings): ProviderConfig {
  return { provider: settings.provider, baseUrl: settings.baseUrl, model: settings.model || undefined, timeoutMs: settings.timeoutMs, retryOnce: settings.retryOnce };
}
function request(settings: ProviderSettings, options: ChatOptions) {
  return {
    config: config(settings), messages: options.messages.map((message) => chatMessageSchema.parse(message)),
    tools: options.tools.map((tool) => toolDefinitionSchema.parse(tool)), toolChoice: options.toolChoice,
    temperature: settings.temperature, topP: settings.topP, topK: settings.topK ?? undefined,
    repetitionPenalty: settings.repetitionPenalty ?? undefined,
    maxTokens: settings.maxOutputTokens,
  };
}
async function call<T>(command: string, args: Record<string, unknown>): Promise<T> {
  try { return await invoke<T>(command, args); }
  catch (error) { throw normalizeModelError(error); }
}
export const modelApi = {
  setApiKey: (apiKey?: string) => call<void>("model_set_api_key", { apiKey }),
  listModels: (settings: ProviderSettings) => call<ModelInfo[]>("model_list_models", { config: config(settings) }),
  healthCheck: (settings: ProviderSettings) => call<void>("model_health_check", { config: config(settings) }),
  chat: (settings: ProviderSettings, options: ChatOptions) => call<ChatResponse>("model_chat", { request: request(settings, options) }),
  streamChat: (settings: ProviderSettings, options: ChatOptions) => call<string>("model_stream_chat", { request: request(settings, options) }),
  cancel: (requestId: string) => call<void>("model_cancel", { requestId }),
};
