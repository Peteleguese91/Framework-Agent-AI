# GravityForge

GravityForge is a local-first, standalone agentic desktop IDE. It is built with Tauri 2, React, TypeScript, and Rust. The model is a replaceable provider; orchestration, tools, memory, permissions, and workspace state belong to the application.

## Current state

Phase 0–3 provide the desktop IDE shell, workspace explorer, and Monaco editor. Phase 4 terminal, Phase 5 filesystem tools, and Phase 6 model providers are IMPLEMENTED / PENDING LINUX CERTIFICATION. Windows 11 is a secondary platform. Phase 7 has not started.

## Prerequisites

- Node.js 22 or newer
- npm 10 or newer
- Rust stable 1.77.2 or newer
- Nobara/Fedora packages required by Tauri 2/WebKitGTK (see docs/platforms.md)

## Development

```bash
npm install
npm run build
npm test
npm run tauri dev
```

Architecture and implementation boundaries are documented in [`docs/architecture.md`](docs/architecture.md). The phased roadmap lives in [`docs/development.md`](docs/development.md).

The platform policy and phase certification rules are in [docs/platforms.md](docs/platforms.md). Linux and Windows bundle targets are configured separately.
