import { create } from "zustand";
import { useAgentStore } from "../agent/agentStore";
import { useModelStore } from "../models/modelStore";
import { useComputerStore } from "../computer/computerStore";
import { dispatchVoiceText, isEmergencyCommand } from "./inputRouter";
import { voiceApi } from "./voiceApi";
import { DEFAULT_VOICE_SETTINGS, type AudioDevice, type InputLevel, type MicrophoneSelfTest, type Transcript, type VoiceHardwareRecommendation, type VoiceMetrics, type VoiceSettings, type VoiceStatus } from "./types";

const KEY = "gravityforge.voice.v1";
const idle: VoiceStatus = { phase: "OFF", microphoneActive: false, playbackActive: false, error: null };
const errorValue = (error: unknown) => typeof error === "object" && error && "code" in error ? error as VoiceStatus["error"] : { code: "STT_FAILED" as const, message: error instanceof Error ? error.message : String(error) };
export function loadVoiceSettings(storage: Pick<Storage, "getItem">): VoiceSettings {
  try { const value = JSON.parse(storage.getItem(KEY) ?? "null") as Partial<VoiceSettings> | null; return value ? { ...DEFAULT_VOICE_SETTINGS, ...value, enabled: false } : DEFAULT_VOICE_SETTINGS; }
  catch { return DEFAULT_VOICE_SETTINGS; }
}
interface VoiceStore {
  status: VoiceStatus; settings: VoiceSettings; devices: AudioDevice[]; hardware: VoiceHardwareRecommendation | null; transcript: Transcript | null; response: string; metrics: VoiceMetrics; busy: boolean; inputLevel: InputLevel; selfTest: MicrophoneSelfTest | null;
  initialize: () => Promise<void>; updateSettings: (patch: Partial<VoiceSettings>) => void; enable: () => Promise<void>; disable: () => Promise<void>;
  testMicrophone: (playBack: boolean) => Promise<void>; captureIntegrityTest: () => Promise<void>; beginPushToTalk: () => Promise<void>; endPushToTalk: () => Promise<void>; interruptSpeech: () => Promise<void>; stopAgent: () => Promise<void>;
}
export const useVoiceStore = create<VoiceStore>((set, get) => ({
  status: idle, settings: DEFAULT_VOICE_SETTINGS, devices: [], hardware: null, transcript: null, response: "", metrics: {}, busy: false, inputLevel: { rms: 0, peak: 0, zeroPercent: 100, level: "NO_SIGNAL" }, selfTest: null,
  initialize: async () => {
    const settings = loadVoiceSettings(localStorage);
    try { const [status, devices, hardware] = await Promise.all([voiceApi.status(), voiceApi.devices(), voiceApi.hardwareRecommendation()]); const microphones = devices.filter((item) => item.kind === "input" && item.category === "Microphone" && item.available); const inputDevice = (microphones.some((item) => item.id === settings.inputDevice) ? settings.inputDevice : "") || microphones.find((item) => item.isDefault)?.id || microphones[0]?.id || ""; const outputDevice = settings.outputDevice || devices.find((item) => item.kind === "output" && item.isDefault)?.id || ""; set({ settings: { ...settings, inputDevice, outputDevice }, status, devices, hardware }); await voiceApi.onInputLevel((inputLevel) => set({ inputLevel })); }
    catch (error) { set({ status: { ...idle, phase: "FAILED", error: errorValue(error) } }); }
  },
  updateSettings: (patch) => { const previous = get().settings; const settings = { ...previous, ...patch }; localStorage.setItem(KEY, JSON.stringify({ ...settings, enabled: false })); set({ settings, ...((patch.inputDevice !== undefined || patch.sttEnabled === false) ? { selfTest: null, inputLevel: { rms: 0, peak: 0, zeroPercent: 100, level: "NO_SIGNAL" as const } } : {}) }); if (previous.enabled && (patch.sttEnabled !== undefined || patch.ttsEnabled !== undefined)) { void voiceApi.interrupt().then(() => voiceApi.loadRuntime(settings)).catch((error) => set({ status: { ...idle, phase: "FAILED", error: errorValue(error) } })); } },
  enable: async () => { set({ busy: true }); try { await voiceApi.loadRuntime(get().settings); const status = await voiceApi.enable(); get().updateSettings({ enabled: true }); set({ status }); } catch (error) { set({ status: { ...idle, phase: "FAILED", error: errorValue(error) } }); } finally { set({ busy: false }); } },
  disable: async () => { set({ busy: true }); try { set({ status: await voiceApi.disable(), transcript: null, response: "" }); await voiceApi.unloadRuntime(); get().updateSettings({ enabled: false }); } finally { set({ busy: false }); } },
  testMicrophone: async (playBack) => { set({ busy: true, selfTest: null, inputLevel: { rms: 0, peak: 0, zeroPercent: 100, level: "NO_SIGNAL" } }); try { const selfTest = await voiceApi.microphoneSelfTest(get().settings.inputDevice, get().settings.outputDevice, playBack); set({ selfTest, inputLevel: selfTest.level, status: { ...idle, phase: selfTest.result.startsWith("PASS") ? "IDLE" : "FAILED", error: selfTest.result.startsWith("PASS") ? null : { code: selfTest.result === "FAIL_DEVICE_UNAVAILABLE" ? "DEVICE_DISCONNECTED" : "MIC_SELF_TEST_REQUIRED", message: selfTest.result.replaceAll("_", " ") } } }); } catch (error) { set({ status: { ...idle, phase: "FAILED", error: errorValue(error) } }); } finally { set({ busy: false }); } },
  captureIntegrityTest: async () => { set({ busy: true, transcript: null }); try { const transcript = await voiceApi.captureIntegrityTest(get().settings); set({ transcript, status: { ...idle, phase: "IDLE" }, metrics: { speechEndToTranscriptMs: transcript.speechEndToTranscriptMs } }); } catch (error) { set({ status: { ...idle, phase: "FAILED", error: errorValue(error) } }); } finally { set({ busy: false }); } },
  beginPushToTalk: async () => {
    if (!get().settings.enabled || !get().settings.sttEnabled || get().status.phase === "LISTENING") return;
    await voiceApi.interrupt(); set({ response: "", transcript: null, metrics: {} });
    try { set({ status: await voiceApi.startListening(get().settings.inputDevice) }); } catch (error) { set({ status: { ...idle, phase: "FAILED", error: errorValue(error) } }); }
  },
  endPushToTalk: async () => {
    if (get().status.phase !== "LISTENING") return;
    set((state) => ({ status: { ...state.status, phase: "TRANSCRIBING", microphoneActive: false }, busy: true }));
    const totalStarted = performance.now();
    try {
      const transcript = await voiceApi.stopAndTranscribe(get().settings); set({ transcript, metrics: { speechEndToTranscriptMs: transcript.speechEndToTranscriptMs } });
      if (isEmergencyCommand(transcript.text)) { await get().stopAgent(); return; }
      const dispatchedAt = performance.now(); let firstTokenAt: number | undefined;
      const response = await dispatchVoiceText(transcript.text, () => { firstTokenAt = performance.now(); set((state) => ({ metrics: { ...state.metrics, transcriptToFirstTokenMs: Math.round(firstTokenAt! - dispatchedAt) } })); });
      set((state) => ({ response, status: { ...state.status, phase: "IDLE" }, metrics: { ...state.metrics, transcriptToFirstTokenMs: state.metrics.transcriptToFirstTokenMs ?? Math.round(performance.now() - dispatchedAt), totalResponseMs: Math.round(performance.now() - totalStarted) } }));
      if (response && get().settings.ttsEnabled && get().settings.autoSpeak) { const started = performance.now(); set((state) => ({ status: { ...state.status, phase: "SPEAKING", playbackActive: true }, metrics: { ...state.metrics, firstTtsAudioMs: Math.round(started - dispatchedAt) } })); try { set({ status: await voiceApi.speak(response, get().settings) }); } catch (error) { if ((errorValue(error)?.code ?? "") !== "VOICE_INTERRUPTED") set({ status: { ...idle, phase: "FAILED", error: errorValue(error) } }); } }
    } catch (error) { set({ status: { ...idle, phase: "FAILED", error: errorValue(error) } }); }
    finally { set({ busy: false }); }
  },
  interruptSpeech: async () => { set({ status: await voiceApi.interrupt() }); },
  stopAgent: async () => { await voiceApi.interrupt(); if (useAgentStore.getState().mode === "ASK") await useModelStore.getState().cancel(); else await useAgentStore.getState().stop(); await useComputerStore.getState().stop(); set({ status: { ...idle, phase: "INTERRUPTED" }, busy: false }); },
}));
