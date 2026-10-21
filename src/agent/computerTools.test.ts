import { describe, expect, it, vi } from "vitest";
import { ToolRegistry } from "./toolRegistry";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("Computer Control tool policy", () => {
  const registry = new ToolRegistry();
  it("registers the complete bounded tool surface", () => {
    expect(registry.list().filter((tool) => tool.name.startsWith("computer.")).map((tool) => tool.name)).toHaveLength(14);
  });
  it("classifies perception as safe and close as dangerous", () => {
    expect(registry.validate("computer.get_displays", {}).risk).toBe("SAFE");
    expect(registry.validate("computer.close_window", { window_id: "0_id" }).risk).toBe("DANGEROUS");
  });
  it("raises risk for destructive hotkeys and sensitive typing", () => {
    expect(registry.validate("computer.hotkey", { keys: ["Alt", "F4"] }).risk).toBe("DANGEROUS");
    expect(registry.validate("computer.type_text", { text: "password=abc" }).risk).toBe("DANGEROUS");
    expect(registry.validate("computer.type_text", { text: "hello" }).risk).toBe("MODERATE");
  });
  it("rejects malformed coordinates and commands", () => {
    expect(() => registry.validate("computer.click", { x: 1 })).toThrow();
    expect(() => registry.validate("computer.open_app", { application: "kate;rm" })).toThrow();
  });
});
