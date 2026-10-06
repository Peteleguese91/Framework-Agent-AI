# Platform policy

GravityForge's primary development, test, UX, and release platform is Nobara Linux (Fedora based). Windows 11 is a secondary supported platform. A Windows toolchain failure does not block Linux feature work or a Linux phase certificate.

## Shared boundaries

Keep IPC command names, request and response types, and frontend behavior the same across platforms where possible. Put operating system behavior in dedicated modules selected with target_os configuration. User terminal sessions begin inside the active workspace; a normal shell remains more powerful than the filesystem API. Agent execution remains disabled until an approval policy is implemented.

## Terminal

Linux uses the user's SHELL when set, with /bin/bash fallback, and portable-pty's Unix PTY implementation. Windows prefers Windows PowerShell when installed, then ComSpec or cmd.exe, and portable-pty's ConPTY implementation. Process group/session handling is platform-specific; the public terminal commands stay shared.

## Bundles

The shared Tauri configuration does not select a bundle format. The Linux override selects .deb, AppImage, and .rpm. The Windows override selects .msi and NSIS .exe. The Linux bundle is built and verified on Nobara; Windows packaging is a separate support gate.

## Certification

Use two statuses for each phase:

- Linux complete: build, lint, frontend tests, Rust formatting, Rust tests, Clippy, Tauri build, and required manual behavior pass on Nobara.
- Windows pending or complete: record Windows build, tests, packaging, and manual behavior separately.

Phase 4, Phase 5, and Phase 6 are IMPLEMENTED / PENDING LINUX CERTIFICATION. Independent feature development may continue while Linux certification is pending. If Phase 4 fails on Nobara, repair it before final certification of later phases. Windows checks cannot substitute for Nobara certification.

## Pending Linux Certification — Phase 4

- [ ] Run `bash scripts/verify-linux.sh` on Nobara.
- [ ] Confirm `cargo test` passes.
- [ ] Confirm `cargo clippy -- -D warnings` passes.
- [ ] Confirm the Tauri build produces the configured Linux bundles.
- [ ] Manually run a real PTY session: shell commands, streaming long process, UI responsiveness, interrupt, kill, resize, and cleanup.


Tauri's Fedora prerequisites include WebKitGTK 4.1 development files, OpenSSL development files, appindicator development files, librsvg, libxdo, and C development tools. Consult the current Tauri prerequisites before installing system packages.
