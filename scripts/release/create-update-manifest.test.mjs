import assert from "node:assert/strict";
import test from "node:test";
import { createUpdateManifest } from "./create-update-manifest.mjs";

const fixture = {
  version: "0.1.1",
  channel: "preview",
  installerName: "Clarity Disk_0.1.1_x64-setup.exe",
  signature: Buffer.from("untrusted comment: fixture only\nsignature").toString(
    "base64",
  ),
  notes: "Online updates",
  publishedAt: "2026-09-09T00:00:00Z",
};
test("preview manifest uses immutable official version assets, not mutable feed assets", () => {
  const manifest = createUpdateManifest(fixture);
  assert.equal(manifest.version, "0.1.1");
  assert.equal(
    manifest.platforms["windows-x86_64"].url,
    "https://github.com/leaf-zly/clarity-disk/releases/download/preview-v0.1.1/Clarity%20Disk_0.1.1_x64-setup.exe",
  );
});
test("stable manifests never reference preview installers", () => {
  assert.match(
    createUpdateManifest({ ...fixture, channel: "stable" }).platforms[
      "windows-x86_64"
    ].url,
    /\/v0\.1\.1\//,
  );
});
test("rejects unsafe names, unsupported channels, invalid signatures and versions", () => {
  for (const override of [
    { installerName: "../payload.exe" },
    { channel: "other" },
    { signature: "" },
    { signature: "ZmFrZQ==" },
    { version: "0.1.1-evil" },
    { version: "01.1.1" },
    { publishedAt: "invalid" },
  ]) {
    assert.throws(() => createUpdateManifest({ ...fixture, ...override }));
  }
});
