// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { BottomPanel } from "./BottomPanel";
import { useTerminalStore } from "./terminalStore";
import { useWorkspaceStore } from "../project/workspaceStore";
import { useUiStore } from "../ui/state/uiStore";
import { TopBar } from "../ui/layout/TopBar";
import type { TerminalInfo } from "./types";

vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    loadAddon() {}
    open() {}
    write() {}
    onData() { return { dispose() {} }; }
    focus() {}
    clear() {}
    dispose() {}
  },
}));
vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class { fit() {} },
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("./terminalApi", () => ({
  terminalApi: { resize: vi.fn().mockResolvedValue(undefined) },
}));

const terminal: TerminalInfo = {
  id: "1", shell: "sh", cwd: "/workspace", pid: 123,
  createdAt: 1, status: "running",
};

describe("terminal controls", () => {
  afterEach(cleanup);
  beforeEach(() => {
    vi.stubGlobal("ResizeObserver", class {
      observe() {}
      disconnect() {}
    });
    useWorkspaceStore.setState({
      workspace: null, recent: [], selectedPath: null, file: null,
      loading: false, error: null,
    });
    useTerminalStore.setState({
      terminals: [], activeId: null, error: null,
      create: vi.fn().mockResolvedValue(undefined),
      kill: vi.fn().mockResolvedValue(undefined),
      close: vi.fn().mockResolvedValue(undefined),
      interrupt: vi.fn().mockResolvedValue(undefined),
    });
  });

  it("enables New only for an open workspace", () => {
    const { rerender } = render(<BottomPanel />);
    expect(screen.getByRole("button", { name: "New" }).hasAttribute("disabled")).toBe(true);
    useWorkspaceStore.setState({ workspace: {
      name: "Demo", root: "/workspace",
      projectInfo: { name: "Demo", projectTypes: [], languages: [], detectedFrameworks: [], hasGit: false, mainConfigFiles: [] },
      projectMap: { mainDirectories: [], configFiles: [], filesByExtension: {}, entryPoints: [], hasTests: false, totalFiles: 0, truncated: false },
    } });
    rerender(<BottomPanel />);
    fireEvent.click(screen.getByRole("button", { name: "New" }));
    expect(useTerminalStore.getState().create).toHaveBeenCalledWith("/workspace");
  });

  it("keeps toolbar controls tied to the active session", () => {
    useTerminalStore.setState({ terminals: [terminal], activeId: "1" });
    render(<BottomPanel />);
    fireEvent.click(screen.getByRole("button", { name: "Kill" }));
    expect(useTerminalStore.getState().kill).toHaveBeenCalledWith("1");
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(useTerminalStore.getState().close).toHaveBeenCalledWith("1");
    act(() => useTerminalStore.setState({ terminals: [{ ...terminal, status: "exited" }] }));
    expect(screen.getByRole("button", { name: "Kill" }).hasAttribute("disabled")).toBe(true);
  });

  it("toggles the bottom panel from the top bar", () => {
    useUiStore.setState({ bottomPanelOpen: true });
    render(<TopBar />);
    fireEvent.click(screen.getByTitle("Toggle bottom panel"));
    expect(useUiStore.getState().bottomPanelOpen).toBe(false);
  });
});


