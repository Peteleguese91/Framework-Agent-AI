# Model providers

Providers are adapters behind a common model interface. Initial targets are LM Studio, Ollama, and a generic OpenAI-compatible endpoint. Discovery and health checks are separate from inference. Streaming is normalized into text deltas, tool-call deltas, usage, and terminal status.

Routing profiles separately select coding, vision, fast, and review models. No model is loaded in the Tauri process.
