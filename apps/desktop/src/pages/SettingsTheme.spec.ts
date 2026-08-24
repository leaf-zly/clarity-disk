import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";

import SettingsPage from "@/pages/SettingsPage.vue";
import type { AppSettings } from "@/types/operations";

vi.mock("@/services/operations-service", () => ({
  getAppSettings: vi.fn(async () => ({
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
  })),
  getDiagnosticsSnapshot: vi.fn(async () => ({
    crashReports: [],
    performanceMetrics: [],
    retentionEnabled: true,
  })),
  updateAppSettings: vi.fn(async (settings: AppSettings) => settings),
  runAutomaticMaintenance: vi.fn(),
  clearCrashDiagnostics: vi.fn(),
  checkForUpdates: vi.fn(),
}));

describe("SettingsPage theme", () => {
  afterEach(() => {
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.style.removeProperty("color-scheme");
  });

  it("previews a selected appearance immediately", async () => {
    const wrapper = mount(SettingsPage);
    await flushPromises();

    await wrapper.findAll("select")[0]?.setValue("dark");

    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.documentElement.style.colorScheme).toBe("dark");
  });

  it("switches the visible settings copy when English is selected", async () => {
    const wrapper = mount(SettingsPage);
    await flushPromises();

    await wrapper.findAll("select")[1]?.setValue("english");

    expect(wrapper.get("h1").text()).toBe("Settings & Privacy");
    expect(wrapper.text()).toContain("Save changes");
    expect(document.documentElement.lang).toBe("en");
  });
});
