import { describe, expect, it } from "vitest";
import { DEFAULT_VOICE_SETTINGS } from "./types";
import { loadVoiceSettings } from "./voiceStore";

describe("voice privacy settings", () => {
  it("always restores with the microphone disabled", () => {
    const stored = JSON.stringify({ ...DEFAULT_VOICE_SETTINGS, enabled: true, inputDevice: "mic" });
    expect(loadVoiceSettings({ getItem: () => stored })).toMatchObject({ enabled: false, inputDevice: "mic" });
  });
  it("falls back safely on malformed settings", () => expect(loadVoiceSettings({ getItem: () => "{" }).enabled).toBe(false));
  it("keeps voice optional while exposing independent modules", () => {
    const settings = loadVoiceSettings({ getItem: () => null });
    expect(settings).toMatchObject({ enabled: false, sttEnabled: true, ttsEnabled: true, resourceProfile: "LOW_RESOURCE" });
  });
});
