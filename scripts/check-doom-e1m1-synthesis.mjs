import assert from "node:assert/strict";
import { readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const projectPath = resolve(root, "content/projects/doom-e1m1.project.json");
const project = JSON.parse(readFileSync(projectPath, "utf8"));
const scene = project.scenes[0];
const entities = scene.entities;
const byName = new Map(entities.map((entity) => [entity.name, entity]));

assert.equal(project.projectId, "doom-e1m1");
assert.equal(project.entryScene, "scene/doom-e1m1");
assert.ok(
  statSync(projectPath).size <= 8 * 1024 * 1024,
  "canonical E1M1 must remain admissible",
);

const actorContracts = new Map([
  ["sprite/doom-zombieman", "doom-visual-template-zombieman"],
  ["sprite/doom-shotgun-guy", "doom-visual-template-shotgun-guy"],
  ["sprite/doom-imp", "doom-visual-template-imp"],
]);
const enemies = entities.filter((entity) => entity.enemy === true);
assert.equal(enemies.length, 29, "source E1M1 enemy roster");
for (const enemy of enemies) {
  assert.ok(
    actorContracts.has(enemy.renderable.asset),
    `${enemy.name} Doom sprite asset`,
  );
  assert.deepEqual(enemy.renderable.localTransform, {
    translation: [0, -1, 0],
    rotation: [0, 0, 0, 1],
    scale: [1, 1, 1],
  });
  assert.deepEqual(enemy.health.hitboxHalfExtents, [0.45, 1, 0.45]);
  assert.deepEqual(enemy.kinematic.halfExtents, [0.45, 1, 0.45]);
  assert.equal(
    enemy.renderable.visualBinding,
    undefined,
    "live actors share one family contract",
  );
}
for (const [asset, templateName] of actorContracts) {
  const template = byName.get(templateName);
  assert.equal(template.renderable.asset, asset);
  assert.equal(template.renderable.visible, false);
  assert.deepEqual(
    template.renderable.visualBinding.states.map((state) => state.state),
    ["idle", "moving", "alert", "attacking", "hit", "defeated"],
  );
  assert.ok(
    template.renderable.visualBinding.states.every(
      (state) =>
        state.ticksPerFrame === 1 &&
        state.directionalViews.length === 8 &&
        state.directionalViews.every(
          (view) => view.sourceOriginOffsets.length === state.frames.length,
        ),
    ),
  );
}

const pickupAssets = new Set([
  "sprite/doom-pickup-shot",
  "sprite/doom-pickup-clip",
  "sprite/doom-pickup-shel",
  "sprite/doom-pickup-ammo",
  "sprite/doom-pickup-sbox",
  "sprite/doom-pickup-stim",
  "sprite/doom-pickup-medi",
  "sprite/doom-pickup-bon1",
  "sprite/doom-pickup-bon2",
  "sprite/doom-pickup-arm1",
  "sprite/doom-pickup-arm2",
]);
const pickupHalfExtent = Number(Math.fround(20 / 28).toFixed(8));
const pickups = entities.filter((entity) => entity.pickup != null);
assert.equal(pickups.length, 78, "source pickups plus enemy drops");
for (const pickup of pickups) {
  assert.ok(
    pickupAssets.has(pickup.renderable.asset),
    `${pickup.name} Doom pickup sprite`,
  );
  assert.deepEqual(pickup.bounds, {
    min: [-pickupHalfExtent, -0.35, -pickupHalfExtent],
    max: [pickupHalfExtent, 0.65, pickupHalfExtent],
  });
}
for (const asset of pickupAssets) {
  const suffix = asset.replace("sprite/doom-pickup-", "");
  const template = byName.get(`doom-visual-template-pickup-${suffix}`);
  assert.equal(template.renderable.asset, asset);
  assert.deepEqual(
    template.renderable.visualBinding.states.map((state) => state.state),
    ["dormant", "available", "collected"],
  );
}

const effectContracts = new Map([
  ["doom-fx-template-blood", "sprite/doom-blood"],
  ["doom-fx-template-bullet-puff", "sprite/doom-puff"],
  ["doom-fx-template-projectile-flight", "sprite/doom-imp-fireball"],
  ["doom-fx-template-projectile-impact", "sprite/doom-imp-fireball"],
]);
for (const [name, asset] of effectContracts) {
  const template = byName.get(name);
  assert.equal(template.renderable.asset, asset);
  assert.equal(template.renderable.visible, false);
  assert.deepEqual(
    template.renderable.visualBinding.states.map((state) => state.state),
    ["default"],
  );
}

for (const asset of [
  "sprite/doom-fist-viewmodel",
  "sprite/doom-pistol-viewmodel",
  "sprite/doom-pistol-flash-viewmodel",
  "sprite/doom-shotgun-viewmodel",
  "sprite/doom-shotgun-flash-viewmodel",
]) {
  assert.ok(
    project.assets.some((candidate) => candidate.id === asset),
    `${asset} closure`,
  );
}

assert.equal(entities.filter((entity) => entity.encounter != null).length, 4);
assert.equal(entities.filter((entity) => entity.door != null).length, 4);
assert.equal(entities.filter((entity) => entity.lift != null).length, 1);
assert.equal(entities.filter((entity) => entity.levelExit != null).length, 1);

console.log("Doom E1M1 synthesis contract verified");
