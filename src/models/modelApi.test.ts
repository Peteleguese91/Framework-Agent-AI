import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_SETTINGS, SQUARE_NUMBER_TOOL } from "./types";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
import { modelApi } from "./modelApi";

beforeEach(() => invoke.mockReset());

describe("model IPC contract", () => {
  it("parses model discovery", async () => {
    invoke.mockResolvedValue([{ id: "qwen3-coder", ownedBy: "local" }]);
    await expect(modelApi.listModels(DEFAULT_SETTINGS)).resolves.toEqual([{ id: "qwen3-coder", ownedBy: "local" }]);
    expect(invoke).toHaveBeenCalledWith("model_list_models", {
      config: expect.objectContaining({ baseUrl: "http://localhost:1234/v1" }) as unknown,
    });
  });

  it("sends strong messages, sampling options and tool definitions", async () => {
    invoke.mockResolvedValue("request-1");
    const settings = { ...DEFAULT_SETTINGS, model: "qwen3-coder" };
    await modelApi.streamChat(settings, {
      messages: [{ role: "user", content: "square 12", toolCalls: [] }], tools: [SQUARE_NUMBER_TOOL], toolChoice: "auto",
    });
    expect(invoke).toHaveBeenCalledWith("model_stream_chat", {
      request: expect.objectContaining({
        temperature: 0.7, topP: 0.8, topK: 20, repetitionPenalty: 1.05, maxTokens: 4096,
        tools: [SQUARE_NUMBER_TOOL], toolChoice: "auto",
      }) as unknown,
    });
  });
});
