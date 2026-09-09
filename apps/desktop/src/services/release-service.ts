import type { UpdateRelease } from "@/types/operations";

const RELEASES_URL =
  "https://api.github.com/repos/leaf-zly/clarity-disk/releases?per_page=100";
const RELEASES_PAGE = "https://github.com/leaf-zly/clarity-disk/releases";

/**
 * Checks the official published-release list without downloading an asset.
 * An empty list is a normal pre-release state, not an update-service failure.
 */
export async function checkForUpdates(): Promise<UpdateRelease> {
  const response = await fetch(RELEASES_URL, {
    headers: { Accept: "application/vnd.github+json" },
    signal: AbortSignal.timeout(20_000),
  });
  // GitHub deliberately returns 404 for private/inaccessible repositories as
  // well as missing resources. Neither state proves that an update failed.
  if (response.status === 404) return unpublishedRelease();
  if (!response.ok) throw new Error(`更新服务返回 HTTP ${response.status}`);
  const payload = (await response.json()) as unknown;
  if (!Array.isArray(payload)) throw new Error("更新元数据格式无效");
  // Legacy preview tags and mutable updater feeds are not semantic stable releases.
  const release = payload
    .filter(isReleasePayload)
    .filter((item) => !item.prerelease && !item.draft)
    .sort((left, right) =>
      compareVersions(
        right.tag_name.replace(/^v/, ""),
        left.tag_name.replace(/^v/, ""),
      ),
    )[0];
  if (release === undefined) return unpublishedRelease();
  if (!isReleasePayload(release)) throw new Error("更新元数据格式无效");

  const latestVersion = release.tag_name.replace(/^v/, "");
  const assetNames = release.assets.map((asset) => asset.name.toLowerCase());
  return {
    currentVersion: __APP_VERSION__,
    latestVersion,
    updateAvailable: compareVersions(latestVersion, __APP_VERSION__) > 0,
    releaseUrl: release.html_url,
    publishedAt: release.published_at,
    notes: release.body.slice(0, 8_000),
    hasChecksums: assetNames.includes("sha256sums.txt"),
    hasWindowsInstaller: assetNames.some((name) => name.endsWith(".exe")),
    signatureRequired: true,
  };
}

/** Read-only stable release metadata; never authorizes installation. */
interface GitHubReleasePayload {
  prerelease?: boolean;
  draft?: boolean;
  tag_name: string;
  html_url: string;
  published_at: string;
  body: string;
  assets: Array<{ name: string }>;
}

function unpublishedRelease(): UpdateRelease {
  return {
    currentVersion: __APP_VERSION__,
    latestVersion: __APP_VERSION__,
    updateAvailable: false,
    releaseUrl: RELEASES_PAGE,
    publishedAt: "",
    notes: "官方仓库尚未发布正式版本。",
    hasChecksums: false,
    hasWindowsInstaller: false,
    signatureRequired: true,
  };
}

function isReleasePayload(value: unknown): value is GitHubReleasePayload {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<GitHubReleasePayload>;
  return (
    typeof candidate.tag_name === "string" &&
    /^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(candidate.tag_name) &&
    typeof candidate.html_url === "string" &&
    candidate.html_url.startsWith(`${RELEASES_PAGE}/`) &&
    typeof candidate.published_at === "string" &&
    typeof candidate.body === "string" &&
    Array.isArray(candidate.assets) &&
    candidate.assets.every((asset) => typeof asset?.name === "string")
  );
}

function compareVersions(left: string, right: string): number {
  const normalize = (value: string) =>
    value
      .split("-")[0]
      ?.split(".")
      .slice(0, 3)
      .map((part) => Number.parseInt(part ?? "0", 10)) ?? [0, 0, 0];
  const leftParts = normalize(left);
  const rightParts = normalize(right);
  for (let index = 0; index < 3; index += 1) {
    const difference = (leftParts[index] ?? 0) - (rightParts[index] ?? 0);
    if (difference !== 0) return Math.sign(difference);
  }
  return 0;
}
