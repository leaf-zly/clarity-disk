import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import CleanupExecutionPanel from "@/components/CleanupExecutionPanel.vue";
import type { CleanupPlan } from "@/types/dashboard";

const plan: CleanupPlan = {
  planId: "plan-1",
  scanId: "scan-1",
  candidates: [
    {
      id: "user-temp.v1",
      ruleId: "user-temp.v1",
      ruleVersion: "1",
      title: "用户临时文件",
      description: "临时内容",
      path: "C:\\Temp",
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
  planDigest: "plan-digest",
  executionAuthorized: false,
  createdAtUnixMs: 1,
  expiresAtUnixMs: Date.now() + 60_000,
  sourceVolumeId: "C:",
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
};

describe("CleanupExecutionPanel", () => {
  it("requires exact confirmation text before emitting execution", async () => {
    const wrapper = mount(CleanupExecutionPanel, {
      props: {
        ...baseProps,
        challenge: {
          authorizationId: "authorization-1",
          planId: "plan-1",
          planDigest: "plan-digest",
          candidateIds: ["user-temp.v1"],
          confirmationToken: "secret",
          confirmationPhrase: "确认移入隔离区",
          expiresAtUnixMs: Date.now() + 60_000,
        },
      },
    });
    const execute = wrapper.get(".confirmation-box button");
    expect(execute.attributes("disabled")).toBeDefined();
    await wrapper.get(".confirmation-box input").setValue("确认移入隔离区");
    expect(execute.attributes("disabled")).toBeUndefined();
    await execute.trigger("click");
    expect(wrapper.emitted("execute")?.[0]).toEqual(["确认移入隔离区"]);
  });

  it("restores by backend entry identity and never emits a path", async () => {
    const wrapper = mount(CleanupExecutionPanel, {
      props: {
        ...baseProps,
        quarantine: {
          indexId: "index-1",
          planId: "plan-1",
          createdAtUnixMs: 1,
          filesMoved: true,
          totalBytes: 100,
          entries: [
            {
              entryId: "entry-1",
              candidateId: "user-temp.v1",
              ruleId: "user-temp.v1",
              originalPath: "C:\\Temp\\one.tmp",
              quarantinePath: "C:\\Quarantine\\entry-1",
              bytes: 100,
              metadataDigest: "digest",
              status: "staged",
              movedAtUnixMs: 1,
              restoredAtUnixMs: null,
            },
          ],
        },
      },
    });
    await wrapper.get(".quarantine-list button").trigger("click");
    expect(wrapper.emitted("restore")?.[0]).toEqual(["entry-1"]);
  });
});
