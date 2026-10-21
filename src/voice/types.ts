export type VoicePhase = "OFF" | "IDLE" | "LISTENING" | "TRANSCRIBING" | "THINKING" | "SPEAKING" | "INTERRUPTED" | "FAILED";
export interface VoiceError { code: "NO_MICROPHONE" | "MIC_PERMISSION_DENIED" | "STT_MODEL_MISSING" | "STT_FAILED" | "TTS_MODEL_MISSING" | "TTS_FAILED" | "AUDIO_OUTPUT_FAILED" | "DEVICE_DISCONNECTED" | "VOICE_INTERRUPTED" | "INVALID_CONFIG" | "NO_SPEECH" | "MIC_SELF_TEST_REQUIRED"; message: string }
export interface VoiceStatus { phase: VoicePhase; microphoneActive: boolean; playbackActive: boolean; error: VoiceError | null }
export type AudioDeviceCategory = "Microphone" | "Monitor" | "Virtual" | "Unknown" | "Output";
export interface AudioDevice { id: string; name: string; isDefault: boolean; kind: "input" | "output"; category: AudioDeviceCategory; nodeId?: number; mediaClass: string; sampleSpecification: string; profile: string; activePort: string; muted: boolean; volumePercent: number; available: boolean; hardwareId: string }
export interface DeviceFingerprint { deviceId: string; hardwareId: string; nodeId?: number; mediaClass: string; sampleSpecification: string; profile: string; activePort: string }
export interface InputLevel { rms: number; peak: number; zeroPercent: number; level: "NO_SIGNAL" | "LOW" | "OK" | "CLIPPING" }
export interface MicrophoneSelfTest { result: "PASS_VOICE_SIGNAL_DETECTED" | "FAIL_NO_SIGNAL" | "FAIL_DEVICE_UNAVAILABLE" | "FAIL_LIKELY_MONITOR_LOOPBACK"; deviceId: string; category: AudioDeviceCategory; durationMs: number; sampleRate: number; channels: number; level: InputLevel; playbackCompleted: boolean }
export interface AudioDiagnostics {
  inputSampleRate: number; inputChannels: number; inputBitsPerSample: number; processedSampleRate: number; processedChannels: number;
  rawDurationMs: number; processedDurationMs: number; vadStartMs?: number; vadEndMs?: number; vadDurationMs: number;
  rms: number; peak: number; clippedPercent: number; silencePercent: number; level: "LOW" | "OK" | "CLIPPING";
}
export interface Transcript { text: string; detectedLanguage?: string; durationMs: number; speechMs: number; speechEndToTranscriptMs: number; languageRequested: string; modelName: string; audio: AudioDiagnostics; diagnosticFiles: string[]; whisperWarnings: string[]; captureSessionId: string; deviceFingerprint: DeviceFingerprint; rawCaptureHash: string; postResampleHash: string; whisperInputHash: string }
export interface VoiceMetrics { speechEndToTranscriptMs?: number; transcriptToFirstTokenMs?: number; firstTtsAudioMs?: number; totalResponseMs?: number }
export interface VoiceHardwareRecommendation { totalMemoryGib: number; cpuThreads: number; supportedGpu: boolean; recommendedProfile: "LOW_RESOURCE" | "BALANCED" }
export interface VoiceSettings {
  enabled: boolean; sttEnabled: boolean; ttsEnabled: boolean; resourceProfile: "LOW_RESOURCE" | "BALANCED"; inputDevice: string; outputDevice: string; language: "auto" | "it" | "en";
  sttExecutable: string; sttModelPath: string; vadThreshold: number; minimumSpeechMs: number; silenceTimeoutMs: number;
  diagnosticMode: boolean; diagnosticDirectory: string; useVad: boolean;
  ttsEngine: "piper" | "espeak"; ttsExecutable: string; ttsModelPath: string; ttsConfigPath: string;
  ttsVoice: string; speechRate: number; volume: number; autoSpeak: boolean; shortcut: string;
}
export const DEFAULT_VOICE_SETTINGS: VoiceSettings = {
  enabled: false, sttEnabled: true, ttsEnabled: true, resourceProfile: "LOW_RESOURCE", inputDevice: "", outputDevice: "", language: "auto", sttExecutable: "whisper-cli", sttModelPath: "",
  vadThreshold: 0.012, minimumSpeechMs: 250, silenceTimeoutMs: 900, diagnosticMode: false, diagnosticDirectory: "", useVad: true, ttsEngine: "espeak", ttsExecutable: "espeak-ng",
  ttsModelPath: "", ttsConfigPath: "", ttsVoice: "", speechRate: 1, volume: 1, autoSpeak: true, shortcut: "",
};
