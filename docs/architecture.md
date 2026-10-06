# Architecture

## Principles

GravityForge owns the agent runtime. Models are replaceable reasoning engines behind provider adapters. Core tools are native and typed; MCP is reserved for external extensions. Every privileged action crosses a policy boundary, every mutation is auditable, and unfinished features remain visibly disabled.

## Runtime boundaries

```text
React UI
  -> typed application services
    -> Tauri IPC commands/events
      -> Rust capability modules
        -> workspace / process / git / browser sidecar / OS

Agent orchestrator
  -> context manager -> model router -> provider adapter
  -> tool registry -> approval policy -> native tool executor
  -> event journal -> SQLite memory
```

The React process renders state and collects intent. It never receives unrestricted shell or filesystem access. Rust owns workspace confinement, processes, filesystem mutation, Git, native screenshots, and operating-system integration. Playwright runs as a managed sidecar because its maintained API and browser lifecycle are strongest in Node.

## Modules

Frontend modules live under `src/`: `app`, `ui`, `editor`, `terminal`, `project`, `chat`, `agent`, `tools`, `models`, `memory`, `browser`, `vision`, `computer`, `approvals`, `git`, `diff`, `tasks`, `settings`, and `telemetry`. A module exposes a narrow public API; UI components do not invoke arbitrary IPC strings.

Backend modules live under `src-tauri/src/`. Planned capability domains are `filesystem`, `shell`, `git`, `process`, `security`, `sandbox`, `system`, `screenshots`, and `computer_use`. Commands accept serializable request types and return explicit result/error types.

## Agent runtime

The orchestrator is implemented in TypeScript as a state machine because provider SDKs, OpenAI-compatible streaming, JSON schema tooling, Playwright, and plugin ecosystems are first-class there. Security-sensitive execution remains in Rust. The loop persists transitions and emits sanitized activity events; private model reasoning is never stored or shown.

## Persistence

SQLite is the source of truth for sessions, tasks, plans, tool invocations, approvals, project knowledge, summaries, provider profiles, and crash recovery. Files remain the source of truth for source code. Large outputs and screenshots are stored as workspace-scoped artifacts with database metadata.

## Concurrency

Each task has an append-only event stream. Sub-agents have isolated context and tool budgets. A path lease manager prevents concurrent writes to overlapping files. Long processes have stable IDs and streamed output independent from model turns.

## Security model

All paths are normalized and checked against a canonical workspace root. Symlink resolution will be performed immediately before filesystem operations. Tools declare risk, timeouts, and required capabilities. Policy combines tool risk, current mode, workspace scope, and user grants. Dangerous actions always require explicit approval.

Phase 2 implements this boundary in `WorkspaceManager`: project selection canonicalizes the root, while every directory listing and file read resolves the existing target again and rejects traversal or symlink escape. See `workspace.md`.

Phase 3 extends the same boundary to atomic writes, rename, delete, stat, and bounded file search. Monaco and its language workers are bundled locally. Editor models use relative workspace paths and never perform direct filesystem access.

## Provider boundary

Phase 6 implements `ModelProvider` in TypeScript with a registry containing LM Studio and a generic OpenAI-compatible adapter. The shared Rust transport owns endpoint validation, optional bearer authentication, `/models` discovery, health checks, chat completion, SSE streaming, cancellation, bounded retry, structured errors, and sanitized `MODEL` logs.

Provider messages and tool definitions cross Tauri IPC as typed values. Returned tool calls are checked against the offered tool name and its basic JSON Schema before reaching the UI. The direct chat displays calls for inspection and never executes them. Provider settings and capability overrides are persisted in browser storage; API keys remain only in Rust process memory for the current session. See [models.md](models.md) and [model-providers.md](model-providers.md).

## Terminal execution

Phase 4 introduces a Rust TerminalManager with independent PTY sessions and a separate structured, timeout-bound exec path. See [terminal.md](terminal.md). User terminal sessions are normal shells; agent access remains gated pending an approval policy.


## Platform boundaries

Nobara Linux is the primary platform. Windows 11 is secondary. Terminal operating-system behavior lives in dedicated Linux and Windows modules behind shared IPC and result types. Bundle targets are selected with Tauri platform-specific configuration files. See [platforms.md](platforms.md).

## Filesystem tools

Phase 5 adds a Rust filesystem tool boundary with a typed frontend caller. Operations remain confined to the active workspace, with explicit limits and user-only access until agent approvals exist. See [tools.md](tools.md).
