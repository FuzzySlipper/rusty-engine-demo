import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const readJson = (path) =>
  JSON.parse(readFileSync(resolve(root, path), "utf8"));
const room = readJson("content/projects/doom-player-hurt-room.project.json");
const doom = readJson("content/projects/doom-e1m1.project.json");
const loading = readJson("content/projects/loading-bay.project.json");

const sourcePlayer = doom.scenes[0].entities.find(
  (entity) => entity.playerController != null,
);
const scene = room.scenes[0];
const entities = scene.entities;
const players = entities.filter((entity) => entity.playerController != null);
const hazards = entities.filter((entity) => entity.hazard != null);
const player = players[0];
const hazard = entities.find(
  (entity) => entity.name === "doom-player-hurt-room-staged-hazard",
);
const terminalHazard = entities.find(
  (entity) => entity.name === "doom-player-hurt-room-terminal-hazard",
);
const floor = entities.find(
  (entity) => entity.name === "doom-player-hurt-room-floor",
);

assert.equal(room.projectId, "doom-player-hurt-room");
assert.equal(room.name, "Doom Player Hurt Clean Room");
assert.equal(room.entryScene, "scene/doom-player-hurt-room");
assert.equal(scene.id, room.entryScene);
assert.equal(scene.name, "Doom Player Hurt Clean Room");
assert.equal(
  entities.length,
  4,
  "hurt room must stay bounded to player, two hazards, and floor",
);
assert.equal(players.length, 1, "hurt room must contain exactly one player");
assert.equal(
  hazards.length,
  2,
  "hurt room must contain exactly two scoped hazards",
);
assert.equal(
  entities.filter((entity) => entity.enemyCombat != null).length,
  0,
  "hurt room must not add a combat enemy",
);

assert.ok(player && hazard && terminalHazard && floor);
assert.deepEqual(player.translation, [0, 0.5, -6]);
assert.equal(
  player.renderable.visible,
  false,
  "the Doom player remains a camera/controller owner",
);
assert.equal(player.health.startingHealth, 100);
assert.deepEqual(
  player.playerController.bindings,
  sourcePlayer.playerController.bindings,
  "hurt room must reuse the existing Doom semantic controls",
);
assert.equal(player.playerController.moveSpeedUnitsPerSecond, 6);
assert.equal(player.playerController.moveStepSeconds, 0.1);
assert.equal(player.playerController.initialYawDegrees, 180);
assert.equal(player.playerController.initialPitchDegrees, -15);

assert.deepEqual(hazard.translation, [0, 0.5, -4]);
assert.deepEqual(hazard.bounds, {
  min: [-1.45, -0.45, -1.45],
  max: [1.45, 0.45, 1.45],
});
assert.deepEqual(hazard.hazard, { damage: 10, cooldownTicks: 180 });
assert.equal(hazard.renderable.asset, "mesh/prop-kit/hazard-marker");
assert.equal(
  hazard.renderable.visible,
  true,
  "the hazard must be visible before contact",
);
assert.deepEqual(hazard.renderable.localTransform, {
  translation: [0, -0.38, 0],
  rotation: [0, 0, 0, 1],
  scale: [1, 1, 1],
});
assert.deepEqual(
  hazard.renderable.visualBinding.states.map((state) => state.state),
  ["active", "cooling"],
  "the marker must retain the loading-bay active/cooling presentation contract",
);

assert.deepEqual(terminalHazard.translation, [4, 0.5, -4]);
assert.deepEqual(terminalHazard.bounds, hazard.bounds);
assert.deepEqual(terminalHazard.hazard, { damage: 100, cooldownTicks: 600 });
assert.equal(terminalHazard.renderable.asset, "mesh/prop-kit/hazard-marker");
assert.equal(terminalHazard.renderable.visible, true);

assert.equal(floor.renderable.asset, "mesh/floor-strip");
assert.equal(floor.renderable.visible, true);
assert.deepEqual(floor.translation, [-10, 0, -10]);
assert.deepEqual(floor.renderable.localTransform.scale, [10, 1, 10]);
assert.equal(scene.voxelEnvironment.gameplayProxy, true);
assert.equal(scene.voxelEnvironment.materialVoxels.length, 6_400);
assert.ok(
  scene.voxelEnvironment.materialVoxels.every(
    (voxel) =>
      voxel.materialSlot === 1 &&
      voxel.address[1] === 0 &&
      voxel.address[0] >= -40 &&
      voxel.address[0] < 40 &&
      voxel.address[2] >= -40 &&
      voxel.address[2] < 40,
  ),
  "hurt room must use the bounded loading-bay floor plane",
);

for (const source of [
  loading.assets.find((asset) => asset.id === "mesh/floor-strip"),
  loading.assets.find((asset) => asset.id === "material/brush-kit/floor-strip"),
  loading.assets.find((asset) => asset.id === "mesh/prop-kit/hazard-marker"),
  loading.assets.find(
    (asset) => asset.id === "material/prop-kit/hazard-marker-surface",
  ),
]) {
  assert.ok(source, "loading-bay reference asset must exist");
  assert.deepEqual(
    room.assets.find((asset) => asset.id === source.id),
    source,
    `${source.id} must remain the existing loading-bay asset`,
  );
}

const boundsAt = (entity, translation = entity.translation) => ({
  min: entity.bounds.min.map((value, index) => value + translation[index]),
  max: entity.bounds.max.map((value, index) => value + translation[index]),
});
const overlaps = (left, right) =>
  left.min.every((value, index) => value < right.max[index]) &&
  left.max.every((value, index) => value > right.min[index]);
const playerAt = (z) =>
  boundsAt(player, [player.translation[0], player.translation[1], z]);
const hazardBounds = boundsAt(hazard);
const stepDistance =
  player.playerController.moveSpeedUnitsPerSecond *
  player.playerController.moveStepSeconds;

assert.ok(
  hazard.translation[2] > player.translation[2],
  "hazard must be ahead of the safe spawn",
);
assert.equal(
  overlaps(playerAt(player.translation[2]), hazardBounds),
  false,
  "safe spawn must begin outside the hazard trigger",
);
assert.equal(
  overlaps(playerAt(player.translation[2] + stepDistance), hazardBounds),
  true,
  "one Doom controller movement step must enter the marked hazard",
);
assert.equal(
  overlaps(playerAt(player.translation[2]), hazardBounds),
  false,
  "a step back out must leave the trigger available for retrigger inspection",
);

assert.equal(
  100 - terminalHazard.hazard.damage,
  0,
  "the separate terminal marker must reach zero health in one authoritative application",
);

console.log(
  "Doom player hurt room contract passed: safe spawn, slow staged damage/recovery marker, and separate terminal marker",
);
