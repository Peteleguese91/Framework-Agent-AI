# GravityForge Local Runtime

Status: **IMPLEMENTED + STANDALONE RUNTIME CERTIFIED (Linux x86_64 CPU)**

GravityForge owns a bundled `llama-server` child and exposes it through an ephemeral OpenAI-compatible endpoint on `127.0.0.1`. The chain remains `GravityForge → LocalRuntimeManager → llama-server → AgentEngine`; llama.cpp is not linked into the Rust process.

## Lifecycle and ownership

`STOPPED → STARTING → LOADING_MODEL → READY`

Failures move to `FAILED`. Stop, window destruction, and application shutdown terminate and wait only for the `Child` created by GravityForge. The PID is recorded for status only; GravityForge never kills a process discovered by name or port. Arguments are passed as an array, the model path is data, and no shell interpolation is used.

The server always receives `--host 127.0.0.1` and a dynamically reserved port. No API key is needed for this internal loopback connection and no public listener is created.

## Runtime discovery

Normal operation uses the compatible bundled profile described by `src-tauri/binaries/runtime/manifest.json`. The manifest records the independent runtime version, llama.cpp version/commit, build date, platform, architecture, backend, executable SHA-256, and companion libraries. The checksum and companion files are verified before launch.

Resolution is:

1. compatible bundled runtime (default);
2. custom executable when the advanced **Use custom llama-server** option is enabled;
3. `GRAVITYFORGE_LLAMA_SERVER` when custom mode is enabled and no selected path is valid;
4. structured error.

There is deliberately no opportunistic `PATH` lookup. Ollama remains an independent optional provider.

## Profiles and fallback

- Linux x86_64 CPU: bundled and certified. It always passes `--n-gpu-layers 0` and supports configurable context and thread count.
- Linux x86_64 Vulkan: bundled and supported. Before model load, `llama-server --list-devices` must report a real Vulkan device. If it does not, the UI reports that CPU fallback is ready; explicit Vulkan selection fails safely instead of silently changing hardware.
- Windows CPU/Vulkan/CUDA: resolver architecture prepared, binaries not bundled or certified in this build.
- macOS Metal: future work.

On the certification host only software `llvmpipe` was visible to the packaged official build, so Vulkan was correctly rejected and CPU selected. This is not a Vulkan certification result.

## First run and model checks

The first launch opens AI Runtime settings and shows detected CPU/RAM/GPU, bundled runtime source/version/health, backend recommendation, and model state. Users select a GGUF; endpoint and port are never exposed.

Before launch, Rust verifies that the file exists, is readable, has a `.gguf` extension and starts with the `GGUF` header. It reads the version, tensor count, and metadata count from the fixed header. The UI labels RAM/VRAM/offload values as estimates; model size plus 20% is used as a deliberately simple RAM estimate. Warnings do not arbitrarily block loading.

## Model capabilities

Chat, streaming, tool calling, and Agent Mode are model capabilities, not hardware capabilities. The explicit test accepts tool calling only when `choices[0].message.tool_calls` contains a structured native function call with valid JSON arguments. Text in `assistant.content` is never executed or converted into a call.

The previously tested Qwen2.5-Coder GGUF did not pass this native tool gate. That limits Agent Mode for that model but does not invalidate standalone runtime packaging.

## Current limits

- RAM/VRAM estimates are approximate and do not parse every GGUF metadata value.
- CUDA and Windows are not certified.
- The runtime update layout is versioned and checksum-ready, but downloading, atomic replacement, and rollback are intentionally not implemented yet.
- CPU performance depends heavily on model, quantization, CPU, and context size.

Packaging details and certification evidence are in [runtime-packaging.md](runtime-packaging.md).
