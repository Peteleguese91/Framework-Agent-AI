# Filesystem tools — Phase 5

Status: **IMPLEMENTED / PENDING LINUX CERTIFICATION**. Phase 4 remains **IMPLEMENTED / PENDING LINUX CERTIFICATION**. Final Phase 5 certification on Nobara requires repairing any Phase 4 Linux failure first.

The Rust command `filesystem_tool` accepts a typed `{ actor, action }` request and returns a tagged result or `{ code, message }` error. `src/tools/filesystem.ts` validates the same contract with Zod and lists version 1 risk metadata. Every invocation is logged with category `FS_TOOL`, operation, actor, risk, outcome, error code, and duration. Logs omit paths, file content, search terms, and patch text. Rust rejects every `AGENT` invocation until the approval policy exists.

| Operation | Behavior | Risk |
| --- | --- | --- |
| `fs.list` | Sorted direct children, bounded by `limit` | READ |
| `fs.read` | Full UTF-8 text up to 2 MiB | READ |
| `fs.read_range` | Byte offset and length up to 256 KiB, with total size and `hasMore` | READ |
| `fs.write` | Overwrite only when `expectedContent` matches | WRITE |
| `fs.patch` | Position-based line hunks with expected old lines; rejects mismatches and overlap | WRITE |
| `fs.create` | Create text file without overwrite | WRITE |
| `fs.delete` | File or empty directory; `recursive=true` for a directory | DESTRUCTIVE / DANGEROUS when recursive |
| `fs.move` | Move without replacing destination on Linux | WRITE |
| `fs.copy` | File copy, or directory copy with `recursive=true`; 128 MiB and 10,000 entry caps | WRITE |
| `fs.mkdir` | Create one directory beneath an existing parent | WRITE |
| `fs.exists` | Presence check | READ |
| `fs.stat` | Kind, byte size, timestamps, readonly and symlink flags | READ |
| `fs.search` | Literal filename, path, or content search; result limit | READ |
| `fs.glob` | Glob matches, respecting `.gitignore` and standard exclusions | READ |

All filesystem paths are relative to the active canonical workspace. Absolute paths, parent traversal and symlink escapes are rejected. Direct mutation of symlink entries is unavailable. `stat` can report a symlink entry without reading its target. Recursive copy and delete reject nested symlinks. Search and glob do not follow links; they scan at most 10,000 entries and return at most 200 results. Full text reads, writes and patches cap at 2 MiB. `read_range` permits access to larger text files but a range must begin and end on UTF-8 boundaries. Binary data is rejected by text operations. Patch preserves LF or CRLF line endings; mixed or CR-only line endings are rejected.

Mutating an existing text file uses a temporary file in the destination directory and preserves its permissions. Linux uses `renameat2(RENAME_NOREPLACE)` for moves and publishing copied directories. Windows uses a destination check followed by rename; Windows native certification remains pending. Content comparison and symlink checks are repeated immediately before operations, but concurrent writes from other app paths can still race because the path lease manager belongs to a later phase. No agent or model is connected to these tools yet.

## Nobara verification

Run `bash scripts/verify-linux.sh`, then confirm `cargo test`, `cargo clippy -- -D warnings`, and the Tauri build. The Rust suite covers traversal, symlink escape, range reads, patch conflicts, move, copy, recursive and nonrecursive delete, glob ignore rules, all search modes, binary and oversized files, missing paths, and permission errors. Manually exercise these tools through the Tauri API on a disposable workspace before certifying Phase 5. Phase 4's PTY checklist remains in [platforms.md](platforms.md).