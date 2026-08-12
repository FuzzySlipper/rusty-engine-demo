import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const manifest = JSON.parse(readFileSync(resolve(root, "content/doom-e1m1/sprites/manifest.json"), "utf8"));
const project = JSON.parse(readFileSync(resolve(root, "content/projects/doom-sprite-scale-room.project.json"), "utf8"));
const scale = manifest.contract.scale.presentationDoomUnitsPerEngineUnit;
const close = (actual, expected) => assert.ok(Math.abs(actual - expected) < 1e-6, `${actual} ~= ${expected}`);
assert.equal(scale, 28);
assert.equal(project.projectId, "doom-sprite-scale-room");
assert.equal(project.scenes.length, 1);
assert.equal(project.scenes[0].voxelEnvironment.materialVoxels.length, 168);
assert.equal(project.scenes[0].voxelEnvironment.voxelAssets, undefined);

const actors = manifest.atlases.find((atlas) => atlas.id === "sprite/doom-e1m1-actors");
for (const [prefix, family] of [["POSS", "zombieman"], ["SPOS", "shotgun-guy"], ["TROO", "imp"]]) {
  const contract = manifest.contract.families.find((candidate) => candidate.prefix === prefix);
  assert.deepEqual(contract.dimensionsDoomUnits, { radius: 20, height: 56 });
  const lump = contract.directionalFrames.find((frame) => frame.frame === "A")
    .rotations.find((rotation) => rotation.rotation === 1).sourceLump;
  const frame = actors.frames.find((candidate) => candidate.sourceLump === lump);
  const asset = project.assets.find((candidate) => candidate.id === `sprite/calibration-${family}`);
  assert.equal(asset.spriteAtlas.frames.length, 1);
  assert.equal(asset.spriteAtlas.frames[0].frame, 0);
  asset.spriteAtlas.frames[0].uvMin.forEach((value, index) => close(value, frame.uv.min[index]));
  asset.spriteAtlas.frames[0].uvMax.forEach((value, index) => close(value, frame.uv.max[index]));
  asset.spriteAtlas.frames[0].size.forEach((value, index) => close(value, frame.pixelSize[index] / scale));
  const entity = project.scenes[0].entities.find((candidate) => candidate.renderable?.asset === asset.id);
  close(entity.translation[1], 0.25 + (0.5 - frame.pivot[1]) * frame.pixelSize[1] / scale);
}

const references = project.scenes[0].entities.filter((entity) => entity.renderable?.asset === "mesh/column");
assert.equal(references.length, 3);
assert.ok(references.every((entity) => entity.renderable.localTransform.scale[1] === 1));
console.log("DOOM_SPRITE_SCALE_ROOM_OK actors=3 references=3 actorHeight=2 presentationScale=28:1");
