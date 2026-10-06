import { BRAND } from "../../app/branding";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore } from "../../project/workspaceStore";
import { useEditorStore } from "../../editor/editorStore";
import { useModelStore } from "../../models/modelStore";
import { useAgentStore } from "../../agent/agentStore";

export function TopBar() {
  const toggleBottomPanel = useUiStore((state) => state.toggleBottomPanel);
  const toggleTheme = useUiStore((state) => state.toggleTheme);
  const choose = useWorkspaceStore((state) => state.choose);
  const hasDirty = useEditorStore((state) => state.tabs.some((tab) => tab.dirty));
  const { settings, models, connection, selectModel, openSettings } = useModelStore();
  const { mode, task, setMode } = useAgentStore();
  const taskActive = !!task && !["COMPLETED", "FAILED", "CANCELLED"].includes(task.status);

  return (
    <header className="top-bar">
      <div className="brand-mark" aria-label={BRAND.name}>{BRAND.shortName}</div>
      <button className="top-button" onClick={() => { if (hasDirty) window.alert("Save or close modified files before changing workspace."); else void choose(); }}>Open Project</button>
      <div className="top-divider" />
      <label className="model-select">
        <span>Model</span>
        <select value={settings.model} onChange={(event) => selectModel(event.target.value)} aria-label="Model provider">
          <option value="">{connection === "connected" ? "Select model" : "Not connected"}</option>
          {settings.model && !models.some((model) => model.id === settings.model) ? <option value={settings.model}>{settings.model}</option> : null}
          {models.map((model) => <option key={model.id} value={model.id}>{model.id}</option>)}
        </select>
      </label>
      <span className={`model-status ${connection}`} title={`Model: ${connection}`} />
      <div className="agent-mode" aria-label="Agent mode">
        {(["ASK", "AGENT", "AUTONOMOUS"] as const).map((value) => <button key={value} className={mode === value ? "mode-active" : ""} disabled={taskActive} onClick={() => setMode(value)}>{value === "AUTONOMOUS" ? "Autonomous" : value[0] + value.slice(1).toLowerCase()}</button>)}
      </div>
      <div className="top-spacer" />
      <button className="icon-button" onClick={toggleBottomPanel} title="Toggle bottom panel">▱</button>
      <button className="icon-button" onClick={toggleTheme} title="Toggle theme">◐</button>
      <button className="icon-button" onClick={openSettings} title="Model settings">⚙</button>
    </header>
  );
}
