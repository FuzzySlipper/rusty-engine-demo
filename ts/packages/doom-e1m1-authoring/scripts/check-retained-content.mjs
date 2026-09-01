import { createHash } from "node:crypto";
import { readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(
  fileURLToPath(new URL("..", import.meta.url)),
);
const repoRoot = resolve(packageRoot, "../../../");
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const readJson = (relativePath) =>
  JSON.parse(readFileSync(resolve(repoRoot, relativePath), "utf8"));

const voxelPath = "content/doom-e1m1/doom-e1m1.voxel.json";
const voxel = readJson(voxelPath);
if (
  voxel.assetId !== "voxel-volume/doom-e1m1" ||
  voxel.voxelDataHash !==
    "sha256:321d385afd71fadd4e32e05ae9b58fd251cfcc34ca50113d5e7015b85d47135a" ||
  voxel.contentHash !==
    "sha256:06b2cea963c98806b33bec61b9ac57f98ff2f2c9355483ccee9a1702c31b85df"
) {
  throw new Error(`${voxelPath} is not the retained admitted E1M1 voxel asset`);
}

const textures = readJson("content/doom-e1m1/textures/manifest.json");
for (const entry of textures.entries) {
  const relativePath = `content/doom-e1m1/textures/${entry.kind}/${entry.name}.png`;
  const bytes = readFileSync(resolve(repoRoot, relativePath));
  if (sha256(bytes) !== entry.pngSha256 || bytes.byteLength !== entry.pngByteLength) {
    throw new Error(`${relativePath} no longer matches its E1M1 texture manifest`);
  }
}
const skyPath = "content/doom-e1m1/textures/sky/SKY1.png";
const skyBytes = readFileSync(resolve(repoRoot, skyPath));
if (sha256(skyBytes) !== textures.sky.pngSha256 || skyBytes.byteLength !== textures.sky.pngByteLength) {
  throw new Error(`${skyPath} no longer matches its E1M1 texture manifest`);
}

const props = readJson("content/doom-e1m1/props/source-manifest.json");
for (const license of props.licenses ?? []) {
  const bytes = readFileSync(resolve(repoRoot, license.path));
  if (sha256(bytes) !== license.sha256) {
    throw new Error(`${license.path} no longer matches its recorded license hash`);
  }
}
for (const asset of props.assets ?? []) {
  for (const [path, expectedHash] of [
    [asset.importSourcePath, asset.contentSha256],
    [asset.sourcePath, asset.sourceSha256],
  ]) {
    if (!path || !expectedHash) continue;
    const bytes = readFileSync(resolve(repoRoot, path));
    if (sha256(bytes) !== expectedHash) {
      throw new Error(`${path} no longer matches ${asset.assetId} provenance`);
    }
  }
  for (const dependency of asset.sourceDependencies ?? []) {
    const bytes = readFileSync(resolve(repoRoot, dependency.path));
    if (sha256(bytes) !== dependency.sha256) {
      throw new Error(`${dependency.path} no longer matches ${asset.assetId} provenance`);
    }
  }
}

const intermediatePath = "content/doom-e1m1/e1m1.intermediate.json";
const intermediate = statSync(resolve(repoRoot, intermediatePath));
if (intermediate.size === 0) throw new Error(`${intermediatePath} must not be empty`);

console.log(
  `retained E1M1 forge passed: ${textures.entries.length} textures, ${props.assets.length} props, admitted voxel, and source provenance`,
);
