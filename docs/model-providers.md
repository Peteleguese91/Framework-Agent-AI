# Model providers

Phase 6 ships two adapters behind the `ModelProvider` interface:

- `lm-studio`, the default local provider at `http://localhost:1234/v1`;
- `openai-compatible`, for servers implementing the OpenAI models and chat-completions routes.

Both use the Rust transport in `src-tauri/src/model`. Discovery reads `GET /models`. Chat uses `POST /chat/completions`, including SSE streaming, cancellation, usage metadata, tool definitions, and `tool_choice`. A temporary connection failure or retryable HTTP response may be retried once when enabled. Timeouts, unavailable models, context overflow, malformed responses, malformed tool calls, and interrupted streams have stable error codes.

Capabilities are explicit settings because compatible servers do not expose a reliable common capability endpoint. Users can override tools, streaming, vision, reasoning, context window, and maximum output values. Disabling streaming makes the direct chat use a normal completion request.

The initial coding profile uses Qwen3-Coder-friendly defaults: temperature `0.7`, top-p `0.8`, top-k `20`, repetition penalty `1.05`, and a `32768` token context setting. The model must be loaded in LM Studio with its non-thinking chat template. LM Studio controls the actual loaded-model context; GravityForge's context value records the selected profile and informs future context management.

API keys are sent to the Rust backend and retained in memory for the current app process. They are never written into local storage or logs. Remote endpoints show a warning because prompts and tool schemas leave the device. Logs include provider, model, action, outcome, error type, duration, and token usage; they exclude prompts, responses, and credentials.

See [models.md](models.md) for setup and validation steps.
