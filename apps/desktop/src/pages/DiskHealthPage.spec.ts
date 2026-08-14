import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import DiskHealthPage from "@/pages/DiskHealthPage.vue";
import { loadDiskHealthSnapshot } from "@/services/health-service";
import { diskHealthFixture } from "@/testing/health-fixture";

vi.mock("@/services/health-service", () => ({
  loadDiskHealthSnapshot: vi.fn(),
  downloadDiskHealthReport: vi.fn(),
}));

describe("DiskHealthPage", () => {
  beforeEach(() => {
    vi.mocked(loadDiskHealthSnapshot).mockResolvedValue(
      structuredClone(diskHealthFixture),
    );
  });

  it("renders read-only health evidence and switches physical disks", async () => {
    const wrapper = mount(DiskHealthPage);
    await flushPromises();

    expect(wrapper.get("h1").text()).toBe("磁盘健康");
    expect(wrapper.text()).toContain("未知不代表健康");
    expect(wrapper.text()).toContain("Samsung NVMe SSD");
    expect(wrapper.text()).toContain("预计剩余寿命");
    expect(wrapper.text()).toContain("93%");

    const secondDisk = wrapper.findAll(".disk-pill")[1];
    if (!secondDisk) throw new Error("second fixture disk missing");
    await secondDisk.trigger("click");

    expect(wrapper.text()).toContain("Backup SATA HDD");
    expect(wrapper.text()).toContain("磁盘温度偏高");
    expect(wrapper.text()).toContain("62°C");
    expect(wrapper.text()).toContain("存在已纠正或重试错误");
  });

  it("renders missing evidence as unavailable and unknown", async () => {
    const fixture = structuredClone(diskHealthFixture);
    const disk = fixture.disks[0];
    if (!disk) throw new Error("fixture disk missing");
    disk.status = "unknown";
    disk.smartStatus = "unavailable";
    disk.dataCompleteness = "limited";
    disk.temperatureCelsius = null;
    disk.estimatedLifeRemainingPercent = null;
    disk.powerOnHours = null;
    disk.readErrorsUncorrected = null;
    disk.writeErrorsUncorrected = null;
    disk.signals = [
      {
        code: "dataIncomplete",
        severity: "unknown",
        title: "健康数据不完整",
        detail: "部分字段不可用。",
        recommendation: "不可用字段不会被推断为正常。",
      },
    ];
    fixture.summary.overallStatus = "unknown";
    vi.mocked(loadDiskHealthSnapshot).mockResolvedValue(fixture);

    const wrapper = mount(DiskHealthPage);
    await flushPromises();

    expect(wrapper.text()).toContain("状态未知");
    expect(wrapper.text()).toContain("证据有限");
    expect(wrapper.text()).toContain("不可用");
    expect(wrapper.text()).toContain("健康数据不完整");
  });

  it("fails closed, retries discovery, and exposes no repair action", async () => {
    vi.mocked(loadDiskHealthSnapshot)
      .mockRejectedValueOnce(new Error("storage provider unavailable"))
      .mockResolvedValueOnce(structuredClone(diskHealthFixture));
    const wrapper = mount(DiskHealthPage);
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toContain(
      "未执行任何磁盘修改",
    );
    await wrapper.get('[role="alert"] button').trigger("click");
    await flushPromises();

    expect(loadDiskHealthSnapshot).toHaveBeenCalledTimes(2);
    expect(wrapper.text()).toContain("此页面没有修复、写盘或管理员命令");
    expect(wrapper.find('button[data-action="repair"]').exists()).toBe(false);
    expect(wrapper.find('button[data-action="execute"]').exists()).toBe(false);
  });
});
