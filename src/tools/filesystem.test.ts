import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
import { FilesystemToolError, filesystemActionSchema, filesystemRisk, filesystemTools, normalizeFilesystemError, runFilesystemTool } from "./filesystem";

beforeEach(() => invoke.mockReset());

const fileMetadata = { kind: "file", size: 5, modifiedAt: 1, createdAt: null, readOnly: false, isSymlink: false };

describe("filesystem tool boundary", () => {
  it("passes a typed USER request and validates the result", async () => {
    invoke.mockResolvedValue({ operation: "read", content: "hello", metadata: fileMetadata });
    await expect(runFilesystemTool({ operation: "read", path: "src/main.ts" })).resolves.toEqual({ operation: "read", content: "hello", metadata: fileMetadata });
    expect(invoke).toHaveBeenCalledWith("filesystem_tool", { request: { actor: "USER", action: { operation: "read", path: "src/main.ts" } } });
  });

  it("passes the AGENT actor explicitly for Rust to reject", async () => {
    invoke.mockResolvedValue({ operation: "exists", exists: true });
    await runFilesystemTool({ operation: "exists", path: "a" }, "AGENT");
    expect(invoke).toHaveBeenCalledWith("filesystem_tool", { request: { actor: "AGENT", action: { operation: "exists", path: "a" } } });
  });
  it("rejects invalid schema before IPC", async () => {
    expect(() => filesystemActionSchema.parse({ operation: "read_range", path: "a", startByte: 0, length: 300000 })).toThrow();
    await expect(runFilesystemTool({ operation: "search", query: "", mode: "content", limit: 10 })).rejects.toThrow();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("normalizes structured native errors", () => {
    const error = normalizeFilesystemError({ code: "BINARY_FILE", message: "Binary file" });
    expect(error instanceof FilesystemToolError).toBe(true);
    expect((error as FilesystemToolError).code).toBe("BINARY_FILE");
  });
  it("rejects malformed native output", async () => {
    invoke.mockResolvedValue({ operation: "stat", metadata: { size: "wrong" } });
    await expect(runFilesystemTool({ operation: "stat", path: "a" })).rejects.toThrow();
  });

  it("classifies every operation and recursive delete", () => {
    expect(Object.keys(filesystemTools)).toHaveLength(14);
    expect(filesystemRisk({ operation: "delete", path: "a", recursive: true })).toBe("DANGEROUS");
    expect(filesystemRisk({ operation: "delete", path: "a", recursive: false })).toBe("DESTRUCTIVE");
    expect(filesystemTools.read_range.risk).toBe("READ");
  });
});