# Computer Control — Architecture and Audit

Status: **IMPLEMENTED** (Linux/KDE Wayland real tests completed; certification withheld because active-window verification and close-window are unavailable through a standard Wayland portal)

## Baseline audit

GravityForge already has the correct control boundary: native model `tool_calls` are validated by `ToolRegistry`, classified by risk, optionally approved by `ApprovalManager`, executed by a bounded handler, returned as observations, and evaluated by `CompletionGuard`. Pause prevents new tools from starting and Stop cancels the model turn and rejects pending approvals. Filesystem and terminal tools already reject arbitrary unvalidated shapes; terminal execution uses executable plus argument arrays.

The missing pieces are a dedicated native `ComputerController`, session-scoped user consent, desktop-specific completion evidence, a computer-step limit, audit records with redaction, and visible UI state.

## Host findings

Audit host: Nobara Linux 44, KDE Plasma/KWin 6.7.3, Wayland, two 1920×1080 displays, AMD Radeon RX 6900 XT. XWayland is present but cannot control native Wayland clients globally. `xdg-desktop-portal` exposes Screenshot v2, RemoteDesktop v2, and ScreenCast v4. RemoteDesktop advertises keyboard, pointer, and touchscreen. AT-SPI is present. `xdotool`, `ydotool`, `wtype`, and `kdotool` are not installed and are not architectural dependencies.

## Proposed architecture

`AgentEngine → ToolRegistry schema/risk → ApprovalManager → computer IPC → ComputerController → portal/desktop API`

Perception and action are separate commands. Perception handlers never send input. Action handlers require an enabled, explicitly authorized session and check the stop generation immediately before every OS event. No handler accepts a shell command.

The Wayland input backend uses `org.freedesktop.portal.RemoteDesktop`. Session creation requests keyboard and pointer; `Start` presents the desktop consent dialog. Input uses the portal `Notify*` methods. Screenshot uses the Screenshot portal and stores a reference in the application data area. Application launch resolves a desktop application/executable without a shell. Window enumeration/focus/close are best-effort and reported as limited when no stable desktop API is available.

## Tool surface and initial risk

Perception (`SAFE`): `computer.get_displays`, `computer.get_active_window`, `computer.list_windows`, `computer.screenshot`.

Low-impact actions (`MODERATE` in the existing three-level classifier): `computer.move_mouse`, `computer.scroll`, `computer.focus_window`.

State-changing actions (`MODERATE`): `computer.open_app`, `computer.click`, `computer.double_click`, `computer.key_press`, `computer.hotkey`, `computer.type_text`.

Potential data loss (`DANGEROUS`, always approval): `computer.close_window`. Risk is raised to `DANGEROUS` for destructive/system hotkeys and sensitive-looking typed text. Arguments shown in audit/approval are redacted for `type_text`.

## Wayland versus X11

On Wayland, global input is unavailable until the user grants a RemoteDesktop session. The portal can revoke it at any time. Screenshot may also display portal UI. Window listing, stable IDs, focus, and close are not part of the standard portal and therefore cannot be claimed universally.

On X11, implementation may later use native X11 APIs, but this phase does not add unsafe fallback tools or depend on X11 utilities. XWayland is not treated as permission to bypass Wayland isolation.

## Privacy and completion

Computer Control defaults OFF. Enabling it is explicit per application session, is visibly indicated, and provides an immediate Stop action. No background input is sent.

Completion requires a successful follow-up observation after desktop actions: application launch requires a process/window observation, focus requires active-window observation, and pointer/keyboard actions require a later screenshot or other perception result. The loop has a separate default maximum of 20 computer tool calls.

Screenshot output is only advertised as model-visible when the selected provider declares vision support. No OCR fallback is used.

## Implemented components

The Rust `ComputerController` owns a non-persistent RemoteDesktop/ScreenCast portal session. Computer Control starts OFF. The user must enable it from the visible Agent panel and approve KDE's portal dialog. Stop increments a generation checked before every input event and closes the portal session. Application launch uses `Command` with a validated executable and argument array; no API accepts a shell string.

The registered tools are: `computer.get_displays`, `computer.get_active_window`, `computer.list_windows`, `computer.screenshot`, `computer.open_app`, `computer.focus_window`, `computer.move_mouse`, `computer.click`, `computer.double_click`, `computer.scroll`, `computer.type_text`, `computer.key_press`, `computer.hotkey`, and `computer.close_window`.

Perception is `SAFE`. Ordinary input and launch/focus actions are `MODERATE`. Close, destructive shortcuts (for example Alt+F4 and Ctrl+Q/Ctrl+W), and sensitive-looking typed text are `DANGEROUS` and require explicit approval. Typed text is redacted in both approval UI and native audit records. The AgentEngine has an independent 20-step computer limit. CompletionGuard requires a successful perception observation after every desktop action.

## Capability matrix

| Capability | Linux / KDE Wayland | Result |
|---|---|---|
| Portal authorization and display bounds | XDG RemoteDesktop + ScreenCast | CERTIFIED |
| App launch | Direct validated executable, no shell | CERTIFIED |
| Window enumeration | KWin WindowsRunner stable IDs; KDE-specific | SUPPORTED / LIMITED |
| Window focus | KWin WindowsRunner `Run`; no standard active-window readback | LIMITED (real action executed) |
| Active-window perception | No standard portal API and KWin `getWindowInfo` does not expose active state | UNAVAILABLE |
| Keyboard input | Portal `NotifyKeyboardKeysym` | CERTIFIED |
| Pointer move/click | Portal absolute motion and evdev button notification | CERTIFIED |
| Scroll | Portal discrete axis notification | SUPPORTED |
| Screenshot | Screenshot portal, validated PNG signature and non-zero dimensions | CERTIFIED |
| Immediate stop | generation check + portal session close | CERTIFIED |
| Close arbitrary window | Not exposed by the standard portal | UNAVAILABLE |
| Model screenshot vision | No binary image attachment in the current model turn contract | UNAVAILABLE; path/metadata only, no OCR |
| X11 backend | No unsafe utility fallback added | UNAVAILABLE in this phase |

## Verification

Automatic gates completed on 2026-10-07:

- `npm test -- --run`: PASS, 48 tests;
- `npm run lint`: PASS;
- `npm run build`: PASS;
- `cargo test`: PASS, 41 deterministic tests plus one ignored real-desktop test;
- native tests cover validation, redaction, risk, malformed input, completion verification, stop hook, and the 20-step limit.

Real KDE Wayland smoke test `real_wayland_portal_input_screenshot_and_stop`:

1. KDE portal consent and a real monitor stream: PASS;
2. Kate launch through `Command` without a shell: PASS;
3. absolute pointer movement and non-destructive click: PASS;
4. real keyboard text injection: PASS;
5. real portal screenshot, PNG format and positive dimensions: PASS;
6. portal session close/stop and test-window cleanup: PASS;
7. KWin enumeration returned stable IDs for two real Kate windows: PASS;
8. KWin focus action was executed for both IDs: PASS, but active-state readback is unavailable, therefore capability remains LIMITED.

The first screenshot attempt used the interactive picker and returned `Response(Cancelled)`. The production path and real test were corrected to use the non-interactive portal capture; the scenario then passed. This was a portal-mode bug, not a security bypass. No architecture or unrelated feature was changed.

## Final assessment

Final state: **IMPLEMENTED**.

`IMPLEMENTED + COMPUTER CONTROL CERTIFIED — Linux` is intentionally not claimed because the required window-focus verification cannot be completed reliably on this KDE Wayland host, and arbitrary close-window is unavailable. All other real desktop scenarios listed above are certified. Voice Mode was not started.
