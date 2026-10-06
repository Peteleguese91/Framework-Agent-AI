import { z } from "zod";
import type { ToolDefinition } from "../models/types";
import { terminalApi } from "../terminal/terminalApi";
import type { ExecResult } from "../terminal/types";
import { runAgentFilesystemTool, type FilesystemAction, type FilesystemResult } from "../tools/filesystem";
import { analyzeCommandRisk } from "./commandRisk";
import type { AgentMode, AgentRisk } from "./types";

export interface ToolExecutionContext { mode: Exclude<AgentMode, "ASK">; approved: boolean }
export interface AgentTool<T = unknown> {
  name: string; description: string; schema: z.ZodType<T>; parameters: Record<string, unknown>;
  risk: (argumentsValue: T) => AgentRisk; definitionRisk: ToolDefinition["riskLevel"]; allowedActors: readonly ["AGENT"];
  timeoutMs: number; handler: (argumentsValue: T, context: ToolExecutionContext) => Promise<unknown>;
}

const path = z.string().min(1);
const limit = z.number().int().min(1).max(200).default(100);
const object = (properties: Record<string, unknown>, required: string[] = []) => ({ type: "object", properties, required, additionalProperties: false });
const string = { type: "string" };
const boolean = { type: "boolean" };
const integer = { type: "integer" };

function fsTool<T extends FilesystemAction>(
  name: string, description: string, schema: z.ZodType<T>, parameters: Record<string, unknown>,
  risk: AgentRisk | ((value: T) => AgentRisk), timeoutMs = 30_000,
): AgentTool<T> {
  return {
    name, description, schema, parameters, allowedActors: ["AGENT"], timeoutMs,
    definitionRisk: typeof risk === "function" ? "DANGEROUS" : risk === "SAFE" ? "READ" : "WRITE",
    risk: typeof risk === "function" ? risk : () => risk,
    handler: (value, context) => runAgentFilesystemTool(value, context.mode, context.approved),
  };
}

const tools: AgentTool[] = [
  fsTool("fs.list", "List entries in a workspace directory.", z.object({ operation: z.literal("list").default("list"), path: z.string(), limit }), object({ path: string, limit: { ...integer, minimum: 1, maximum: 200 } }, ["path"]), "SAFE"),
  fsTool("fs.read", "Read a workspace text file up to the full-read limit.", z.object({ operation: z.literal("read").default("read"), path }), object({ path: string }, ["path"]), "SAFE"),
  fsTool("fs.read_range", "Read a bounded byte range from a workspace text file.", z.object({ operation: z.literal("read_range").default("read_range"), path, startByte: z.number().int().nonnegative(), length: z.number().int().min(1).max(262144) }), object({ path: string, startByte: { ...integer, minimum: 0 }, length: { ...integer, minimum: 1, maximum: 262144 } }, ["path", "startByte", "length"]), "SAFE"),
  fsTool("fs.write", "Atomically replace a text file when expectedContent still matches.", z.object({ operation: z.literal("write").default("write"), path, content: z.string(), expectedContent: z.string() }), object({ path: string, content: string, expectedContent: string }, ["path", "content", "expectedContent"]), "MODERATE"),
  fsTool("fs.patch", "Apply minimal line hunks with exact old-line context.", z.object({ operation: z.literal("patch").default("patch"), path, hunks: z.array(z.object({ startLine: z.number().int().min(1), oldLines: z.array(z.string()), newLines: z.array(z.string()) })).min(1).max(100) }), object({ path: string, hunks: { type: "array", items: object({ startLine: integer, oldLines: { type: "array", items: string }, newLines: { type: "array", items: string } }, ["startLine", "oldLines", "newLines"]) } }, ["path", "hunks"]), "MODERATE"),
  fsTool("fs.create", "Create a new workspace text file without overwriting.", z.object({ operation: z.literal("create").default("create"), path, content: z.string() }), object({ path: string, content: string }, ["path", "content"]), "MODERATE"),
  fsTool("fs.delete", "Delete a workspace path. Recursive deletion is dangerous.", z.object({ operation: z.literal("delete").default("delete"), path, recursive: z.boolean().default(false) }), object({ path: string, recursive: boolean }, ["path", "recursive"]), (value) => value.recursive ? "DANGEROUS" : "MODERATE"),
  fsTool("fs.move", "Move or rename a workspace path.", z.object({ operation: z.literal("move").default("move"), from: path, to: path }), object({ from: string, to: string }, ["from", "to"]), "MODERATE"),
  fsTool("fs.copy", "Copy a workspace file or directory.", z.object({ operation: z.literal("copy").default("copy"), from: path, to: path, recursive: z.boolean().default(false) }), object({ from: string, to: string, recursive: boolean }, ["from", "to", "recursive"]), "MODERATE"),
  fsTool("fs.mkdir", "Create a workspace directory.", z.object({ operation: z.literal("mkdir").default("mkdir"), path }), object({ path: string }, ["path"]), "MODERATE"),
  fsTool("fs.exists", "Check whether a workspace-relative path exists.", z.object({ operation: z.literal("exists").default("exists"), path }), object({ path: string }, ["path"]), "SAFE"),
  fsTool("fs.stat", "Read consistent metadata for a workspace path.", z.object({ operation: z.literal("stat").default("stat"), path }), object({ path: string }, ["path"]), "SAFE"),
  fsTool("fs.search", "Search workspace filenames, paths, or text content.", z.object({ operation: z.literal("search").default("search"), query: z.string().min(1).max(256), mode: z.enum(["filename", "path", "content"]), limit }), object({ query: string, mode: { type: "string", enum: ["filename", "path", "content"] }, limit: integer }, ["query", "mode"]), "SAFE", 60_000),
  fsTool("fs.glob", "Find workspace paths using ignore-aware glob matching.", z.object({ operation: z.literal("glob").default("glob"), pattern: z.string().min(1).max(256), limit }), object({ pattern: string, limit: integer }, ["pattern"]), "SAFE", 60_000),
];

const terminalSchema = z.object({
  command: z.string().min(1), args: z.array(z.string()).default([]), cwd: z.string().optional(),
  timeoutMs: z.number().int().min(1).max(3_600_000).default(120_000),
  env: z.record(z.string(), z.string()).default({}),
});
const terminalTool: AgentTool<z.infer<typeof terminalSchema>> = {
  name: "terminal.exec", description: "Run a non-interactive program in the workspace with bounded output and timeout.",
  schema: terminalSchema,
  parameters: object({ command: string, args: { type: "array", items: string }, cwd: string, timeoutMs: { ...integer, minimum: 1, maximum: 3600000 }, env: { type: "object", additionalProperties: { type: "string" } } }, ["command"]),
  risk: (value) => analyzeCommandRisk(value.command, value.args).risk,
  definitionRisk: "DANGEROUS",
  allowedActors: ["AGENT"], timeoutMs: 3_600_000,
  handler: (value, context): Promise<ExecResult> => terminalApi.exec({ ...value, actor: "AGENT", approvalMode: context.mode, approved: context.approved }),
};
tools.push(terminalTool);

export class ToolRegistry {
  private readonly tools = new Map<string, AgentTool>();
  constructor(initial: AgentTool[] = tools) { for (const tool of initial) this.register(tool); }
  register(tool: AgentTool): void { if (this.tools.has(tool.name)) throw new Error(`Tool already registered: ${tool.name}`); this.tools.set(tool.name, tool); }
  get(name: string): AgentTool | undefined { return this.tools.get(name); }
  list(): AgentTool[] { return [...this.tools.values()]; }
  definitions(): ToolDefinition[] { return this.list().map((tool) => ({ name: tool.name, description: tool.description, parameters: tool.parameters, riskLevel: tool.definitionRisk })); }
  validate(name: string, value: unknown): { tool: AgentTool; argumentsValue: unknown; risk: AgentRisk } {
    const tool = this.get(name);
    if (!tool) throw new Error(`Unknown tool: ${name}`);
    const argumentsValue = tool.schema.parse(value);
    return { tool, argumentsValue, risk: tool.risk(argumentsValue) };
  }
}

export function mutationPath(name: string, value: unknown): string[] {
  const record = value as Record<string, unknown>;
  if (["fs.write", "fs.patch", "fs.create", "fs.delete", "fs.mkdir"].includes(name) && typeof record.path === "string") return [record.path];
  if (["fs.move", "fs.copy"].includes(name)) return [record.from, record.to].filter((item): item is string => typeof item === "string");
  return [];
}

export function summarizeResult(name: string, result: unknown): string {
  if (name === "terminal.exec") {
    const value = result as ExecResult;
    return `${value.success ? "Command passed" : "Command failed"}${value.exitCode === undefined ? "" : ` with exit code ${value.exitCode}`}`;
  }
  const value = result as FilesystemResult;
  if (value.operation === "mutation") return `Changed ${value.path}`;
  if (value.operation === "read") return `Read ${value.metadata.size} bytes`;
  if (value.operation === "list" || value.operation === "glob") return `Found ${value.entries.length} entries`;
  if (value.operation === "search") return `Found ${value.matches.length} matches`;
  if (value.operation === "exists") return value.exists ? "Path exists" : "Path does not exist";
  return `Completed ${name}`;
}
