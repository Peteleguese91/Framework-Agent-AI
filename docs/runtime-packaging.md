# Runtime Packaging

## Bundled runtime

GravityForge 0.1.0 bundles a separately versioned runtime:

- runtime manifest version: `gravityforge-local-runtime-1`;
- llama.cpp: `0.6.0-dev`, build `b11429`;
- commit: `d81235049384534c167caea52b85a694f6103d14`;
- upstream build date: 2026-10-05;
- source: official `ggml-org/llama.cpp` Linux x86_64 CPU and Vulkan release archives;
- executable SHA-256: `5faf410bbbc00a1b94b09ee01623699f255365628dfa73b1ee552d9e52a2231d`;
- license: MIT, copied beside each profile.

`scripts/prepare-runtime-linux.sh` pins upstream archive URLs and published SHA-256 values. It rejects a mismatched archive and installs only `llama-server`, required companion libraries, CPU dispatch libraries, Vulkan backend, and license. Tauri packages `src-tauri/binaries/runtime/` as resources in DEB, AppImage, and RPM.

## Native dependency audit

`readelf` confirms `$ORIGIN` lookup for bundled companions. `ldd` on the certification host resolves the remaining system ABI dependencies: glibc (maximum referenced symbol `GLIBC_2.34`), libstdc++, libgcc, libgomp, OpenSSL 3, zlib, and libm. Vulkan additionally requires the system Vulkan loader/ICD and a usable hardware device. These core/system/driver libraries are not copied indiscriminately into the application.

## Update-ready layout

Each profile is isolated at `runtime/<os>/<arch>/<backend>`. The manifest makes a future updater able to stage a new version, verify SHA-256, atomically switch versions, and roll back. No runtime download or execution of unverified files exists in this phase.

## Certification evidence (2026-10-07)

The clean-path test did not use LM Studio, a PATH binary, or `GRAVITYFORGE_LLAMA_SERVER`:

1. all three release bundles built successfully;
2. the AppImage launched successfully;
3. the AppImage resource tree contained both profiles, manifest, license, executable, and companion libraries;
4. the `llama-server` copied into the produced AppImage loaded `Llama-3.2-1B-Instruct-Q4_K_M.gguf` on CPU;
5. `/health`, `/v1/models`, exact non-streaming chat, and SSE streaming passed;
6. model load took about 0.9 seconds; measured generation was about 40 tokens/s for the small deterministic probes;
7. shutdown left no `llama-server` process;
8. repository scan found no hardcoded personal development path.

Package sizes: DEB 70.11 MiB, AppImage 135.91 MiB, RPM 70.12 MiB.

## Platform matrix

| Platform/profile | Implementation | Evidence | Status |
|---|---|---|---|
| Linux x86_64 CPU | bundled | real AppImage runtime, GGUF, health/chat/stream/stop | **CERTIFIED** |
| Linux x86_64 Vulkan | bundled with preflight | current session exposed no usable hardware Vulkan device | **SUPPORTED / NOT CERTIFIED** |
| Windows x86_64 CPU/Vulkan/CUDA | resolver model only | no Windows machine/package test | **PLANNED / NOT CERTIFIED** |
| macOS Metal | none | none | **PLANNED** |

Tool calling is intentionally tracked as a separate model capability and was not used to assign packaging certification.
