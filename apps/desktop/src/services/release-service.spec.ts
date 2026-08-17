import { afterEach, describe, expect, it, vi } from "vitest";

import { checkForUpdates } from "@/services/release-service";

describe("release service", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("reports that no formal release has been published without an error", async () => {
    vi.stubGlobal("__APP_VERSION__", "0.1.0");
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("[]", { status: 200 })),
    );

    const release = await checkForUpdates();

    expect(release.updateAvailable).toBe(false);
    expect(release.latestVersion).toBe("0.1.0");
    expect(release.publishedAt).toBe("");
  });

  it("treats GitHub's privacy-preserving 404 as an unpublished state", async () => {
    vi.stubGlobal("__APP_VERSION__", "0.1.0");
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("Not Found", { status: 404 })),
    );

    const release = await checkForUpdates();

    expect(release.updateAvailable).toBe(false);
    expect(release.publishedAt).toBe("");
    expect(release.notes).toContain("尚未发布");
  });

  it("accepts integrity metadata only from the official release page", async () => {
    vi.stubGlobal("__APP_VERSION__", "0.1.0");
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify([
              {
                tag_name: "v0.2.0",
                html_url:
                  "https://github.com/leaf-zly/clarity-disk/releases/tag/v0.2.0",
                published_at: "2026-08-17T00:00:00Z",
                body: "Signed release",
                assets: [
                  { name: "Clarity Disk.exe" },
                  { name: "SHA256SUMS.txt" },
                ],
              },
            ]),
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
