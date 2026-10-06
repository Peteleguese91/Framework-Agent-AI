import { modelApi } from "./modelApi";
import type { ChatOptions, ChatResponse, ModelCapabilities, ModelInfo, ProviderSettings } from "./types";

export interface ModelProvider {
  readonly id: ProviderSettings["provider"];
  healthCheck(settings: ProviderSettings): Promise<void>;
  listModels(settings: ProviderSettings): Promise<ModelInfo[]>;
  chat(settings: ProviderSettings, options: ChatOptions): Promise<ChatResponse>;
  streamChat(settings: ProviderSettings, options: ChatOptions): Promise<string>;
  supportsTools(settings: ProviderSettings): boolean;
  supportsVision(settings: ProviderSettings): boolean;
  supportsStreaming(settings: ProviderSettings): boolean;
  capabilities(settings: ProviderSettings): ModelCapabilities;
}
export class OpenAICompatibleProvider implements ModelProvider {
  readonly id: ProviderSettings["provider"] = "openai-compatible";
  healthCheck(settings: ProviderSettings) { return modelApi.healthCheck(settings); }
  listModels(settings: ProviderSettings) { return modelApi.listModels(settings); }
  chat(settings: ProviderSettings, options: ChatOptions) { return modelApi.chat(settings, options); }
  streamChat(settings: ProviderSettings, options: ChatOptions) { return modelApi.streamChat(settings, options); }
  supportsTools(settings: ProviderSettings) { return settings.capabilities.supportsTools; }
  supportsVision(settings: ProviderSettings) { return settings.capabilities.supportsVision; }
  supportsStreaming(settings: ProviderSettings) { return settings.capabilities.supportsStreaming; }
  capabilities(settings: ProviderSettings) { return settings.capabilities; }
}
export class LMStudioProvider extends OpenAICompatibleProvider { override readonly id = "lm-studio" as const; }
export class ProviderRegistry {
  private readonly providers = new Map<ProviderSettings["provider"], ModelProvider>();
  register(provider: ModelProvider): void { if (this.providers.has(provider.id)) throw new Error(`Provider already registered: ${provider.id}`); this.providers.set(provider.id, provider); }
  get(id: ProviderSettings["provider"]): ModelProvider { const provider = this.providers.get(id); if (!provider) throw new Error(`Unknown provider: ${id}`); return provider; }
  list(): ModelProvider[] { return [...this.providers.values()]; }
}
export const providerRegistry = new ProviderRegistry();
providerRegistry.register(new LMStudioProvider());
providerRegistry.register(new OpenAICompatibleProvider());
