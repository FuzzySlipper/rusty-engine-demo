#!/usr/bin/env node

import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const projectPath = resolve(root, "content/projects/doom-e1m1.project.json");
const voxelPath = resolve(root, "content/doom-e1m1/doom-e1m1.voxel.json");
const textureManifestPath = resolve(root, "content/doom-e1m1/textures/manifest.json");
const outputPath = resolve(root, "content/doom-e1m1/doom-e1m1.asset-catalog.json");

const [project, voxel, textureManifest] = await Promise.all(
  [projectPath, voxelPath, textureManifestPath].map(async (path) => JSON.parse(await readFile(path, "utf8"))),
);
const assets = new Map(project.assets.map((asset) => [asset.id, asset]));
const texturesByHash = new Map(textureManifest.entries.map((entry) => [entry.pngSha256, entry]));
const paletteBySlot = new Map(voxel.materialPalette.map((row) => [row.materialSlot, row]));
if (paletteBySlot.size !== voxel.materialPalette.length) throw new Error("voxel material palette has duplicate slots");
const usedSlots = new Set(voxel.representation?.sparseRuns?.map(({ materialSlot }) => materialSlot));
if (usedSlots.size === 0) throw new Error("voxel representation has no used material slots");
const materialIds = [...usedSlots]
  .sort((left, right) => left - right)
  .map((slot) => {
    const row = paletteBySlot.get(slot);
    if (!row?.materialAssetId) throw new Error(`voxel representation references unknown palette slot ${slot}`);
    return row.materialAssetId;
  });
if (new Set(materialIds).size !== materialIds.length) throw new Error("voxel material palette has duplicate material asset IDs");

const normalizeHash = (hash, label) => {
  if (typeof hash !== "string") throw new Error(`${label} lacks a SHA-256 hash`);
  return hash.replace(/^sha256:/, "");
};
const canonicalReference = (reference, label) => ({
  id: reference.id,
  version: reference.version ?? { req: "any" },
  hash: reference.hash === null || reference.hash === undefined ? null : normalizeHash(reference.hash, label),
});
const canonicalSurface = (surface, label) => {
  if (!surface || surface.mapping?.kind !== "repeat") throw new Error(`${label} must define a repeat voxel surface`);
  return {
    schemaVersion: surface.schemaVersion,
    mapping: {
      kind: "repeat",
      texture: canonicalReference(surface.mapping.texture, `${label} voxel texture`),
      tile_scale_cells: surface.mapping.tileScaleCells ?? surface.mapping.tile_scale_cells,
      tile_origin_cells: surface.mapping.tileOriginCells ?? surface.mapping.tile_origin_cells,
    },
    alphaMode: surface.alphaMode,
  };
};

const entries = [];
const textureIds = new Set();
for (const materialId of materialIds) {
  const asset = assets.get(materialId);
  if (!asset?.catalog || !asset.material) throw new Error(`missing authored material ${materialId}`);
  const surface = canonicalSurface(asset.material.style.voxelSurface, materialId);
  textureIds.add(surface.mapping.texture.id);
  entries.push({
    id: asset.id,
    version: asset.catalog.version,
    hash: normalizeHash(asset.catalog.hash, materialId),
    sourcePath: asset.catalog.sourcePath,
    label: asset.catalog.label,
    dependencies: (asset.catalog.dependencies ?? []).map((reference) => canonicalReference(reference, `${materialId} dependency`)),
    material: {
      authority: asset.material.authority,
      style: { ...asset.material.style, voxelSurface: surface },
    },
    texture: null,
    voxelAtlas: null,
  });
}

for (const textureId of [...textureIds].sort()) {
  const asset = assets.get(textureId);
  if (!asset?.catalog) throw new Error(`missing authored texture ${textureId}`);
  const hash = normalizeHash(asset.catalog.hash, textureId);
  const manifestEntry = texturesByHash.get(hash);
  if (!manifestEntry) throw new Error(`texture manifest lacks ${textureId} (${hash})`);
  entries.push({
    id: asset.id,
    version: asset.catalog.version,
    hash,
    sourcePath: asset.catalog.sourcePath,
    label: asset.catalog.label,
    dependencies: (asset.catalog.dependencies ?? []).map((reference) => canonicalReference(reference, `${textureId} dependency`)),
    material: null,
    texture: { width: manifestEntry.width, height: manifestEntry.height, filter: "nearest", wrap: "repeat" },
    voxelAtlas: null,
  });
}

await writeFile(outputPath, `${JSON.stringify({ entries }, null, 2)}\n`);
console.log(`wrote ${outputPath}: ${materialIds.length} used material entries, ${textureIds.size} texture entries`);
