#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"

npm run build
npm run lint
npm run test

cd src-tauri
cargo clean
cargo fmt --check
cargo test
cargo clippy -- -D warnings
cargo build

cd "$repo_dir"
npm run tauri -- build

printf '\nAutomated Nobara gate passed. Complete the manual PTY checklist in docs/terminal.md.\n'
