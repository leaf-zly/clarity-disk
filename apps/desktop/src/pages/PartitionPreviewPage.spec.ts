import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import PartitionPreviewPage from "@/pages/PartitionPreviewPage.vue";
import {
  loadPartitionTopology,
  previewPartitionMerge,
} from "@/services/partition-service";
import {
  createPartitionMergeFixture,
  partitionTopologyFixture,
} from "@/testing/partition-fixture";

vi.mock("@/services/partition-service", () => ({
  loadPartitionTopology: vi.fn(),
  previewPartitionMerge: vi.fn(),
  downloadPartitionPreviewReport: vi.fn(),
}));

describe("PartitionPreviewPage", () => {
  beforeEach(() => {
    vi.mocked(loadPartitionTopology).mockResolvedValue(
      structuredClone(partitionTopologyFixture),
    );
    vi.mocked(previewPartitionMerge).mockImplementation((request) =>
      Promise.resolve(createPartitionMergeFixture(request)),
    );
  });

  it("renders a read-only topology and explained feasible simulation", async () => {
    const wrapper = mount(PartitionPreviewPage);
    await flushPromises();

    expect(wrapper.get("h1").text()).toBe("分区预演");
    expect(wrapper.text()).toContain("只读模式");
    expect(wrapper.text()).toContain("Samsung NVMe SSD");
    expect(wrapper.text()).toContain("这里没有删除、格式化或执行入口");

    await wrapper.get(".primary-button").trigger("click");
    await flushPromises();

    expect(previewPartitionMerge).toHaveBeenCalledWith({
      targetPartitionId: "fixture-work",
      sourcePartitionId: "fixture-archive",
    });
    expect(wrapper.text()).toContain("当前条件具备合并可能");
    expect(wrapper.text()).toContain("执行授权");
    expect(wrapper.text()).toContain("未授权");
    expect(wrapper.text()).toContain("模拟后布局");
    expect(wrapper.findAll(".region.expanded")).toHaveLength(1);
  });

  it("renders backend blockers without exposing an execution action", async () => {
    vi.mocked(previewPartitionMerge).mockResolvedValue(
      createPartitionMergeFixture({
        targetPartitionId: "fixture-archive",
        sourcePartitionId: "fixture-work",
      }),
    );
    const wrapper = mount(PartitionPreviewPage);
    await flushPromises();
    await wrapper.get(".primary-button").trigger("click");
    await flushPromises();

    expect(wrapper.text()).toContain("当前条件不支持合并");
    expect(wrapper.text()).toContain("当前只支持将右侧相邻数据分区");
    expect(
      wrapper.find('button[type="button"][class*="execute"]').exists(),
    ).toBe(false);
  });

  it("fails closed when topology discovery is unavailable", async () => {
    vi.mocked(loadPartitionTopology).mockRejectedValue(
      new Error("storage provider unavailable"),
    );
    const wrapper = mount(PartitionPreviewPage);
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toContain(
      "未执行任何磁盘修改",
    );
    expect(wrapper.get('[role="alert"] button').text()).toBe("重试");
  });
});
