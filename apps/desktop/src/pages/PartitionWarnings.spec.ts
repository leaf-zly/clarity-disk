import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";

import PartitionPreviewPage from "@/pages/PartitionPreviewPage.vue";
import { partitionTopologyFixture } from "@/testing/partition-fixture";

vi.mock("@/services/partition-service", () => ({
  loadPartitionTopology: vi.fn(async () => ({
    ...structuredClone(partitionTopologyFixture),
    discoveryWarnings: [
      "当前权限无法核验 BitLocker 状态。",
      "磁盘未提供介质可靠性计数。",
    ],
  })),
  previewPartitionMerge: vi.fn(),
  downloadPartitionPreviewReport: vi.fn(),
}));

describe("PartitionPreviewPage provider limitations", () => {
  it("keeps advanced provider limitations compact and discoverable", async () => {
    const wrapper = mount(PartitionPreviewPage);
    await flushPromises();

    const limitations = wrapper.get("details.provider-limitations");
    expect(limitations.get("summary").text()).toContain("基础分区信息已读取");
    expect(limitations.get("summary").text()).toContain("2 项");
    expect(limitations.findAll("li")).toHaveLength(2);
  });
});
