import { useState } from "react";
import { useModelStore } from "../models/modelStore";
import { useAgentStore } from "./agentStore";

export function AgentPanel() {
  const agent = useAgentStore();
  if (agent.mode !== "ASK") return <AgentTaskPanel />;
  const { messages, settings, connection, generating, usage, error, send, cancel, openSettings, clearError } = useModelStore();
  const [draft, setDraft] = useState("");
  const submit = () => { const value = draft; setDraft(""); void send(value); };
  return (
    <aside className="side-panel agent-panel">
      <div className="panel-heading"><span>CHAT</span><button onClick={openSettings} title="Model settings">•••</button></div>
      {messages.length === 0 ? <div className="agent-empty">
        <div className="pulse-orb" />
        <h2>{settings.model ? settings.model : "Ready when connected"}</h2>
        <p>This panel is a direct model chat. Tool calls are displayed for inspection and are never executed.</p>
      </div> : <div className="chat-messages">{messages.map((message) => <article key={message.id} className={`chat-message ${message.role}`}><strong>{message.role}</strong><div>{message.content || (generating && message.role === "assistant" ? "…" : "")}</div>{message.toolCalls.map((call) => <details key={call.id} className="tool-call"><summary>Tool call: {call.function.name}</summary><pre>{JSON.stringify(call.function.arguments, null, 2)}</pre><small>Not executed</small></details>)}</article>)}</div>}
      {usage ? <div className="token-usage">Input {usage.prompt_tokens} · Output {usage.completion_tokens} · Total {usage.total_tokens}</div> : null}
      {error ? <div className="model-error" role="alert">{error}<button onClick={clearError}>×</button></div> : null}
      <div className="composer"><textarea value={draft} onChange={(event) => setDraft(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); submit(); } }} disabled={!settings.model || generating} placeholder={settings.model ? "Message model…" : "Configure and select a model…"} aria-label="Model prompt" />
        {generating ? <button onClick={() => void cancel()}>Cancel</button> : <button onClick={submit} disabled={!draft.trim() || connection === "connecting"}>Send</button>}
      </div>
    </aside>
  );
}

function AgentTaskPanel() {
  const { mode, task, events, approval, error, start, submitUserInput, resolveApproval, pause, resume, stop, clear } = useAgentStore();
  const [goal, setGoal] = useState("");
  const active = !!task && !["COMPLETED", "FAILED", "CANCELLED"].includes(task.status);
  const submit = () => {
    const value = goal.trim(); if (!value) return; setGoal("");
    if (task?.status === "WAITING_USER") submitUserInput(value); else void start(value);
  };
  return <aside className="side-panel agent-panel">
    <div className="panel-heading"><span>{mode} ENGINE</span>{task && !active ? <button onClick={clear}>New</button> : null}</div>
    {!task ? <div className="agent-empty"><div className="pulse-orb"/><h2>Controlled autonomous loop</h2><p>Set a goal. GravityForge will inspect, act, observe, and verify through the registered tools.</p></div> : <>
      <section className="agent-task-summary"><small>GOAL</small><strong>{task.goal}</strong><span className={`agent-status ${task.status.toLowerCase()}`}>{task.status.replaceAll("_", " ")}</span></section>
      <section className="agent-plan"><small>PLAN</small>{task.steps.map((step) => <div key={step.id} className={step.status.toLowerCase()}><span>{step.status === "DONE" ? "✓" : step.status === "RUNNING" ? "●" : "○"}</span>{step.title}</div>)}</section>
      <section className="agent-events"><small>ACTIONS</small>{events.map((event, index) => <div key={`${event.at}-${index}`}><time>{new Date(event.at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}</time><span>{event.summary}</span></div>)}</section>
      {task.modifiedFiles.length ? <section className="agent-files"><small>FILES CHANGED</small>{task.modifiedFiles.map((file) => <div key={file}>{file}</div>)}</section> : null}
      {task.finalResult ? <article className="agent-result">{task.finalResult}</article> : null}
      <div className="agent-controls">{task.status === "PAUSED" ? <button onClick={resume}>Resume</button> : active ? <button onClick={pause}>Pause</button> : null}{active ? <button className="danger-button" onClick={() => void stop()}>Stop</button> : null}</div>
    </>}
    {approval ? <section className="approval-card" role="alertdialog" aria-label="Agent approval"><strong>{approval.risk}: {approval.tool}</strong><p>{approval.reason}</p><pre>{JSON.stringify(approval.arguments, null, 2)}</pre><div><button onClick={() => resolveApproval("REJECT")}>Reject</button><button className="primary-button" onClick={() => resolveApproval("APPROVE_ONCE")}>Approve once</button></div></section> : null}
    {error ? <div className="model-error">{error}</div> : null}
    <div className="composer"><textarea value={goal} onChange={(event) => setGoal(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); submit(); } }} disabled={active && task?.status !== "WAITING_USER"} placeholder={task?.status === "WAITING_USER" ? "Provide the requested information…" : active ? "Agent is working…" : "Describe a goal…"}/><button onClick={submit} disabled={!goal.trim() || (active && task?.status !== "WAITING_USER")}>{task?.status === "WAITING_USER" ? "Reply" : "Start"}</button></div>
  </aside>;
}
