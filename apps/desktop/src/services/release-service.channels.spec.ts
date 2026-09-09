import { afterEach, expect, it, vi } from "vitest";
import { checkForUpdates } from "@/services/release-service";

afterEach(() => vi.unstubAllGlobals());

it("ignores mutable updater feeds and legacy preview tags when listing stable releases", async () => {
  vi.stubGlobal("__APP_VERSION__", "0.1.1");
  const entry = {
    html_url: "https://github.com/leaf-zly/clarity-disk/releases/tag/v0.2.0",
    published_at: "2026-09-09T00:00:00Z",
    body: "release",
    assets: [],
  };
  vi.stubGlobal(
    "fetch",
    vi.fn(
      async () =>
        new Response(
          JSON.stringify([
            { ...entry, tag_name: "update-preview" },
            { ...entry, tag_name: "preview-v0.1.0-f645bfe" },
            { ...entry, tag_name: "v9.0.0", prerelease: true },
            { ...entry, tag_name: "v0.2.0" },
          ]),
        ),
    ),
  );
  const release = await checkForUpdates();
  expect(release.latestVersion).toBe("0.2.0");
  expect(release.updateAvailable).toBe(true);
});
