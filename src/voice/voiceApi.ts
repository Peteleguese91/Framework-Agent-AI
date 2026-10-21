import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AudioDevice, InputLevel, MicrophoneSelfTest, Transcript, VoiceHardwareRecommendation, VoiceSettings, VoiceStatus } from "./types";

export const voiceApi = {
  status: () => invoke<VoiceStatus>("voice_status"), devices: () => invoke<AudioDevice[]>("voice_devices"),
  hardwareRecommendation: () => invoke<VoiceHardwareRecommendation>("voice_hardware_recommendation"),
  enable: () => invoke<VoiceStatus>("voice_enable"), disable: () => invoke<VoiceStatus>("voice_disable"),
  startListening: (inputDevice: string) => invoke<VoiceStatus>("voice_start_listening", { inputDevice }),
  microphoneSelfTest: (inputDevice: string, outputDevice: string, playBack: boolean) => invoke<MicrophoneSelfTest>("voice_microphone_self_test", { inputDevice, outputDevice, playBack }),
  onInputLevel: (callback: (level: InputLevel) => void) => listen<InputLevel>("voice-input-level", (event) => callback(event.payload)),
  stopAndTranscribe: (settings: VoiceSettings) => invoke<Transcript>("voice_stop_and_transcribe", { config: {
    executable: settings.sttExecutable, modelPath: settings.sttModelPath, language: settings.language,
    vadThreshold: settings.vadThreshold, minimumSpeechMs: settings.minimumSpeechMs, silenceTimeoutMs: settings.silenceTimeoutMs, diagnosticMode: settings.diagnosticMode, diagnosticDirectory: settings.diagnosticDirectory, useVad: settings.useVad,
  } }),
  captureIntegrityTest: (settings: VoiceSettings) => invoke<Transcript>("voice_capture_integrity_test", { inputDevice: settings.inputDevice, outputDevice: settings.outputDevice, config: {
    executable: settings.sttExecutable, modelPath: settings.sttModelPath, language: settings.language,
    vadThreshold: settings.vadThreshold, minimumSpeechMs: settings.minimumSpeechMs, silenceTimeoutMs: settings.silenceTimeoutMs, diagnosticMode: settings.diagnosticMode, diagnosticDirectory: settings.diagnosticDirectory, useVad: settings.useVad,
  } }),
  speak: (text: string, settings: VoiceSettings) => invoke<VoiceStatus>("voice_speak", { text, config: {
    engine: settings.ttsEngine, executable: settings.ttsExecutable, modelPath: settings.ttsModelPath,
    configPath: settings.ttsConfigPath, voice: settings.ttsVoice, language: settings.language === "it" ? "it" : "en",
    outputDevice: settings.outputDevice, speechRate: settings.speechRate, volume: settings.volume,
  } }),
  interrupt: () => invoke<VoiceStatus>("voice_interrupt"),
  loadRuntime: (settings: VoiceSettings) => invoke("voice_runtime_load", { stt: settings.sttEnabled ? { executable: settings.sttExecutable, modelPath: settings.sttModelPath, language: settings.language, vadThreshold: settings.vadThreshold, minimumSpeechMs: settings.minimumSpeechMs, silenceTimeoutMs: settings.silenceTimeoutMs, diagnosticMode: settings.diagnosticMode, diagnosticDirectory: settings.diagnosticDirectory, useVad: settings.useVad } : null, tts: settings.ttsEnabled ? { engine: settings.ttsEngine, executable: settings.ttsExecutable, modelPath: settings.ttsModelPath, configPath: settings.ttsConfigPath, voice: settings.ttsVoice, language: settings.language === "it" ? "it" : "en", outputDevice: settings.outputDevice, speechRate: settings.speechRate, volume: settings.volume } : null }),
  unloadRuntime: () => invoke("voice_runtime_unload"),
};
