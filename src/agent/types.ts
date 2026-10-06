import type { ChatMessage, ToolCall } from "../models/types";

export type AgentMode = "ASK" | "AGENT" | "AUTONOMOUS";
export type AgentStatus = "IDLE" | "PLANNING" | "RUNNING" | "WAITING_APPROVAL" | "WAITING_USER" | "VERIFYING" | "COMPLETED" | "FAILED" | "CANCELLED" | "PAUSED";
export type AgentStepStatus = "PENDING" | "RUNNING" | "DONE" | "FAILED" | "BLOCKED" | "SKIPPED";
export type AgentRisk = "SAFE" | "MODERATE" | "DANGEROUS";
export type VerificationStatus = "NOT_REQUIRED" | "PENDING" | "ATTEMPTED" | "PASSED" | "FAILED";

export interface AgentStep { id: string; title: string; status: AgentStepStatus }
export interface AgentCommandRecord { command: string; cwd?: string; exitCode?: number; durationMs: number; success: boolean }
export interface ToolObservation {
  tool: string; status: "success" | "error"; summary: string; data?: unknown;
  truncated: boolean; durationMs: number; error?: string;
}
export interface AgentToolRecord {
  id: string; name: string; arguments: unknown; risk: AgentRisk; startedAt: number;
  finishedAt?: number; observation?: ToolObservation;
}
export interface AgentTask {
  id: string; goal: string; mode: Exclude<AgentMode, "ASK">; status: AgentStatus;
  createdAt: number; startedAt?: number; finishedAt?: number; plan: string[]; steps: AgentStep[];
  observations: ToolObservation[]; toolCalls: AgentToolRecord[]; modifiedFiles: string[];
  commands: AgentCommandRecord[]; errors: string[]; verificationStatus: VerificationStatus;
  finalResult?: string; iteration: number;
}
export interface AgentLimits {
  maxIterations: number; maxToolCalls: number; maxConsecutiveErrors: number;
  maxSameToolRetries: number; maxObservationBytes: number;
}
export const DEFAULT_AGENT_LIMITS: AgentLimits = {
  maxIterations: 50, maxToolCalls: 100, maxConsecutiveErrors: 3,
  maxSameToolRetries: 2, maxObservationBytes: 64 * 1024,
};

export type AgentEventName =
  | "agent.started" | "agent.plan.updated" | "agent.step.started" | "agent.tool.requested"
  | "agent.approval.requested" | "agent.tool.started" | "agent.tool.completed" | "agent.tool.failed"
  | "agent.verification.started" | "agent.completed" | "agent.failed" | "agent.cancelled";
export interface AgentEvent { name: AgentEventName; at: number; summary: string; tool?: string; risk?: AgentRisk }

export interface AgentTurnResult { message: ChatMessage; toolCalls: ToolCall[] }
export interface ApprovalRequest { id: string; tool: string; arguments: unknown; risk: AgentRisk; reason: string }
export type ApprovalDecision = "APPROVE_ONCE" | "REJECT";
