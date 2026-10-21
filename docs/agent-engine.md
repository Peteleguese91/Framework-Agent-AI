# Agent Engine — Phase 7

Status: **IMPLEMENTED / PENDING LINUX AND MODEL INTEGRATION CERTIFICATION**.

`AgentEngine` is a provider-independent state machine. It receives a model runner, central tool registry, approval manager, workspace context, and configurable limits. React only renders task snapshots and events.

The lifecycle is `IDLE → PLANNING → RUNNING`, with explicit `WAITING_APPROVAL`, `WAITING_USER`, `VERIFYING`, `PAUSED`, and terminal `COMPLETED`, `FAILED`, or `CANCELLED` states. Multiple tool calls are validated and run sequentially, converted to bounded observations, appended as tool messages, and followed by another model turn.

The registry exposes fourteen workspace filesystem operations and structured `terminal.exec`; interactive PTYs remain user-only. Loop protection limits iterations, tool calls, identical retries, output size, and model failures. Context overflow gets one compacted retry. Mutations require a successful recognized verification command before completion.

Task history is currently a bounded browser-storage summary rather than crash-safe full persistence. Parallel tools, semantic memory, Git automation, browser, vision, and multi-agent execution remain outside Phase 7. Final certification requires a tool-capable model and the three specified scenarios on Nobara.
