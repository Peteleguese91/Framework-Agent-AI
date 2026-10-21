import { describe, expect, it } from "vitest";
import { matchesVoiceShortcut } from "./shortcut";
describe("push-to-talk shortcut", () => {
  const event = { key: " ", code: "Space", ctrlKey: true, altKey: false, shiftKey: false, metaKey: false };
  it("matches an explicitly configured in-app chord", () => expect(matchesVoiceShortcut(event, "Ctrl+Space")).toBe(true));
  it("is disabled by default and rejects missing modifiers", () => { expect(matchesVoiceShortcut(event, "")).toBe(false); expect(matchesVoiceShortcut({ ...event, ctrlKey: false }, "Ctrl+Space")).toBe(false); });
});
