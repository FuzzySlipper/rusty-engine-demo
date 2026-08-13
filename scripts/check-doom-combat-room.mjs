import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const close = (actual, expected, message) => {
  assert.equal(actual.length, expected.length, message);
  actual.forEach((pair, index) => {
    assert.equal(pair.length, expected[index].length, message);
    pair.forEach((value, component) => {
      assert.ok(
        Math.abs(value - expected[index][component]) <= 1e-6,
        `${message}: ${value} != ${expected[index][component]}`,
      );
    });
  });
};
const room = JSON.parse(
  readFileSync(resolve(root, "content/projects/doom-combat-room.project.json"), "utf8"),
);
const doom = JSON.parse(
  readFileSync(resolve(root, "content/projects/doom-e1m1.project.json"), "utf8"),
);
const manifest = JSON.parse(
  readFileSync(resolve(root, "content/doom-e1m1/sprites/manifest.json"), "utf8"),
);

assert.equal(room.projectId, "doom-combat-room");
assert.equal(room.scenes.length, 1);
const scene = room.scenes[0];
const enemies = scene.entities.filter((entity) => entity.enemyCombat != null);
assert.equal(enemies.length, 1, "room must contain exactly one live combat enemy");
const enemy = enemies[0];
const drop = scene.entities.find((entity) => entity.id === enemy.defeatDrop?.pickup);
assert.ok(drop, "enemy drop must exist");
assert.deepEqual(
  enemy.rotation ?? [0, 0, 0, 1],
  [0, 0, 0, 1],
  "logical actor facing is south",
);
assert.deepEqual(
  enemy.renderable.localTransform.rotation ?? [0, 0, 0, 1],
  [0, 0, 0, 1],
);
assert.deepEqual(enemy.renderable.localTransform.scale, [1, 1, 1]);
assert.equal(enemy.renderable.visualBinding.version, 2);

const sourceEnemy = doom.scenes[0].entities.find(
  (entity) => entity.name === "doom-zombieman-12",
);
assert.ok(sourceEnemy);
assert.deepEqual(
  enemy.enemyCombat,
  sourceEnemy.enemyCombat,
  "room must reuse the production Zombieman combat definition",
);
assert.deepEqual(drop.pickup, { item: "ammo/bullets", quantity: 5 });
assert.equal(drop.renderable.visible, false);

const family = manifest.contract.families.find(
  (candidate) => candidate.prefix === "POSS",
);
const atlas = manifest.atlases.find(
  (candidate) => candidate.id === "sprite/doom-e1m1-actors",
);
const asset = room.assets.find(
  (candidate) => candidate.id === "sprite/combat-room-zombieman",
);
assert.ok(family && atlas && asset);
const familySources = atlas.frames.filter((frame) => frame.family === "POSS");
const localByName = new Map(familySources.map((frame, index) => [frame.name, index]));

const stateToClip = new Map([
  ["idle", "idle"],
  ["moving", "walk"],
  ["alert", "idle"],
  ["attacking", "attack"],
  ["hit", "pain"],
  ["defeated", "death"],
]);
for (const [stateName, clipId] of stateToClip) {
  const state = enemy.renderable.visualBinding.states.find(
    (candidate) => candidate.state === stateName,
  );
  const clip = family.clips.find((candidate) => candidate.id === clipId);
  assert.ok(state && clip, `missing ${stateName}/${clipId}`);
  assert.equal(state.loopMode, clip.loopMode);
  assert.equal(state.ticksPerFrame, 1);
  assert.equal(state.directionalViews.length, 8);
  let sourceTicks = 0;
  let runtimeTicks = 0;
  const expected = Array.from({ length: 8 }, () => ({ frames: [], offsets: [] }));
  for (const step of clip.steps) {
    sourceTicks += Math.max(0, step.tics);
    const nextRuntimeTicks =
      step.tics < 0
        ? runtimeTicks + 1
        : Math.round((sourceTicks * 60) / manifest.contract.tickRateHz);
    const duration = Math.max(1, nextRuntimeTicks - runtimeTicks);
    runtimeTicks = nextRuntimeTicks;
    const directional = family.directionalFrames.find(
      (candidate) => candidate.frame === step.frame,
    );
    for (let index = 0; index < 8; index += 1) {
      const rotation = index + 1;
      const selected =
        directional.rotations.find((candidate) => candidate.rotation === rotation) ??
        directional.rotations.find((candidate) => candidate.rotation === 0);
      const source = atlas.frames.find((frame) => frame.name === selected.sourceLump);
      const frame = localByName.get(source.name);
      const size = [
        source.pixelSize[0] /
          manifest.contract.scale.presentationDoomUnitsPerEngineUnit,
        source.pixelSize[1] /
          manifest.contract.scale.presentationDoomUnitsPerEngineUnit,
      ];
      const offset = [
        (0.5 - source.pivot[0]) * size[0],
        (0.5 - source.pivot[1]) * size[1],
      ];
      for (let tick = 0; tick < duration; tick += 1) {
        expected[index].frames.push(frame);
        expected[index].offsets.push(offset);
      }
    }
  }
  state.directionalViews.forEach((view, index) => {
    assert.equal(view.rotation, index + 1);
    assert.deepEqual(view.frames, expected[index].frames);
    close(
      view.sourceOriginOffsets,
      expected[index].offsets,
      `${stateName} rotation ${index + 1} source offsets`,
    );
  });
  assert.deepEqual(state.frames, expected[0].frames);
}

const moving = enemy.renderable.visualBinding.states.find(
  (state) => state.state === "moving",
);
assert.notDeepEqual(
  moving.directionalViews[0].frames,
  moving.directionalViews[4].frames,
  "walk must preserve distinct front and rear source frames",
);
const defeated = enemy.renderable.visualBinding.states.find(
  (state) => state.state === "defeated",
);
for (const view of defeated.directionalViews.slice(1)) {
  assert.deepEqual(
    view.frames,
    defeated.directionalViews[0].frames,
    "rotation-0 death art must remain view-invariant",
  );
}

const blockerVoxels = scene.voxelEnvironment.materialVoxels.filter(
  (voxel) => voxel.address[2] === -4 && voxel.address[1] > 0,
);
assert.equal(blockerVoxels.length, 29 * 8, "authored cover wall must be exact");
assert.ok(
  scene.entities.some((entity) => entity.name.startsWith("combat-room-sight-blocker")),
  "cover wall must also be visibly represented",
);

console.log(
  "Doom combat room contract passed: one production Zombieman, logical-facing directional animations, exact cover, pain/death, and bullet drop",
);
