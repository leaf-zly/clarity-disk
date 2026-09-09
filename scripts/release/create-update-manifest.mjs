import { readFile, writeFile } from "node:fs/promises";
import { basename, resolve } from "node:path";
import { pathToFileURL } from "node:url";

/**
 * Builds a Windows x64 Tauri manifest pointing at an immutable official release.
 * Rejects malformed versions/signatures and unsafe asset names before publication.
 * @param {{version: string, channel: 'preview'|'stable', installerName: string, signature: string, notes: string, publishedAt: string}} input Validated build metadata.
 * @returns {object} Static updater manifest. The signature authenticates installer bytes, not the JSON.
 */
export function createUpdateManifest(input) {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(input.version))
    throw new Error("Invalid update version");
  if (!["preview", "stable"].includes(input.channel))
    throw new Error("Invalid update channel");
  if (!/^[\w .-]+_x64-setup\.exe$/.test(input.installerName))
    throw new Error("Unexpected Windows installer name");
  const signature = input.signature.trim();
  if (
    !/^[A-Za-z0-9+/]+={0,2}$/.test(signature) ||
    !Buffer.from(signature, "base64")
      .toString("utf8")
      .startsWith("untrusted comment:")
  )
    throw new Error("Invalid updater signature encoding");
  if (!Number.isFinite(Date.parse(input.publishedAt)))
    throw new Error("Invalid publication date");
  const tag = `${input.channel === "preview" ? "preview-" : ""}v${input.version}`;
  return {
    version: input.version,
    notes: input.notes.slice(0, 8000),
    pub_date: new Date(input.publishedAt).toISOString(),
    platforms: {
      "windows-x86_64": {
        signature,
        // GitHub normalizes spaces in uploaded asset names to dots, even when
        // the local filename supplied to `gh release upload` contains spaces.
        url: `https://github.com/leaf-zly/clarity-disk/releases/download/${tag}/${encodeURIComponent(input.installerName.replaceAll(" ", "."))}`,
      },
    },
  };
}

// File-writing CLI is intentionally restricted to the GitHub-hosted release workflow.
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  if (process.env.GITHUB_ACTIONS !== "true")
    throw new Error("Updater manifests are generated only in GitHub Actions");
  const [installer, signatureFile, notesFile, output] = process.argv.slice(2);
  if (!installer || !signatureFile || !notesFile || !output)
    throw new Error("Expected installer, signature, notes and output paths");
  const manifest = createUpdateManifest({
    version: process.env.APP_VERSION,
    channel: process.env.VITE_UPDATE_CHANNEL ?? "preview",
    installerName: basename(installer),
    signature: await readFile(signatureFile, "utf8"),
    notes: await readFile(notesFile, "utf8"),
    publishedAt: new Date().toISOString(),
  });
  await writeFile(output, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
}
