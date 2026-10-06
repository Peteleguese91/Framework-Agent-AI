import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS, ModelProviderError, normalizeModelError, providerSettingsSchema, toolCallSchema } from "./types";
import { loadProviderSettings } from "./modelStore";

describe("model configuration", () => {
  it("accepts the Qwen-oriented defaults", () => {
    expect(providerSettingsSchema.parse(DEFAULT_SETTINGS)).toMatchObject({
      provider: "lm-studio", baseUrl: "http://localhost:1234/v1", temperature: 0.7,
      topP: 0.8, topK: 20, repetitionPenalty: 1.05, contextLength: 32768,
    });
  });

  it("falls back when persisted settings are malformed", () => {
    const storage = { getItem: () => JSON.stringify({ provider: "bad" }) };
    expect(loadProviderSettings(storage)).toEqual(DEFAULT_SETTINGS);
  });

  it("restores valid persisted settings", () => {
    const saved = { ...DEFAULT_SETTINGS, model: "qwen3-coder" };
    expect(loadProviderSettings({ getItem: () => JSON.stringify(saved) }).model).toBe("qwen3-coder");
  });

  it("maps structured backend errors", () => {
    const error = normalizeModelError({ code: "CONTEXT_OVERFLOW", message: "too long", status: 400, retryable: false });
    expect(error).toBeInstanceOf(ModelProviderError);
    expect(error.code).toBe("CONTEXT_OVERFLOW");
  });

  it("validates tool call structure", () => {
    expect(toolCallSchema.parse({ id: "call_1", type: "function", function: { name: "square_number", arguments: { value: 12 } } }).function.arguments).toEqual({ value: 12 });
    expect(() => toolCallSchema.parse({ id: "", type: "function", function: { name: "", arguments: {} } })).toThrow();
  });
});
