import { enableAutoUnmount, flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import SettingsPage from "@/pages/SettingsPage.vue";
import {
  clearCrashDiagnostics,
  getAppSettings,
  getDiagnosticsSnapshot,
  updateAppSettings,
} from "@/services/operations-service";

vi.mock("@/services/operations-service", () => ({
  getAppSettings: vi.fn(),
  getDiagnosticsSnapshot: vi.fn(),
  updateAppSettings: vi.fn(),
  clearCrashDiagnostics: vi.fn(),
  runAutomaticMaintenance: vi.fn(),
}));

enableAutoUnmount(afterEach);

describe("SettingsPage independent diagnostics", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(getAppSettings).mockResolvedValue({
      schemaVersion: 1,
      theme: "system",
      language: "simplifiedChinese",
      logLevel: "standard",
      retainCrashDiagnostics: true,
      notificationsEnabled: true,
      launchAtLogin: false,
      quarantineRetentionDays: 30,
      quarantineMaxBytes: 10 * 1024 ** 3,
      automaticMaintenance: "disabled",
      ignoredScanRoots: [],
      updateChecksEnabled: true,
    });
    vi.mocked(getDiagnosticsSnapshot).mockResolvedValue({
      crashReports: [
        {
          reportId: "crash-1",
          appVersion: "0.1.0",
          occurredAtUnixMs: 1,
          sourceFile: null,
          sourceLine: null,
          classification: "panic",
        },
      ],
      performanceMetrics: [],
      retentionEnabled: true,
    });
    vi.mocked(updateAppSettings).mockImplementation(async (value) => value);
    vi.mocked(clearCrashDiagnostics).mockResolvedValue(undefined);
  });

  it("loads editable preferences when diagnostics cannot be read", async () => {
    vi.mocked(getDiagnosticsSnapshot).mockRejectedValueOnce(
      new Error("read failed"),
    );
    const wrapper = mount(SettingsPage);
    await flushPromises();
    expect(
      wrapper.get("button.primary").attributes("disabled"),
    ).toBeUndefined();
    expect(wrapper.find("select").exists()).toBe(true);
    expect(wrapper.text()).toContain("诊断信息暂时无法读取");
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
  });

  it("preserves save success and theme when the subsequent diagnostic read fails", async () => {
    const wrapper = mount(SettingsPage);
    await flushPromises();
    await wrapper.get("select").setValue("dark");
    vi.mocked(getDiagnosticsSnapshot).mockRejectedValueOnce(
      new Error("read failed"),
    );
    await wrapper.get("button.primary").trigger("click");
    await flushPromises();
    expect(updateAppSettings).toHaveBeenLastCalledWith(
      expect.objectContaining({ theme: "dark" }),
    );
    expect(wrapper.text()).toContain("设置已验证并保存。");
    expect(wrapper.text()).toContain("诊断信息暂时无法读取");
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("does not misreport a completed clear as failed after a read error", async () => {
    const wrapper = mount(SettingsPage);
    await flushPromises();
    vi.mocked(getDiagnosticsSnapshot).mockRejectedValueOnce(
      new Error("read failed"),
    );
    await wrapper
      .findAll("button")
      .find((button) => button.text().includes("清除崩溃标记"))!
      .trigger("click");
    await flushPromises();
    expect(clearCrashDiagnostics).toHaveBeenCalledOnce();
    expect(wrapper.text()).toContain("本地崩溃标记已清除。");
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
  });

  it("locks the preference form and duplicate saves while persistence is pending", async () => {
    const wrapper = mount(SettingsPage);
    await flushPromises();
    let finish!: () => void;
    vi.mocked(updateAppSettings).mockImplementationOnce(
      (value) =>
        new Promise((resolve) => {
          finish = () => resolve(value);
        }),
    );
    await wrapper.get("button.primary").trigger("click");
    expect(wrapper.get("fieldset").attributes("disabled")).toBeDefined();
    await wrapper.get("button.primary").trigger("click");
    expect(updateAppSettings).toHaveBeenCalledOnce();
    finish();
    await flushPromises();
    expect(wrapper.get("fieldset").attributes("disabled")).toBeUndefined();
  });
});
