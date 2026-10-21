# Model providers

The primary adapters behind `ModelProvider` are now:

- `gravityforge-local`, the default provider backed by an owned `llama-server` sidecar and internal dynamic endpoint;
- `ollama`, the optional external runtime through its OpenAI-compatible API.

Legacy `lm-studio` and generic `openai-compatible` identifiers remain readable during migration, but LM Studio is no longer the default or an architectural dependency.

Both use the Rust transport in `src-tauri/src/model`. Discovery reads `GET /models`. Chat uses `POST /chat/completions`, including SSE streaming, cancellation, usage metadata, tool definitions, and `tool_choice`. A temporary connection failure or retryable HTTP response may be retried once when enabled. Timeouts, unavailable models, context overflow, malformed responses, malformed tool calls, and interrupted streams have stable error codes.

GravityForge Local capabilities are measured by a real chat, streaming, and structured-tool test. Agent Mode remains unavailable until the structured test passes. External compatible servers retain explicit capability settings because they do not expose a reliable common capability endpoint.

The initial coding profile uses Qwen3-Coder-friendly defaults: temperature `0.7`, top-p `0.8`, top-k `20`, repetition penalty `1.05`, and a `32768` token context setting. The model must be loaded in LM Studio with its non-thinking chat template. LM Studio controls the actual loaded-model context; GravityForge's context value records the selected profile and informs future context management.

API keys are sent to the Rust backend and retained in memory for the current app process. They are never written into local storage or logs. Remote endpoints show a warning because prompts and tool schemas leave the device. Logs include provider, model, action, outcome, error type, duration, and token usage; they exclude prompts, responses, and credentials.

See [models.md](models.md) for setup and validation steps.
