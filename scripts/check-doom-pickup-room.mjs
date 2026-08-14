import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const room = JSON.parse(
  readFileSync(
    resolve(root, "content/projects/doom-pickup-room.project.json"),
    "utf8",
  ),
);
const manifest = JSON.parse(
  readFileSync(
    resolve(root, "content/doom-e1m1/sprites/manifest.json"),
    "utf8",
  ),
);
const scene = room.scenes[0];
const expected = {
  SHOT: ["A"],
  CLIP: ["A"],
  SHEL: ["A"],
  AMMO: ["A"],
  SBOX: ["A"],
  STIM: ["A"],
  MEDI: ["A"],
  BON1: ["A", "B", "C", "D", "C", "B"],
  BON2: ["A", "B", "C", "D", "C", "B"],
  ARM1: ["A", "B"],
  ARM2: ["A", "B"],
};

assert.equal(room.projectId, "doom-pickup-room");
assert.equal(room.entryScene, "scene/doom-pickup-room");
assert.equal(scene.name, "Doom Pickup and Item Clean Room");
assert.ok(!room.assets.some((asset) => asset.id === "voxel-volume/doom-e1m1"));
assert.deepEqual(scene.voxelEnvironment.materialVoxels, []);
assert.equal(manifest.contract.scale.presentationDoomUnitsPerEngineUnit, 28);

const families = new Map(
  manifest.contract.families.map((family) => [family.prefix, family]),
);
const pickups = scene.entities.filter((entity) => entity.pickup != null);
assert.equal(
  pickups.length,
  12,
  "eleven live families plus one dormant enemy drop",
);
for (const [prefix, frames] of Object.entries(expected)) {
  const family = families.get(prefix);
  assert.equal(
    family.role,
    "item",
    `${prefix} must be source-classified as an item`,
  );
  assert.deepEqual(
    family.clips[0].steps.map((step) => step.frame),
    frames,
    `${prefix} canonical animation order`,
  );
  const entity = pickups.find((candidate) =>
    candidate.name.includes(`-${prefix.toLowerCase()}-`),
  );
  assert.ok(entity, `${prefix} room entity`);
  assert.equal(
    entity.renderable.asset,
    `sprite/doom-pickup-${prefix.toLowerCase()}`,
  );
  assert.deepEqual(entity.renderable.localTransform.scale, [1, 1, 1]);
  assert.deepEqual(
    entity.renderable.visualBinding.states.map((state) => state.state),
    ["dormant", "available", "collected"],
  );
  for (const state of entity.renderable.visualBinding.states) {
    assert.equal(state.ticksPerFrame, 1);
    assert.equal(
      state.directionalViews.length,
      8,
      `${prefix} source origins require complete view metadata`,
    );
    assert.ok(
      state.directionalViews.every(
        (view) => view.sourceOriginOffsets.length === state.frames.length,
      ),
    );
  }
}

const floor = scene.entities.find(
  (entity) => entity.name === "doom-pickup-room-floor",
);
assert.deepEqual(floor.bounds, { min: [-14, -0.25, -11], max: [14, 0.25, 13] });
assert.ok(
  pickups
    .filter((entity) => entity.renderable.visible)
    .every((entity) => entity.translation[1] === 0.25),
  "placed pickups anchor to the floor plane",
);

const enemy = scene.entities.find(
  (entity) => entity.name === "doom-pickup-room-drop-zombieman",
);
const drop = scene.entities.find(
  (entity) => entity.name === "doom-pickup-room-zombieman-clip-drop",
);
assert.deepEqual(enemy.defeatDrop, { pickup: drop.id });
assert.equal(enemy.enemy, true);
assert.equal(
  enemy.enemyCombat,
  undefined,
  "drop target must not interfere with inspection",
);
assert.equal(drop.renderable.visible, false, "enemy drop starts dormant");
assert.deepEqual(drop.pickup, { item: "ammo/bullets", quantity: 10 });
assert.deepEqual(
  drop.renderable.localTransform.translation,
  [0, -1, 0],
  "drop local anchor compensates for enemy center height",
);

const player = scene.entities.find((entity) => entity.playerController != null);
assert.equal(
  player.health.startingHealth,
  75,
  "health pickups must have observable room consequence",
);
assert.equal(player.playerController.initialYawDegrees, 180, "player must face the pickup rows at startup");
assert.equal(player.playerController.initialPitchDegrees, -15, "player must see floor-anchored pickups at startup");
assert.deepEqual(player.inventory.startingStacks, [
  { item: "weapon/fist", quantity: 1 },
  { item: "weapon/pistol", quantity: 1 },
  { item: "ammo/bullets", quantity: 20 },
]);

console.log("Doom pickup room contract verified");
