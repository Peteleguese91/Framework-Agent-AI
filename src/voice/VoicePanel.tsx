import { useState } from "react";
import { useComputerStore } from "../computer/computerStore";
import { useVoiceStore } from "./voiceStore";

export function VoicePanel() {
  const voice = useVoiceStore(); const computer = useComputerStore((state) => state.status); const [settingsOpen, setSettingsOpen] = useState(false);
  const active = voice.settings.enabled;
  return <section className={`voice-panel ${voice.status.phase === "LISTENING" ? "listening" : ""}`} aria-label="Voice Mode">
    <div className="voice-heading"><strong>VOICE</strong><span>{voice.status.phase}</span><button onClick={() => setSettingsOpen((value) => !value)} aria-label="Voice settings">⚙</button></div>
    <div className="voice-summary"><span>Microphone: {voice.devices.find((item) => item.id === voice.settings.inputDevice)?.name ?? "None"}</span><span>Computer: {computer?.authorized ? "ON" : "OFF"}</span></div>
    <div className="voice-level" aria-label={`Input level ${voice.inputLevel.level}`}><span>Input level: {voice.inputLevel.level}</span><div><i style={{ width: `${Math.min(100, voice.inputLevel.peak * 400)}%` }} /></div></div>
    {settingsOpen ? <VoiceSettings /> : null}
    {active ? <div className="voice-actions">
      <button className="push-to-talk" disabled={!voice.settings.sttEnabled || (voice.busy && voice.status.phase !== "LISTENING") || voice.selfTest?.result !== "PASS_VOICE_SIGNAL_DETECTED" || voice.selfTest.deviceId !== voice.settings.inputDevice} onPointerDown={(event) => { event.currentTarget.setPointerCapture(event.pointerId); void voice.beginPushToTalk(); }} onPointerUp={() => void voice.endPushToTalk()} onPointerCancel={() => void voice.endPushToTalk()}>Hold to Talk</button>
      {voice.status.phase === "SPEAKING" ? <button onClick={() => void voice.interruptSpeech()}>Stop speech</button> : null}
      <button className="danger-button" onClick={() => void voice.stopAgent()}>Stop all</button><button onClick={() => void voice.disable()}>Off</button>
    </div> : <button className="voice-enable" disabled={voice.busy} onClick={() => void voice.enable()}>Voice Features: OFF — Enable</button>}
    {voice.transcript ? <div className="voice-transcript"><b>YOU</b> {voice.transcript.text}<small>Language: {voice.transcript.detectedLanguage ?? voice.transcript.languageRequested} · model: {voice.transcript.modelName} · STT {voice.transcript.speechEndToTranscriptMs} ms</small><small>Input {voice.transcript.audio.inputSampleRate} Hz/{voice.transcript.audio.inputChannels} ch → {voice.transcript.audio.processedSampleRate} Hz mono · level {voice.transcript.audio.level} · RMS {voice.transcript.audio.rms.toFixed(4)} · peak {voice.transcript.audio.peak.toFixed(4)} · silence {voice.transcript.audio.silencePercent.toFixed(1)}%</small><small>Session {voice.transcript.captureSessionId} · RAW {voice.transcript.rawCaptureHash} · RESAMPLED {voice.transcript.postResampleHash} · WHISPER {voice.transcript.whisperInputHash}</small>{voice.transcript.diagnosticFiles.length ? <small>Diagnostic files: {voice.transcript.diagnosticFiles.join(" · ")}</small> : null}</div> : null}
    {voice.response ? <div className="voice-transcript"><b>ASSISTANT</b> {voice.response}</div> : null}
    {Object.keys(voice.metrics).length ? <small className="voice-metrics">STT {voice.metrics.speechEndToTranscriptMs ?? "—"} ms · LLM first {voice.metrics.transcriptToFirstTokenMs ?? "—"} ms · total {voice.metrics.totalResponseMs ?? "—"} ms</small> : null}
    {voice.status.error ? <p className="voice-error">{voice.status.error.code}: {voice.status.error.message}</p> : null}
  </section>;
}

function VoiceSettings() {
  const { settings, devices, hardware, updateSettings } = useVoiceStore();
  const testMicrophone = useVoiceStore((state) => state.testMicrophone); const captureIntegrityTest = useVoiceStore((state) => state.captureIntegrityTest); const selfTest = useVoiceStore((state) => state.selfTest); const busy = useVoiceStore((state) => state.busy); const [advanced, setAdvanced] = useState(false); const [playBack, setPlayBack] = useState(true);
  const inputs = devices.filter((device) => device.kind === "input" && (advanced || device.category === "Microphone"));
  const modelName = settings.sttModelPath.split("/").at(-1) || "Not configured"; const estimatedMemory = modelName.includes("medium") ? "~2.0 GiB measured" : modelName.includes("small") ? "~752 MiB measured" : modelName.includes("base") ? "~285 MiB measured" : "Unknown";
  return <div className="voice-settings">
    <label>Resource profile<select value={settings.resourceProfile} onChange={(e) => updateSettings({ resourceProfile: e.target.value as "LOW_RESOURCE" | "BALANCED" })}><option value="LOW_RESOURCE">LOW RESOURCE — CPU-friendly / Push-to-Talk</option><option value="BALANCED">BALANCED — selected model / Push-to-Talk</option></select></label>
    <small>System: {hardware ? `${hardware.totalMemoryGib.toFixed(1)} GiB RAM · ${hardware.cpuThreads} CPU threads · ${hardware.supportedGpu ? "supported Voice GPU" : "CPU Voice backend"}` : "detecting…"} · recommendation: {hardware?.recommendedProfile ?? "LOW_RESOURCE"}. Advisory only.</small>
    <small>Voice compute: CPU · STT: CPU / RAM · TTS: CPU / RAM · Wake Word: CPU (future, currently OFF). GPU remains reserved for the LLM runtime.</small>
    <label className="voice-check"><input type="checkbox" checked={settings.sttEnabled} onChange={(e) => updateSettings({ sttEnabled: e.target.checked })} />Voice Input / Speech-to-Text</label>
    <label className="voice-check"><input type="checkbox" checked={settings.ttsEnabled} onChange={(e) => updateSettings({ ttsEnabled: e.target.checked })} />Voice Output / Text-to-Speech</label>
    <small>STT model: {modelName} · memory: {estimatedMemory} · status: {settings.enabled && settings.sttEnabled ? "READY (on demand)" : "UNLOADED"}</small>
    <small>TTS status: {settings.enabled && settings.ttsEnabled ? "READY (on demand)" : "UNLOADED"} · Continuous Conversation: OFF · Wake Word: OFF</small>
    <fieldset disabled={!settings.sttEnabled}>
    <label>Input<select value={settings.inputDevice} onChange={(e) => updateSettings({ inputDevice: e.target.value })}>{inputs.map((d) => <option key={d.id} value={d.id}>{d.category}: {d.name}{d.isDefault ? " (default)" : ""}{!d.available ? " — unavailable" : ""}</option>)}</select></label>
    <label className="voice-check"><input type="checkbox" checked={advanced} onChange={(e) => setAdvanced(e.target.checked)} />Show monitor/virtual/unknown inputs</label>
    {devices.find((device) => device.id === settings.inputDevice) ? <small>{devices.find((device) => device.id === settings.inputDevice)?.sampleSpecification} · node {devices.find((device) => device.id === settings.inputDevice)?.nodeId ?? "?"} · profile {devices.find((device) => device.id === settings.inputDevice)?.profile} · gain {devices.find((device) => device.id === settings.inputDevice)?.volumePercent}%</small> : null}
    <div className="voice-self-test"><button disabled={busy || !settings.inputDevice} onClick={() => void testMicrophone(playBack)}>Test Microphone (5 s)</button><label className="voice-check"><input type="checkbox" checked={playBack} onChange={(e) => setPlayBack(e.target.checked)} />Play sample locally</label></div>
    {selfTest ? <small className={selfTest.result.startsWith("PASS") ? "voice-test-pass" : "voice-error"}>{selfTest.result.replaceAll("_", " ")} · RMS {selfTest.level.rms.toFixed(4)} · peak {selfTest.level.peak.toFixed(4)} · zero {selfTest.level.zeroPercent.toFixed(1)}% · {selfTest.durationMs} ms{selfTest.playbackCompleted ? " · playback confirmed" : ""}</small> : null}
    <button disabled={busy || selfTest?.result !== "PASS_VOICE_SIGNAL_DETECTED" || selfTest.deviceId !== settings.inputDevice} onClick={() => void captureIntegrityTest()}>Single-buffer integrity test (7 s)</button>
    <label>Output<select value={settings.outputDevice} onChange={(e) => updateSettings({ outputDevice: e.target.value })}>{devices.filter((d) => d.kind === "output").map((d) => <option key={d.id} value={d.id}>{d.name}{d.isDefault ? " (default)" : ""}</option>)}</select></label>
    <label>Language<select value={settings.language} onChange={(e) => updateSettings({ language: e.target.value as "auto" | "it" | "en" })}><option value="auto">Auto</option><option value="it">Italiano</option><option value="en">English</option></select></label>
    <label>STT model<input value={settings.sttModelPath} onChange={(e) => updateSettings({ sttModelPath: e.target.value })} placeholder="/path/ggml-base.bin" /></label>
    <small>Measured local profiles: tiny = fast/lower accuracy, base = balanced, small = higher accuracy and recommended when resources allow. Use multilingual models for Italian, never <code>.en</code>.</small>
    <label>STT runtime<input value={settings.sttExecutable} onChange={(e) => updateSettings({ sttExecutable: e.target.value })} /></label>
    <label>VAD threshold<input type="number" min="0.001" max="1" step="0.001" value={settings.vadThreshold} onChange={(e) => updateSettings({ vadThreshold: Number(e.target.value) })} /></label>
    <label>Silence timeout<input type="number" min="100" max="5000" value={settings.silenceTimeoutMs} onChange={(e) => updateSettings({ silenceTimeoutMs: Number(e.target.value) })} /></label>
    <label className="voice-check"><input type="checkbox" checked={settings.useVad} onChange={(e) => updateSettings({ useVad: e.target.checked })} />Use VAD segment</label>
    <label className="voice-check"><input type="checkbox" checked={settings.diagnosticMode} onChange={(e) => updateSettings({ diagnosticMode: e.target.checked })} />Preserve diagnostic audio</label>
    {settings.diagnosticMode ? <label>Diagnostic directory<input value={settings.diagnosticDirectory} onChange={(e) => updateSettings({ diagnosticDirectory: e.target.value })} placeholder="Existing local directory" /></label> : null}
    </fieldset>
    <fieldset disabled={!settings.ttsEnabled}>
    <label>TTS engine<select value={settings.ttsEngine} onChange={(e) => updateSettings({ ttsEngine: e.target.value as "piper" | "espeak", ttsExecutable: e.target.value === "piper" ? "piper" : "espeak-ng" })}><option value="espeak">espeak-ng</option><option value="piper">Piper</option></select></label>
    <label>TTS voice<input value={settings.ttsVoice} onChange={(e) => updateSettings({ ttsVoice: e.target.value })} placeholder={settings.language === "it" ? "it" : "en"} /></label>
    {settings.ttsEngine === "piper" ? <><label>Piper model<input value={settings.ttsModelPath} onChange={(e) => updateSettings({ ttsModelPath: e.target.value })} /></label><label>Piper config<input value={settings.ttsConfigPath} onChange={(e) => updateSettings({ ttsConfigPath: e.target.value })} /></label></> : null}
    <label>Speech rate<input type="number" min="0.5" max="2" step="0.1" value={settings.speechRate} onChange={(e) => updateSettings({ speechRate: Number(e.target.value) })} /></label>
    <label>PTT shortcut<input value={settings.shortcut} onChange={(e) => updateSettings({ shortcut: e.target.value })} placeholder="Ctrl+Space (optional)" /></label>
    <label className="voice-check"><input type="checkbox" checked={settings.autoSpeak} onChange={(e) => updateSettings({ autoSpeak: e.target.checked })} />Auto speak</label>
    </fieldset>
    <small>Push to Talk only. Diagnostic audio persistence is OFF by default. No wake word or background listening.</small>
  </div>;
}
