import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import CleanupPreviewPanel from "@/components/CleanupPreviewPanel.vue";
import type { CleanupPreview } from "@/types/dashboard";

const base = {
  ruleVersion: "1",
  evidence: ["固定允许目录"],
  itemCount: 2,
  recoverable: true,
  defaultSelected: false,
  observedAtUnixMs: 1,
};
const preview: CleanupPreview = {
  scan: {
    scanId: "cleanup-1",
    status: "completed",
    scannedItems: 42,
    skippedItems: 1,
    message: "清理扫描完成（仅预览）",
    sourceVolumeId: "C:",
  },
  candidates: [
    {
      ...base,
      id: "temp.v1",
      ruleId: "temp.v1",
      title: "用户临时文件",
      description: "应用产生的临时内容",
      path: "C:\\Users\\demo\\Temp",
      executionRoots: [],
      bytes: 1024,
      risk: "review",
      requiresAdmin: false,
      recoveryStrategy: "quarantine",
      quarantineEligible: true,
      metadataDigest: "digest",
    },
    {
      ...base,
      id: "update.v1",
      ruleId: "update.v1",
      title: "Windows 更新下载缓存",
      description: "系统管理的缓存",
      path: "C:\\Windows\\SoftwareDistribution\\Download",
      executionRoots: [],
      bytes: 2048,
      risk: "confirmationRequired",
      requiresAdmin: true,
      recoveryStrategy: "windowsManaged",
      quarantineEligible: false,
      metadataDigest: "digest-2",
    },
  ],
  ruleStatuses: [
    {
      ruleId: "temp.v1",
      title: "用户临时文件",
      availability: "available",
      reason: null,
      requiresAdmin: false,
      risk: "review",
    },
  ],
  totalReclaimableBytes: 3072,
};

function mountPanel() {
  return mount(CleanupPreviewPanel, {
    props: {
      preview,
      selectedIds: ["temp.v1"],
      selectedBytes: 1024,
      highestRisk: "review",
      plan: undefined,
      quarantine: undefined,
      auditEvents: [],
      error: undefined,
      isLoading: false,
      isPreparingPlan: false,
      isPreparingQuarantine: false,
    },
  });
}

describe("CleanupPreviewPanel", () => {
  it("groups risk, exposes evidence, and warns about privileged system rules", async () => {
    const wrapper = mountPanel();
    expect(wrapper.text()).toContain("需要复核");
    expect(wrapper.text()).toContain("需单独确认");
    expect(wrapper.text()).toContain("Windows 更新下载缓存");
    await wrapper
      .findAll("details.candidate-card")[1]
      ?.get("summary")
      .trigger("click");
    expect(wrapper.text()).toContain("不会请求管理员权限");
  });

  it("emits typed selection and plan actions while disabled quarantine stays inert", async () => {
    const wrapper = mountPanel();
    await wrapper.get('input[aria-label="选择用户临时文件"]').setValue(false);
    await wrapper.get("button.primary-button").trigger("click");
    await wrapper.get(".workflow-card button").trigger("click");
    expect(wrapper.emitted("update-selection")?.[0]).toEqual([
      "temp.v1",
      false,
    ]);
    expect(wrapper.emitted("prepare-plan")).toHaveLength(1);
    expect(wrapper.emitted("prepare-quarantine")).toBeUndefined();
  });
});
