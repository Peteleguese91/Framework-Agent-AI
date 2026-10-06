import { z } from "zod";
import type { WorkspaceInfo } from "../project/types";
import type { ChatMessage, ToolCall } from "../models/types";
import { normalizeModelError } from "../models/types";
import { ApprovalManager } from "./approval";
import { CompletionGuard, isVerificationCommand } from "./completionGuard";
import { ContextBuilder } from "./contextBuilder";
import type { ModelTurnRunner } from "./modelTurn";
import { mutationPath, summarizeResult, ToolRegistry } from "./toolRegistry";
import {
  DEFAULT_AGENT_LIMITS, type AgentEvent, type AgentLimits, type AgentMode, type AgentTask,
  type ApprovalRequest, type ToolObservation,
} from "./types";

export interface AgentEngineOptions {
  runner: ModelTurnRunner; registry: ToolRegistry; approvals: ApprovalManager; workspace: WorkspaceInfo;
  limits?: Partial<AgentLimits>; onEvent?: (event: AgentEvent, task: AgentTask) => void;
  onTask?: (task: AgentTask) => void;
}

const now = () => Date.now();
const id = () => globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
const stable = (value: unknown) => JSON.stringify(value, Object.keys((value ?? {}) as object).sort());

function initialPlan(goal: string): string[] {
  const readOnly = /analy|explain|structure|inspect|review|descri|spiega|struttura/iu.test(goal) && !/fix|correct|create|write|modif|build error|correggi|crea/iu.test(goal);
  return readOnly
    ? ["Inspect workspace structure", "Read relevant project files", "Synthesize findings from evidence"]
    : ["Inspect workspace and reproduce the issue", "Locate the relevant files and cause", "Apply minimal changes", "Run focused verification", "Review the final result"];
}

export class AgentEngine {
  readonly task: AgentTask;
  private readonly limits: AgentLimits;
  private readonly context = new ContextBuilder();
  private readonly guard = new CompletionGuard();
  private messages: ChatMessage[] = [];
  private stopped = false;
  private paused = false;
  private resumeWaiter: (() => void) | null = null;
  private userWaiter: ((input: string) => void) | null = null;
  private consecutiveErrors = 0;
  private contextRetry = false;
  private repeats = new Map<string, number>();

  constructor(goal: string, mode: Exclude<AgentMode, "ASK">, private readonly options: AgentEngineOptions) {
    const plan = initialPlan(goal);
    this.limits = { ...DEFAULT_AGENT_LIMITS, ...options.limits };
    this.task = {
      id: id(), goal, mode, status: "IDLE", createdAt: now(), plan,
      steps: plan.map((title) => ({ id: id(), title, status: "PENDING" })), observations: [],
      toolCalls: [], modifiedFiles: [], commands: [], errors: [], verificationStatus: "PENDING", iteration: 0,
    };
  }

  private update(): void { this.options.onTask?.(structuredClone(this.task)); }
  private emit(name: AgentEvent["name"], summary: string, extra: Partial<AgentEvent> = {}): void {
    this.options.onEvent?.({ name, at: now(), summary, ...extra }, structuredClone(this.task));
    this.update();
  }
  private setStatus(status: AgentTask["status"]): void { this.task.status = status; this.update(); }
  private fail(message: string): AgentTask {
    this.task.status = "FAILED"; this.task.finishedAt = now(); this.task.errors.push(message);
    const step = this.task.steps.find((item) => item.status === "RUNNING"); if (step) step.status = "FAILED";
    this.emit("agent.failed", message); return this.task;
  }

  async run(): Promise<AgentTask> {
    this.task.startedAt = now(); this.setStatus("PLANNING");
    this.emit("agent.started", this.task.goal); this.emit("agent.plan.updated", this.task.plan.join(" → "));
    this.messages = this.context.build(this.task, this.options.workspace);
    this.setStatus("RUNNING");
    while (!this.stopped && this.task.iteration < this.limits.maxIterations) {
      await this.waitIfPaused();
      if (this.stopped) break;
      this.task.iteration += 1;
      try {
        const turn = await this.options.runner.run(this.messages, this.options.registry.definitions());
        if (this.stopped) break;
        this.consecutiveErrors = 0;
        if (!turn.message.content && turn.toolCalls.length === 0) {
          this.consecutiveErrors += 1;
          this.task.errors.push("Model returned an empty response");
          this.messages.push({ role: "system", content: "Your response was empty. Use a tool or provide a concrete final answer.", toolCalls: [] });
          continue;
        }
        this.messages.push(turn.message);
        if (turn.toolCalls.length > 0) {
          for (const call of turn.toolCalls) {
            if (this.stopped) break;
            await this.executeTool(call);
          }
          continue;
        }
        const content = turn.message.content?.trim() ?? "";
        if (content.startsWith("NEED_USER_INPUT:")) {
          this.setStatus("WAITING_USER");
          const input = await new Promise<string>((resolve) => { this.userWaiter = resolve; });
          this.userWaiter = null;
          if (this.stopped) break;
          this.messages.push({ role: "user", content: input, toolCalls: [] });
          this.setStatus("RUNNING");
          continue;
        }
        const decision = this.guard.evaluate(this.task);
        if (!decision.complete) {
          this.setStatus("VERIFYING"); this.emit("agent.verification.started", decision.reasons.join("; "));
          this.messages.push({ role: "system", content: `Completion blocked: ${decision.reasons.join("; ")}. Use available tools to verify or resolve this before answering.`, toolCalls: [] });
          this.setStatus("RUNNING");
          continue;
        }
        this.task.status = "COMPLETED"; this.task.finishedAt = now(); this.task.finalResult = content;
        this.task.steps.forEach((step) => { if (step.status === "PENDING" || step.status === "RUNNING") step.status = "DONE"; });
        this.emit("agent.completed", content || "Task completed"); return this.task;
      } catch (error) {
        const modelError = normalizeModelError(error);
        if (modelError.code === "CANCELLED" && this.stopped) break;
        if (modelError.code === "CONTEXT_OVERFLOW" && !this.contextRetry) {
          this.contextRetry = true; this.messages = this.context.compact(this.messages); continue;
        }
        this.consecutiveErrors += 1; this.task.errors.push(modelError.message);
        if (this.consecutiveErrors >= this.limits.maxConsecutiveErrors) return this.fail(modelError.message);
        this.messages.push({ role: "system", content: `Model request failed: ${modelError.message}. Reassess and continue without repeating the same action.`, toolCalls: [] });
      }
    }
    if (this.stopped) return this.cancelTask();
    return this.fail("MAX_ITERATIONS reached before completion");
  }

  private async executeTool(call: ToolCall): Promise<void> {
    if (this.task.toolCalls.length >= this.limits.maxToolCalls) throw new Error("MAX_TOOL_CALLS reached");
    let checked: ReturnType<ToolRegistry["validate"]>;
    try { checked = this.options.registry.validate(call.function.name, call.function.arguments); }
    catch (error) { this.recordToolError(call, error instanceof z.ZodError ? error.issues[0]?.message ?? "Malformed arguments" : String(error)); return; }
    const key = `${call.function.name}:${stable(checked.argumentsValue)}`;
    const repeated = (this.repeats.get(key) ?? 0) + 1; this.repeats.set(key, repeated);
    if (repeated > this.limits.maxSameToolRetries + 1) throw new Error("LOOP_DETECTED: repeated identical tool call");
    if (repeated > this.limits.maxSameToolRetries) {
      this.recordToolError(call, "Repeated tool call blocked. Reassess using new evidence."); return;
    }
    const record = { id: call.id, name: call.function.name, arguments: checked.argumentsValue, risk: checked.risk, startedAt: now() };
    this.task.toolCalls.push(record); this.emit("agent.tool.requested", call.function.name, { tool: call.function.name, risk: checked.risk });
    let approved = false;
    if (this.options.approvals.requiresApproval(this.task.mode, checked.risk)) {
      const request: ApprovalRequest = { id: call.id, tool: call.function.name, arguments: checked.argumentsValue, risk: checked.risk, reason: "Dangerous actions always require explicit approval" };
      this.setStatus("WAITING_APPROVAL"); this.emit("agent.approval.requested", request.reason, { tool: request.tool, risk: request.risk });
      approved = await this.options.approvals.authorize(this.task.mode, request);
      if (this.stopped) return;
      this.setStatus("RUNNING");
      if (!approved) { this.recordToolError(call, "User rejected this action"); return; }
    }
    this.startRelevantStep(call.function.name);
    this.emit("agent.tool.started", call.function.name, { tool: call.function.name, risk: checked.risk });
    const started = performance.now();
    try {
      const result = await checked.tool.handler(checked.argumentsValue, { mode: this.task.mode, approved });
      const observation = this.observation(call.function.name, "success", summarizeResult(call.function.name, result), result, performance.now() - started);
      record.finishedAt = now(); record.observation = observation; this.task.observations.push(observation);
      for (const path of mutationPath(call.function.name, checked.argumentsValue)) if (!this.task.modifiedFiles.includes(path)) this.task.modifiedFiles.push(path);
      if (call.function.name === "terminal.exec") this.trackCommand(checked.argumentsValue, result);
      this.messages.push({ role: "tool", content: JSON.stringify(observation), toolCallId: call.id, name: call.function.name, toolCalls: [] });
      this.finishCurrentStep(true); this.emit("agent.tool.completed", observation.summary, { tool: call.function.name, risk: checked.risk });
    } catch (error) {
      record.finishedAt = now(); this.recordToolError(call, error instanceof Error ? error.message : String(error), performance.now() - started, record);
    }
  }

  private observation(tool: string, status: "success" | "error", summary: string, data: unknown, durationMs: number): ToolObservation {
    const serialized = JSON.stringify(data); const limit = this.limits.maxObservationBytes; const truncated = serialized.length > limit;
    return { tool, status, summary, data: truncated ? serialized.slice(0, limit) : data, truncated, durationMs: Math.round(durationMs) };
  }
  private recordToolError(call: ToolCall, message: string, durationMs = 0, existing?: AgentTask["toolCalls"][number]): void {
    const observation = this.observation(call.function.name, "error", message, undefined, durationMs); observation.error = message;
    const record = existing ?? { id: call.id, name: call.function.name, arguments: call.function.arguments, risk: "DANGEROUS", startedAt: now() };
    if (!existing) this.task.toolCalls.push(record); record.finishedAt = now(); record.observation = observation;
    this.task.observations.push(observation); this.task.errors.push(message);
    this.messages.push({ role: "tool", content: JSON.stringify(observation), toolCallId: call.id, name: call.function.name, toolCalls: [] });
    this.finishCurrentStep(false); this.emit("agent.tool.failed", message, { tool: call.function.name, risk: record.risk });
  }
  private trackCommand(value: unknown, result: unknown): void {
    const args = value as { command: string; args: string[]; cwd?: string }; const output = result as { exitCode?: number; durationMs: number; success: boolean };
    this.task.commands.push({ command: [args.command, ...args.args].join(" "), cwd: args.cwd, exitCode: output.exitCode, durationMs: output.durationMs, success: output.success });
    if (isVerificationCommand(args.command, args.args)) this.task.verificationStatus = output.success ? "PASSED" : "FAILED";
  }
  private startRelevantStep(tool: string): void {
    const current = this.task.steps.find((step) => step.status === "RUNNING"); if (current) return;
    const step = this.task.steps.find((item) => item.status === "PENDING"); if (!step) return;
    step.status = "RUNNING"; this.emit("agent.step.started", `${step.title}: ${tool}`, { tool });
  }
  private finishCurrentStep(success: boolean): void {
    const step = this.task.steps.find((item) => item.status === "RUNNING"); if (step) step.status = success ? "DONE" : "PENDING";
  }
  pause(): void { if (["RUNNING", "VERIFYING"].includes(this.task.status)) { this.paused = true; this.setStatus("PAUSED"); } }
  resume(): void { if (!this.paused) return; this.paused = false; this.setStatus("RUNNING"); this.resumeWaiter?.(); this.resumeWaiter = null; }
  provideUserInput(input: string): void { this.userWaiter?.(input); }
  async stop(): Promise<void> { this.stopped = true; this.options.approvals.rejectAll(); this.resume(); this.userWaiter?.(""); try { await this.options.runner.cancel(); } catch { /* cancellation is best effort */ } }
  private waitIfPaused(): Promise<void> { return this.paused ? new Promise((resolve) => { this.resumeWaiter = resolve; }) : Promise.resolve(); }
  private cancelTask(): AgentTask { this.task.status = "CANCELLED"; this.task.finishedAt = now(); this.emit("agent.cancelled", "Task cancelled"); return this.task; }
}
