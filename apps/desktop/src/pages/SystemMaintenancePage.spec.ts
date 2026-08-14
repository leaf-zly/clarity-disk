import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import SystemMaintenancePage from "@/pages/SystemMaintenancePage.vue";
import {
  executePrivilegedOperation,
  getPrivilegedAuditEvents,
  getPrivilegedCapabilities,
  prepareMaintenanceExecution,
} from "@/services/privileged-service";

vi.mock("@/services/privileged-service", () => ({
  executePrivilegedOperation: vi.fn(),
  getPrivilegedAuditEvents: vi.fn(),
  getPrivilegedCapabilities: vi.fn(),
  prepareMaintenanceExecution: vi.fn(),
}));

describe("SystemMaintenancePage", () => {
  beforeEach(() => {
    vi.mocked(getPrivilegedCapabilities).mockResolvedValue({
      schemaVersion: 1,
      serviceVersion: "0.1.0",
      serviceAvailable: true,
      maintenanceOperations: [
        "hibernation",
        "windowsUpdateDownloadCache",
        "systemRestorePoint",
      ],
      partitionWriterCompiled: true,
      partitionWriterRuntimeEnabled: false,
    });
    vi.mocked(getPrivilegedAuditEvents).mockResolvedValue([]);
    vi.mocked(prepareMaintenanceExecution).mockResolvedValue({
      challengeId: "challenge-1",
      confirmationToken: "token-1",
      confirmationPhrase: "确认关闭休眠功能",
      expiresAtUnixMs: Date.now() + 60_000,
      impact: "关闭休眠会影响快速启动。",
    });
    vi.mocked(executePrivilegedOperation).mockResolvedValue({
      requestId: "request-1",
      status: "completed",
      message: "休眠功能已关闭。",
      completedAtUnixMs: Date.now(),
      recoveryState: null,
    });
  });

  it("shows a versioned broker handshake and closed runtime partition gate", async () => {
    const wrapper = mount(SystemMaintenancePage);
    await flushPromises();

    expect(wrapper.get("h1").text()).toBe("管理员维护");
    expect(wrapper.text()).toContain("协议 v1");
    expect(wrapper.text()).toContain("真实分区合并执行器");
    expect(wrapper.text()).toContain("运行时门禁关闭");
  });

  it("requires the exact one-time phrase before invoking UAC", async () => {
    const wrapper = mount(SystemMaintenancePage);
    await flushPromises();
    await wrapper.get(".item-card button").trigger("click");
    await flushPromises();

    const executeButton = wrapper.get(".confirmation-card button");
    expect(executeButton.attributes("disabled")).toBeDefined();
    await wrapper.get(".confirmation-card input").setValue("确认关闭休眠功能");
    await executeButton.trigger("click");
    await flushPromises();

    expect(executePrivilegedOperation).toHaveBeenCalledWith({
      challengeId: "challenge-1",
      confirmationToken: "token-1",
      confirmationPhrase: "确认关闭休眠功能",
    });
    expect(wrapper.text()).toContain("休眠功能已关闭");
  });
});
