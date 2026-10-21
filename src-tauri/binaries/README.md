# llama-server runtime resources

`runtime/manifest.json` describes the pinned, redistributable llama.cpp profiles bundled as Tauri resources. Linux x86_64 CPU and Vulkan builds are prepared by `scripts/prepare-runtime-linux.sh`; the script verifies the official release archive checksums before installing only `llama-server`, its companion libraries, and the MIT license.

The runtime manager verifies the executable checksum again before launch. Custom binaries and `GRAVITYFORGE_LLAMA_SERVER` remain advanced/debug fallbacks and are not required for normal operation.
