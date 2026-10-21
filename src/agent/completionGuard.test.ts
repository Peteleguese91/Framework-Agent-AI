import { describe, expect, it } from "vitest";
import { CompletionGuard } from "./completionGuard";
import type { AgentTask } from "./types";

const task = (): AgentTask => ({
  id: "1", goal: "change code", mode: "AGENT", status: "RUNNING", createdAt: 1,
  plan: [], steps: [], observations: [], toolCalls: [], modifiedFiles: [], commands: [], errors: [],
  verificationStatus: "PENDING", iteration: 1,
});

describe("CompletionGuard", () => {
  it("permits a read-only completed analysis", () => expect(new CompletionGuard().evaluate(task()).complete).toBe(true));
  it("blocks unverified mutations and unresolved failures", () => {
    const value = task(); value.modifiedFiles.push("src/a.ts");
    expect(new CompletionGuard().evaluate(value).complete).toBe(false);
    value.verificationStatus = "PASSED"; value.observations.push({ tool: "test", status: "error", summary: "failed", truncated: false, durationMs: 1 });
    expect(new CompletionGuard().evaluate(value).complete).toBe(false);
  });
  it("blocks desktop actions until a later perception succeeds", () => {
    const value = task();
    value.observations.push({ tool: "computer.click", status: "success", summary: "clicked", truncated: false, durationMs: 1 });
    expect(new CompletionGuard().evaluate(value).reasons).toContain("Desktop actions require a successful follow-up perception check");
    value.observations.push({ tool: "computer.screenshot", status: "success", summary: "captured", truncated: false, durationMs: 1 });
    expect(new CompletionGuard().evaluate(value).complete).toBe(true);
  });
});
