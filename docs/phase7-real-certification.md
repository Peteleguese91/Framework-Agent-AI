# Phase 7.5 — Real Agent Certification

Certification sessions: 2026-10-06 and 2026-10-07
Host: Nobara Linux 44 KDE, kernel `7.1.4-200.nobara.fc44.x86_64`
LM Studio: Flatpak `0.4.23-1`; runtime reported `0.4.25-b1`
Status: **PARTIAL FAILURE — NOT MODEL CERTIFIED**

## Executive result

The full Linux build gate passes, including Debian, AppImage, and RPM bundles. Real chat and SSE streaming pass. The requested `Qwen3-Coder-30B-A3B-Instruct` is not installed. The available fallback `Qwen2.5-Coder-14B-Instruct` was loaded with 32768 context and tested against the real LM Studio endpoint.

The fallback does not emit OpenAI `tool_calls`: it writes pseudo calls inside assistant text even with `tool_choice: required`. GravityForge correctly treats `tool_calls: []` as no call. The three mandatory agent scenarios therefore cannot execute and Phase 7 remains **IMPLEMENTED / PENDING MODEL INTEGRATION CERTIFICATION**. No unsafe parser for model-specific executable prose was added.

## Pre-check

| Check | Result | Evidence |
| --- | --- | --- |
| Nobara host | PASS | Nobara Linux 44 KDE |
| LM Studio API | PASS | `127.0.0.1:1234` |
| `/v1/models` | PASS | Returned three model identifiers |
| Qwen3-Coder-30B-A3B-Instruct | FAIL | Not installed or loadable |
| Fallback Qwen | PASS | `qwen2.5-coder-14b-instruct`, 8.37 GiB |
| Context | PASS | Loaded with `--context-length 32768` |
| Chat | PASS | Exact `CONNECTION_OK`; usage 40/4/44 tokens |
| SSE streaming | PASS | Ordered deltas, `[DONE]`, usage frame |
| Structured tool calling | FAIL | LM Studio returned `tool_calls: []` |

## Build gate

| Gate | Result |
| --- | --- |
| `npm test` | PASS — 40/40 |
| `npm run build` | PASS |
| `npm run lint` | PASS |
| `cargo test` | PASS — 28/28 |
| `cargo fmt --check` | PASS |
| `cargo clippy -- -D warnings` | PASS |
| `npm run tauri build` | PASS |

Bundles: Debian 5.83 MiB, AppImage 111.52 MiB, RPM 5.84 MiB. The first AppImage attempt found a real packaging bug: no square icon was declared. Declaring the existing icons in `tauri.conf.json` fixed all bundle formats.

## Tool-call format audit

### Definitive fallback audit — 2026-10-07

The loaded file is `Qwen/Qwen2.5-Coder-14B-Instruct-GGUF/qwen2.5-coder-14b-instruct-q4_k_m.gguf`. LM Studio identifies it as Qwen2, Q4_K_M, 14B, 8.99 GB, loaded at 32768 context. The native `/api/v1/models` metadata is decisive: `capabilities.trained_for_tool_use` is `false`.

The GGUF does contain a tool-aware Jinja chat template. LM Studio applied that embedded template, not a plain chat template. The rendered model input included:

```text
# Tools
You are provided with function signatures within <tools></tools> XML tags:
<tools>
{"type":"function","function":{"name":"square_number",...}}
</tools>
For each function call, return ... within <tool_call></tool_call> XML tags
```

That is the Hermes-style format documented by Qwen. The installed template is therefore present, receives the tool definition, and asks for the correct `<tool_call>` delimiter. Qwen recommends this format for function calling; LM Studio additionally requires the model output to match its supported format before it can populate the OpenAI `message.tool_calls` field.

The exact diagnostic request body, apart from JSON whitespace, was:

```json
{
  "model": "qwen2.5-coder-14b-instruct",
  "messages": [
    {"role": "system", "content": "You must call the provided function. Do not calculate or answer in text."},
    {"role": "user", "content": "Call square_number with value 12. Return only the function call."}
  ],
  "tools": [{
    "type": "function",
    "function": {
      "name": "square_number",
      "description": "Return the square of a number.",
      "parameters": {
        "type": "object",
        "properties": {"value": {"type": "number", "description": "The number to square."}},
        "required": ["value"],
        "additionalProperties": false
      }
    }
  }],
  "tool_choice": "auto | required",
  "temperature": 0,
  "max_tokens": 128,
  "stream": false
}
```

`tool_choice: auto` returned this complete assistant message:

```json
{
  "role": "assistant",
  "content": "<call>\n{\n  \"name\": \"square_number\",\n  \"arguments\": {\n    \"value\": 12\n  }\n}\n</call>",
  "reasoning_content": "",
  "tool_calls": []
}
```

Its `finish_reason` was `stop`, with usage 191 prompt / 32 completion / 223 total tokens. `tool_choice: required` still returned `tool_calls: []`; its content began with the same invalid `<call>` block and continued with invented examples until `finish_reason: length` at 128 completion tokens (319 total).

LM Studio's model-input log was identical for `auto` and `required`: the endpoint accepted the field but did not add a distinct forced-call instruction to the rendered prompt. LM Studio documents `required` support for llama.cpp, but the model must still generate the parser-supported native delimiter. Here it generated `<call>` instead of `<tool_call>` in both modes, so the server correctly left `message.tool_calls` empty.

GravityForge's Rust transport constructs the standard OpenAI-compatible `tools` array, forwards `tool_choice`, and parses only `message.tool_calls`. The manually sent payload above bypassed GravityForge and reproduced the same failure. This excludes the AgentEngine and GravityForge request format as causes.

Conclusion by hypothesis:

1. **Model/quantized artifact limitation: confirmed.** The artifact is explicitly reported as not trained for tool use and fails the required delimiter even at temperature 0.
2. **Missing or plain chat template: excluded.** The effective embedded template is tool-aware and matches the expected Hermes/Qwen structure.
3. **Broken OpenAI-compatible request transport: excluded for this case.** The endpoint receives and renders the full tool definition; a direct request reproduces the failure.
4. **Malformed `tools` / `tool_choice`: excluded.** The schema follows the documented OpenAI shape and is present verbatim in the rendered prompt. `required` is accepted but cannot repair non-conforming model output.

The fallback is therefore **definitively incompatible with agent certification**. Assistant prose remains inert: no parser was added to convert `<call>`, `<tools>`, JSON-like text, or any other `assistant.content` into executable calls.

References: [LM Studio Tool Use](https://lmstudio.ai/docs/developer/openai-compat/tools), [LM Studio API changelog](https://lmstudio.ai/docs/developer/api-changelog), [Qwen3 function calling](https://github.com/QwenLM/Qwen3/blob/main/docs/source/framework/function_call.md).

### Qwen3-Coder environment preparation

Host resources:

- AMD Ryzen 5 5600X, 6 cores / 12 threads;
- AMD Radeon RX 6900 XT (`gfx1030`), 16 GiB VRAM;
- 32 GiB system RAM plus 8 GiB swap;
- 351 GiB free model storage.

The target has 30.5B total / 3.3B active parameters, 48 layers, 4 KV heads, native 262144 context, and non-thinking operation. Qwen explicitly recommends reducing context to 32768 on memory pressure and documents OpenAI-compatible tool calling for this model. Its specialized function-call format requires a compatible server-side parser; model availability alone is not sufficient.

Quantization assessment:

| Quantization | File size | Assessment on this host |
| --- | ---: | --- |
| Q2_K | 11.26 GB | Full GPU but significant quality loss; rejected for certification |
| Q3_K_M | 14.71 GB | LM Studio-selected partial-GPU option; usable candidate, quality compromise |
| Q4_K_M | 18.56 GB | Better-quality candidate but cannot fit wholly in 16 GiB VRAM; requires hybrid RAM/offload |

LM Studio's resolver offered Q2_K as full-GPU and Q3_K_M as partial-GPU; Q4_K_M was not offered for this hardware profile. Q3_K_M was selected to prepare a realistic runnable environment. Download began but timed out after only a few megabytes due the available connection, so the target is not yet installed. The interrupted partial download is not treated as a model or smoke-test result.

When the download can be resumed, load settings must start conservatively at context 32768, parallel 1, Vulkan/partial GPU offload, flash attention enabled, and no speculative decoding. Before any E2E rerun, inspect `/api/v1/models` for `trained_for_tool_use`, capture the effective template, then run exactly one minimal `square_number` request with `tool_choice: required`. PASS requires a non-empty structured `message.tool_calls`; textual pseudo-calls are a failure.

References: [official Qwen3-Coder-30B-A3B-Instruct card](https://huggingface.co/Qwen/Qwen3-Coder-30B-A3B-Instruct), [GGUF quantization sizes](https://huggingface.co/tensorblock/Qwen_Qwen3-Coder-30B-A3B-Instruct-GGUF), [Qwen3-Coder repository](https://github.com/QwenLM/Qwen3-Coder).

## Mandatory E2E scenarios

| Scenario | Result | Reason |
| --- | --- | --- |
| Analyze project | BLOCKED | No structured call reaches AgentEngine |
| Create `hello.txt` | BLOCKED | No filesystem call; no file created |
| Find and fix build error | BLOCKED | No filesystem or terminal call |

No iterations, approvals, modifications, or completion claims were fabricated.

## Streaming, context, and performance

Ten sequential real SSE runs returned exactly `RUN_0` through `RUN_9`, without mixed IDs, duplicates, missing chunks, or ordering errors.

- warm first-token latency: 69–91 ms;
- warm completion duration: 169–191 ms;
- warm throughput: 20.89–23.70 tokens/s;
- cold/queued first run: 13.15 s to first token, 13.25 s total;
- each short run: 34 prompt, 4 completion, 38 total tokens.

Context pressure used 8040 prompt and 16 completion tokens. Transport stayed stable (385 ms first token, 922 ms total, 17.35 tokens/s), but the model copied observation text instead of returning `CONTEXT_OK`. Transport/context handling passed; instruction retention under noisy context failed for this model. Advanced compaction was not added.

## Approval, lifecycle, limits, and completion

Live approval and Pause/Resume/Stop could not enter the loop because the model produced no tool calls. Deterministic engine/backend evidence remains green:

- SAFE/MODERATE paths and mandatory DANGEROUS approval;
- traversal, outside-workspace, and destructive command classification;
- pause/resume/stop state behavior;
- recursive delete requires explicit Rust-side approval;
- CompletionGuard blocks unverified mutations and unresolved failures;
- oversized observations truncate below the 64 KiB message budget with `truncated: true` (new regression test);
- structured non-zero exit, timeout, missing-file, and permission errors;
- malformed and unknown calls do not crash the engine.

These checks support safety but do not replace real-model E2E certification.

## Failure injection

| Injection | Result |
| --- | --- |
| LM Studio stopped | PASS — endpoint failed immediately; server restored |
| Command non-zero | PASS — native test |
| Command timeout | PASS — native process-tree test |
| Malformed tool call | PASS deterministically; BLOCKED live |
| Missing file | PASS — structured filesystem test |
| Unverified completion | PASS — CompletionGuard test |

## Fixes applied

1. Declared square icons so AppImage packaging succeeds.
2. Added oversized-observation regression coverage; runtime truncation already behaved correctly.

No provider-specific parser or scenario-specific workaround was introduced.

## Remaining certification requirements

1. Resume and complete the selected Qwen3-Coder download; no fallback substitution is permitted.
2. Load at 32768 context / parallel 1 and verify the model capability metadata and effective tool template.
3. Run only the minimal `square_number` / `tool_choice: required` smoke test.
4. Continue only if LM Studio returns a non-empty structured `message.tool_calls` array.
5. Then repeat analysis, file creation, and build-fix in disposable workspaces.
6. Repeat live approvals, lifecycle controls, negative completion, limits, and failure injection through the actual loop.
7. Record live iterations, calls, modifications, approvals, and verification.

Only then may status become **IMPLEMENTED + LINUX + MODEL CERTIFIED**. Phase 8 has not started.
