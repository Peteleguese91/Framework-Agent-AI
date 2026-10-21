import { create } from "zustand";
import { providerRegistry } from "../models/provider";
import { useModelStore } from "../models/modelStore";
import { useWorkspaceStore } from "../project/workspaceStore";
import { AgentEngine } from "./AgentEngine";
import { ApprovalManager } from "./approval";
import { ProviderTurnRunner } from "./modelTurn";
import { ToolRegistry } from "./toolRegistry";
import type { AgentEvent, AgentMode, AgentTask, ApprovalDecision, ApprovalRequest } from "./types";
import { computerApi } from "../computer/computerApi";

const HISTORY_KEY = "gravityforge.agent.history.v1";
export const AUDIT_KEY = "gravityforge.agent.audit.v1";
interface TaskSummary { id: string; goal: string; status: AgentTask["status"]; createdAt: number; finishedAt?: number; modifiedFiles: string[]; commands: number; finalResult?: string }
export interface ApprovalAuditEvent { at: number; request: ApprovalRequest; decision?: ApprovalDecision }
export interface AgentAuditRecord { task: AgentTask; events: AgentEvent[]; approvals: ApprovalAuditEvent[]; updatedAt: number }
interface AgentStore {
  mode: AgentMode; task: AgentTask | null; events: AgentEvent[]; approval: ApprovalRequest | null;
  history: TaskSummary[]; error: string | null;
  setMode: (mode: AgentMode) => void; start: (goal: string) => Promise<void>; submitUserInput: (input: string) => void;
  resolveApproval: (decision: ApprovalDecision) => void; pause: () => void; resume: () => void; stop: () => Promise<void>;
  clear: () => void;
}

let engine: AgentEngine | null = null;
let approvalManager: ApprovalManager | null = null;
let auditEvents: AgentEvent[] = [];
let auditApprovals: ApprovalAuditEvent[] = [];

function loadHistory(): TaskSummary[] {
  try { const value: unknown = JSON.parse(localStorage.getItem(HISTORY_KEY) ?? "[]"); return Array.isArray(value) ? value as TaskSummary[] : []; }
  catch { return []; }
}
function saveHistory(task: AgentTask): TaskSummary[] {
  const current = loadHistory().filter((item) => item.id !== task.id);
  const next = [{ id: task.id, goal: task.goal, status: task.status, createdAt: task.createdAt, finishedAt: task.finishedAt, modifiedFiles: task.modifiedFiles, commands: task.commands.length, finalResult: task.finalResult }, ...current].slice(0, 20);
  localStorage.setItem(HISTORY_KEY, JSON.stringify(next)); return next;
}
export function loadAgentAudits(storage: Pick<Storage, "getItem"> = localStorage): AgentAuditRecord[] {
  try { const value: unknown = JSON.parse(storage.getItem(AUDIT_KEY) ?? "[]"); return Array.isArray(value) ? value as AgentAuditRecord[] : []; }
  catch { return []; }
}
export function saveAgentAudit(
  task: AgentTask,
  events: AgentEvent[],
  approvals: ApprovalAuditEvent[],
  storage: Pick<Storage, "getItem" | "setItem"> = localStorage,
): void {
  const previous = loadAgentAudits(storage).filter((record) => record.task.id !== task.id);
  const record: AgentAuditRecord = {
    task: structuredClone(task), events: structuredClone(events), approvals: structuredClone(approvals), updatedAt: Date.now(),
  };
  storage.setItem(AUDIT_KEY, JSON.stringify([record, ...previous].slice(0, 20)));
}
const message = (error: unknown) => error instanceof Error ? error.message : String(error);

export const useAgentStore = create<AgentStore>((set, get) => ({
  mode: "ASK", task: null, events: [], approval: null, history: loadHistory(), error: null,
  setMode: (mode) => { if (!get().task || ["COMPLETED", "FAILED", "CANCELLED"].includes(get().task!.status)) set({ mode }); },
  start: async (goal) => {
    const clean = goal.trim(); if (!clean || get().mode === "ASK") return;
    const workspace = useWorkspaceStore.getState().workspace;
    const settings = useModelStore.getState().settings;
    if (!workspace) { set({ error: "Open a workspace before starting an agent task" }); return; }
    if (!settings.model) { set({ error: "Select a model before starting an agent task" }); return; }
    if (!settings.capabilities.supportsTools) { set({ error: "Agent Mode is unavailable until structured tool calling passes capability testing" }); return; }
    const provider = providerRegistry.get(settings.provider);
    auditEvents = []; auditApprovals = [];
    approvalManager = new ApprovalManager((approval) => {
      auditApprovals.push({ at: Date.now(), request: structuredClone(approval) });
      if (engine) saveAgentAudit(engine.task, auditEvents, auditApprovals);
      set({ approval });
    });
    const mode = get().mode;
    if (mode === "ASK") return;
    engine = new AgentEngine(clean, mode, {
      runner: new ProviderTurnRunner(provider, settings), registry: new ToolRegistry(), approvals: approvalManager, workspace,
      onEvent: (event, task) => { auditEvents = [...auditEvents, event].slice(-200); saveAgentAudit(task, auditEvents, auditApprovals); set({ events: auditEvents }); },
      onTask: (task) => { saveAgentAudit(task, auditEvents, auditApprovals); set({ task }); },
      onStop: () => computerApi.stop(),
    });
    set({ task: structuredClone(engine.task), events: [], approval: null, error: null });
    try {
      const task = await engine.run(); set({ task, history: saveHistory(task), approval: null });
    } catch (error) { set({ error: message(error), approval: null }); }
  },
  submitUserInput: (input) => engine?.provideUserInput(input),
  resolveApproval: (decision) => {
    const approval = get().approval; if (!approval) return;
    const pending = [...auditApprovals].reverse().find((event) => event.request.id === approval.id && event.decision === undefined);
    if (pending) pending.decision = decision;
    if (engine) saveAgentAudit(engine.task, auditEvents, auditApprovals);
    approvalManager?.resolve(approval.id, decision); set({ approval: null });
  },
  pause: () => engine?.pause(), resume: () => engine?.resume(),
  stop: async () => { await engine?.stop(); },
  clear: () => { if (!get().task || ["COMPLETED", "FAILED", "CANCELLED"].includes(get().task!.status)) set({ task: null, events: [], error: null }); },
}));
