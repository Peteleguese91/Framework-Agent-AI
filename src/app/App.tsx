import { useEffect } from "react";
import { AgentPanel } from "../agent/AgentPanel";
import { EditorSurface } from "../editor/EditorSurface";
import { ProjectExplorer } from "../project/ProjectExplorer";
import { BottomPanel } from "../terminal/BottomPanel";
import { useTerminalStore } from "../terminal/terminalStore";
import { TopBar } from "../ui/layout/TopBar";
import { useUiStore } from "../ui/state/uiStore";
import { useWorkspaceStore } from "../project/workspaceStore";
import { ModelSettings } from "../models/ModelSettings";
import { useModelStore } from "../models/modelStore";

export function App() {
  const bottomPanelOpen = useUiStore((state) => state.bottomPanelOpen);
  const initialize = useWorkspaceStore((state) => state.initialize);
  useEffect(() => { void initialize(); }, [initialize]);
  useEffect(() => { void useTerminalStore.getState().initialize(); }, []);
  useEffect(() => { void useModelStore.getState().initialize(); }, []);
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.code !== "Backquote") return;
      event.preventDefault();
      if (event.shiftKey) {
        const workspace = useWorkspaceStore.getState().workspace;
        if (workspace) {
          if (!useUiStore.getState().bottomPanelOpen) useUiStore.getState().toggleBottomPanel();
          void useTerminalStore.getState().create(workspace.root);
        }
      } else {
        useUiStore.getState().toggleBottomPanel();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  return (
    <main className="app-shell">
      <TopBar />
      <section className="workspace" aria-label="Development workspace">
        <ProjectExplorer />
        <section className={"workbench" + (bottomPanelOpen ? "" : " bottom-closed")}>
          <EditorSurface />
          {bottomPanelOpen ? <BottomPanel /> : null}
        </section>
        <AgentPanel />
      </section>
      <footer className="status-bar">
        <span>Local workspace</span>
        <span className="status-spacer" />
        <span>UTF-8</span>
        <span>Ln 1, Col 1</span>
      </footer>
      <ModelSettings />
    </main>
  );
}
