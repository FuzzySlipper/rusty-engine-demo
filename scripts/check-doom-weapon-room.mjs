import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const room = JSON.parse(
  readFileSync(resolve(root, "content/projects/doom-weapon-room.project.json"), "utf8"),
);
const manifest = JSON.parse(
  readFileSync(resolve(root, "content/doom-e1m1/sprites/manifest.json"), "utf8"),
);

assert.equal(room.projectId, "doom-weapon-room");
assert.equal(room.scenes.length, 1);
const player = room.scenes[0].entities.find((entity) => entity.playerController != null);
const target = room.scenes[0].entities.find((entity) => entity.enemy === true);
assert.ok(player && target);
assert.equal(target.enemyCombat, undefined, "inspection target must never attack");
assert.deepEqual(
  player.inventory.startingStacks.filter((stack) => stack.item.startsWith("weapon/")),
  [
    { item: "weapon/fist", quantity: 1 },
    { item: "weapon/pistol", quantity: 1 },
    { item: "weapon/shotgun", quantity: 1 },
  ],
);
assert.deepEqual(player.inventory.weaponSlots, [
  "weapon/pistol",
  "weapon/shotgun",
  "weapon/fist",
]);
for (const id of [
  "sprite/doom-fist-viewmodel",
  "sprite/doom-pistol-viewmodel",
  "sprite/doom-pistol-flash-viewmodel",
  "sprite/doom-shotgun-viewmodel",
  "sprite/doom-shotgun-flash-viewmodel",
]) {
  assert.ok(room.assets.some((asset) => asset.id === id), `missing ${id}`);
}
const weaponFamilies = Object.fromEntries(
  manifest.contract.families
    .filter((family) => family.role === "weapon")
    .map((family) => [family.prefix, family.clips.map((clip) => clip.id)]),
);
assert.deepEqual(weaponFamilies, {
  PUNG: ["ready", "fire"],
  PISG: ["ready", "fire"],
  PISF: ["flash"],
  SHTG: ["ready", "fire"],
  SHTF: ["flash"],
});
assert.equal(room.scenes[0].voxelEnvironment.materialVoxels.length, 6400);
console.log("Doom weapon room contract passed: three weapons, exact generated clips, inert target, and bounded floor");
