import { beforeEach, describe, expect, it, vi } from "vitest";

import { useCleanupExecution } from "@/composables/use-cleanup-execution";
import {
  executeCleanup,
  getExecutionQuarantineIndex,
  prepareCleanupExecution,
  restoreQuarantineBatch,
  restoreQuarantineEntry,
  updateQuarantinePolicy,
} from "@/services/cleanup-execution-service";
import type { CleanupPlan } from "@/types/dashboard";

vi.mock("@/services/cleanup-execution-service", () => ({
  prepareCleanupExecution: vi.fn(),
  executeCleanup: vi.fn(),
  getExecutionQuarantineIndex: vi.fn(),
  restoreQuarantineEntry: vi.fn(),
  restoreQuarantineBatch: vi.fn(),
  updateQuarantinePolicy: vi.fn(),
}));

const plan = {
  planId: "plan-1",
  scanId: "scan-1",
  candidates: [
    { id: "user-temp.v1", ruleId: "user-temp.v1" },
    { id: "browser-cache.v1", ruleId: "browser-cache.v1" },
    { id: "recycle-bin.v1", ruleId: "recycle-bin.v1" },
  ],
  planDigest: "digest",
  executionAuthorized: false,
  createdAtUnixMs: 1,
  expiresAtUnixMs: 2,
  sourceVolumeId: "C:",
} as CleanupPlan;

describe("useCleanupExecution", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(getExecutionQuarantineIndex).mockResolvedValue(null);
    vi.mocked(prepareCleanupExecution).mockImplementation(async (request) => ({
      authorizationId: "authorization-1",
      planId: "plan-1",
      planDigest: "digest",
      candidateIds: request.candidateIds,
      mode: request.mode,
      confirmationToken: "secret",
      confirmationPhrase:
        request.mode === "quarantine" ? "确认移入隔离区" : "确认永久清空回收站",
      expiresAtUnixMs: Date.now() + 60_000,
    }));
  });

  it("never mixes quarantine and recycle-bin candidate IDs", async () => {
    const workflow = useCleanupExecution();
    await workflow.prepare(plan, "quarantine");
    expect(prepareCleanupExecution).toHaveBeenCalledWith(
      {
        planId: "plan-1",
        candidateIds: ["user-temp.v1", "browser-cache.v1"],
        mode: "quarantine",
      },
      plan,
    );
    await workflow.prepare(plan, "windowsRecycleBin");
    expect(prepareCleanupExecution).toHaveBeenLastCalledWith(
      {
        planId: "plan-1",
        candidateIds: ["recycle-bin.v1"],
        mode: "windowsRecycleBin",
      },
      plan,
    );
  });

  it("clears the one-time token before awaiting execution", async () => {
    const workflow = useCleanupExecution();
    await workflow.prepare(plan, "quarantine");
    vi.mocked(executeCleanup).mockResolvedValue({
      executionId: "execution-1",
      planId: "plan-1",
      mode: "quarantine",
      results: [],
      stagedBytes: 0,
      estimatedProcessedBytes: 0,
      startedAtUnixMs: 1,
      finishedAtUnixMs: 2,
      executionAuthorized: true,
    });
    const pending = workflow.execute(plan, "确认移入隔离区");
    expect(workflow.challenge.value).toBeUndefined();
    await pending;
  });

  it("submits only IDs for batch restore", async () => {
    const workflow = useCleanupExecution();
    vi.mocked(restoreQuarantineBatch).mockResolvedValue({
      results: [],
      index: {
        indexId: "index",
        planId: "plan",
        createdAtUnixMs: 1,
        entries: [],
        filesMoved: false,
        totalBytes: 0,
        policy: { retentionDays: 30, maxBytes: 10 * 1024 ** 3 },
      },
    });
    await workflow.restoreBatch(["entry-1", "entry-2"]);
    expect(restoreQuarantineBatch).toHaveBeenCalledWith(["entry-1", "entry-2"]);
    expect(restoreQuarantineEntry).not.toHaveBeenCalled();
    expect(updateQuarantinePolicy).not.toHaveBeenCalled();
  });
});
