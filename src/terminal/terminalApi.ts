import { invoke } from "@tauri-apps/api/core";
import type { TerminalInfo, ExecRequest, ExecResult } from "./types";

export const terminalApi = {
  create: (workspaceRoot: string, shell?: string, cwd?: string) =>
    invoke<TerminalInfo>("terminal_create", { workspaceRoot, actor: "USER", shell, cwd }),
  write: (id: string, data: Uint8Array) =>
    invoke<void>("terminal_write", { id, data: Array.from(data) }),
  resize: (id: string, cols: number, rows: number) =>
    invoke<void>("terminal_resize", { id, cols, rows }),
  interrupt: (id: string) => invoke<void>("terminal_interrupt", { id }),
  kill: (id: string) => invoke<void>("terminal_kill", { id }),
  close: (id: string) => invoke<void>("terminal_close", { id }),
  status: (id: string) => invoke<TerminalInfo>("terminal_status", { id }),
  exec: (request: ExecRequest) => invoke<ExecResult>("terminal_exec", { request }),
};

