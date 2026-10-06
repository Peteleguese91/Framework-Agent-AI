# Models

## LM Studio quick start

1. Load a chat-capable model in LM Studio. The Phase 6 target is `Qwen3-Coder-30B-A3B-Instruct`; select its non-thinking chat template and a context of at least 32768 tokens when resources allow.
2. Start LM Studio's local OpenAI-compatible server on port 1234.
3. Open **Settings → Models** in GravityForge.
4. Select **LM Studio**, keep `http://localhost:1234/v1`, and choose **Refresh Models**.
5. Select a discovered model, save, and use the right-side Chat panel.

The model selector in the top bar uses the same saved profile. **Test Connection** calls `/models` without starting a generation. The status indicator reports disconnected, connecting, connected, or error.

## Defaults and controls

The LM Studio profile starts with temperature `0.7`, top-p `0.8`, top-k `20`, repetition penalty `1.05`, context length `32768`, maximum output `4096`, a 120 second timeout, one retry, tools enabled, streaming enabled, vision disabled, and reasoning disabled.

Provider capabilities can be overridden manually. Use this when a compatible server or loaded model does not support tools or streaming. Vision and reasoning flags describe capability only; Phase 6 direct chat sends text messages.

The API key field is optional for a local LM Studio server. A supplied key lives only in backend memory until GravityForge exits and must be entered again after restart. Profile settings contain no key and are stored locally.

## Tool-call validation

Phase 6 offers a harmless `square_number` schema to exercise OpenAI function calling. GravityForge validates each returned function name, JSON arguments, required fields, primitive types, arrays, enums, and additional-property restrictions covered by the schema. The Agent panel displays valid calls and labels them **Not executed**. Phase 6 has no tool execution loop.

## Error handling

Provider failures reach the UI as structured categories: invalid configuration, connection failure, timeout, HTTP failure, unavailable model, context overflow, invalid response, malformed tool call, interrupted stream, cancellation, or internal failure. Retry is limited to one additional attempt and applies only to temporary connection errors, server errors, and rate limiting.

## Verification

Automated frontend checks:

```bash
npm run build
npm run lint
npm run test
```

Rust and desktop checks:

```bash
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
npm run tauri build
```

Manual LM Studio check:

- discover the loaded model;
- test the connection;
- send `Rispondi soltanto con: CONNECTION_OK` and confirm the exact streamed response;
- cancel a long response and confirm generation stops;
- ask the model to call `square_number` and confirm that the call is displayed but not executed;
- stop LM Studio and confirm that the UI reports a typed connection error.

Nobara remains the certification host. Phase 6 is **IMPLEMENTED / PENDING LINUX CERTIFICATION** until the Rust gates, Tauri build, and real LM Studio session pass there.
