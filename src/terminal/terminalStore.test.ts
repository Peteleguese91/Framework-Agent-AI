// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  create: vi.fn(),
  write: vi.fn(),
  resize: vi.fn(),
  interrupt: vi.fn(),
  kill: vi.fn(),
  close: vi.fn(),
  status: vi.fn(),
}));
vi.mock("./terminalApi", () => ({ terminalApi: api }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

import { useTerminalStore } from "./terminalStore";
import type { TerminalInfo } from "./types";

function info(id: string): TerminalInfo {
  return {
    id, shell: "sh", cwd: "/workspace", pid: Number(id),
    createdAt: 1, status: "running",
  };
}

describe("terminal sessions", () => {
  beforeEach(() => {
    localStorage.clear();
    Object.values(api).forEach((mock) => mock.mockReset());
    api.create.mockImplementation(() => Promise.resolve(info(String(api.create.mock.calls.length))));
    api.status.mockImplementation((id: string) => Promise.resolve(info(id)));
    api.close.mockResolvedValue(undefined);
    useTerminalStore.setState({ terminals: [], activeId: null, error: null });
  });

  it("creates independent tabs and switches the active terminal", async () => {
    await useTerminalStore.getState().create("/workspace");
    await useTerminalStore.getState().create("/workspace");
    expect(useTerminalStore.getState().terminals.map((item) => item.id)).toEqual(["1", "2"]);
    expect(useTerminalStore.getState().activeId).toBe("2");
    useTerminalStore.getState().activate("1");
    expect(useTerminalStore.getState().activeId).toBe("1");
  });

  it("closes one session without closing another", async () => {
    await useTerminalStore.getState().create("/workspace");
    await useTerminalStore.getState().create("/workspace");
    await useTerminalStore.getState().close("2");
    expect(api.close).toHaveBeenCalledWith("2");
    expect(useTerminalStore.getState().terminals.map((item) => item.id)).toEqual(["1"]);
    expect(useTerminalStore.getState().activeId).toBe("1");
  });

  it("reports create failures", async () => {
    api.create.mockRejectedValue(new Error("PTY unavailable"));
    await useTerminalStore.getState().create("/workspace");
    expect(useTerminalStore.getState().error).toContain("PTY unavailable");
    expect(useTerminalStore.getState().terminals).toHaveLength(0);
  });
});

