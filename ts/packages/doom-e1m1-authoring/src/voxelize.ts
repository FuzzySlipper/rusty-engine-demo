import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import type { E1M1Intermediate } from "./types.js";

const SCALE = 16;
const MIN_X = -768;
const MIN_Y_DOOM = -4864; // doom Y maps to voxel Z
const MIN_FLOOR = -136;

export interface VoxelAssetJson {
  schemaVersion: number;
  assetId: string;
  grid: { coordinateSystem: string; cellSize: number; chunkSize: number; origin: [number, number, number] };
  bounds: { min: [number, number, number]; max: [number, number, number] };
  representation: { kind: string; sparseRuns: { start: [number, number, number]; length: number; materialSlot: number }[] };
  materialPalette: { materialSlot: number; materialAssetId: string; displayName?: string }[];
  materialMap: { sourceMaterialSlot: number; sourceMaterialName?: string; voxelMaterialSlot: number }[];
  provenance: { kind: string; sourcePath: string; sourceSha256: string; sourceByteCount: number; converter: string; settingsSha256: string };
  voxelDataHash: string;
  contentHash: string;
}

function wadToVoxelX(doomX: number): number {
  return Math.floor((doomX - MIN_X) / SCALE);
}
function wadToVoxelZ(doomY: number): number {
  return Math.floor((doomY - MIN_Y_DOOM) / SCALE);
}
function heightToVoxelY(doomHeight: number): number {
  // floorHeight -136 => 0
  return Math.floor((doomHeight - MIN_FLOOR) / SCALE);
}

export function buildDoomVoxelAsset(intermediatePath: string = fileURLToPath(new URL("../../../../content/doom-e1m1/e1m1.intermediate.json", import.meta.url)),
  manifestPath: string = fileURLToPath(new URL("../../../../content/doom-e1m1/textures/manifest.json", import.meta.url)),
  outPath: string = fileURLToPath(new URL("../../../../content/doom-e1m1/doom-e1m1.voxel.json", import.meta.url))): VoxelAssetJson {
  const inter: E1M1Intermediate = JSON.parse(readFileSync(intermediatePath, "utf8"));
  const manifest: { entries: { name: string; kind: "flat" | "wall" }[]; wadSha256: string; wadByteLength: number } = JSON.parse(readFileSync(manifestPath, "utf8"));

  const flatNames = manifest.entries.filter((e) => e.kind === "flat").map((e) => e.name).sort();
  const wallNames = manifest.entries.filter((e) => e.kind === "wall").map((e) => e.name).sort();

  const flatSlot = new Map<string, number>();
  flatNames.forEach((n, i) => flatSlot.set(n, 1 + i));
  const wallSlot = new Map<string, number>();
  wallNames.forEach((n, i) => wallSlot.set(n, 1 + flatNames.length + i));

  // Collect voxels as set of {x,y,z,slot}
  type Voxel = { x: number; y: number; z: number; slot: number };
  const voxels = new Map<string, number>(); // key "x,y,z" -> slot

  const setVoxel = (x: number, y: number, z: number, slot: number) => {
    const key = `${x},${y},${z}`;
    // keep first slot if conflict? Prefer wall over floor? For now first wins, but walls will overwrite floor if same y.
    if (!voxels.has(key)) voxels.set(key, slot);
    else {
      // if existing is floor and new is wall, prefer wall (more visible)
      const existing = voxels.get(key)!;
      // naive: overwrite if wall slot > flat slot
      if (slot > 22) voxels.set(key, slot);
    }
  };

  // Build sector vertex lookup for bbox
  // Map sector index -> sidedefs that reference it
  const sectorSidedefs = new Map<number, number[]>(); // sector idx -> sidedef idxs
  inter.level.sidedefs.forEach((sd, idx) => {
    if (sd.sector < 0 || sd.sector >= inter.level.sectors.length) return;
    const arr = sectorSidedefs.get(sd.sector) ?? [];
    arr.push(idx);
    sectorSidedefs.set(sd.sector, arr);
  });

  // For each sector, emit floor and ceiling layers using bbox fill (over-approx; refines to polygon later)
  for (let si = 0; si < inter.level.sectors.length; si += 1) {
    const sec = inter.level.sectors[si]!;
    const floorSlot = flatSlot.get(sec.floorTexture) ?? 1;
    const ceilSlot = flatSlot.get(sec.ceilingTexture) ?? 1;
    // skip sky ceilings: F_SKY1 should be no voxel (open sky)
    const isSky = sec.ceilingTexture === "F_SKY1";
    // Gather vertices for this sector via its sidedefs' linedefs
    const sidedefIdxs = sectorSidedefs.get(si) ?? [];
    const verts: { x: number; y: number }[] = [];
    for (const sdi of sidedefIdxs) {
      // find linedefs that reference this sidedef as front or back
      for (const ld of inter.level.linedefs) {
        if (ld.frontSidedef === sdi || ld.backSidedef === sdi) {
          const v1 = inter.level.vertices[ld.startVertex]!;
          const v2 = inter.level.vertices[ld.endVertex]!;
          verts.push(v1, v2);
        }
      }
    }
    if (verts.length === 0) continue;
    const xs = verts.map((v) => v.x);
    const ys = verts.map((v) => v.y);
    const minXw = Math.min(...xs);
    const maxXw = Math.max(...xs);
    const minYw = Math.min(...ys);
    const maxYw = Math.max(...ys);
    const minXv = wadToVoxelX(minXw);
    const maxXv = wadToVoxelX(maxXw);
    const minZv = wadToVoxelZ(minYw);
    const maxZv = wadToVoxelZ(maxYw);
    const floorY = heightToVoxelY(sec.floorHeight);
    const ceilY = heightToVoxelY(sec.ceilingHeight);
    // Clamp Y to 0..64
    // Floor layer
    for (let x = minXv; x <= maxXv; x += 1) {
      for (let z = minZv; z <= maxZv; z += 1) {
        // For now fill entire bbox (over-approx); real polygon test would prune
        setVoxel(x, floorY, z, floorSlot);
        if (!isSky && ceilY > floorY) setVoxel(x, ceilY - 1, z, ceilSlot);
      }
    }
  }

  // Walls: for each linedef, emit vertical column along its edge
  for (const ld of inter.level.linedefs) {
    const v1 = inter.level.vertices[ld.startVertex]!;
    const v2 = inter.level.vertices[ld.endVertex]!;
    const frontSd = inter.level.sidedefs[ld.frontSidedef];
    if (!frontSd) continue;
    const frontSec = inter.level.sectors[frontSd.sector];
    if (!frontSec) continue;
    const backSd = ld.backSidedef !== -1 ? inter.level.sidedefs[ld.backSidedef] : null;
    const backSec = backSd ? inter.level.sectors[backSd.sector] : null;

    // Determine wall material: sidedef middle texture for one-sided, else lower/upper
    // For now use middleTexture if present else upper else lower
    const wallName = frontSd.middleTexture && frontSd.middleTexture !== "-"
      ? frontSd.middleTexture
      : frontSd.upperTexture && frontSd.upperTexture !== "-"
      ? frontSd.upperTexture
      : frontSd.lowerTexture && frontSd.lowerTexture !== "-"
      ? frontSd.lowerTexture
      : null;
    const slotForWall = wallName ? (wallSlot.get(wallName) ?? 23) : 23;

    // Height range: one-sided from front floor to front ceiling; two-sided: lower wall from min floor to max floor, upper from min ceiling to max ceiling
    // Simplify: emit from min floor to max ceiling
    let y0: number, y1: number;
    if (!backSec) {
      y0 = heightToVoxelY(frontSec.floorHeight);
      y1 = heightToVoxelY(frontSec.ceilingHeight);
    } else {
      // two-sided: emit lower wall between floors, upper between ceilings
      const lower0 = Math.min(heightToVoxelY(frontSec.floorHeight), heightToVoxelY(backSec.floorHeight));
      const lower1 = Math.max(heightToVoxelY(frontSec.floorHeight), heightToVoxelY(backSec.floorHeight));
      const upper0 = Math.min(heightToVoxelY(frontSec.ceilingHeight), heightToVoxelY(backSec.ceilingHeight));
      const upper1 = Math.max(heightToVoxelY(frontSec.ceilingHeight), heightToVoxelY(backSec.ceilingHeight));
      // For now emit both ranges if non-zero
      // We'll handle via two passes: lower and upper
      // To keep simple, emit full from lower0 to upper1, but that would fill gap where door is.
      // Instead emit lower and upper separately
      // We'll directly emit here for lower and upper
      const wallVox = (a: number, b: number) => {
        if (a >= b) return;
        const steps = Math.max(Math.abs(wadToVoxelX(v2.x) - wadToVoxelX(v1.x)), Math.abs(wadToVoxelZ(v2.y) - wadToVoxelZ(v1.y)), 1);
        for (let s = 0; s <= steps; s += 1) {
          const t = steps === 0 ? 0 : s / steps;
          const x = Math.round(wadToVoxelX(v1.x) * (1 - t) + wadToVoxelX(v2.x) * t);
          const z = Math.round(wadToVoxelZ(v1.y) * (1 - t) + wadToVoxelZ(v2.y) * t);
          for (let y = a; y < b; y += 1) setVoxel(x, y, z, slotForWall);
        }
      };
      wallVox(lower0, lower1);
      wallVox(upper0, upper1);
      continue;
    }

    if (y1! <= y0!) continue;
    const steps = Math.max(Math.abs(wadToVoxelX(v2.x) - wadToVoxelX(v1.x)), Math.abs(wadToVoxelZ(v2.y) - wadToVoxelZ(v1.y)), 1);
    for (let s = 0; s <= steps; s += 1) {
      const t = steps === 0 ? 0 : s / steps;
      const x = Math.round(wadToVoxelX(v1.x) * (1 - t) + wadToVoxelX(v2.x) * t);
      const z = Math.round(wadToVoxelZ(v1.y) * (1 - t) + wadToVoxelZ(v2.y) * t);
      for (let y = y0; y < y1; y += 1) setVoxel(x, y, z, slotForWall);
    }
  }

  // Convert map to sorted runs along +X
  // Group by y,z, then sort x and merge contiguous same slot
  const byYZ = new Map<string, { x: number; slot: number }[]>();
  for (const [key, slot] of voxels) {
    const [xStr, yStr, zStr] = key.split(",");
    const x = Number(xStr), y = Number(yStr), z = Number(zStr);
    const k = `${y},${z}`;
    const arr = byYZ.get(k) ?? [];
    arr.push({ x, slot });
    byYZ.set(k, arr);
  }

  const runs: { start: [number, number, number]; length: number; materialSlot: number }[] = [];
  for (const [yz, list] of byYZ) {
    const [yStr, zStr] = yz.split(",");
    const y = Number(yStr), z = Number(zStr);
    list.sort((a, b) => a.x - b.x || a.slot - b.slot);
    let runStart = list[0]!.x;
    let runSlot = list[0]!.slot;
    let runLen = 1;
    let prevX = runStart;
    for (let i = 1; i < list.length; i += 1) {
      const cur = list[i]!;
      if (cur.x === prevX + 1 && cur.slot === runSlot) {
        runLen += 1;
      } else {
        runs.push({ start: [runStart, y, z], length: runLen, materialSlot: runSlot });
        runStart = cur.x;
        runSlot = cur.slot;
        runLen = 1;
      }
      prevX = cur.x;
    }
    runs.push({ start: [runStart, y, z], length: runLen, materialSlot: runSlot });
  }

  runs.sort((a, b) => a.start[1] - b.start[1] || a.start[2] - b.start[2] || a.start[0] - b.start[0]);

  // Bounds
  const xs = [...voxels.keys()].map((k) => Number(k.split(",")[0]));
  const ys = [...voxels.keys()].map((k) => Number(k.split(",")[1]));
  const zs = [...voxels.keys()].map((k) => Number(k.split(",")[2]));
  const minXv = xs.length ? Math.min(...xs) : 0;
  const maxXv = xs.length ? Math.max(...xs) : 0;
  const minYv = ys.length ? Math.min(...ys) : 0;
  const maxYv = ys.length ? Math.max(...ys) : 0;
  const minZv = zs.length ? Math.min(...zs) : 0;
  const maxZv = zs.length ? Math.max(...zs) : 0;

  // Material palette/map
  const palette: { materialSlot: number; materialAssetId: string; displayName?: string }[] = [];
  const matMap: { sourceMaterialSlot: number; sourceMaterialName?: string; voxelMaterialSlot: number }[] = [];
  let srcSlot = 0;
  const toKebab = (s: string) => s.toLowerCase().replace(/_/g, "-").replace(/[^a-z0-9-]/g, "-");
  for (const name of flatNames) {
    const slot = flatSlot.get(name)!;
    palette.push({ materialSlot: slot, materialAssetId: `material/doom-flat-${toKebab(name)}`, displayName: name });
    matMap.push({ sourceMaterialSlot: srcSlot++, sourceMaterialName: name, voxelMaterialSlot: slot });
  }
  for (const name of wallNames) {
    const slot = wallSlot.get(name)!;
    palette.push({ materialSlot: slot, materialAssetId: `material/doom-wall-${toKebab(name)}`, displayName: name });
    matMap.push({ sourceMaterialSlot: srcSlot++, sourceMaterialName: name, voxelMaterialSlot: slot });
  }
  palette.sort((a, b) => a.materialSlot - b.materialSlot);
  matMap.sort((a, b) => a.voxelMaterialSlot - b.voxelMaterialSlot);

  const voxelCount = [...voxels.keys()].length;
  const asset: VoxelAssetJson = {
    schemaVersion: 1,
    assetId: "voxel-volume/doom-e1m1",
    grid: { coordinateSystem: "rightHandedYUp", cellSize: 1, chunkSize: 16, origin: [0, 0, 0] },
    bounds: { min: [minXv, minYv, minZv], max: [maxXv, maxYv, maxZv] },
    representation: { kind: "sparseRuns", sparseRuns: runs },
    materialPalette: palette,
    materialMap: matMap,
    provenance: {
      kind: "generatedEnvironment",
      sourcePath: "doom1.wad:E1M1",
      sourceSha256: `sha256:${inter.source.wadSha256}`,
      sourceByteCount: inter.source.wadByteLength,
      converter: "doom-e1m1.voxelize.v1",
      settingsSha256: "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    },
    voxelDataHash: "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    contentHash: "sha256:0000000000000000000000000000000000000000000000000000000000000000",
  };

  // Budget checks
  if (voxelCount > 1_000_000) throw new Error(`voxel count ${voxelCount} exceeds 1M`);
  if (runs.length > 100_000) throw new Error(`run count ${runs.length} exceeds 100k`);
  if (asset.bounds.max[0] - asset.bounds.min[0] > 1000 || asset.bounds.max[2] - asset.bounds.min[2] > 1000) {
    console.warn(`bounds large: ${JSON.stringify(asset.bounds)}`);
  }

  mkdirSync(resolve(outPath, ".."), { recursive: true });
  writeFileSync(outPath, `${JSON.stringify(asset, null, 2)}\n`, "utf8");
  console.log(`Wrote ${outPath} voxels=${voxelCount} runs=${runs.length} bounds=${JSON.stringify(asset.bounds)} palette=${palette.length}`);
  return asset;
}

// CLI
if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  buildDoomVoxelAsset();
}
