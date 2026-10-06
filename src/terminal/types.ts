export type TerminalStatus = "starting" | "running" | "exited" | "failed";

export interface TerminalInfo {
  id: string;
  shell: string;
  cwd: string;
  pid?: number;
  createdAt: number;
  status: TerminalStatus;
  exitCode?: number;
}

export interface ExecRequest {
  command: string;
  args: string[];
  cwd?: string;
  timeoutMs: number;
  env: Record<string, string>;
  actor: "USER" | "AGENT";
  approvalMode?: "AGENT" | "AUTONOMOUS";
  approved?: boolean;
}

export interface ExecResult {
  stdout: string;
  stderr: string;
  exitCode?: number;
  success: boolean;
  durationMs: number;
  timedOut: boolean;
}
