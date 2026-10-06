# Terminal

Phase 4 adds two separate execution paths.

## Interactive PTY

The bottom panel uses xterm.js and the fit addon. Rust owns a TerminalManager with one native portable-pty session per terminal ID. Each session records shell, workspace cwd, process ID, creation time, status, and exit code. PTY reader threads emit terminal-output events containing the terminal ID and raw bytes. The frontend retains a bounded 1 MiB display buffer per session so hiding the bottom panel does not stop processes or lose recent output. A separate status event announces exit or failure.

The UI can create, write, resize, interrupt, kill, close, and query sessions. Ctrl+C is sent through the PTY as ETX; the terminal input handler also passes interactive keys through. Kill targets the process tree where the OS permits. Closing a session kills it and removes it from the manager. Terminal processes are not restored after app restart.

Ctrl+Backquote toggles the bottom panel. Ctrl+Shift+Backquote creates a new terminal for the current workspace. Shell, font size, cursor style, and scrollback preferences are local to the app. On Linux the default shell is the user's SHELL with /bin/bash fallback. On Windows the default is Windows PowerShell when installed, then ComSpec or cmd.exe. These choices live in dedicated platform modules.

## Non-interactive exec

The terminal_exec command accepts a structured command and argument vector, optional workspace-relative or in-workspace absolute cwd, environment additions, timeout, and actor. It returns stdout, stderr, exit code, success, duration, and timeout status. It drains stdout and stderr concurrently and caps each captured stream at 8 MiB. Timeout kills the process tree where possible. This path is for future build, test, and agent tools; it does not emulate a terminal.

## Security boundary

The initial cwd must resolve inside the active workspace. A user terminal is a normal shell: once started, the user can navigate or execute outside that workspace. It is therefore more powerful than the workspace filesystem API. The UI creates user sessions only. The terminal_exec command currently accepts the USER actor and rejects AGENT; a future agent tool must route through explicit approvals and a distinct capability policy. No model is connected to either path.

Logs use the TERMINAL category for lifecycle and errors without recording command content or PTY output.

## Verification

On Nobara run `bash scripts/verify-linux.sh` from the repository root. It performs a clean Rust build, frontend checks, Rust formatting, tests, Clippy, compilation, and Tauri packaging. Use the Linux bundle configuration for .deb, AppImage, and .rpm. Manually exercise pwd, ls or dir, echo hello, installed runtime version commands, a long running process, streaming output, Ctrl+C, resize, multiple sessions, kill, and close. Phase 4 remains unverified until native and manual checks pass.

Windows checks are tracked separately and do not block Linux certification. The current Windows host lacks MSVC Build Tools and the Windows SDK, so its native gate remains pending. See [platforms.md](platforms.md).
