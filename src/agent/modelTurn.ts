import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ModelProvider } from "../models/provider";
import { modelApi } from "../models/modelApi";
import { normalizeModelError, type ChatMessage, type ProviderSettings, type ToolCall, type ToolDefinition } from "../models/types";
import type { AgentTurnResult } from "./types";

interface CompleteEvent { requestId: string; content: string; toolCalls: ToolCall[] }
interface ErrorEvent { requestId: string; error: unknown }

export interface ModelTurnRunner {
  run(messages: ChatMessage[], tools: ToolDefinition[]): Promise<AgentTurnResult>;
  cancel(): Promise<void>;
}

export class ProviderTurnRunner implements ModelTurnRunner {
  private activeRequestId: string | null = null;
  constructor(private readonly provider: ModelProvider, private readonly settings: ProviderSettings) {}

  async run(messages: ChatMessage[], tools: ToolDefinition[]): Promise<AgentTurnResult> {
    let expected: string | null = null;
    const unlisteners: UnlistenFn[] = [];
    try {
      const result = new Promise<AgentTurnResult>((resolve, reject) => {
        void listen<CompleteEvent>("model-stream-complete", ({ payload }) => {
          if (expected && payload.requestId !== expected) return;
          expected = payload.requestId;
          resolve({ message: { role: "assistant", content: payload.content, toolCalls: payload.toolCalls }, toolCalls: payload.toolCalls });
        }).then((unlisten) => unlisteners.push(unlisten), reject);
        void listen<ErrorEvent>("model-stream-error", ({ payload }) => {
          if (expected && payload.requestId !== expected) return;
          expected = payload.requestId;
          reject(normalizeModelError(payload.error));
        }).then((unlisten) => unlisteners.push(unlisten), reject);
      });
      expected = await this.provider.streamChat(this.settings, { messages, tools, toolChoice: "auto" });
      this.activeRequestId = expected;
      return await result;
    } finally {
      this.activeRequestId = null;
      unlisteners.forEach((unlisten) => unlisten());
    }
  }

  async cancel(): Promise<void> {
    if (!this.activeRequestId) return;
    await modelApi.cancel(this.activeRequestId);
  }
}
