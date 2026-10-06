# Development roadmap

Each phase must inspect existing code, plan, implement, test, build, fix failures, update documentation, and report. Nobara Linux is the primary certification platform. A broken Linux build blocks final phase certification, while independent feature development may continue. Windows support is tracked separately. See [platforms.md](platforms.md).

- Phase 0: repository, boundaries, documentation, strict build configuration.
- Phase 1: desktop shell and real responsive IDE layout.
- Phase 2: complete — secure persistent workspace, lazy project tree, deterministic detection and Project Map V1.
- Phase 3: Monaco editor, secure atomic saves, tabs, diff, palette, Quick Open, session restore, and external-change protection.
- Phase 4: IMPLEMENTED / PENDING LINUX CERTIFICATION. Pending Linux Certification checklist is in [platforms.md](platforms.md). Windows is separately pending. See [terminal.md](terminal.md).
- Phase 5: IMPLEMENTED / PENDING LINUX CERTIFICATION. Independent development may continue while Phase 4 awaits Nobara; final Phase 5 certification requires repairing any Phase 4 Linux failure first. See [tools.md](tools.md).
- Phase 6: IMPLEMENTED / PENDING LINUX CERTIFICATION. LM Studio and generic OpenAI-compatible providers, discovery, direct chat, streaming, cancellation, capability overrides, and validated display-only tool calls. See [models.md](models.md).
- Phase 7–11: agent loop, changes/approvals, Git, planner, memory. Phase 7 has not started.
- Phase 12–18: browser, vision, OCR, computer use, agents, and context management.
- Phase 19–23: workflows, hardening, UX, complete test suite, and releases.

## Definition of done

A feature is done only when its controls call real behavior, typed errors reach the UI, relevant tests pass, production builds pass, and documentation reflects operational limits. Otherwise it is disabled and labeled by phase.

