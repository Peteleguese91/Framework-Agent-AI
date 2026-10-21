import { beforeEach, describe, expect, it, vi } from "vitest";

const send = vi.fn(); const start = vi.fn();
let agent = { mode: "ASK", start, task: null as { finalResult?: string } | null };
let model = { generating: false, messages: [{ role: "assistant", content: "four" }], send };
vi.mock("../agent/agentStore", () => ({ useAgentStore: { getState: () => agent } }));
vi.mock("../models/modelStore", () => ({ useModelStore: { getState: () => model, subscribe: vi.fn(() => () => undefined) } }));

import { dispatchVoiceText, isEmergencyCommand } from "./inputRouter";

describe("voice input transport", () => {
  beforeEach(() => { send.mockReset().mockResolvedValue(undefined); start.mockReset().mockImplementation(() => { agent.task = { finalResult: "done" }; return Promise.resolve(); }); agent = { mode: "ASK", start, task: null }; model = { generating: false, messages: [{ role: "assistant", content: "four" }], send }; });
  it("uses the exact typed-chat path in ASK mode", async () => { expect(await dispatchVoiceText("Quanto fa due più due?")).toBe("four"); expect(send).toHaveBeenCalledWith("Quanto fa due più due?"); expect(start).not.toHaveBeenCalled(); });
  it("uses the exact AgentEngine store path in agent mode", async () => { agent.mode = "AGENT"; expect(await dispatchVoiceText("Apri Firefox")).toBe("done"); expect(start).toHaveBeenCalledWith("Apri Firefox"); expect(send).not.toHaveBeenCalled(); });
  it("recognizes only local stop commands", () => { expect(isEmergencyCommand("Fermati!")).toBe(true); expect(isEmergencyCommand("Apri Firefox")).toBe(false); });
});
