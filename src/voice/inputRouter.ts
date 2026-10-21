import { useAgentStore } from "../agent/agentStore";
import { useModelStore } from "../models/modelStore";

export const normalizeEmergencyCommand = (text: string) => text.trim().replace(/[.!?,;:]+$/u, "").toLocaleLowerCase();
export const isEmergencyCommand = (text: string) => ["stop", "fermati", "annulla", "cancel", "mute", "muto"].includes(normalizeEmergencyCommand(text));

function waitForModel(): Promise<string> {
  const current = useModelStore.getState();
  if (!current.generating) return Promise.resolve(current.messages.at(-1)?.content ?? "");
  return new Promise((resolve) => {
    const unsubscribe = useModelStore.subscribe((state) => {
      if (state.generating) return;
      unsubscribe(); resolve(state.messages.at(-1)?.role === "assistant" ? state.messages.at(-1)?.content ?? "" : "");
    });
  });
}

export async function dispatchVoiceText(text: string, onFirstToken?: () => void): Promise<string> {
  const agent = useAgentStore.getState();
  if (agent.mode !== "ASK") {
    await agent.start(text);
    return useAgentStore.getState().task?.finalResult ?? "";
  }
  let first = true;
  const unsubscribe = useModelStore.subscribe((state) => {
    const content = state.messages.at(-1)?.role === "assistant" ? state.messages.at(-1)?.content : "";
    if (first && content) { first = false; onFirstToken?.(); unsubscribe(); }
  });
  await useModelStore.getState().send(text);
  const result = await waitForModel(); unsubscribe(); return result;
}
