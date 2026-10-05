import { BRAND } from "../../app/branding";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore } from "../../project/workspaceStore";
import { useEditorStore } from "../../editor/editorStore";

export function TopBar() {
  const toggleBottomPanel = useUiStore((state) => state.toggleBottomPanel);
  const toggleTheme = useUiStore((state) => state.toggleTheme);
  const choose = useWorkspaceStore((state) => state.choose);
  const hasDirty = useEditorStore((state) => state.tabs.some((tab) => tab.dirty));

  return (
    <header className="top-bar">
      <div className="brand-mark" aria-label={BRAND.name}>{BRAND.shortName}</div>
      <button className="top-button" onClick={() => { if (hasDirty) window.alert("Save or close modified files before changing workspace."); else void choose(); }}>Open Project</button>
      <div className="top-divider" />
      <label className="model-select">
        <span>Model</span>
        <select disabled aria-label="Model provider" title="Available after provider setup">
          <option>Not connected</option>
        </select>
      </label>
      <div className="agent-mode" aria-label="Agent mode">
        <button className="mode-active">Ask</button>
        <button disabled>Agent</button>
        <button disabled>Autonomous</button>
      </div>
      <div className="top-spacer" />
      <button className="icon-button" onClick={toggleBottomPanel} title="Toggle bottom panel">▱</button>
      <button className="icon-button" onClick={toggleTheme} title="Toggle theme">◐</button>
    </header>
  );
}
