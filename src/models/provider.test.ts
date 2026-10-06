import { describe, expect, it } from "vitest";
import { LMStudioProvider, OpenAICompatibleProvider, ProviderRegistry } from "./provider";
import { DEFAULT_SETTINGS } from "./types";

describe("provider registry", () => {
  it("registers interchangeable providers and rejects duplicates", () => {
    const registry = new ProviderRegistry();
    registry.register(new LMStudioProvider());
    registry.register(new OpenAICompatibleProvider());
    expect(registry.get("lm-studio").id).toBe("lm-studio");
    expect(registry.list()).toHaveLength(2);
    expect(() => registry.register(new LMStudioProvider())).toThrow("already registered");
  });

  it("uses manual capability overrides", () => {
    const provider = new LMStudioProvider();
    const settings = { ...DEFAULT_SETTINGS, capabilities: { ...DEFAULT_SETTINGS.capabilities, supportsTools: false, supportsVision: true } };
    expect(provider.supportsTools(settings)).toBe(false);
    expect(provider.supportsVision(settings)).toBe(true);
    expect(provider.capabilities(settings).contextWindow).toBe(32768);
  });
});
