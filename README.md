# GravityForge

GravityForge is a local-first, standalone agentic desktop IDE. It is built with Tauri 2, React, TypeScript, and Rust. The model is a replaceable provider; orchestration, tools, memory, permissions, and workspace state belong to the application.

## Current state

Phase 0 establishes the repository, architecture, documentation, strict TypeScript frontend, and Tauri backend. Phase 1 adds a responsive desktop IDE shell with explicit disabled states for features that are not implemented yet—no control pretends to work.

## Prerequisites

- Node.js 22 or newer
- npm 10 or newer
- Rust stable 1.77.2 or newer
- Linux packages required by Tauri 2/WebKitGTK

## Development

```bash
npm install
npm run build
npm test
npm run tauri dev
```

Architecture and implementation boundaries are documented in [`docs/architecture.md`](docs/architecture.md). The phased roadmap lives in [`docs/development.md`](docs/development.md).
