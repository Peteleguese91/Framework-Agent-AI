export function AgentPanel() {
  return (
    <aside className="side-panel agent-panel">
      <div className="panel-heading"><span>AGENT</span><button disabled title="No active task">•••</button></div>
      <div className="agent-empty">
        <div className="pulse-orb" />
        <h2>Ready when connected</h2>
        <p>Provider configuration and the autonomous loop are intentionally disabled until their implementation phases.</p>
      </div>
      <div className="composer">
        <textarea disabled placeholder="Connect a model provider to start…" aria-label="Agent prompt" />
        <button disabled>Send</button>
      </div>
    </aside>
  );
}
