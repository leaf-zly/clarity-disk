import { beforeEach, describe, expect, it, vi } from "vitest";

import { useCleanupExecution } from "@/composables/use-cleanup-execution";
import {
  executeCleanup,
  getExecutionQuarantineIndex,
  prepareCleanupExecution,
  restoreQuarantineEntry,
} from "@/services/cleanup-execution-service";
import type { CleanupPlan } from "@/types/dashboard";

vi.mock("@/services/cleanup-execution-service", () => ({
  prepareCleanupExecution: vi.fn(),
  executeCleanup: vi.fn(),
  getExecutionQuarantineIndex: vi.fn(),
  restoreQuarantineEntry: vi.fn(),
}));

const plan = {
  planId: "plan-1",
  scanId: "scan-1",
  candidates: [{ id: "user-temp.v1", ruleId: "user-temp.v1" }],
  planDigest: "digest",
  executionAuthorized: false,
  createdAtUnixMs: 1,
  expiresAtUnixMs: 2,
  sourceVolumeId: "C:",
} as CleanupPlan;

describe("useCleanupExecution", () => {
  beforeEach(() => {
    vi.mocked(getExecutionQuarantineIndex).mockResolvedValue(null);
    vi.mocked(prepareCleanupExecution).mockResolvedValue({
      authorizationId: "authorization-1",
      planId: "plan-1",
      planDigest: "digest",
      candidateIds: ["user-temp.v1"],
      confirmationToken: "secret",
      confirmationPhrase: "确认移入隔离区",
      expiresAtUnixMs: Date.now() + 60_000,
    });
  });

  it("submits only executable candidate IDs and clears the token before awaiting", async () => {
    const workflow = useCleanupExecution();
    await workflow.prepare(plan);
    expect(prepareCleanupExecution).toHaveBeenCalledWith(
      { planId: "plan-1", candidateIds: ["user-temp.v1"] },
      plan,
    );
    vi.mocked(executeCleanup).mockResolvedValue({
      executionId: "execution-1",
      planId: "plan-1",
      results: [],
      stagedBytes: 0,
      startedAtUnixMs: 1,
      finishedAtUnixMs: 2,
      executionAuthorized: true,
    });
    const executing = workflow.execute(plan, "确认移入隔离区");
    expect(workflow.challenge.value).toBeUndefined();
    await executing;
    expect(executeCleanup).toHaveBeenCalledWith(
      {
        authorizationId: "authorization-1",
        confirmationToken: "secret",
        confirmationPhrase: "确认移入隔离区",
      },
      plan,
    );
  });

  it("restores by identity and refreshes persisted quarantine state", async () => {
    const workflow = useCleanupExecution();
    vi.mocked(restoreQuarantineEntry).mockResolvedValue({
      entryId: "entry-1",
      status: "restored",
      reason: "已恢复到原位置",
    });
    await workflow.restore("entry-1");
    expect(restoreQuarantineEntry).toHaveBeenCalledWith("entry-1");
    expect(getExecutionQuarantineIndex).toHaveBeenCalled();
  });
});
