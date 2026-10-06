import type { AgentRisk } from "./types";

const SAFE = new Set(["pwd", "ls", "dir", "cat", "type", "rg", "grep", "findstr", "where", "which"]);
const DANGEROUS = new Set(["rm", "rmdir", "del", "erase", "sudo", "shutdown", "reboot", "mkfs", "diskpart", "format", "chmod", "chown"]);
const FORMATTERS = new Set(["prettier", "rustfmt", "black", "ruff"]);

function executable(value: string): string {
  return value.replaceAll("\\", "/").split("/").at(-1)?.toLowerCase().replace(/\.(exe|cmd|bat)$/u, "") ?? "";
}
function hasOutsidePath(args: string[]): boolean {
  return args.some((arg) => /(^|[\\/])\.\.([\\/]|$)/u.test(arg) || /^([a-zA-Z]:[\\/]|[\\/]{1,2}|~[\\/])/u.test(arg));
}

export function analyzeCommandRisk(command: string, args: string[]): { risk: AgentRisk; reason: string; outsideWorkspace: boolean } {
  const name = executable(command);
  const lowerArgs = args.map((arg) => arg.toLowerCase());
  const outsideWorkspace = hasOutsidePath(args);
  if (outsideWorkspace) return { risk: "DANGEROUS", reason: "Command contains an explicit path outside the workspace", outsideWorkspace };
  if (DANGEROUS.has(name)) return { risk: "DANGEROUS", reason: `${name} can remove or alter system data`, outsideWorkspace };
  if (name === "git" && ["reset", "clean"].includes(lowerArgs[0] ?? "")) return { risk: "DANGEROUS", reason: "Destructive Git command", outsideWorkspace };
  if ((name === "npm" && lowerArgs[0] === "install") || (name === "cargo" && lowerArgs[0] === "add") || (name === "git" && lowerArgs[0] === "add") || FORMATTERS.has(name)) {
    return { risk: "MODERATE", reason: "Command can modify workspace files or dependencies", outsideWorkspace };
  }
  if (SAFE.has(name)) return { risk: "SAFE", reason: "Read-only inspection command", outsideWorkspace };
  if (name === "git" && ["status", "diff", "log", "show"].includes(lowerArgs[0] ?? "")) return { risk: "SAFE", reason: "Read-only Git command", outsideWorkspace };
  if (name === "npm" && (lowerArgs[0] === "test" || lowerArgs[0] === "run")) return { risk: "SAFE", reason: "Project verification command", outsideWorkspace };
  if (name === "cargo" && ["test", "check", "clippy", "build"].includes(lowerArgs[0] ?? "")) return { risk: "SAFE", reason: "Rust verification command", outsideWorkspace };
  return { risk: "MODERATE", reason: "Unknown command is classified conservatively", outsideWorkspace };
}
