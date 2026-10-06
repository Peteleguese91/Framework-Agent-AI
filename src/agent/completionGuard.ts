import type { AgentTask } from "./types";

export interface CompletionDecision { complete: boolean; reasons: string[] }

export function isVerificationCommand(command: string, args: string[]): boolean {
  const name = command.replaceAll("\\", "/").split("/").at(-1)?.toLowerCase().replace(/\.(exe|cmd|bat)$/u, "");
  const first = args[0]?.toLowerCase();
  return (name === "npm" && (first === "test" || first === "run"))
    || (name === "cargo" && ["test", "check", "clippy", "build"].includes(first ?? ""))
    || ["pytest", "vitest", "jest", "eslint", "tsc"].includes(name ?? "");
}

export class CompletionGuard {
  evaluate(task: AgentTask): CompletionDecision {
    const reasons: string[] = [];
    if (task.modifiedFiles.length > 0 && task.verificationStatus !== "PASSED") reasons.push("Modified files require successful verification");
    if (task.steps.some((step) => step.status === "FAILED" || step.status === "BLOCKED")) reasons.push("A required plan step remains unresolved");
    const lastObservation = task.observations.at(-1);
    if (lastObservation?.status === "error") reasons.push("The most recent tool failure is unresolved");
    return { complete: reasons.length === 0, reasons };
  }
}
