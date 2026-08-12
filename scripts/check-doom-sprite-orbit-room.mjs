import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const project = JSON.parse(
  readFileSync(
    resolve(root, "content/projects/doom-sprite-orbit-room.project.json"),
    "utf8",
  ),
);
const manifest = JSON.parse(
  readFileSync(
    resolve(root, "content/doom-e1m1/sprites/manifest.json"),
    "utf8",
  ),
);

function invariant(condition, message) {
  if (!condition) throw new Error(message);
}

const exact = (actual, expected, message) =>
  invariant(JSON.stringify(actual) === JSON.stringify(expected), message);
const closeArray = (actual, expected, message) => {
  invariant(actual.length === expected.length, message);
  actual.forEach((value, index) => {
    if (Array.isArray(value)) {
      closeArray(value, expected[index], message);
    } else {
      invariant(Math.abs(value - expected[index]) < 1e-6, message);
    }
  });
};

invariant(
  project.projectId === "doom-sprite-orbit-room",
  "wrong orbit-room project identity",
);
invariant(project.scenes.length === 1, "orbit room must have one scene");
const scene = project.scenes[0];
invariant(
  scene.id === project.entryScene,
  "orbit room entry scene must be direct",
);
invariant(
  scene.voxelEnvironment?.kind === "material",
  "orbit room needs only its material floor",
);
invariant(
  scene.voxelEnvironment.materialVoxels.length === 96 * 96 + 4 * (96 * 4 - 4),
  "orbit room must have a 24 by 24 Engine-unit floor and four-unit-high voxel perimeter",
);
invariant(
  project.voxelEnvironment == null,
  "orbit room must not contain a project voxel volume",
);

const spriteAssets = project.assets.filter(
  (asset) => asset.spriteAtlas != null,
);
invariant(
  spriteAssets.length === 1,
  "orbit room must contain one sprite atlas asset",
);
const sprite = spriteAssets[0];
invariant(
  sprite.id === "sprite/orbit-room-imp",
  "orbit room sprite must be the Imp fixture",
);
invariant(
  sprite.spriteAtlas.frames.length === 8,
  "orbit room Imp must expose eight local frames",
);

const actors = manifest.atlases.find(
  (atlas) => atlas.id === "sprite/doom-e1m1-actors",
);
const idleContract = manifest.contract.families
  .find((family) => family.prefix === "TROO")
  ?.directionalFrames.find((frame) => frame.frame === "A");
invariant(
  actors && idleContract,
  "canonical Imp A directional contract is missing",
);
const sourceByName = new Map(actors.frames.map((frame) => [frame.name, frame]));
const scale = manifest.contract.scale.presentationDoomUnitsPerEngineUnit;

const impEntities = scene.entities.filter(
  (entity) => entity.renderable?.asset === "sprite/orbit-room-imp",
);
invariant(impEntities.length === 1, "orbit room must have one stationary Imp");
const imp = impEntities[0];
exact(imp.translation, [0, 0.25, 0], "Imp source origin must sit on the floor");
invariant(
  imp.enemy == null && imp.enemyCombat == null,
  "orbit-room Imp must stay stationary",
);
const binding = imp.renderable.visualBinding;
invariant(
  binding?.version === 2,
  "directional Imp must use visual binding version 2",
);
invariant(
  binding.states.length === 1,
  "directional Imp must have one bounded idle state",
);
const state = binding.states[0];
invariant(
  state.state === "default" && state.kind === "spriteFrames",
  "directional Imp must use the default sprite-frame state",
);
exact(
  state.frames,
  [0],
  "directional Imp base state must have one animation frame",
);
invariant(
  state.directionalViews.length === 8,
  "directional Imp must have eight views",
);

for (let index = 0; index < 8; index += 1) {
  const contract = idleContract.rotations[index];
  const source = sourceByName.get(contract.sourceLump);
  const frame = sprite.spriteAtlas.frames[index];
  const view = state.directionalViews[index];
  invariant(
    source?.id === contract.atlasFrame,
    `rotation ${index + 1} atlas provenance drifted`,
  );
  closeArray(frame.uvMin, source.uv.min, `rotation ${index + 1} uvMin drifted`);
  closeArray(frame.uvMax, source.uv.max, `rotation ${index + 1} uvMax drifted`);
  const size = [source.pixelSize[0] / scale, source.pixelSize[1] / scale];
  closeArray(frame.size, size, `rotation ${index + 1} world scale drifted`);
  invariant(view.rotation === index + 1, `rotation ${index + 1} label drifted`);
  exact(view.frames, [index], `rotation ${index + 1} local frame drifted`);
  invariant(
    view.mirrored === contract.mirrored,
    `rotation ${index + 1} mirror drifted`,
  );
  closeArray(
    view.sourceOriginOffsets,
    [[(0.5 - source.pivot[0]) * size[0], (0.5 - source.pivot[1]) * size[1]]],
    `rotation ${index + 1} source-origin offset drifted`,
  );
}

const players = scene.entities.filter(
  (entity) => entity.playerController != null,
);
invariant(players.length === 1, "orbit room must have one live player camera");
exact(
  players[0].translation,
  [0, 0.5, -7.5],
  "player must spawn at the south/front view",
);
invariant(
  players[0].playerController.initialYawDegrees === 180,
  "player must face the Imp",
);
invariant(
  players[0].playerController.moveSpeedUnitsPerSecond === 3,
  "player must be able to orbit",
);
invariant(
  scene.entities.filter((entity) => entity.name.startsWith("orbit-marker-"))
    .length === 8,
  "orbit room must have eight physical orientation markers",
);
invariant(
  scene.entities.filter((entity) => entity.name.endsWith("-boundary"))
    .length === 4,
  "orbit room must have four visible perimeter boundaries",
);
invariant(
  project.assets.filter(
    (asset) =>
      asset.id.startsWith("texture/doom-flat-") ||
      asset.id.startsWith("texture/doom-wall-"),
  ).length === 54,
  "orbit room must preserve the 54 Doom renderer textures",
);

console.log(
  "Doom sprite orbit room contract passed: 8 canonical Imp views, explicit mirrors/origins, one stationary live fixture",
);
