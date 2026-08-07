import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { test } from "node:test";

/**
 * Palette → material descriptor test for Doom E1M1 (E1M1-6).
 * Mirrors the browser-shell view-model style: it proves the immutable
 * projection can turn the voxel palette into typed descriptors without
 * acquiring a second RendererSurface or gameplay authority.
 */

const voxelPath = fileURLToPath(new URL("../../../../content/doom-e1m1/doom-e1m1.voxel.json", import.meta.url));
const manifestPath = fileURLToPath(new URL("../../../../content/doom-e1m1/textures/manifest.json", import.meta.url));

test("doom palette maps each flat/wall to a VTX6 repeat descriptor", () => {
  const voxel = JSON.parse(readFileSync(voxelPath, "utf8")) as {
    materialPalette: { materialSlot: number; materialAssetId: string }[];
    materialMap: { sourceMaterialSlot: number; voxelMaterialSlot: number; sourceMaterialName?: string }[];
  };
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8")) as {
    entries: { name: string; kind: "flat" | "wall"; width: number; height: number; tileScale: [number, number] | null }[];
  };

  assert.equal(voxel.materialPalette.length, 54, "palette must be 22 flats + 32 walls");
  assert.equal(manifest.entries.length, 54);

  const manifestByName = new Map(manifest.entries.map((e) => [e.name, e] as const));
  for (const entry of voxel.materialPalette) {
    // materialAssetId is material/doom-flat-*/wall-*
    assert.match(entry.materialAssetId, /^material\/doom-(flat|wall)-[a-z0-9-]+$/);
    const display = (entry as unknown as { displayName?: string }).displayName;
    assert.ok(display, `palette ${entry.materialSlot} should carry displayName`);
    const manifestEntry = manifestByName.get(display!);
    assert.ok(manifestEntry, `manifest should contain ${display}`);
    const expectedScale: [number, number] = manifestEntry!.tileScale ?? [1 / manifestEntry!.width, 1 / manifestEntry!.height];
    // VTX bounds: 1/256 .. 4096
    for (const scale of expectedScale) {
      assert.ok(scale >= 1 / 256 && scale <= 4096, `tileScale ${scale} out of VTX bounds for ${display}`);
    }
    // flat must be 1/64 exactly, wall 1/width
    if (manifestEntry!.kind === "flat") {
      assert.equal(expectedScale[0], 1 / 64);
      assert.equal(expectedScale[1], 1 / 64);
    } else {
      assert.equal(expectedScale[0], 1 / manifestEntry!.width);
    }
  }

  // materialMap must be 1:1 with palette and stable sorted
  assert.equal(voxel.materialMap.length, 54);
  const sortedPalette = [...voxel.materialPalette].sort((a, b) => a.materialSlot - b.materialSlot);
  const sortedMap = [...voxel.materialMap].sort((a, b) => a.voxelMaterialSlot - b.voxelMaterialSlot);
  for (let i = 0; i < 54; i += 1) {
    assert.equal(sortedPalette[i]!.materialSlot, sortedMap[i]!.voxelMaterialSlot);
  }
});

test("doom voxel asset stays within the single RendererSurface budget", () => {
  const voxel = JSON.parse(readFileSync(voxelPath, "utf8")) as {
    representation: { kind: string; sparseRuns: { start: [number, number, number]; length: number }[] };
    bounds: { min: [number, number, number]; max: [number, number, number] };
  };
  const runs = voxel.representation.sparseRuns;
  const voxels = runs.reduce((sum, r) => sum + r.length, 0);
  assert.ok(voxels <= 1_000_000, `voxels ${voxels} must be ≤1M`);
  assert.ok(runs.length <= 100_000, `runs ${runs.length} must be ≤100k`);
  // The same greedy-quad path that VTX measured at +32 B/quad will see ~runs quads
  assert.ok(runs.length < 20000, `runs ${runs.length} well below headroom for one surface`);
});
