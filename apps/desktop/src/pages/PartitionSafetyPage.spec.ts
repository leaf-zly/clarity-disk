import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import PartitionSafetyPage from "@/pages/PartitionSafetyPage.vue";
import { assessPartitionMergeSafety } from "@/services/partition-safety-service";
import {
  loadPartitionTopology,
  previewPartitionMerge,
} from "@/services/partition-service";
import { createPartitionSafetyFixture } from "@/testing/partition-safety-fixture";
import {
  createPartitionMergeFixture,
  partitionTopologyFixture,
} from "@/testing/partition-fixture";
import type { PartitionSafetyAssessment } from "@/types/partition-safety";

vi.mock("@/services/partition-service", () => ({
  loadPartitionTopology: vi.fn(),
  previewPartitionMerge: vi.fn(),
}));

vi.mock("@/services/partition-safety-service", () => ({
  assessPartitionMergeSafety: vi.fn(),
}));

const defaultRequest = {
  targetPartitionId: "fixture-work",
  sourcePartitionId: "fixture-archive",
};

describe("PartitionSafetyPage", () => {
  beforeEach(() => {
    vi.mocked(loadPartitionTopology).mockResolvedValue(
      structuredClone(partitionTopologyFixture),
    );
    vi.mocked(previewPartitionMerge).mockResolvedValue(
      createPartitionMergeFixture(defaultRequest),
    );
    vi.mocked(assessPartitionMergeSafety).mockResolvedValue(
      createPartitionSafetyFixture(defaultRequest),
    );
  });

  it("builds a blocked immutable plan without exposing a writer", async () => {
    const wrapper = mount(PartitionSafetyPage);
    await flushPromises();

    expect(wrapper.get("h1").text()).toBe("分区安全基础");
    expect(wrapper.text()).toContain("写入能力未安装");
    await wrapper.get(".primary-button").trigger("click");
    await flushPromises();

    expect(previewPartitionMerge).toHaveBeenCalledWith(defaultRequest);
    expect(assessPartitionMergeSafety).toHaveBeenCalledWith(defaultRequest);
    expect(wrapper.text()).toContain("不可变计划");
    expect(wrapper.text()).toContain("缺少可独立验证的最新备份");
    expect(wrapper.text()).toContain("恢复状态机协议 v1");
    expect(wrapper.text()).toContain("执行授权");
    expect(wrapper.text()).toContain("未授权");
    expect(wrapper.find('button[data-action="execute"]').exists()).toBe(false);
  });

  it("fails closed when the backend unexpectedly exposes write capability", async () => {
    const unsafe = {
      ...createPartitionSafetyFixture(defaultRequest),
      writeCapabilityPresent: true,
    } as unknown as PartitionSafetyAssessment;
    vi.mocked(assessPartitionMergeSafety).mockResolvedValue(unsafe);
    const wrapper = mount(PartitionSafetyPage);
    await flushPromises();
    await wrapper.get(".primary-button").trigger("click");
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toContain(
      "没有执行任何分区写入",
    );
    expect(wrapper.find(".outcome-card").exists()).toBe(false);
  });

  it("keeps all safety evidence absent when topology discovery fails", async () => {
    vi.mocked(loadPartitionTopology).mockRejectedValue(
      new Error("provider unavailable"),
    );
    const wrapper = mount(PartitionSafetyPage);
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toContain(
      "未执行任何磁盘修改",
    );
    expect(wrapper.find(".primary-button").exists()).toBe(false);
  });
});
