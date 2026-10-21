import { describe, expect, it } from "vitest";
import { ApprovalManager } from "./approval";
import { analyzeCommandRisk } from "./commandRisk";

describe("agent safety policy", () => {
  const approvals = new ApprovalManager(() => undefined);
  it("automates safe and moderate actions but always approves dangerous actions", () => {
    expect(approvals.requiresApproval("AGENT", "SAFE")).toBe(false);
    expect(approvals.requiresApproval("AGENT", "MODERATE")).toBe(false);
    expect(approvals.requiresApproval("AUTONOMOUS", "DANGEROUS")).toBe(true);
  });
  it("classifies traversal, destructive commands, interpreters, and unknown tools conservatively", () => {
    expect(analyzeCommandRisk("cat", ["../secret"]).risk).toBe("DANGEROUS");
    expect(analyzeCommandRisk("rm", ["-rf", "."]).risk).toBe("DANGEROUS");
    expect(analyzeCommandRisk("git", ["reset", "--hard"]).risk).toBe("DANGEROUS");
    expect(analyzeCommandRisk("sh", ["-c", "echo ok"]).risk).toBe("DANGEROUS");
    expect(analyzeCommandRisk("custom-tool", []).risk).toBe("MODERATE");
    expect(analyzeCommandRisk("npm", ["run", "build"]).risk).toBe("SAFE");
    expect(analyzeCommandRisk("npm", ["run", "release"]).risk).toBe("MODERATE");
  });
});
