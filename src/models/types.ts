import { z } from "zod";

export const modelCapabilitiesSchema = z.object({
  supportsTools: z.boolean(), supportsVision: z.boolean(), supportsStreaming: z.boolean(),
  supportsReasoning: z.boolean(), contextWindow: z.number().int().min(1024), maxOutputTokens: z.number().int().min(1),
});

export const providerSettingsSchema = z.object({
  provider: z.enum(["lm-studio", "openai-compatible"]), baseUrl: z.string().url(), model: z.string(),
  temperature: z.number().min(0).max(2), topP: z.number().min(0).max(1),
  topK: z.number().int().min(0).max(1000).nullable(), repetitionPenalty: z.number().min(0.1).max(2).nullable(),
  contextLength: z.number().int().min(1024).max(1_048_576), maxOutputTokens: z.number().int().min(1).max(131_072),
  timeoutMs: z.number().int().min(1).max(600_000), retryOnce: z.boolean(), capabilities: modelCapabilitiesSchema,
});

export type ProviderSettings = z.infer<typeof providerSettingsSchema>;
export type ModelCapabilities = z.infer<typeof modelCapabilitiesSchema>;

export const DEFAULT_SETTINGS: ProviderSettings = {
  provider: "lm-studio", baseUrl: "http://localhost:1234/v1", model: "", temperature: 0.7, topP: 0.8,
  topK: 20, repetitionPenalty: 1.05, contextLength: 32_768, maxOutputTokens: 4_096, timeoutMs: 120_000,
  retryOnce: true,
  capabilities: { supportsTools: true, supportsVision: false, supportsStreaming: true, supportsReasoning: false, contextWindow: 32_768, maxOutputTokens: 4_096 },
};

export const toolDefinitionSchema = z.object({
  name: z.string().regex(/^[a-zA-Z0-9_.-]{1,64}$/), description: z.string().min(1).max(1024),
  parameters: z.record(z.string(), z.unknown()), riskLevel: z.enum(["READ", "WRITE", "DESTRUCTIVE", "DANGEROUS"]),
});
export type ToolDefinition = z.infer<typeof toolDefinitionSchema>;

export const toolCallSchema = z.object({
  id: z.string().min(1), type: z.literal("function"),
  function: z.object({ name: z.string().min(1), arguments: z.unknown() }),
});
export type ToolCall = z.infer<typeof toolCallSchema>;

export const chatMessageSchema = z.object({
  role: z.enum(["system", "user", "assistant", "tool"]), content: z.string().nullable(),
  name: z.string().optional(), toolCallId: z.string().optional(), toolCalls: z.array(toolCallSchema).default([]),
});
export type ChatMessage = z.infer<typeof chatMessageSchema>;

export interface ModelInfo { id: string; ownedBy?: string }
export interface TokenUsage { prompt_tokens: number; completion_tokens: number; total_tokens: number }
export interface ChatOptions { messages: ChatMessage[]; tools: ToolDefinition[]; toolChoice?: "auto" | "none" | Record<string, unknown> }
export interface ChatResponse { message: ChatMessage; usage?: TokenUsage }

export const modelErrorSchema = z.object({
  code: z.enum(["INVALID_CONFIG", "CONNECTION_FAILED", "TIMEOUT", "HTTP_ERROR", "INVALID_RESPONSE", "MODEL_UNAVAILABLE", "CONTEXT_OVERFLOW", "MALFORMED_TOOL_CALL", "STREAM_INTERRUPTED", "CANCELLED", "INTERNAL"]),
  message: z.string(), status: z.number().nullable().optional(), retryable: z.boolean().optional(),
});

export class ModelProviderError extends Error {
  constructor(public readonly code: z.infer<typeof modelErrorSchema>["code"], message: string, public readonly status?: number | null, public readonly retryable = false) {
    super(message); this.name = "ModelProviderError";
  }
}

export function normalizeModelError(error: unknown): ModelProviderError {
  const parsed = modelErrorSchema.safeParse(error);
  if (parsed.success) return new ModelProviderError(parsed.data.code, parsed.data.message, parsed.data.status, parsed.data.retryable);
  return new ModelProviderError("INTERNAL", error instanceof Error ? error.message : String(error));
}

export const SQUARE_NUMBER_TOOL: ToolDefinition = {
  name: "square_number",
  description: "Return the square of a number. Request this tool instead of calculating directly when the user asks for a square.",
  parameters: { type: "object", properties: { value: { type: "number", description: "Number to square" } }, required: ["value"], additionalProperties: false },
  riskLevel: "READ",
};
