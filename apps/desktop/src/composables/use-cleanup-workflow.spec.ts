import { beforeEach, describe, expect, it, vi } from "vitest";

import { useCleanupWorkflow } from "@/composables/use-cleanup-workflow";
import {
  getAuditEvents,
  getQuarantineIndex,
  loadCleanupPreview,
  prepareCleanupPlan,
  prepareQuarantineIndex,
} from "@/services/dashboard-service";

vi.mock("@/services/dashboard-service", () => ({
  loadCleanupPreview: vi.fn(),
  prepareCleanupPlan: vi.fn(),
  prepareQuarantineIndex: vi.fn(),
  getAuditEvents: vi.fn(),
  getQuarantineIndex: vi.fn(),
}));

const candidate = {
  id: "temp.v1",
  ruleId: "temp.v1",
  ruleVersion: "1",
  title: "临时文件",
  description: "临时内容",
  path: "C:\\Temp",
  evidence: ["固定目录"],
  bytes: 100,
  itemCount: 1,
  risk: "review" as const,
  recoverable: true,
  requiresAdmin: false,
  recoveryStrategy: "quarantine" as const,
  quarantineEligible: true,
  defaultSelected: false,
  metadataDigest: "digest",
  observedAtUnixMs: 1,
};

describe("useCleanupWorkflow", () => {
  beforeEach(() => {
    vi.mocked(loadCleanupPreview).mockResolvedValue({
      scan: {
        scanId: "scan-1",
        status: "completed",
        scannedItems: 1,
        skippedItems: 0,
        message: "完成",
        sourceVolumeId: "C:",
      },
      candidates: [candidate],
      ruleStatuses: [],
      totalReclaimableBytes: 100,
    });
    vi.mocked(getAuditEvents).mockResolvedValue([]);
    vi.mocked(getQuarantineIndex).mockResolvedValue(null);
  });

  it("resets selection from defaults and submits only selected candidate IDs", async () => {
    const workflow = useCleanupWorkflow();
    await workflow.scan();
    expect(workflow.selectedIds.value).toEqual([]);
    workflow.setSelected("temp.v1", true);
    vi.mocked(prepareCleanupPlan).mockResolvedValue({
      planId: "plan-1",
      scanId: "scan-1",
      candidates: [candidate],
      planDigest: "digest",
      executionAuthorized: false,
      createdAtUnixMs: 1,
      expiresAtUnixMs: 2,
      sourceVolumeId: "C:",
    });
    await workflow.createPlan();
    expect(prepareCleanupPlan).toHaveBeenCalledWith({
      scanId: "scan-1",
      candidateIds: ["temp.v1"],
    });
    expect(workflow.plan.value?.executionAuthorized).toBe(false);
  });

  it("keeps quarantine output explicitly preview-only", async () => {
    const workflow = useCleanupWorkflow();
    await workflow.scan();
    workflow.setSelected("temp.v1", true);
    vi.mocked(prepareCleanupPlan).mockResolvedValue({
      planId: "plan-1",
      scanId: "scan-1",
      candidates: [candidate],
      planDigest: "digest",
      executionAuthorized: false,
      createdAtUnixMs: 1,
      expiresAtUnixMs: 2,
      sourceVolumeId: "C:",
    });
    vi.mocked(prepareQuarantineIndex).mockResolvedValue({
      indexId: "index-1",
      planId: "plan-1",
      createdAtUnixMs: 1,
      entries: [],
      filesMoved: false,
      totalBytes: 100,
    });
    await workflow.createPlan();
    await workflow.createQuarantineIndex();
    expect(workflow.quarantine.value?.filesMoved).toBe(false);
  });
});
