# Voice Agent Foundation

Status: **IMPLEMENTED** — real Linux audio tests partially completed; certification intentionally withheld.

## Initial audit

GravityForge already has two user-input paths: direct text chat in `modelStore.send` and tool-capable tasks in `agentStore.start`, which constructs the existing `AgentEngine`. Voice must dispatch into those same entry points according to the selected ASK/AGENT/AUTONOMOUS mode; it must not parse intents or create another agent. The existing AgentEngine already owns native tool calls, risk classification, approval UI, CompletionGuard, pause/resume/stop, and Computer Control authorization.

Model streaming emits Tauri delta/completion/error events. Agent stop already cancels the model request and Computer Control. Voice therefore needs separate `interruptSpeech` and `stopAgent` operations: the former kills playback and clears queued speech only, while the latter also invokes the existing agent/model stop path.

The native host is Nobara Linux 44 with PipeWire 1.6.8 and PulseAudio compatibility. Real enumeration found three microphone sources (USB AUDIO default, Logitech G935, integrated analog) and four output sinks (G935 default, USB AUDIO, HDMI, IEC958). `pw-record`, `pw-play`, `pactl`, FFmpeg, SoX, espeak-ng and speech-dispatcher are installed. whisper.cpp, Piper, STT models and Piper voice models are not installed. No audio dependency currently exists in Cargo.

## Proposed architecture

`Microphone → pw-record/PipeWire → temporary WAV → energy VAD → whisper.cpp → existing text input router → AgentEngine/provider → existing approval/tools/CompletionGuard → final text → TTS engine → pw-play/PipeWire`

`VoiceEngine` is a Rust lifecycle/resource owner for capture, temporary buffers, STT/TTS child processes, playback and immediate interruption. A frontend voice store presents state and sends the transcript through the same functions used by typed input. No speech phrase other than local emergency STOP/MUTE/CANCEL is interpreted outside the model, and emergency commands can only stop work.

## Runtime and distribution decision

STT uses a configurable `whisper-cli` executable and multilingual GGML model. `base` or `small` multilingual is recommended; `.en` models are unsuitable because Italian is required. The model is not bundled automatically because of package size. whisper.cpp JSON-full output supplies detected language and timestamps; confidence remains absent unless actually reported.

TTS supports configurable Piper executable/model/config paths. For the installed host, espeak-ng is an explicit lightweight offline fallback with both Italian and English voices, enabling functional output without enlarging the package. Piper remains the recommended higher-quality engine. Voice/model licenses must be reviewed per selected model; no model is silently redistributed.

Audio is captured only while Push to Talk is held. WAV files live under the OS temporary directory and are removed after transcription, including error paths. There is no wake word, always-listening mode, background microphone, cloud upload, voice approval, or persistent recording.

## Risks and constraints

- External runtimes may be missing, incompatible, or disconnected; all failures must be structured and must not break text chat.
- PipeWire node names can disappear between enumeration and capture.
- Energy VAD is deterministic and lightweight but less accurate than a neural VAD; its configurable threshold/minimum speech/silence settings are exposed and the full recording preserves pre-roll.
- Tool-capable voice commands require the user to select AGENT/AUTONOMOUS just like typed commands. Dangerous actions still require the existing UI approval.
- Full certification requires real microphone speech, a multilingual STT model, a working LLM, TTS playback, Computer Control authorization, multi-turn tests and approval verification.

## Implemented VoiceEngine

The Rust `VoiceEngine` owns the microphone child process, temporary WAV path, playback process, runtime health and state machine: `OFF`, `IDLE`, `LISTENING`, `TRANSCRIBING`, `THINKING`, `SPEAKING`, `INTERRUPTED`, `FAILED`. It enumerates PipeWire devices through PulseAudio's structured JSON interface, captures the selected node at 16 kHz mono PCM, validates speech using configurable RMS VAD, invokes whisper.cpp with JSON-full output, synthesizes with Piper or espeak-ng, and plays through the selected PipeWire sink.

Runtime commands provide status, configuration validation/load, unload, capture, transcription, speech and interruption. A runtime is considered `READY` only after the STT executable/model and configured TTS engine/model validate. CLI models are loaded per request, so unload means stopping children, clearing readiness and releasing temporary resources rather than retaining a hidden resident model.

The UI exposes device selection, Auto/Italian/English, STT executable/model, VAD threshold, silence timeout, TTS engine/voice/model/config, rate, Auto Speak, optional in-app Push-to-Talk shortcut and separate `Stop speech`/`Stop all` controls. Pointer-held Push to Talk is the only mandatory listening mode. There is no global shortcut registration.

## Agent, safety and conversation integration

Transcripts use `dispatchVoiceText`: ASK calls the exact existing `modelStore.send` path and retains that chat's multi-turn history; AGENT/AUTONOMOUS calls the exact existing `agentStore.start` path, which constructs the existing AgentEngine. There is no intent regex and no second agent. Consequently native tool calls, ToolRegistry validation, risk, approvals, CompletionGuard, filesystem/terminal policy and Computer Control authorization are unchanged. Voice cannot approve actions; dangerous approvals remain UI-only.

The only local phrases are exact emergency commands (`stop`, `fermati`, `annulla`, `cancel`, `mute`, `muto`). They can only interrupt speech/agent/computer activity. They cannot start a tool or confer approval.

One existing product limitation remains: independent AgentEngine tasks do not currently share task history. ASK voice conversations are multi-turn; referential multi-turn AGENT tasks are not certified, because changing AgentEngine semantics was explicitly out of scope.

## Privacy and failure behavior

Voice is OFF on every application start even if settings were previously stored. Capture starts only on a held button/configured in-app chord. Temporary audio is removed after transcription, failure, interrupt, disable and window destruction. Debug recording persistence is not implemented, so it cannot be enabled accidentally. No captured audio is uploaded.

Structured errors cover missing/disconnected microphone, permission failure, missing STT/TTS model, STT/TTS failure, output failure, interruption, invalid configuration and no speech. These states are isolated from text chat.

## Verification results (Nobara Linux 44)

Deterministic tests cover state transitions, missing runtimes/models, invalid executable/device configuration, interrupt and playback cleanup, temporary-file deletion, privacy restore, emergency commands, local shortcut matching, ASK input-path identity and AGENT input-path identity. Existing ApprovalManager/AgentEngine tests remain the authority for approval behavior.

Real environment preparation used upstream whisper.cpp v1.9.4 and the multilingual `ggml-base.bin` model (approximately 142 MiB) under `work/voice-runtime`; neither is included in application bundles. espeak-ng supplied installed Italian and English voices.

Real tests:

| Scenario | Result |
|---|---|
| PipeWire device enumeration | PASS — 3 inputs, 4 outputs |
| Microphone capture | PASS — two real 7.00 s, 16 kHz, mono, 16-bit WAV captures |
| Italian phrase accuracy | FAIL — USB captured unrelated ambient speech; G935 was classified as `[Musica]` |
| Local whisper.cpp execution | PASS — base multilingual inference completed in 0.80 s and 0.71 s |
| Italian TTS generation/output | PASS — espeak-ng WAV played on G935; playback process completed in 2.52 s |
| Speech interruption | PASS — long playback terminated after approximately 1 s |
| Microphone/playback cleanup | PASS — no audio processes remained and all six test WAV/JSON files were deleted |
| Voice chat + LLM response | NOT CERTIFIED — required phrase was not transcribed correctly |
| Voice Computer Control command | NOT CERTIFIED |
| Referential Agent multi-turn | NOT CERTIFIED |
| Approval preserved | PASS deterministically; not repeated from a successful live voice command |

Measured values are observations, not targets. Speech-end-to-transcript through the production UI was not reported because the phrase-accuracy test failed. `first TTS audio` and total voice-response latency likewise remain unreported rather than inferred.

## Final status

**IMPLEMENTED**

`IMPLEMENTED + VOICE FOUNDATION CERTIFIED — Linux` is not claimed. Although capture, local STT execution, TTS, interruption and cleanup ran for real, the prescribed Italian utterance, voice chat, voice-originated Computer Control, approval and multi-turn scenarios did not all pass end to end. No wake word, always-listening, speaker identification, voice biometrics or voice approval was added.

## STT reliability follow-up — 2026-10-08

### Original failure audit

The original USB capture was transcribed as `quando acaba si svegli...` instead of `Questo è un test vocale di GravityForge`; the G935 capture returned `[Musica]`. The earlier test deleted its temporary WAV/JSON as required by the normal privacy policy, so the complete USB suffix, original JSON language field, exact VAD timestamps and stderr cannot be reconstructed and are not invented here. The retained facts are:

- whisper.cpp `1.9.4-dev`, `ggml-base.bin`, multilingual, 147,951,465 bytes (142 MiB), SHA-256 `60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe`;
- command arguments: `-m MODEL -f WAV -l auto -ojf -of PREFIX -np`; translation was absent/false, temperature used the upstream `0.0` default, fallback increment `0.2`, and the upstream default thread count was 4;
- two 7.00 s captures requested from PipeWire as PCM16, 16 kHz, mono and sent unchanged to Whisper;
- the former energy VAD only counted active 20 ms frames. It produced no start/end timestamps and did not crop audio, so duration before and after “VAD” was the same 7.00 s. `silenceTimeoutMs` was validated but unused;
- no preprocessing beyond PipeWire's implicit conversion was applied and no whisper warning was retained.

The device audit found materially different native formats: USB AUDIO is s24le mono 48 kHz, G935 is s16le mono 48 kHz, and integrated analog is s32le stereo 48 kHz. This made the implicit conversion opaque, although it was not by itself the observed recognition failure.

### Root cause and new real captures

A new 32.528 s USB capture was valid PCM16 stereo 48 kHz, RMS `0.020791`, peak `0.155060`, with no clipping. It did not contain the requested phrase; it contained unrelated desktop/room speech, transcribed with forced Italian as `Sì ma 1111 giochi Cioè lui c'ha il beige guardiano dell'industria video ludica 1111`. A new 33.707 s G935 capture contained exact digital silence (RMS `0`, peak `0`) and small multilingual hallucinated `[Musica]`. Thus the original real failure is an input-path/device problem: the USB device acquired other audible content and the G935 microphone path was muted or disconnected. It is not an English-only model, translation, stereo-interleaving, clipping, or Italian language-selection problem.

### Pipeline changes

Capture now requests PCM16 stereo 48 kHz, preserving what the application actually receives as the raw stage. Rust parses RIFF chunks, validates PCM16, downmixes all channels by frame, and linearly resamples 44.1/48 kHz inputs to PCM16 mono 16 kHz. No automatic gain is applied. Deterministic tests cover 44.1 and 48 kHz stereo resampling, WAV round-trip, clipping/low-level reporting, VAD padding, and cleanup.

Energy diagnostics report RMS, absolute peak, clipped-sample percentage, silence-frame percentage and the descriptive `LOW`, `OK`, or `CLIPPING` state. These are operational heuristics tied to the configured VAD threshold, not claims of scientifically calibrated loudness.

The segmentation layer now records first/last active timestamps, keeps 300 ms pre-roll, and uses the configured silence timeout as post-speech padding with a 400 ms minimum. The minimum-speech check is preserved. The same controlled sample produced the correct accepted transcript with the full 4.351 s audio and with its 4.0 s padded VAD segment; therefore VAD was not the cause on that sample.

Explicit diagnostic mode can preserve three files in an already-existing user-selected directory: `raw.wav`, `resampled.wav`, and `vad.wav`. It is OFF by default. Normal mode still deletes capture, processed audio, VAD audio and JSON on success, error, interruption, disable and window destruction.

The UI passes `it` for Italian, `en` for English and `auto` only for Auto. It displays the requested/detected language, model, input/output format, level metrics and latency. No confidence-derived approval or action was added; AgentEngine policy remains unchanged.

### Controlled model comparison

The identical locally synthesized phrase with 0.5 s pre-roll and 0.9 s post-roll was used to isolate decoding from broken microphone routing. These are controlled-input results, not substitutes for human microphone certification.

| Model | Size | Language | Transcript | Latency | Max RSS | Result |
|---|---:|---|---|---:|---:|---|
| tiny multilingual | 75 MiB | auto | `Questo è un test vocale di graviti folche` | 0.57 s | 177 MiB | FAIL |
| tiny multilingual | 75 MiB | it | same error | 0.36 s | 179 MiB | FAIL |
| base multilingual | 142 MiB | auto | `Questo è un test vocale di Graviti Forge` | 1.18 s | 288 MiB | PASS |
| base multilingual | 142 MiB | it | same accepted wording | 0.69 s | 285 MiB | PASS |
| small multilingual | 466 MiB | auto | `Questo è un test vocale di gravity forge` | 4.32 s | 752 MiB | PASS |
| small multilingual | 466 MiB | it | same accepted wording | 2.30 s | 750 MiB | PASS |

Forced Italian avoided language detection and was substantially faster on this sample without changing correctness. Auto remains unforced in the product. Tiny is not recommended. Base is the balanced profile; small is the accuracy-oriented default recommendation when approximately 0.8 GiB process memory and roughly 2.3 s controlled-sample latency are acceptable. CPU use was approximately 361–379% during the measured runs. Real-time factor for small/Italian was approximately `0.53` on the 4.351 s sample.

### Repeatability, noise and regression

Five independently synthesized speeds (135, 145, 155, 165, 175 words/minute) with small/Italian all preserved the main Italian words but rendered the product name as `gravidi forte`, `graviti forge`, or `graviti forte`. Under the strict allowed tolerance (`Gravity Forge` only), the controlled 5-run result is **0/5 PASS**. This also demonstrates why synthetic espeak output cannot certify human speech.

Noise conditions for small/Italian:

| Condition | Transcript outcome | Result |
|---|---|---|
| quiet controlled input | exact words, `gravity forge` | PASS |
| low pink-noise desktop approximation | product rendered `graviti forte` | FAIL |
| one-second silence before and after | exact words, `gravity forge` | PASS |

Additional Italian controlled set with small/Italian:

| Expected | Actual | Result |
|---|---|---|
| Apri Firefox | `a pretty fire fox` | FAIL |
| Apri l'editor di testo | `apri le dito di testo` | FAIL |
| Scrivi GravityForge voice test | `Scambi migrammi di forge, mori che pest` | FAIL |
| Fai uno screenshot | `Fai lo scringshot` | FAIL |
| Ferma tutto | `Fema tutto` | FAIL (strict text) |
| Quanto fa due più due? | `Quanto fa due due due?` | FAIL |

English regression with small/English passed 3/3: `This is a gravity forge voice test`, `Open Firefox`, and `Take a screenshot`. Individual small-model controlled runs took 2.09–2.45 s, used approximately 750–755 MiB maximum RSS, and about 374–380% CPU on a Ryzen 5 5600X.

### E2E and final status

The required real Italian STT gate did not pass because neither available microphone path captured the requested human utterance. Per the mandated ordering, voice chat, voice Computer Control, live multi-turn and the new live approval pass were not rerun. Previously passing TTS, interruption, approval-policy unit coverage and microphone/process cleanup remain valid, but they cannot elevate this run to certification.

Final status remains **IMPLEMENTED — STT RELIABILITY IMPROVED, REAL MICROPHONE BLOCKED**. `IMPLEMENTED + VOICE FOUNDATION CERTIFIED — Linux` is not claimed. Continuous conversation, wake word, always listening, cloud STT/TTS, voice approval and pseudo-tool-call parsing were not introduced.

## Real microphone routing and final certification attempt — 2026-10-08

### PipeWire audit

Nobara Linux 44 runs PulseAudio compatibility on PipeWire 1.6.8. The default source was `alsa_input.usb-ME6_2866_721_MS_-3.7db_USB_AUDIO-00.mono-fallback`; the source explicitly selected by GravityForge is passed to `pw-record --target`, so capture does not silently fall back to another node.

| Source | PipeWire node | Classification | Format | Profile/route | Mute/gain | Availability |
|---|---:|---|---|---|---|---|
| G935 Gaming Headset Mono | 58 | Microphone, ALSA `Audio/Source` | s16le mono 48 kHz | `mono-fallback`, `analog-input-mic` | unmuted, 100%, 0 dB | profile available; no signal |
| Starship/Matisse analog | 61 | Microphone, ALSA `Audio/Source` | s32le stereo 48 kHz | `analog-stereo`, front mic selected | unmuted, 100%, 0 dB | all front/rear/line ports reported not available |
| USB AUDIO Mono | 95 (Pulse source serial 112) | Microphone, ALSA `Audio/Source` | s24le mono 48 kHz | `mono-fallback`, `analog-input-mic` | unmuted, 39%, -24.65 dB | available and running |
| G935 output monitor | 57 | Monitor/desktop output | s16le stereo 48 kHz | sink monitor | unmuted, 100% | available |
| HDMI output monitor | 59 | Monitor/desktop output | s32le stereo 48 kHz | HDMI 4 monitor | unmuted, 100% | available |
| Integrated S/PDIF monitor | 60 | Monitor/desktop output | s32le stereo 48 kHz | IEC958 monitor | unmuted, 100% | available |
| USB AUDIO output monitor | 44 (Pulse source serial 111) | Monitor/desktop output | s16le stereo 48 kHz | speaker sink monitor | unmuted, 100% | available |

`USB AUDIO Mono` is not a monitor, loopback, or virtual node: its factory is `api.alsa.pcm.source`, media class is `Audio/Source`, ALSA stream is `capture`, and `monitor_source` is empty. Its separate monitor node is explicitly classified as `Monitor`. The G935 active card profile was already `output:analog-stereo+input:mono-fallback`; input-only, IEC958 duplex and Pro Audio alternatives exist, but the correct capture node remained digital silence. The likely cause is hardware boom/mute/radio state rather than an output-only PipeWire profile.

### Raw signal and playback checks

The user confirmed that playback of the USB AUDIO diagnostic recording clearly contained their real voice saying `Uno due tre quattro GravityForge`.

| Input | Duration | Captured format | RMS | Peak | Zero samples | Self-test |
|---|---:|---|---:|---:|---:|---|
| USB AUDIO | 9.989 s | PCM16 stereo 48 kHz | 0.015781 | 0.058502 | 2.101% | PASS — VOICE SIGNAL DETECTED; playback human-confirmed |
| Logitech G935 | 9.984 s | PCM16 mono 48 kHz | 0 | 0 | 100% | FAIL — NO SIGNAL |
| Integrated analog | not recorded | native s32le stereo 48 kHz | — | — | — | FAIL — DEVICE UNAVAILABLE |

The G935 sample was played locally but contained silence. It was not sent to Whisper. The integrated analog input was not recorded because PipeWire reported every physical input route disconnected.

### Product changes for hardware safety

Device enumeration now returns node ID, media class, sample specification, profile, active port, mute, gain, availability and a `Microphone`, `Monitor`, `Virtual`, `Unknown`, or `Output` classification. Real microphones are listed first; monitor/virtual/unknown sources are hidden unless Advanced is enabled and are never selected as the normal default.

`Test Microphone (5 s)` captures only the selected node, emits live RMS/peak level events, calculates zero percentage, labels `NO SIGNAL`, `LOW`, `OK`, or `CLIPPING`, and can play the temporary sample locally. Results are `PASS — VOICE SIGNAL DETECTED`, `FAIL — NO SIGNAL`, `FAIL — DEVICE UNAVAILABLE`, or `FAIL — LIKELY MONITOR/LOOPBACK`. The backend remembers the exact passing node only for the current enabled runtime. Device changes, Voice OFF, runtime unload and application exit invalidate the result.

Whisper capture is now backend-gated: `voice_start_listening` returns `MIC_SELF_TEST_REQUIRED` unless the same selected microphone passed self-test. A monitor, unavailable input, or zero-only stream therefore cannot reach STT. This changes no Whisper, VAD, AgentEngine, tool-call, or approval behavior.

### Forced-Italian real certification

The required `small` multilingual model and forced `it` were used. Each file contained valid nonzero signal, but the available USB path continued to capture unrelated spoken program/room audio instead of the prescribed utterance.

| Run | Duration / RMS / peak | Transcript | Result |
|---|---|---|---|
| 1 | 7.989 s / 0.012453 / 0.058502 | `Alla fine venga a scoprire una persona si deve guardare tra le 60 e l'80 pubblicità al giorno.` | FAIL |
| 2 | 7.989 s / 0.016205 / 0.058502 | `99 euro che poi al netto sono, vabbè togliva, togli tasse diciamo dovrebbero essere sui 3 euro e 50.` | FAIL |
| 3 | 7.989 s / 0.005550 / 0.058502 | `E lui si è fissato l'este pubblicità.` | FAIL |
| 4 | not run | — | NOT RUN — threshold already impossible |
| 5 | not run | — | NOT RUN — threshold already impossible |

After three failures, the required `>= 4/5` became mathematically impossible. The mandated gate was applied immediately. Italian command transcripts, English regression, voice chat E2E, Computer Control voice, multi-turn, TTS interrupt and live approval were not rerun in this attempt because A2 did not pass.

### Certification state

Microphone hardware classification and self-test are implemented. USB AUDIO is a **VALID REAL MICROPHONE INPUT**, while G935 is `NO_SIGNAL`. Nevertheless, the final real STT certification failed because the three certification captures did not contain the requested utterance.

Final status: **IMPLEMENTED — VALID MICROPHONE FOUND — VOICE FOUNDATION NOT CERTIFIED**.

`MICROPHONE INPUT CERTIFIED — Linux`, `IMPLEMENTED + VOICE FOUNDATION CERTIFIED — Linux`, and `IMPLEMENTED + CONTINUOUS VOICE CERTIFIED — Linux` are not claimed. Continuous Voice Conversation was not started, in compliance with the gate. Wake word, always-listening, cloud speech, pseudo-tool-call parsing and AgentEngine changes remain absent.

## End-to-end audio integrity audit — 2026-10-09

### Capture paths and fix

The audit found that self-test and STT previously duplicated the `pw-record` setup. Both passed the selected Pulse/PipeWire source explicitly, but independent implementations made later drift possible. They now share one `start_audio_capture`/`Recording` path: selected `AudioDevice` → explicit `pw-record --target` → unique raw PCM16 WAV → in-memory read → stereo downmix → 16 kHz resample → existing energy VAD → exact selected WAV passed to `whisper-cli`. No Whisper, VAD, language, decoding, AgentEngine, tool-calling, or approval behavior was changed.

The current USB identity is:

- source: `alsa_input.usb-ME6_2866_721_MS_-3.7db_USB_AUDIO-00.mono-fallback`;
- hardware card: `alsa_card.usb-ME6_2866_721_MS_-3.7db_USB_AUDIO-00`;
- bus ID: `usb-ME6_2866_721_MS_-3.7db_USB_AUDIO-00`;
- ALSA path: `hw:0`;
- PipeWire node during this audit: `55` (node IDs are session-local and may change across PipeWire restarts);
- media class/factory: `Audio/Source`, `api.alsa.pcm.source`;
- profile/route: `mono-fallback`, `analog-input-mic`;
- native advertised format: s24le mono 48 kHz;
- locked GravityForge capture configuration: s16le stereo 48 kHz.

The same-session certificate stores device name, hardware identity, node, media class, profile, active route and the requested capture configuration. A real bug appeared during validation: PipeWire can change the source's advertised `sample_specification` after format negotiation, falsely invalidating the same device. The fingerprint now locks the actual GravityForge capture format instead of comparing that dynamic native description. A regression test covers this case.

### PipeWire graph

The active graph was inspected while the exact capture command was running. `pw-link -l` showed:

`alsa_input.usb-ME6_2866_721_MS_-3.7db_USB_AUDIO-00.mono-fallback:capture_MONO → pw-record:input_MONO`

`wpctl status` independently showed stream `pw-record` input connected to `USB AUDIO:capture_MONO` on source node 55. The separate desktop-output monitor was node 54 and had no link to the capture stream. This rules out a direct digital monitor/loopback route for the audited capture.

### Single-buffer integrity result

The diagnostic command records once for seven seconds, closes the writer, plays that exact raw WAV, then derives the resampled/VAD buffers and invokes Whisper without reopening any input. The user confirmed that playback contained their actual voice. The tested utterance was `Adesso proviamo il secondo test single buffer integrity test`; GravityForge returned `Adesso approviamo il secondo test single buffer integrity test`.

Session `22507-1791563264431170535` produced:

| Stage | Format/duration | PCM SHA-256 |
|---|---|---|
| raw capture | PCM16 stereo 48 kHz, 7.03 s | `23cf869d99e2379af15e0b03b8f1de26f367f0041b1debb5790fb08ea490c495` |
| post-resample | PCM16 mono 16 kHz, 7.03 s | `46ad303407d16613cc1f3a259382c4e1e059e33dd466bd185fa98f1bca2300b2` |
| exact Whisper/VAD input | PCM16 mono 16 kHz, 4.94 s | `160179a160ad8e4ad91a8740b616b3c779b9fcc9e1ef40b5045b6feed43d7da4` |

The files are unique per capture session. The capture writer is terminated and closed before any read; processed and transcript paths are also unique. Code inspection found no default-source open, autonomous Whisper capture, fixed `/tmp/voice.wav`, stale-file reuse, or input/output path collision.

### Direct Whisper comparison and root cause

The exact VAD WAV was invoked directly outside VoiceEngine with the same `ggml-small.bin`, `-l it`, and no translation. It returned exactly the GravityForge result: `Adesso approviamo il secondo test single buffer integrity test`. Direct invocation on the raw playback WAV produced the same result. Consequently:

- `DIRECT WHISPER RESULT` equals `GRAVITYFORGE WHISPER RESULT`;
- downmix, resampling, VAD selection, serialization and process construction do not account for the substitution;
- the capture is the approved physical USB microphone and not its desktop monitor;
- `proviamo` → `approviamo` is a small-model recognition error on correctly routed microphone audio. Microphone acoustics can affect intelligibility, but there is no evidence of source substitution in this run.

The required phrase gate has not yet produced a semantically correct same-buffer transcript, so the five-run `Questo è un test vocale di GravityForge` certification was not restarted. The strict outcome is **STT DECODING / WHISPER BLOCKER**. Voice Foundation remains **IMPLEMENTED — AUDIO PATH INTEGRITY VERIFIED — VOICE FOUNDATION NOT CERTIFIED**. Continuous Voice Conversation was not started.

## STT model accuracy evaluation — partial, 2026-10-09

The installed whisper.cpp revision explicitly supports multilingual `medium`, `large-v3`, and `large-v3-turbo`, including documented quantized variants. The existing build is CPU-only (`GGML_HIP=OFF`, `GGML_VULKAN=OFF`, `GGML_OPENCL=OFF`); this preserves a reliable CPU fallback on the current Ryzen 5 5600X. The host has 32 GiB RAM, 16 GiB RX 6900 XT VRAM, and 336 GiB free disk. GPU acceleration was not made mandatory and was not introduced during this evaluation.

Only one new model was downloaded: `ggml-medium.bin` (1.5 GiB). Its SHA-1 is `fd9727b6e1217c2f614f9b698455c4ffd82463b4`, matching the checksum documented by the bundled whisper.cpp model manifest. whisper.cpp is MIT licensed. `small` remains installed and was preserved as the baseline. Large-v3 and Large-v3-turbo were not downloaded because Medium must first demonstrate that its accuracy gain justifies the additional latency.

### Identical-buffer comparison

Every comparison below used the exact same per-utterance VAD WAV for both models, with forced `language=it`, translation disabled, and default decoding. No benchmark transcript was routed to AgentEngine or tools.

| Input / ground truth | Small | Medium | Result |
|---|---|---|---|
| La parmigiana di melanzane si fa con le melanzane e il pomodoro e la mozzarella | exact | exact | both pass |
| Proviamo una nuova operazione | exact | exact | both preserve `proviamo` |
| Questo è un test vocale di GravityForge | `testo vocale`, `Gravity Forge` | same | both fail strict wording; meaning mostly retained |
| Proviamo ad aprire Firefox | exact | exact | both pass |
| Apri Firefox | `Apri five folks` | exact | Medium fixes a command-critical Small error |
| Apri l'editor di testo | exact | exact | both pass |
| Scrivi GravityForge voice test | exact apart from spacing/quotes | exact apart from spacing/case | both pass |
| Fai uno screenshot | exact | `fai una screenshot` | both semantically pass |
| Ferma tutto | exact | `Fermatutto` | both semantically pass |
| Quanto fa due più due? | exact | `quanto fa 2+2?` | both semantically pass |
| Non approviamo questa operazione | exact | exact | both preserve negation and contrast |

On a 6.16 s real Italian VAD buffer, Small used 752 MiB and 4.00 s (RTF 0.65), while Medium used 2.02 GiB and 9.86 s (RTF 1.60). On the shorter command buffers Small used about 750 MiB and 2.20–2.89 s. The product's displayed integrity-test latency is larger because that diagnostic deliberately measures from speech end across playback of the same buffer and then transcription; direct model timings above exclude playback.

| Model | Italian Accuracy | Safety Phrases | English | Latency | RAM | Result |
|---|---|---|---|---:|---:|---|
| Small multilingual | 9/10 semantic on the controlled command set; failed `Apri Firefox` | `proviamo` and one negated approval phrase passed | previous 3/3 controlled only; new real regression pending | 4.00 s on 6.16 s input | 752 MiB | baseline retained; command-critical error |
| Medium multilingual | 10/10 semantic on the controlled command set; one strict `testo/test` wording error shared with Small | `proviamo` and one negated approval phrase passed | real regression pending | 9.86 s on 6.16 s input | 2.02 GiB | leading candidate, not yet selected/default |
| Large-v3 | supported, not downloaded | pending | pending | pending | documented model size 2.9 GiB | deferred |
| Large-v3-turbo | supported, not downloaded | pending | pending | pending | documented model size 1.5 GiB | deferred |

### Deferred certification work

At the user's request, repetitive live recording stopped after the initial command set and one negation/contrast pair. The full safety-sensitive pair set, negation set, Italian 5-run, real English regression, Voice Chat E2E, multi-turn, TTS/interrupt rerun, and cleanup certification remain pending. No model preset or product default was changed because the evidence is not yet sufficient. The locally persisted development selection is Medium solely to resume the benchmark later.

Current status remains **IMPLEMENTED — AUDIO PATH INTEGRITY VERIFIED — STT MODEL EVALUATION PARTIAL — VOICE FOUNDATION NOT CERTIFIED**. Continuous Voice Conversation was not started.

## Optional and modular Voice runtime — 2026-10-09

Voice is now an explicitly optional subsystem. A first installation and every application restart restore the master `Voice Features` state to OFF. With the master OFF, the backend runtime is `UNLOADED`, microphone certification is cleared, active capture/playback children are terminated, and neither STT nor TTS configuration is validated or started. Text chat, AgentEngine and Computer Control remain independent.

STT and TTS have separate switches. Master ON with STT OFF passes no STT configuration to the backend and rejects microphone/self-test/transcription commands; TTS may still be used. Master ON with TTS OFF passes no TTS configuration, rejects synthesis commands and retains Push-to-Talk/STT. Changing either module while Voice is active interrupts current audio work and reconfigures the runtime. Continuous Conversation and Wake Word are shown as OFF and remain unimplemented.

The runtime remains more aggressively lazy than a resident model server: `READY (on demand)` means paths have been validated, not that model weights are resident. `whisper-cli`, Piper and espeak are spawned only for an actual transcription or synthesis and are reaped afterward. Therefore enabling STT does not permanently add the measured 752 MiB (Small) or 2.02 GiB (Medium); that memory exists only while the per-utterance child runs. Disabling Voice terminates any such child and returns all modules to `UNLOADED`.

The UI exposes the selected STT model, measured/estimated memory and `UNLOADED` versus `READY (on demand)`. It offers only evidence-backed profiles: `LOW RESOURCE` (CPU-friendly, Push-to-Talk, lighter selected model recommended) and `BALANCED` (selected model, Push-to-Talk). No `ACCURATE` profile was added because Large-v3/Turbo are not certified. On a 16 GiB CPU-only system the recommendation is LOW RESOURCE; on the current 32 GiB host it is BALANCED. This is advisory and never blocks manual selection or non-Voice app use.

Acceptance audit with the debug build and Voice OFF found no `whisper-cli`, Piper, espeak, `pw-record`, or `pw-play` child. GravityForge idle RSS was approximately 181.7 MiB and no Voice CPU consumer existed. Since models are per-request children, the already measured transient model peaks are the load delta (Small ~752 MiB; Medium ~2.02 GiB); process exit/unload releases them entirely. Unit/build gates also confirm that text/agent/computer modules remain independent of Voice settings.

### Permanent Voice compute policy

The supported Voice compute path is CPU plus system RAM. GravityForge now passes `--no-gpu` to every `whisper-cli` invocation even if a user-selected whisper.cpp binary was compiled with CUDA, HIP/ROCm, Vulkan, Metal, or another GPU backend. The shipped/tested build additionally has `GGML_CUDA=OFF`, `GGML_HIP=OFF`, `GGML_VULKAN=OFF`, and `GGML_OPENCL=OFF`. Models therefore reside in system RAM and do not consume LLM VRAM. Resampling, preprocessing, energy VAD and postprocessing are native CPU code. Supported TTS engines (espeak-ng and Piper CLI) are invoked through their CPU paths and no GPU backend is selected.

The resource boundary is permanent: Local LLM Runtime owns its independently configured GPU/offload policy; VoiceEngine does not inspect or mutate LLM GPU layers, unload an LLM, restart its runtime, or reserve VRAM. Any future Voice GPU feature must be a separate advanced opt-in, OFF by default, and CPU-only operation must remain fully supported. No such opt-in is implemented now.

Future Wake Word work is constrained to a lightweight CPU detector. It must not keep Whisper loaded continuously. The permitted future flow is lightweight CPU wake detector → on-demand CPU STT → AgentEngine → independently configured LLM → CPU TTS. Continuous Conversation and Wake Word remain OFF and unimplemented.

The Voice settings UI states `Voice compute: CPU`, `STT: CPU / RAM`, `TTS: CPU / RAM`, and `Wake Word: CPU (future, OFF)`. Hardware recommendation deliberately reports `supportedGpu=false` for Voice because the certified runtime is CPU-only, even when the host has a discrete GPU.

A best-effort RX 6900 XT audit ran Medium with the explicit `--no-gpu` flag. `rocm-smi` reported 1,530,896,384 bytes used during the sample and 1,418,231,808 bytes immediately afterward (about 107 MiB transient difference amid an active desktop/build session), while the Whisper process itself peaked near 2.02 GiB system RAM and about 364% CPU. The Voice model-sized allocation was therefore in RAM, not VRAM; no ~2 GiB Voice allocation appeared on the GPU. GPU busy percentage remained a non-exclusive system-wide measurement (10% during, 6% after) and cannot be attributed to Voice. The Local LLM runtime configuration and allocation were not changed or restarted.

## Large-v3-Turbo evaluation — 2026-10-10

whisper.cpp `v1.9.4` explicitly supports the multilingual `large-v3-turbo` GGML model and the CLI's `--no-gpu` switch. The integrated build has CUDA, HIP/ROCm, Vulkan, OpenCL, CoreML and OpenVINO disabled. Only the F16 Turbo model was downloaded; Large-v3 full was not downloaded. `ggml-large-v3-turbo.bin` is 1,624,555,275 bytes and its SHA-1, `4af2b29d7ec73d781377bfd1758ca957a807e941`, exactly matches the bundled official model manifest. Small and Medium remain installed. whisper.cpp and the upstream Whisper release are MIT-licensed.

The USB AUDIO self-test immediately preceding the recordings passed with RMS 0.0212, peak 0.1016, 1.4% zero samples, 5029 ms duration and confirmed playback. Five distinct real recordings were retained as unique VAD WAVs and were decoded directly by both models with forced `language=it`, translation disabled and `--no-gpu`. No transcript entered AgentEngine or Computer Control.

| Run | Audio duration | Medium | Turbo | Strict result |
|---:|---:|---|---|---|
| 1 | 4.34 s | `Questo è un testo vocale di Gravity Forge.` | same | both FAIL: `testo` for `test` |
| 2 | 5.46 s | `Questo è un test vocale di Gravity Forge` | same | both PASS |
| 3 | 4.24 s | `Questo è un test per Gravity Forge` | same | both FAIL: omission/substitution |
| 4 | 4.66 s | `Questo è un testo vocale di Gravity Forge` | same | both FAIL: `testo` for `test` |
| 5 | 4.68 s | `Questo è un test vocale di Gravity Forge` | same | both PASS |

Turbo therefore scored **2/5**, below the required 4/5. It did not repair any Medium error on the identical audio. The supplied phase state records Medium's earlier UI evaluation as 3/5; the stricter reproducible same-buffer replay above is 2/5 for both models. Per the ordered gate, the Turbo safety/negation set, command set, English regression and Voice Foundation E2E were not started.

Across the five files, Medium averaged 8.28 s and Turbo 12.72 s. Mean audio duration was 4.68 s, giving approximate RTF 1.77 for Medium and 2.72 for Turbo. Medium peaked at about 2.02 GiB RSS and 354–380% CPU; Turbo peaked at 1.76–1.77 GiB RSS and 376–387% CPU. A separate Turbo timing run reported 293 ms model load, 12.24 s encode and 12.91 s total on the 5.46 s file.

Best-effort `rocm-smi` readings around that Turbo run were 2,553,561,088 bytes VRAM before, 2,556,055,552 during and 2,535,411,712 after. The roughly 2.5 MB during/before difference is desktop measurement noise, not a model-sized allocation; CPU/RAM-only execution is confirmed. After completion there was no `whisper-cli`, `pw-record`, `pw-play`, Piper or espeak process. The per-request child released its RAM.

No profile or default was changed. FAST remains Small and BALANCED remains Medium; ACCURATE/RECOMMENDED Turbo is not created because Turbo failed the accuracy gate and was slower. Voice Foundation remains **IMPLEMENTED — AUDIO PATH INTEGRITY VERIFIED — VOICE FOUNDATION NOT CERTIFIED**. Continuous Voice and Wake Word were not started. Tool calling and approval preservation were not reached and remain separate from this failed STT gate; AgentEngine was not modified.
