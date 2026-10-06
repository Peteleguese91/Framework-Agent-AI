import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";

const path = z.string().min(1);
const limit = z.number().int().min(1).max(200);
const hunk = z.object({ startLine: z.number().int().min(1), oldLines: z.array(z.string()), newLines: z.array(z.string()) });

export const filesystemActionSchema = z.discriminatedUnion("operation", [
  z.object({ operation: z.literal("list"), path: z.string(), limit }),
  z.object({ operation: z.literal("read"), path }),
  z.object({ operation: z.literal("read_range"), path, startByte: z.number().int().nonnegative(), length: z.number().int().min(1).max(262144) }),
  z.object({ operation: z.literal("write"), path, content: z.string(), expectedContent: z.string() }),
  z.object({ operation: z.literal("patch"), path, hunks: z.array(hunk).min(1).max(100) }),
  z.object({ operation: z.literal("create"), path, content: z.string() }),
  z.object({ operation: z.literal("delete"), path, recursive: z.boolean() }),
  z.object({ operation: z.literal("move"), from: path, to: path }),
  z.object({ operation: z.literal("copy"), from: path, to: path, recursive: z.boolean() }),
  z.object({ operation: z.literal("mkdir"), path }),
  z.object({ operation: z.literal("exists"), path }),
  z.object({ operation: z.literal("stat"), path }),
  z.object({ operation: z.literal("search"), query: z.string().min(1).max(256), mode: z.enum(["filename", "path", "content"]), limit }),
  z.object({ operation: z.literal("glob"), pattern: z.string().min(1).max(256), limit }),
]);

const metadata = z.object({
  kind: z.enum(["file", "directory", "symlink"]),
  size: z.number().nonnegative(),
  modifiedAt: z.number().nullable(),
  createdAt: z.number().nullable(),
  readOnly: z.boolean(),
  isSymlink: z.boolean(),
});
const entry = z.object({ path: z.string(), metadata });
const match = z.object({ path: z.string(), line: z.number().nullable(), text: z.string().nullable() });
export const filesystemResultSchema = z.discriminatedUnion("operation", [
  z.object({ operation: z.literal("list"), entries: z.array(entry), truncated: z.boolean() }),
  z.object({ operation: z.literal("read"), content: z.string(), metadata }),
  z.object({ operation: z.literal("read_range"), content: z.string(), startByte: z.number(), endByte: z.number(), totalBytes: z.number(), hasMore: z.boolean() }),
  z.object({ operation: z.literal("exists"), exists: z.boolean() }),
  z.object({ operation: z.literal("stat"), metadata }),
  z.object({ operation: z.literal("search"), matches: z.array(match), truncated: z.boolean() }),
  z.object({ operation: z.literal("glob"), entries: z.array(entry), truncated: z.boolean() }),
  z.object({ operation: z.literal("mutation"), path: z.string() }),
]);
export const filesystemErrorSchema = z.object({
  code: z.enum(["INVALID_INPUT", "OUTSIDE_WORKSPACE", "NOT_FOUND", "PERMISSION_DENIED", "ALREADY_EXISTS", "NOT_FILE", "NOT_DIRECTORY", "BINARY_FILE", "FILE_TOO_LARGE", "CONFLICT", "UNSUPPORTED", "IO_ERROR", "INTERNAL"]),
  message: z.string(),
});

export type FilesystemAction = z.infer<typeof filesystemActionSchema>;
export type FilesystemResult = z.infer<typeof filesystemResultSchema>;
export type FilesystemActor = "USER" | "AGENT";
export type FilesystemRisk = "READ" | "WRITE" | "DESTRUCTIVE" | "DANGEROUS";
export type FilesystemOperation = FilesystemAction["operation"];

export const filesystemTools: Readonly<Record<FilesystemOperation, { version: 1; risk: FilesystemRisk }>> = {
  list: { version: 1, risk: "READ" },
  read: { version: 1, risk: "READ" },
  read_range: { version: 1, risk: "READ" },
  write: { version: 1, risk: "WRITE" },
  patch: { version: 1, risk: "WRITE" },
  create: { version: 1, risk: "WRITE" },
  delete: { version: 1, risk: "DESTRUCTIVE" },
  move: { version: 1, risk: "WRITE" },
  copy: { version: 1, risk: "WRITE" },
  mkdir: { version: 1, risk: "WRITE" },
  exists: { version: 1, risk: "READ" },
  stat: { version: 1, risk: "READ" },
  search: { version: 1, risk: "READ" },
  glob: { version: 1, risk: "READ" },
};

export function filesystemRisk(action: FilesystemAction): FilesystemRisk {
  return action.operation === "delete" && action.recursive ? "DANGEROUS" : filesystemTools[action.operation].risk;
}

export class FilesystemToolError extends Error {
  constructor(public readonly code: z.infer<typeof filesystemErrorSchema>["code"], message: string) {
    super(message);
    this.name = "FilesystemToolError";
  }
}

export function normalizeFilesystemError(error: unknown): Error {
  const parsed = filesystemErrorSchema.safeParse(error);
  return parsed.success ? new FilesystemToolError(parsed.data.code, parsed.data.message) : error instanceof Error ? error : new Error(String(error));
}

export async function runFilesystemTool(action: FilesystemAction, actor: FilesystemActor = "USER"): Promise<FilesystemResult> {
  const checked = filesystemActionSchema.parse(action);
  try {
    const result = await invoke<unknown>("filesystem_tool", { request: { actor, action: checked } });
    return filesystemResultSchema.parse(result);
  } catch (error) {
    throw normalizeFilesystemError(error);
  }
}

export async function runAgentFilesystemTool(action: FilesystemAction, mode: "AGENT" | "AUTONOMOUS", approved: boolean): Promise<FilesystemResult> {
  const checked = filesystemActionSchema.parse(action);
  try {
    const result = await invoke<unknown>("filesystem_tool", { request: { actor: "AGENT", action: checked, approvalMode: mode, approved } });
    return filesystemResultSchema.parse(result);
  } catch (error) {
    throw normalizeFilesystemError(error);
  }
}
