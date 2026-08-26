import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";

import DashboardPage from "@/pages/DashboardPage.vue";
import { dashboardFixture } from "@/testing/dashboard-fixture";

vi.mock("@/services/dashboard-service", () => ({
  loadDashboardSnapshot: vi.fn(async () => structuredClone(dashboardFixture)),
  loadCleanupPreview: vi.fn(async () => ({
    scan: {
      scanId: "navigation-test",
      status: "completed",
      scannedItems: 0,
      skippedItems: 0,
      message: "扫描完成",
      sourceVolumeId: "C:",
    },
    candidates: [],
    ruleStatuses: [],
    totalReclaimableBytes: 0,
  })),
  prepareCleanupPlan: vi.fn(),
  prepareQuarantineIndex: vi.fn(),
  getQuarantineIndex: vi.fn(async () => null),
  getAuditEvents: vi.fn(async () => []),
  getSpaceScanHistory: vi.fn(async () => []),
  getDefaultSpaceScanRequest: vi.fn(async (rootPath: string) => ({
    rootPath,
    maxDepth: 8,
    maxEntries: 100_000,
    excludedPaths: [],
  })),
  startSpaceScan: vi.fn(),
  getSpaceScan: vi.fn(),
  cancelSpaceScan: vi.fn(),
  pauseSpaceScan: vi.fn(),
  resumeSpaceScan: vi.fn(),
}));

describe("DashboardPage navigation", () => {
  it("renders each destination as an isolated view without remounting", async () => {
    const wrapper = mount(DashboardPage, { props: { section: "overview" } });
    await flushPromises();
    expect(wrapper.get("h1").text()).toBe("下午好");
    expect(wrapper.text()).toContain("磁盘与卷");
    expect(wrapper.text()).not.toContain("扫描范围");

    const cleanupButton = wrapper
      .findAll("button")
      .find((button) => button.text().includes("查看清理项目"));
    expect(cleanupButton).toBeDefined();
    await cleanupButton?.trigger("click");
    expect(wrapper.emitted("navigate")?.at(-1)).toEqual(["cleanup"]);

    await wrapper
      .findAll("button")
      .find((button) => button.text().includes("查看报告"))
      ?.trigger("click");
    expect(wrapper.emitted("navigate")?.at(-1)).toEqual(["history"]);

    await wrapper
      .findAll("button")
      .find((button) => button.text().includes("全部建议"))
      ?.trigger("click");
    expect(wrapper.emitted("navigate")?.at(-1)).toEqual(["cleanup"]);

    await wrapper.setProps({ section: "space" });
    expect(wrapper.get("h1").text()).toBe("空间分析");
    expect(wrapper.text()).toContain("扫描范围");
    expect(wrapper.text()).not.toContain("磁盘与卷");

    await wrapper.setProps({ section: "cleanup" });
    await flushPromises();
    expect(wrapper.get("h1").text()).toBe("智能清理");
    expect(wrapper.text()).toContain("安全清理中心");
    expect(wrapper.text()).not.toContain("扫描范围");
  });
});
