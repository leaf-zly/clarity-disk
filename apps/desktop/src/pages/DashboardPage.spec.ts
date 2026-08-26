import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import DashboardPage from "@/pages/DashboardPage.vue";
import {
  loadCleanupPreview,
  loadDashboardSnapshot,
} from "@/services/dashboard-service";
import { dashboardFixture } from "@/testing/dashboard-fixture";

vi.mock("@/services/dashboard-service", () => ({
  loadCleanupPreview: vi.fn(),
  loadDashboardSnapshot: vi.fn(),
  prepareCleanupPlan: vi.fn(),
  prepareQuarantineIndex: vi.fn(),
  getQuarantineIndex: vi.fn().mockResolvedValue(null),
  getAuditEvents: vi.fn().mockResolvedValue([]),
  startSpaceScan: vi.fn(),
  getSpaceScan: vi.fn(),
  cancelSpaceScan: vi.fn(),
  pauseSpaceScan: vi.fn(),
  resumeSpaceScan: vi.fn(),
  getSpaceScanHistory: vi.fn().mockResolvedValue([]),
  getDefaultSpaceScanRequest: vi.fn((rootPath: string) =>
    Promise.resolve({
      rootPath,
      maxDepth: 8,
      maxEntries: 100_000,
      excludedPaths: [],
    }),
  ),
}));

describe("DashboardPage", () => {
  beforeEach(() => {
    vi.mocked(loadCleanupPreview).mockResolvedValue({
      scan: {
        scanId: "test",
        status: "completed",
        scannedItems: 1,
        skippedItems: 0,
        message: "扫描完成",
        sourceVolumeId: "C:",
      },
      candidates: [],
      ruleStatuses: [],
      totalReclaimableBytes: 0,
    });
  });

  it("renders disk capacity and recommendations returned by the service", async () => {
    vi.mocked(loadDashboardSnapshot).mockResolvedValue(
      structuredClone(dashboardFixture),
    );
    const wrapper = mount(DashboardPage);
    await flushPromises();
    expect(wrapper.get("h1").text()).toBe("下午好");
    expect(wrapper.text()).toContain("Windows · 本地磁盘 (C:)");
    expect(wrapper.text()).toContain("40.43 GB");
    expect(wrapper.text()).toContain("浏览器缓存");
    expect(wrapper.text()).toContain("可安全清理");
    expect(wrapper.text()).toContain("磁盘与卷");
    expect(wrapper.text()).toContain("资料 · 本地磁盘 (D:)");
    expect(wrapper.text()).not.toContain("扫描范围");
  });

  it("offers a retry when disk discovery fails", async () => {
    vi.mocked(loadDashboardSnapshot).mockRejectedValue(
      new Error("disk discovery failed"),
    );
    const wrapper = mount(DashboardPage);
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain(
      "暂时无法读取磁盘状态",
    );
    expect(wrapper.get('[role="alert"] button').text()).toBe("重试");
  });
});
