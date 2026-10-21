import { useComputerStore } from "./computerStore";

export function ComputerControl() {
  const { status, busy, error, enable, disable, stop } = useComputerStore();
  const enabled = Boolean(status?.enabled && status.authorized);
  return <section className={`computer-control ${enabled ? "enabled" : "disabled"}`} aria-label="Computer Control">
    <div><strong>COMPUTER CONTROL</strong><span>{enabled ? "ON · PORTAL AUTHORIZED" : "OFF"}</span></div>
    <small>{enabled ? `${status?.displayCount} display(s) · ${status?.backend}` : "No desktop input is allowed"}</small>
    <div>{enabled ? <><button className="danger-button" disabled={busy} onClick={() => void stop()}>Stop now</button><button disabled={busy} onClick={() => void disable()}>Disable</button></> : <button disabled={busy} onClick={() => void enable()}>{busy ? "Waiting for portal…" : "Enable for this session"}</button>}</div>
    {error ? <p role="alert">{error}</p> : null}
  </section>;
}
