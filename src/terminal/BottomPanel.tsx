import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { useWorkspaceStore } from "../project/workspaceStore";
import { useTerminalStore, subscribeTerminalOutput, terminalOutput, clearTerminalOutput } from "./terminalStore";
import type { TerminalInfo } from "./types";

function readNumber(key: string, fallback: number, min: number, max: number) {
  const value = Number(localStorage.getItem(key));
  return Number.isFinite(value) && value >= min && value <= max ? value : fallback;
}

function TerminalView({
  terminal,
  fontSize,
  scrollback,
  cursorStyle,
  clearSignal,
}: {
  terminal: TerminalInfo;
  fontSize: number;
  scrollback: number;
  cursorStyle: "block" | "underline" | "bar";
  clearSignal: number;
}) {
  const host = useRef<HTMLDivElement>(null);
  const instance = useRef<Terminal | null>(null);
  const lastClear = useRef(clearSignal);
  const store = useTerminalStore;
  useEffect(() => {
    if (!host.current) return;
    const xterm = new Terminal({
      cursorBlink: true,
      cursorStyle,
      fontSize,
      scrollback,
      convertEol: false,
      theme: { background: "#0a0c0f", foreground: "#d7dbe4" },
    });
    const fit = new FitAddon();
    xterm.loadAddon(fit);
    xterm.open(host.current);
    instance.current = xterm;
    for (const chunk of terminalOutput(terminal.id)) xterm.write(chunk);
    const unsubscribe = subscribeTerminalOutput(terminal.id, (data) => xterm.write(data));
    const dataSubscription = xterm.onData((data) => {
      void store.getState().write(terminal.id, new TextEncoder().encode(data));
    });
    const observer = new ResizeObserver(() => {
      fit.fit();
      if (xterm.cols > 0 && xterm.rows > 0) {
        void store.getState().resize(terminal.id, xterm.cols, xterm.rows);
      }
    });
    observer.observe(host.current);
    fit.fit();
    void store.getState().resize(terminal.id, xterm.cols, xterm.rows);
    xterm.focus();
    return () => {
      observer.disconnect();
      dataSubscription.dispose();
      unsubscribe();
      instance.current = null;
      xterm.dispose();
    };
  }, [terminal.id, fontSize, scrollback, cursorStyle, store]);
  useEffect(() => {
    if (clearSignal !== lastClear.current) instance.current?.clear();
    lastClear.current = clearSignal;
  }, [clearSignal]);
  return <div ref={host} className="terminal-host" aria-label={"Terminal " + terminal.id} />;
}

export function BottomPanel() {
  const workspace = useWorkspaceStore((state) => state.workspace);
  const { terminals, activeId, error, create, activate, interrupt, kill, close, clearError } = useTerminalStore();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [shell, setShell] = useState(() => localStorage.getItem("gravityforge.terminal.shell") ?? "");
  const [fontSize, setFontSize] = useState(() => readNumber("gravityforge.terminal.fontSize", 13, 9, 32));
  const [scrollback, setScrollback] = useState(() => readNumber("gravityforge.terminal.scrollback", 5000, 100, 50000));
  const [cursorStyle, setCursorStyle] = useState<"block" | "underline" | "bar">(
    () => (localStorage.getItem("gravityforge.terminal.cursorStyle") as "block" | "underline" | "bar") || "block",
  );
  const [clearSignal, setClearSignal] = useState(0);
  const active = terminals.find((terminal) => terminal.id === activeId);
  return (
    <section className="bottom-panel" aria-label="Terminal panel">
      <nav className="bottom-tabs" aria-label="Bottom panel">
        <button className="active">TERMINAL</button>
        <button disabled title="Planned diagnostics">PROBLEMS <span className="badge">0</span></button>
        <button disabled title="Planned output panel">OUTPUT</button>
      </nav>
      <div className="terminal-toolbar">
        <div className="terminal-session-tabs">
          {terminals.map((terminal, index) => (
            <button
              key={terminal.id}
              className={terminal.id === activeId ? "selected" : ""}
              onClick={() => activate(terminal.id)}
              title={terminal.cwd}
            >
              Terminal {index + 1}{terminal.status === "exited" ? " • exited" : ""}
            </button>
          ))}
        </div>
        <button onClick={() => workspace && void create(workspace.root)} disabled={!workspace} title="New Terminal">New</button>
        <button onClick={() => active && void interrupt(active.id)} disabled={!active || active.status !== "running"} title="Interrupt (Ctrl+C)">Interrupt</button>
        <button onClick={() => active && void kill(active.id)} disabled={!active || active.status !== "running"} title="Kill process tree">Kill</button>
        <button onClick={() => active && void close(active.id)} disabled={!active} title="Close Terminal">Close</button>
        <button onClick={() => { if (active) clearTerminalOutput(active.id); setClearSignal((value) => value + 1); }} disabled={!active} title="Clear display">Clear</button>
        <button onClick={() => setSettingsOpen((value) => !value)} title="Terminal settings">Settings</button>
      </div>
      {settingsOpen ? (
        <div className="terminal-settings">
          <label>Shell <input value={shell} onChange={(event) => {
            setShell(event.target.value);
            localStorage.setItem("gravityforge.terminal.shell", event.target.value);
          }} placeholder="System default" /></label>
          <label>Font size <input type="number" min={9} max={32} value={fontSize} onChange={(event) => {
            const value = Number(event.target.value);
            if (value >= 9 && value <= 32) {
              setFontSize(value);
              localStorage.setItem("gravityforge.terminal.fontSize", String(value));
            }
          }} /></label>
          <label>Scrollback <input type="number" min={100} max={50000} value={scrollback} onChange={(event) => {
            const value = Number(event.target.value);
            if (value >= 100 && value <= 50000) {
              setScrollback(value);
              localStorage.setItem("gravityforge.terminal.scrollback", String(value));
            }
          }} /></label>
          <label>Cursor <select value={cursorStyle} onChange={(event) => {
            const value = event.target.value as "block" | "underline" | "bar";
            setCursorStyle(value);
            localStorage.setItem("gravityforge.terminal.cursorStyle", value);
          }}><option value="block">Block</option><option value="underline">Underline</option><option value="bar">Bar</option></select></label>
        </div>
      ) : null}
      {active ? (
        <>
          <div className="terminal-meta">{active.shell} • {active.cwd} • {active.status}{active.exitCode != null ? " (" + active.exitCode + ")" : ""}</div>
          <TerminalView terminal={active} fontSize={fontSize} scrollback={scrollback} cursorStyle={cursorStyle} clearSignal={clearSignal} />
        </>
      ) : (
        <div className="terminal-empty">{workspace ? "Select New to open a terminal in this workspace." : "Open a project to use the terminal."}</div>
      )}
      {error ? <div role="alert" className="terminal-error">{error}<button onClick={clearError}>Dismiss</button></div> : null}
    </section>
  );
}


