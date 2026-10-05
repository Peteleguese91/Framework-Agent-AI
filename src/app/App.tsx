import { useEffect } from "react";
import { AgentPanel } from "../agent/AgentPanel";
import { EditorSurface } from "../editor/EditorSurface";
import { ProjectExplorer } from "../project/ProjectExplorer";
import { BottomPanel } from "../terminal/BottomPanel";
import { TopBar } from "../ui/layout/TopBar";
import { useUiStore } from "../ui/state/uiStore";
import { useWorkspaceStore } from "../project/workspaceStore";

export function App() {
  const bottomPanelOpen = useUiStore((state) => state.bottomPanelOpen);
  const initialize = useWorkspaceStore((state) => state.initialize);
  useEffect(() => { void initialize(); }, [initialize]);

  return (
    <main className="app-shell">
      <TopBar />
      <section className="workspace" aria-label="Development workspace">
        <ProjectExplorer />
        <section className="workbench">
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
    </main>
  );
}
