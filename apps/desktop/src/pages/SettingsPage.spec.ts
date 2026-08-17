import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import SettingsPage from "@/pages/SettingsPage.vue";
import { updateAppSettings } from "@/services/operations-service";

const settings = {
  schemaVersion: 1 as const,
  theme: "system" as const,
  language: "simplifiedChinese" as const,
  logLevel: "standard" as const,
  retainCrashDiagnostics: true,
  notificationsEnabled: true,
  launchAtLogin: false,
  quarantineRetentionDays: 30 as const,
  quarantineMaxBytes: 10 * 1024 ** 3,
  automaticMaintenance: "disabled" as const,
  ignoredScanRoots: [],
  updateChecksEnabled: true,
};

vi.mock("@/services/operations-service", () => ({
  getAppSettings: vi.fn(async () => structuredClone(settings)),
  getDiagnosticsSnapshot: vi.fn(async () => ({
    crashReports: [],
    performanceMetrics: [],
    retentionEnabled: true,
  })),
  updateAppSettings: vi.fn(async (value) => value),
  runAutomaticMaintenance: vi.fn(async () => ({
    decision: {
      shouldRun: false,
      reason: "disabled",
      nextEligibleAtUnixMs: null,
    },
    preview: null,
  })),
  clearCrashDiagnostics: vi.fn(async () => undefined),
  checkForUpdates: vi.fn(),
}));

describe("SettingsPage", () => {
  beforeEach(() => {
    vi.mocked(updateAppSettings).mockImplementation(async (value) => value);
  });

  it("shows privacy-first automatic maintenance and release protections", async () => {
    const wrapper = mount(SettingsPage);
    await flushPromises();

    expect(wrapper.get("h1").text()).toBe("设置与隐私");
    expect(wrapper.text()).toContain("不会自动删除");
    expect(wrapper.text()).toContain("Authenticode 与 SHA-256");
    expect(wrapper.text()).toContain("首页 1.5 秒");
  });

  it("shows a localized actionable error when Windows startup integration fails", async () => {
    vi.mocked(updateAppSettings).mockRejectedValueOnce(
      new Error("Windows rejected the fixed launch-at-login update"),
    );
    const wrapper = mount(SettingsPage);
    await flushPromises();

    await wrapper.get("button.primary").trigger("click");
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toBe(
      "Windows 登录启动设置保存失败，其他设置未更改。",
    );
  });
});
