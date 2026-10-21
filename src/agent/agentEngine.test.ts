import { describe, expect, it, vi } from "vitest";
import { z } from "zod";
import type { WorkspaceInfo } from "../project/types";
import type { ChatMessage, ToolCall } from "../models/types";
import { AgentEngine } from "./AgentEngine";
import { ApprovalManager } from "./approval";
import type { ModelTurnRunner } from "./modelTurn";
import { ToolRegistry, type AgentTool } from "./toolRegistry";
import type { AgentTurnResult, ApprovalRequest } from "./types";

const workspace: WorkspaceInfo = {
  name: "fixture", root: "/fixture",
  projectInfo: { name: "fixture", projectTypes: ["node"], languages: ["TypeScript"], detectedFrameworks: [], hasGit: false, mainConfigFiles: [] },
  projectMap: { mainDirectories: ["src"], configFiles: ["package.json"], filesByExtension: { ts: 1 }, entryPoints: ["src/main.ts"], hasTests: true, totalFiles: 2, truncated: false },
};

const call = (id: string, name: string, args: unknown): ToolCall => ({ id, type: "function", function: { name, arguments: args } });
const turn = (content: string | null, toolCalls: ToolCall[] = []): AgentTurnResult => ({ message: { role: "assistant", content, toolCalls }, toolCalls });

class MockRunner implements ModelTurnRunner {
  private index = 0;
  readonly seen: ChatMessage[][] = [];
  readonly cancel = vi.fn(() => Promise.resolve());
  constructor(private readonly turns: AgentTurnResult[]) {}
  run(messages: ChatMessage[]): Promise<AgentTurnResult> { this.seen.push(structuredClone(messages)); return Promise.resolve(this.turns[this.index++] ?? turn("done")); }
}

function tool(name: string, risk: "SAFE" | "MODERATE" | "DANGEROUS", handler: AgentTool["handler"]): AgentTool {
  return { name, description: name, schema: z.object({ value: z.string().optional() }), parameters: { type: "object" }, risk: () => risk, definitionRisk: risk === "SAFE" ? "READ" : risk === "MODERATE" ? "WRITE" : "DANGEROUS", allowedActors: ["AGENT"], timeoutMs: 1000, handler };
}

function engine(turns: AgentTurnResult[], tools: AgentTool[], notify: (request: ApprovalRequest) => void = () => undefined, limits = {}) {
  const runner = new MockRunner(turns);
  return { runner, value: new AgentEngine("Analyze the project", "AGENT", { runner, registry: new ToolRegistry(tools), approvals: new ApprovalManager(notify), workspace, limits }) };
}

describe("AgentEngine", () => {
  it("executes one tool and continues to a final answer", async () => {
    const handler = vi.fn(() => Promise.resolve({ operation: "exists", exists: true }));
    const { value } = engine([turn(null, [call("1", "fs.exists", { value: "a" })]), turn("verified answer")], [tool("fs.exists", "SAFE", handler)]);
    const task = await value.run();
    expect(task.status).toBe("COMPLETED"); expect(handler).toHaveBeenCalledOnce(); expect(task.observations).toHaveLength(1);
  });

  it("executes multiple calls sequentially", async () => {
    const order: string[] = [];
    const tools = [tool("first", "SAFE", () => { order.push("first"); return Promise.resolve({}); }), tool("second", "SAFE", () => { order.push("second"); return Promise.resolve({}); })];
    const { value } = engine([turn(null, [call("1", "first", {}), call("2", "second", {})]), turn("done")], tools);
    expect((await value.run()).status).toBe("COMPLETED"); expect(order).toEqual(["first", "second"]);
  });

  it("returns tool failures to the model and permits recovery", async () => {
    let attempts = 0;
    const failing = tool("probe", "SAFE", () => { attempts += 1; return attempts === 1 ? Promise.reject(new Error("temporary")) : Promise.resolve({}); });
    const { value } = engine([turn(null, [call("1", "probe", { value: "a" })]), turn(null, [call("2", "probe", { value: "b" })]), turn("recovered")], [failing]);
    const task = await value.run(); expect(task.status).toBe("COMPLETED"); expect(task.observations.map((item) => item.status)).toEqual(["error", "success"]);
  });

  it("records malformed and unknown calls without crashing", async () => {
    const known = tool("known", "SAFE", () => Promise.resolve({}));
    const { value } = engine([turn(null, [call("1", "known", { value: 4 }), call("2", "missing", {})]), turn("handled")], [known]);
    const task = await value.run(); expect(task.status).toBe("FAILED"); expect(task.errors.join(" ")).toContain("Unknown tool");
  });

  it("waits for dangerous approval", async () => {
    const handler = vi.fn(() => Promise.resolve({}));
    const runner = new MockRunner([turn(null, [call("danger", "delete", {})]), turn("done")]);
    const manager = new ApprovalManager((request) => manager.resolve(request.id, "APPROVE_ONCE"));
    const value = new AgentEngine("Delete generated files", "AGENT", { runner, registry: new ToolRegistry([tool("delete", "DANGEROUS", handler)]), approvals: manager, workspace });
    expect((await value.run()).status).toBe("COMPLETED"); expect(handler).toHaveBeenCalledOnce();
  });

  it("detects repeated identical calls", async () => {
    const repeated = call("same", "probe", { value: "same" });
    const { value } = engine([turn(null, [repeated]), turn(null, [repeated]), turn(null, [repeated]), turn(null, [repeated])], [tool("probe", "SAFE", () => Promise.resolve({}))]);
    const task = await value.run(); expect(task.status).toBe("FAILED"); expect(task.errors.join(" ")).toContain("LOOP_DETECTED");
  });

  it("honors max iterations and cancellation", async () => {
    const maxed = engine([turn(null, [call("1", "probe", { value: "1" })])], [tool("probe", "SAFE", () => Promise.resolve({}))], undefined, { maxIterations: 1 });
    expect((await maxed.value.run()).status).toBe("FAILED");
    const cancelled = engine([turn("NEED_USER_INPUT: value")], []);
    const pending = cancelled.value.run(); await Promise.resolve(); await cancelled.value.stop();
    expect((await pending).status).toBe("CANCELLED"); expect(cancelled.runner.cancel).toHaveBeenCalledOnce();
  });

  it("does not start a new tool while paused", async () => {
    let release: ((value: AgentTurnResult) => void) | undefined;
    const runner: ModelTurnRunner = {
      run: () => new Promise((resolve) => { release = resolve; }),
      cancel: () => Promise.resolve(),
    };
    const handler = vi.fn(() => Promise.resolve({}));
    const value = new AgentEngine("Analyze", "AGENT", { runner, registry: new ToolRegistry([tool("probe", "SAFE", handler)]), approvals: new ApprovalManager(() => undefined), workspace, limits: { maxIterations: 1 } });
    const pending = value.run();
    await vi.waitFor(() => expect(release).toBeTypeOf("function"));
    value.pause(); release?.(turn(null, [call("1", "probe", {})])); await Promise.resolve();
    expect(handler).not.toHaveBeenCalled(); value.resume();
    expect((await pending).status).toBe("FAILED"); expect(handler).toHaveBeenCalledOnce();
  });

  it("truncates oversized observations before the next model turn", async () => {
    const { runner, value } = engine([turn(null, [call("1", "large", {})]), turn("done")], [tool("large", "SAFE", () => Promise.resolve({ text: "x".repeat(70_000) }))]);
    const task = await value.run();
    expect(task.observations[0]?.truncated).toBe(true);
    expect(runner.seen[1]?.at(-1)?.content).toContain('"truncated":true');
    expect((runner.seen[1]?.at(-1)?.content?.length ?? 0)).toBeLessThan(66_000);
  });

  it("enforces the independent computer step limit", async () => {
    const calls = Array.from({ length: 21 }, (_, index) => call(String(index), "computer.probe", { value: String(index) }));
    const { value } = engine([turn(null, calls)], [tool("computer.probe", "SAFE", () => Promise.resolve({}))], undefined, { maxComputerSteps: 20 });
    const result = await value.run();
    expect(result.status).toBe("FAILED");
    expect(result.errors.join(" ")).toContain("MAX_COMPUTER_STEPS");
    expect(result.toolCalls).toHaveLength(20);
  });

  it("calls the desktop stop hook immediately", async () => {
    const onStop = vi.fn(() => Promise.resolve());
    const runner = new MockRunner([turn("NEED_USER_INPUT: wait")]);
    const value = new AgentEngine("Wait", "AGENT", { runner, registry: new ToolRegistry([]), approvals: new ApprovalManager(() => undefined), workspace, onStop });
    const pending = value.run(); await Promise.resolve(); await value.stop(); await pending;
    expect(onStop).toHaveBeenCalledOnce();
  });
});
