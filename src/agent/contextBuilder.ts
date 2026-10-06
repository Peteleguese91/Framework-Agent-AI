import type { WorkspaceInfo } from "../project/types";
import type { ChatMessage } from "../models/types";
import { AGENT_SYSTEM_PROMPT } from "./systemPrompt";
import type { AgentTask } from "./types";

export class ContextBuilder {
  build(task: AgentTask, workspace: WorkspaceInfo): ChatMessage[] {
    const workspaceContext = {
      name: workspace.name,
      project: workspace.projectInfo,
      map: workspace.projectMap,
      plan: task.plan,
      modifiedFiles: task.modifiedFiles,
      recentErrors: task.errors.slice(-5),
    };
    return [
      { role: "system", content: AGENT_SYSTEM_PROMPT, toolCalls: [] },
      { role: "system", content: `Workspace context:\n${JSON.stringify(workspaceContext)}`, toolCalls: [] },
      { role: "user", content: task.goal, toolCalls: [] },
    ];
  }

  compact(messages: ChatMessage[]): ChatMessage[] {
    const fixed = messages.filter((message, index) => index < 3 || message.role === "system");
    const recent = messages.slice(-8);
    return [...fixed, { role: "system", content: "Older tool observations were removed after context overflow. Continue from the task summary and recent evidence.", toolCalls: [] }, ...recent];
  }
}
