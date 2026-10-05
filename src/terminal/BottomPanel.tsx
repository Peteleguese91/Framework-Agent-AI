export function BottomPanel() {
  return (
    <section className="bottom-panel">
      <nav className="bottom-tabs" aria-label="Bottom panel">
        <button className="active">TERMINAL</button>
        <button>PROBLEMS <span className="badge">0</span></button>
        <button>OUTPUT</button>
        <button>LOGS</button>
      </nav>
      <div className="terminal-placeholder">
        <span className="prompt">gravityforge</span><span className="muted"> terminal PTY will be enabled in Phase 4</span>
      </div>
    </section>
  );
}
