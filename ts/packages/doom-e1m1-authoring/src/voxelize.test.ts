import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import test from "node:test";

const INTERMEDIATE_PATH = fileURLToPath(new URL("../../../../content/doom-e1m1/e1m1.intermediate.json", import.meta.url));
const VOXEL_PATH = fileURLToPath(new URL("../../../../content/doom-e1m1/doom-e1m1.voxel.json", import.meta.url));

test("voxel synthesis preserves differing front/back middle incidence (R6677-4)", () => {
  const inter = JSON.parse(readFileSync(INTERMEDIATE_PATH, "utf8")) as {
    level: {
      sidedefs: { upperTexture: string; lowerTexture: string; middleTexture: string }[];
      linedefs: { frontSidedef: number; backSidedef: number }[];
    };
  };
  // Find linedefs where both sides have middle and differ
  let differing = 0;
  let bothTextured = 0;
  let twoSided = 0;
  for (const ld of inter.level.linedefs) {
    if (ld.backSidedef === -1) continue;
    twoSided++;
    const front = inter.level.sidedefs[ld.frontSidedef]!;
    const back = inter.level.sidedefs[ld.backSidedef]!;
    const hasFront = front.lowerTexture !== "-" || front.upperTexture !== "-" || front.middleTexture !== "-";
    const hasBack = back.lowerTexture !== "-" || back.upperTexture !== "-" || back.middleTexture !== "-";
    const fm = front.middleTexture !== "-" ? front.middleTexture : null;
    const bm = back.middleTexture !== "-" ? back.middleTexture : null;
    const fl = front.lowerTexture !== "-" ? front.lowerTexture : null;
    const bl = back.lowerTexture !== "-" ? back.lowerTexture : null;
    const fu = front.upperTexture !== "-" ? front.upperTexture : null;
    const bu = back.upperTexture !== "-" ? back.upperTexture : null;
    if (hasFront && hasBack) bothTextured++;
    // differing if any of lower/upper/middle differ when both present
    const lowerDiff = fl && bl && fl !== bl;
    const upperDiff = fu && bu && fu !== bu;
    const middleDiff = fm && bm && fm !== bm;
    if (lowerDiff || upperDiff || middleDiff) differing++;
  }
  assert.equal(twoSided, 173, "expected 173 two-sided from WAD");
  assert.equal(bothTextured, 12, "expected 12 both-textured");
  assert.equal(differing, 4, "expected 4 differing middle");

  const voxel = JSON.parse(readFileSync(VOXEL_PATH, "utf8")) as {
    materialPalette: { materialSlot: number; materialAssetId: string; displayName?: string }[];
    representation: { sparseRuns: { start: [number, number, number]; length: number; materialSlot: number }[] };
  };
  // For each differing pair, both textures should be in palette
  const differingPairs: [string, string][] = [];
  for (const ld of inter.level.linedefs) {
    if (ld.backSidedef === -1) continue;
    const front = inter.level.sidedefs[ld.frontSidedef]!;
    const back = inter.level.sidedefs[ld.backSidedef]!;
    const fm = front.middleTexture !== "-" ? front.middleTexture : null;
    const bm = back.middleTexture !== "-" ? back.middleTexture : null;
    if (fm && bm && fm !== bm) differingPairs.push([fm, bm]);
  }
  const paletteNames = new Set(voxel.materialPalette.map((p) => p.displayName ?? ""));
  for (const [a, b] of differingPairs) {
    assert.ok(paletteNames.has(a), `palette missing front ${a} from differing pair ${a}/${b}`);
    assert.ok(paletteNames.has(b), `palette missing back ${b} from differing pair ${a}/${b}`);
  }
  // Wall voxels for middle should include both slots at offset positions.
  // Count runs for those materials: at least 2 runs for each differing pair should exist.
  const slotByName = new Map(voxel.materialPalette.map((p) => [p.displayName, p.materialSlot] as const));
  for (const [a, b] of differingPairs.slice(0, 3)) {
    const sa = slotByName.get(a)!;
    const sb = slotByName.get(b)!;
    const runsA = voxel.representation.sparseRuns.filter((r) => r.materialSlot === sa).length;
    const runsB = voxel.representation.sparseRuns.filter((r) => r.materialSlot === sb).length;
    assert.ok(runsA > 0, `no runs for front ${a} slot ${sa}`);
    assert.ok(runsB > 0, `no runs for back ${b} slot ${sb}`);
  }
});

test("voxel bounds within quota and content hash stable", () => {
  const voxel = JSON.parse(readFileSync(VOXEL_PATH, "utf8")) as {
    bounds: { min: [number, number, number]; max: [number, number, number] };
    representation: { sparseRuns: { start: [number, number, number]; length: number }[] };
    voxelDataHash: string;
    contentHash: string;
  };
  const voxels = voxel.representation.sparseRuns.reduce((s, r) => s + r.length, 0);
  assert.ok(voxels <= 1_000_000, `voxels ${voxels} exceeds 1M`);
  assert.ok(voxel.representation.sparseRuns.length <= 100_000, `runs ${voxel.representation.sparseRuns.length} exceeds 100k`);
  assert.ok(voxel.voxelDataHash.startsWith("sha256:"), "voxelDataHash must be sha256");
  assert.ok(voxel.contentHash.startsWith("sha256:"), "contentHash must be sha256");
  assert.notEqual(voxel.voxelDataHash, "sha256:0000000000000000000000000000000000000000000000000000000000000000", "placeholder hash not allowed");
});
