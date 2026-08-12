import { readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";

const root = resolve(import.meta.dirname, "..");
const doomPath = resolve(root, "content/projects/doom-e1m1.project.json");
const loadingPath = resolve(root, "content/projects/loading-bay.project.json");
const manifestPath = resolve(root, "content/doom-e1m1/sprites/manifest.json");
const outputPath = resolve(root, "content/projects/doom-sprite-scale-room.project.json");

const doom = JSON.parse(readFileSync(doomPath, "utf8"));
const loading = JSON.parse(readFileSync(loadingPath, "utf8"));
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
const actors = manifest.atlases.find((atlas) => atlas.id === "sprite/doom-e1m1-actors");
if (!actors) throw new Error("canonical actor atlas is missing");

const presentationScale = manifest.contract.scale.presentationDoomUnitsPerEngineUnit;
const doomPlayer = doom.scenes[0].entities.find((entity) => entity.playerController != null);
const sourceByName = new Map(actors.frames.map((frame) => [frame.name, frame]));
const sourceAsset = (family) => doom.assets.find((asset) => asset.id === `sprite/doom-${family}`);
const sourceFrame = (family, frameLetter) => {
  const contract = manifest.contract.families.find((candidate) => candidate.prefix === family);
  const direction = contract?.directionalFrames
    .find((candidate) => candidate.frame === frameLetter)
    ?.rotations.find((candidate) => candidate.rotation === 1);
  const frame = direction && sourceByName.get(direction.sourceLump);
  if (!frame) throw new Error(`canonical ${family}${frameLetter} front frame is missing`);
  return frame;
};

const definitions = [
  { family: "POSS", assetFamily: "zombieman", label: "ZOMBIEMAN · 56 DU / 2 EU", x: -4 },
  { family: "SPOS", assetFamily: "shotgun-guy", label: "SHOTGUN GUY · 56 DU / 2 EU", x: 0 },
  { family: "TROO", assetFamily: "imp", label: "IMP · 56 DU / 2 EU", x: 4 },
];

const calibrationAssets = definitions.map((definition) => {
  const source = structuredClone(sourceAsset(definition.assetFamily));
  const frame = sourceFrame(definition.family, "A");
  source.id = `sprite/calibration-${definition.assetFamily}`;
  source.catalog.label = definition.label;
  source.spriteAtlas.id = source.id;
  source.spriteAtlas.frames = [{
    frame: 0,
    uvMin: frame.uv.min,
    uvMax: frame.uv.max,
    size: [frame.pixelSize[0] / presentationScale, frame.pixelSize[1] / presentationScale],
  }];
  return source;
});

const cloneAsset = (id) => structuredClone(loading.assets.find((asset) => asset.id === id));
const column = cloneAsset("mesh/column");
const columnMaterial = cloneAsset("material/brush-kit/column");
const floorStrip = cloneAsset("mesh/floor-strip");
const floorMaterial = cloneAsset("material/brush-kit/floor-strip");
if (!column || !columnMaterial || !floorStrip || !floorMaterial) throw new Error("calibration room reference assets are missing");

const entities = [{
  id: 1,
  name: "fixed-calibration-camera",
  translation: [0, 0.5, 9],
  bounds: { min: [-0.25, -0.25, -0.25], max: [0.25, 0.25, 0.25] },
  collision: { enabled: true, staticCollider: false },
  renderable: { asset: "mesh/player-marker", visible: false },
  health: { max: 100, startingHealth: 100, hitboxHalfExtents: [0.25, 0.5, 0.25] },
  kinematic: { halfExtents: [0.25, 0.25, 0.25], velocity: [0, 0, 0] },
  playerController: {
    moveSpeedUnitsPerSecond: 0.01,
    moveStepSeconds: 0.1,
    lookDegreesPerUnit: 0.01,
    initialYawDegrees: 0,
    initialPitchDegrees: -8,
    traversal: {
      maxStepHeight: 0.1,
      gravityUnitsPerSecondSquared: 0.01,
      jumpImpulseUnitsPerSecond: 0.01,
      groundProbeDistance: 0.1,
      eyeHeight: 1,
      manualJumpEnabled: false,
      maxAirJumps: 0,
    },
    bindings: {
      moveForward: "KeyW", moveBackward: "KeyS", moveLeft: "KeyA", moveRight: "KeyD",
      mouseLook: "pointer", primaryFire: "Mouse0", jump: "Space", selectWeapon: ["Digit1", "Digit2", "Digit3"],
    },
  },
  inventory: structuredClone(doomPlayer.inventory),
}];

let id = 10;
for (const definition of definitions) {
  const frame = sourceFrame(definition.family, "A");
  const pivot = frame.pivot;
  const size = [frame.pixelSize[0] / presentationScale, frame.pixelSize[1] / presentationScale];
  entities.push({
    id: id++,
    name: `${definition.label} · source origin on floor`,
    translation: [definition.x + (0.5 - pivot[0]) * size[0], 0.25 + (0.5 - pivot[1]) * size[1], 0],
    renderable: { asset: `sprite/calibration-${definition.assetFamily}`, visible: true },
  });
  entities.push({
    id: id++,
    name: `${definition.label} · TWO ENGINE UNIT REFERENCE`,
    translation: [definition.x + 1.35, 0.25, 0],
    renderable: { asset: "mesh/column", visible: true, localTransform: {
      translation: [0, 0, 0], rotation: [0, 0, 0, 1], scale: [0.8, 1, 0.8],
    } },
  });
}
entities.push({
  id: id++,
  name: "calibration-room-floor",
  translation: [-7, 0, -2],
  renderable: { asset: "mesh/floor-strip", visible: true, localTransform: {
    translation: [0, 0, 0], rotation: [0, 0, 0, 1], scale: [7, 1, 6],
  } },
});

const floorVoxels = [];
for (let x = -7; x < 7; x += 1) {
  for (let z = -2; z < 10; z += 1) floorVoxels.push({ address: [x, 0, z], materialSlot: 1 });
}

const project = {
  schemaVersion: doom.schemaVersion,
  projectId: "doom-sprite-scale-room",
  name: "Doom Sprite Scale Calibration Room",
  entryScene: "scene/doom-sprite-scale-room",
  itemDefinitions: structuredClone(doom.itemDefinitions),
  assets: [
    ...doom.assets.filter((asset) => asset.spriteAtlas == null),
    ...calibrationAssets,
    ...(doom.assets.some((asset) => asset.id === columnMaterial.id) ? [] : [columnMaterial]),
    column,
    ...(doom.assets.some((asset) => asset.id === floorMaterial.id) ? [] : [floorMaterial]),
    floorStrip,
  ],
  scenes: [{
    id: "scene/doom-sprite-scale-room",
    name: "Doom Sprite Scale Calibration Room",
    voxelEnvironment: { kind: "material", voxelSize: 0.25, chunkSize: 16, materialVoxels: floorVoxels, gameplayProxy: true },
    entities,
  }],
};

writeFileSync(outputPath, `${JSON.stringify(project, null, 2)}\n`);
const canonical = `${outputPath}.canon`;
const admitted = spawnSync("cargo", ["run", "--quiet", "--locked", "-p", "loading-bay-game", "--bin", "project-store", "--", "--input", outputPath, "--output", canonical], { cwd: root, encoding: "utf8" });
if (admitted.status !== 0) throw new Error(`${admitted.stderr}${admitted.stdout}`);
writeFileSync(outputPath, readFileSync(canonical));
unlinkSync(canonical);
console.log(`Wrote ${outputPath}`);
