import type { AgentMode, AgentRisk, ApprovalDecision, ApprovalRequest } from "./types";

export type ApprovalListener = (request: ApprovalRequest) => void;

export class ApprovalManager {
  private pending = new Map<string, (decision: ApprovalDecision) => void>();
  constructor(private readonly notify: ApprovalListener) {}

  requiresApproval(mode: Exclude<AgentMode, "ASK">, risk: AgentRisk): boolean {
    if (risk === "DANGEROUS") return true;
    return mode === "AGENT" ? false : false;
  }

  async authorize(mode: Exclude<AgentMode, "ASK">, request: ApprovalRequest): Promise<boolean> {
    if (!this.requiresApproval(mode, request.risk)) return true;
    this.notify(request);
    return new Promise<boolean>((resolve) => {
      this.pending.set(request.id, (decision) => resolve(decision === "APPROVE_ONCE"));
    });
  }

  resolve(id: string, decision: ApprovalDecision): void {
    const pending = this.pending.get(id);
    if (!pending) return;
    this.pending.delete(id);
    pending(decision);
  }

  rejectAll(): void {
    for (const pending of this.pending.values()) pending("REJECT");
    this.pending.clear();
  }
}
