import { afterEach, describe, expect, it, vi } from "vitest";

import {
  checkForUpdates,
  getAppSettings,
  updateAppSettings,
} from "@/services/operations-service";

describe("operations service", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("validates browser settings tiers before storing them", async () => {
    const settings = await getAppSettings();
    const updated = await updateAppSettings({
      ...settings,
      automaticMaintenance: "weekly",
      quarantineRetentionDays: 15,
    });
    expect(updated.automaticMaintenance).toBe("weekly");
    await expect(
      updateAppSettings({ ...settings, quarantineMaxBytes: 17 }),
    ).rejects.toThrow("不受支持");
  });

  it("accepts only official release metadata and reports integrity assets", async () => {
    vi.stubGlobal("__APP_VERSION__", "0.1.0");
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              tag_name: "v0.2.0",
              html_url:
                "https://github.com/leaf-zly/clarity-disk/releases/tag/v0.2.0",
              published_at: "2026-08-17T00:00:00Z",
              body: "Signed release",
              assets: [{ name: "Clarity.exe" }, { name: "SHA256SUMS.txt" }],
            }),
            { status: 200 },
          ),
      ),
    );

    const release = await checkForUpdates();
    expect(release.updateAvailable).toBe(true);
    expect(release.hasChecksums).toBe(true);
    expect(release.hasWindowsInstaller).toBe(true);
  });
});
