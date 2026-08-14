import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import CleanupExecutionPanel from "@/components/CleanupExecutionPanel.vue";
import type { CleanupPlan } from "@/types/dashboard";

const plan: CleanupPlan = {
  planId: "plan-1",
  scanId: "scan-1",
  planDigest: "digest",
  executionAuthorized: false,
  createdAtUnixMs: 1,
  expiresAtUnixMs: Date.now() + 60_000,
  sourceVolumeId: "C:",
  candidates: [
    {
      id: "user-temp.v1",
      ruleId: "user-temp.v1",
      ruleVersion: "1",
      title: "用户临时文件",
      description: "临时内容",
      path: "C:\\Temp",
      executionRoots: ["C:\\Temp"],
      evidence: ["固定目录"],
      bytes: 100,
      itemCount: 1,
      risk: "review",
      recoverable: true,
      requiresAdmin: false,
      recoveryStrategy: "quarantine",
      quarantineEligible: true,
      defaultSelected: false,
      metadataDigest: "digest",
      observedAtUnixMs: 1,
    },
  ],
};

const baseProps = {
  plan,
  challenge: undefined,
  report: undefined,
  quarantine: undefined,
  error: undefined,
  isPreparing: false,
  isExecuting: false,
  restoringEntryId: undefined,
  isRestoringBatch: false,
  isUpdatingPolicy: false,
};

describe("CleanupExecutionPanel", () => {
  it("uses the recycle-bin-specific confirmation phrase", async () => {
    const wrapper = mount(CleanupExecutionPanel, {
      props: {
        ...baseProps,
        challenge: {
          authorizationId: "authorization-1",
          planId: "plan-1",
          planDigest: "digest",
          candidateIds: ["recycle-bin.v1"],
          mode: "windowsRecycleBin",
          confirmationToken: "secret",
          confirmationPhrase: "确认永久清空回收站",
          expiresAtUnixMs: Date.now() + 60_000,
        },
      },
    });
    const button = wrapper.get(".confirmation-box button");
    await wrapper.get(".confirmation-box input").setValue("确认移入隔离区");
    expect(button.attributes("disabled")).toBeDefined();
    await wrapper.get(".confirmation-box input").setValue("确认永久清空回收站");
    await button.trigger("click");
    expect(wrapper.emitted("execute")?.[0]).toEqual(["确认永久清空回收站"]);
  });

  it("emits only backend IDs for batch restore and fixed policy tiers", async () => {
    const wrapper = mount(CleanupExecutionPanel, {
      props: {
        ...baseProps,
        quarantine: {
          indexId: "index-1",
          planId: "plan-1",
          createdAtUnixMs: 1,
          filesMoved: true,
          totalBytes: 100,
          policy: { retentionDays: 30, maxBytes: 10 * 1024 ** 3 },
          entries: [
            {
              entryId: "entry-1",
              candidateId: "user-temp.v1",
              ruleId: "user-temp.v1",
              originalPath: "C:\\Temp\\one.tmp",
              quarantinePath: "C:\\Quarantine\\entry-1",
              bytes: 100,
              metadataDigest: "digest",
              status: "expired",
              movedAtUnixMs: 1,
              restoredAtUnixMs: null,
              expiresAtUnixMs: 2,
              transferKind: "rename",
              integrityDigest: null,
              transactionPath: null,
            },
          ],
        },
      },
    });
    await wrapper.get(".entry-check").setValue(true);
    await wrapper.get(".quarantine-heading button").trigger("click");
    expect(wrapper.emitted("restore-batch")?.[0]).toEqual([["entry-1"]]);
    await wrapper.findAll(".policy-panel select")[0]?.setValue("7");
    await wrapper.get(".policy-panel button").trigger("click");
    expect(wrapper.emitted("update-policy")?.[0]).toEqual([
      { retentionDays: 7, maxBytes: 10 * 1024 ** 3 },
    ]);
  });
});
