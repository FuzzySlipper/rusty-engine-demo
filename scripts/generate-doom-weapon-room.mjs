import { readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";

const root = resolve(import.meta.dirname, "..");
const doom = JSON.parse(
  readFileSync(resolve(root, "content/projects/doom-e1m1.project.json"), "utf8"),
);
const loading = JSON.parse(
  readFileSync(resolve(root, "content/projects/loading-bay.project.json"), "utf8"),
);
const outputPath = resolve(root, "content/projects/doom-weapon-room.project.json");

const sourceScene = doom.scenes[0];
const player = structuredClone(
  sourceScene.entities.find((entity) => entity.playerController != null),
);
const target = structuredClone(
  sourceScene.entities.find((entity) => entity.name === "doom-zombieman-12"),
);
if (!player || !target) throw new Error("canonical Doom player or target is missing");

player.name = "doom-weapon-room-player";
player.translation = [0, 0.5, -6];
player.renderable.visible = false;
player.playerController.initialYawDegrees = 180;
player.playerController.initialPitchDegrees = 0;
player.playerController.traversal.maxStepHeight = 0.5;
player.playerController.traversal.eyeHeight = 1;
player.inventory.startingStacks = [
  { item: "weapon/fist", quantity: 1 },
  { item: "weapon/pistol", quantity: 1 },
  { item: "weapon/shotgun", quantity: 1 },
  { item: "ammo/bullets", quantity: 200 },
  { item: "ammo/shells", quantity: 50 },
];

target.name = "doom-weapon-room-stationary-target";
target.translation = [0, 1.25, 5];
target.rotation = [0, 0, 0, 1];
target.health.max = 10000;
target.health.startingHealth = 10000;
target.renderable.localTransform = {
  translation: [0, -1, 0],
  rotation: [0, 0, 0, 1],
  scale: [1, 1, 1],
};
delete target.enemyCombat;
delete target.navigation;
delete target.defeatDrop;

const asset = (source, id) => {
  const selected = source.assets.find((candidate) => candidate.id === id);
  if (!selected) throw new Error(`missing room asset ${id}`);
  return structuredClone(selected);
};
const assetIds = [
  "mesh/player-marker",
  "sprite/doom-zombieman",
  "sprite/doom-fist-viewmodel",
  "sprite/doom-pistol-viewmodel",
  "sprite/doom-pistol-flash-viewmodel",
  "sprite/doom-shotgun-viewmodel",
  "sprite/doom-shotgun-flash-viewmodel",
];
const floorMaterial = asset(loading, "material/brush-kit/floor-strip");
const floor = asset(loading, "mesh/floor-strip");
const assetsById = new Map(
  doom.assets
    .filter((candidate) => candidate.spriteAtlas == null)
    .map((candidate) => [candidate.id, structuredClone(candidate)]),
);
for (const id of assetIds.filter((id) => id.startsWith("sprite/"))) {
  assetsById.set(id, asset(doom, id));
}
assetsById.set(floorMaterial.id, floorMaterial);
assetsById.set(floor.id, floor);
const assets = [...assetsById.values()].sort((left, right) =>
  left.id.localeCompare(right.id),
);

const materialVoxels = [];
for (let x = -40; x < 40; x += 1) {
  for (let z = -40; z < 40; z += 1) {
    materialVoxels.push({ address: [x, 0, z], materialSlot: 1 });
  }
}
const entities = [
  player,
  target,
  {
    id: 40,
    name: "doom-weapon-room-floor",
    translation: [-10, 0, -10],
    renderable: {
      asset: floor.id,
      visible: true,
      localTransform: {
        translation: [0, 0, 0],
        rotation: [0, 0, 0, 1],
        scale: [10, 1, 10],
      },
    },
  },
].sort((left, right) => left.id - right.id);

const project = {
  schemaVersion: doom.schemaVersion,
  projectId: "doom-weapon-room",
  name: "Doom Player Weapon Inspection Room",
  entryScene: "scene/doom-weapon-room",
  itemDefinitions: structuredClone(doom.itemDefinitions),
  assets,
  scenes: [
    {
      id: "scene/doom-weapon-room",
      name: "Doom Player Weapon Inspection Room",
      voxelEnvironment: {
        kind: "material",
        voxelSize: 0.25,
        chunkSize: 16,
        materialVoxels,
        gameplayProxy: true,
      },
      entities,
    },
  ],
};

writeFileSync(outputPath, `${JSON.stringify(project, null, 2)}\n`);
const canonical = `${outputPath}.canon`;
const admitted = spawnSync(
  "cargo",
  [
    "run", "--quiet", "--locked", "-p", "loading-bay-game", "--bin",
    "project-store", "--", "--input", outputPath, "--output", canonical,
  ],
  { cwd: root, encoding: "utf8" },
);
if (admitted.status !== 0) throw new Error(`${admitted.stderr}${admitted.stdout}`);
writeFileSync(outputPath, readFileSync(canonical));
unlinkSync(canonical);
console.log(`Wrote ${outputPath}`);
