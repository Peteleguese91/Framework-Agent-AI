import { invoke } from "@tauri-apps/api/core";

export interface ComputerStatus { enabled: boolean; backend: string; authorized: boolean; displayCount: number; windowControl: string; screenshotRequiresConsent: boolean }
export type ComputerRequest = Record<string, unknown> & { operation: string };
export interface ComputerAuditEntry { timestampMs: number; operation: string; arguments: unknown; risk: string; approved: boolean; outcome: string }

export const computerApi = {
  status: () => invoke<ComputerStatus>("computer_status"),
  enable: () => invoke<ComputerStatus>("computer_enable"),
  disable: () => invoke<void>("computer_disable"),
  stop: () => invoke<void>("computer_stop"),
  execute: (request: ComputerRequest, approved: boolean) => invoke<unknown>("computer_execute", { request, actor: "AGENT", approved }),
  auditLog: () => invoke<ComputerAuditEntry[]>("computer_audit_log"),
};
