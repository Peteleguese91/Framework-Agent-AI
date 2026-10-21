import { describe, expect, it } from "vitest";
import { AUDIT_KEY, loadAgentAudits, saveAgentAudit, type ApprovalAuditEvent } from "./agentStore";
import type { AgentEvent, AgentTask } from "./types";

class MemoryStorage {
  values = new Map<string, string>();
  getItem(key: string) { return this.values.get(key) ?? null; }
  setItem(key: string, value: string) { this.values.set(key, value); }
}

const task = (id: string): AgentTask => ({
  id, goal: "Analyze read-only", mode: "AGENT", status: "COMPLETED", createdAt: 1, startedAt: 2, finishedAt: 3,
  plan: ["Inspect"], steps: [], observations: [{ tool: "fs.list", status: "success", summary: "5 entries", truncated: false, durationMs: 4 }],
  toolCalls: [{ id: "call-1", name: "fs.list", arguments: { path: "" }, risk: "SAFE", startedAt: 2, finishedAt: 3,
    observation: { tool: "fs.list", status: "success", summary: "5 entries", truncated: false, durationMs: 4 } }],
  modifiedFiles: [], commands: [], errors: [], verificationStatus: "NOT_REQUIRED", finalResult: "Five files found.", iteration: 2,
});

describe("agent audit persistence", () => {
  it("persists the full task, ordered events, approvals and final response", () => {
    const storage = new MemoryStorage();
    const events: AgentEvent[] = [{ name: "agent.tool.completed", at: 3, summary: "5 entries", tool: "fs.list", risk: "SAFE" }];
    const approvals: ApprovalAuditEvent[] = [];
    saveAgentAudit(task("task-1"), events, approvals, storage);
    const [audit] = loadAgentAudits(storage);
    expect(audit.task.toolCalls[0]?.arguments).toEqual({ path: "" });
    expect(audit.task.finalResult).toBe("Five files found.");
    expect(audit.events).toEqual(events);
    expect(audit.approvals).toEqual([]);
    expect(storage.values.has(AUDIT_KEY)).toBe(true);
  });

  it("updates an existing task and retains only the latest twenty audits", () => {
    const storage = new MemoryStorage();
    for (let index = 0; index < 22; index += 1) saveAgentAudit(task(`task-${index}`), [], [], storage);
    const updated = task("task-21"); updated.finalResult = "updated";
    saveAgentAudit(updated, [], [], storage);
    const audits = loadAgentAudits(storage);
    expect(audits).toHaveLength(20);
    expect(audits[0]?.task.finalResult).toBe("updated");
    expect(audits.filter((audit) => audit.task.id === "task-21")).toHaveLength(1);
  });
});
