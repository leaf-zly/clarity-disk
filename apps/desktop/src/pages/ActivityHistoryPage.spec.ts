import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";

import ActivityHistoryPage from "@/pages/ActivityHistoryPage.vue";

vi.mock("@/services/operations-service", () => ({
  getActivityHistory: vi.fn(async () => ({
    cleanup: [
      {
        eventId: "event-1",
        kind: "scanCompleted",
        subjectId: "scan-1",
        occurredAtUnixMs: Date.now(),
        reason: "只读扫描完成",
        candidateCount: 2,
        totalBytes: 1024,
        ruleIds: ["browser-cache.v1"],
      },
    ],
    scans: [],
    privileged: [],
  })),
  clearActivityHistory: vi.fn(async () => undefined),
}));

describe("ActivityHistoryPage", () => {
  it("filters privacy-safe activity summaries", async () => {
    const wrapper = mount(ActivityHistoryPage);
    await flushPromises();

    expect(wrapper.text()).toContain("清理扫描完成");
    await wrapper.get('input[type="search"]').setValue("不存在");
    expect(wrapper.text()).toContain("没有符合条件的活动");
    expect(wrapper.text()).toContain("隔离文件与分区恢复日志不会随历史清除");
  });
});
