import { readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";

const root = resolve(import.meta.dirname, "..");
const doomPath = resolve(root, "content/projects/doom-e1m1.project.json");
const loadingPath = resolve(root, "content/projects/loading-bay.project.json");
const manifestPath = resolve(root, "content/doom-e1m1/sprites/manifest.json");
const outputPath = resolve(
  root,
  "content/projects/doom-sprite-orbit-room.project.json",
);

const doom = JSON.parse(readFileSync(doomPath, "utf8"));
const loading = JSON.parse(readFileSync(loadingPath, "utf8"));
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
const actors = manifest.atlases.find(
  (atlas) => atlas.id === "sprite/doom-e1m1-actors",
);
const impContract = manifest.contract.families.find(
  (family) => family.prefix === "TROO",
);
const idleContract = impContract?.directionalFrames.find(
  (frame) => frame.frame === "A",
);
if (!actors || !idleContract || idleContract.rotations.length !== 8) {
  throw new Error("canonical Imp A directional contract is incomplete");
}

const presentationScale =
  manifest.contract.scale.presentationDoomUnitsPerEngineUnit;
const sourceByName = new Map(actors.frames.map((frame) => [frame.name, frame]));
const sourceImp = doom.assets.find((asset) => asset.id === "sprite/doom-imp");
const doomPlayer = doom.scenes[0].entities.find(
  (entity) => entity.playerController != null,
);
if (!sourceImp || !doomPlayer)
  throw new Error("canonical Doom Imp or player is missing");

const rotations = idleContract.rotations.map((rotation, index) => {
  const source = sourceByName.get(rotation.sourceLump);
  if (!source || source.id !== rotation.atlasFrame) {
    throw new Error(
      `canonical rotation ${rotation.rotation} source frame is missing`,
    );
  }
  const size = [
    source.pixelSize[0] / presentationScale,
    source.pixelSize[1] / presentationScale,
  ];
  return {
    localFrame: index,
    rotation: rotation.rotation,
    source,
    size,
    mirrored: rotation.mirrored,
    sourceOriginOffset: [
      (0.5 - source.pivot[0]) * size[0],
      (0.5 - source.pivot[1]) * size[1],
    ],
  };
});

const sprite = structuredClone(sourceImp);
sprite.id = "sprite/orbit-room-imp";
sprite.catalog.label = "Directional Imp · TROO A1..A8";
sprite.spriteAtlas.id = sprite.id;
sprite.spriteAtlas.frames = rotations.map(({ localFrame, source, size }) => ({
  frame: localFrame,
  uvMin: source.uv.min,
  uvMax: source.uv.max,
  size,
}));

const cloneAsset = (id) =>
  structuredClone(loading.assets.find((asset) => asset.id === id));
const column = cloneAsset("mesh/column");
const columnMaterial = cloneAsset("material/brush-kit/column");
const floorStrip = cloneAsset("mesh/floor-strip");
const floorMaterial = cloneAsset("material/brush-kit/floor-strip");
if (!column || !columnMaterial || !floorStrip || !floorMaterial) {
  throw new Error("orbit room reference assets are missing");
}

const entities = [
  {
    id: 1,
    name: "orbit-room-player",
    translation: [0, 0.5, -7.5],
    bounds: { min: [-0.25, -0.25, -0.25], max: [0.25, 0.25, 0.25] },
    collision: { enabled: true, staticCollider: false },
    renderable: { asset: "mesh/player-marker", visible: false },
    health: {
      max: 100,
      startingHealth: 100,
      hitboxHalfExtents: [0.25, 0.5, 0.25],
    },
    kinematic: { halfExtents: [0.25, 0.25, 0.25], velocity: [0, 0, 0] },
    playerController: {
      moveSpeedUnitsPerSecond: 3,
      moveStepSeconds: 0.1,
      lookDegreesPerUnit: 12,
      initialYawDegrees: 180,
      initialPitchDegrees: -4,
      traversal: {
        maxStepHeight: 0.5,
        gravityUnitsPerSecondSquared: 18,
        jumpImpulseUnitsPerSecond: 6,
        groundProbeDistance: 0.3,
        eyeHeight: 1,
        manualJumpEnabled: false,
        maxAirJumps: 0,
      },
      bindings: {
        moveForward: "KeyW",
        moveBackward: "KeyS",
        moveLeft: "KeyA",
        moveRight: "KeyD",
        mouseLook: "pointer",
        primaryFire: "Mouse0",
        jump: "Space",
        selectWeapon: ["Digit1", "Digit2", "Digit3"],
      },
    },
    inventory: structuredClone(doomPlayer.inventory),
  },
  {
    id: 10,
    name: "stationary-directional-imp",
    translation: [0, 0.25, 0],
    renderable: {
      asset: sprite.id,
      visible: true,
      visualBinding: {
        version: 2,
        states: [
          {
            state: "default",
            kind: "spriteFrames",
            frames: [0],
            ticksPerFrame: 8,
            loopMode: "repeat",
            directionalViews: rotations.map((rotation) => ({
              rotation: rotation.rotation,
              frames: [rotation.localFrame],
              mirrored: rotation.mirrored,
              sourceOriginOffsets: [rotation.sourceOriginOffset],
            })),
          },
        ],
      },
    },
  },
];

const markerNames = [
  "FRONT / SOUTH",
  "FRONT-DIAGONAL / SOUTHWEST",
  "SIDE / WEST",
  "REAR-DIAGONAL / NORTHWEST",
  "REAR / NORTH",
  "REAR-DIAGONAL / NORTHEAST",
  "SIDE / EAST",
  "FRONT-DIAGONAL / SOUTHEAST",
];
const markerHeights = [1.5, 1.25, 1, 0.75, 0.5, 0.75, 1, 1.25];
const markerRadius = 10;
for (let index = 0; index < 8; index += 1) {
  const angle = -Math.PI / 2 - (index * Math.PI) / 4;
  entities.push({
    id: 20 + index,
    name: `orbit-marker-${index + 1} · ${markerNames[index]}`,
    translation: [
      Number((Math.cos(angle) * markerRadius).toFixed(6)),
      0.25,
      Number((Math.sin(angle) * markerRadius).toFixed(6)),
    ],
    renderable: {
      asset: "mesh/column",
      visible: true,
      localTransform: {
        translation: [-0.1875, 0, -0.1875],
        rotation: [0, 0, 0, 1],
        scale: [0.5, markerHeights[index], 0.5],
      },
    },
  });
}

entities.push({
  id: 40,
  name: "orbit-room-floor",
  translation: [-12, 0, -12],
  renderable: {
    asset: "mesh/floor-strip",
    visible: true,
    localTransform: {
      translation: [0, 0, 0],
      rotation: [0, 0, 0, 1],
      scale: [12, 1, 12],
    },
  },
});

const perimeterSegments = [
  { id: 41, name: "orbit-room-south-boundary", translation: [-12, 0, -12], scale: [32, 0.5, 0.5] },
  { id: 42, name: "orbit-room-north-boundary", translation: [-12, 0, 11.625], scale: [32, 0.5, 0.5] },
  { id: 43, name: "orbit-room-west-boundary", translation: [-12, 0, -12], scale: [0.5, 0.5, 32] },
  { id: 44, name: "orbit-room-east-boundary", translation: [11.625, 0, -12], scale: [0.5, 0.5, 32] },
];
for (const segment of perimeterSegments) {
  entities.push({
    id: segment.id,
    name: segment.name,
    translation: segment.translation,
    renderable: {
      asset: "mesh/column",
      visible: true,
      localTransform: {
        translation: [0, 0, 0],
        rotation: [0, 0, 0, 1],
        scale: segment.scale,
      },
    },
  });
}

const floorVoxels = [];
for (let x = -48; x < 48; x += 1) {
  for (let z = -48; z < 48; z += 1) {
    floorVoxels.push({ address: [x, 0, z], materialSlot: 1 });
  }
}
for (let y = 1; y <= 4; y += 1) {
  for (let coordinate = -48; coordinate < 48; coordinate += 1) {
    floorVoxels.push({ address: [coordinate, y, -48], materialSlot: 1 });
    floorVoxels.push({ address: [coordinate, y, 47], materialSlot: 1 });
    if (coordinate > -48 && coordinate < 47) {
      floorVoxels.push({ address: [-48, y, coordinate], materialSlot: 1 });
      floorVoxels.push({ address: [47, y, coordinate], materialSlot: 1 });
    }
  }
}

const project = {
  schemaVersion: doom.schemaVersion,
  projectId: "doom-sprite-orbit-room",
  name: "Doom Directional Sprite Orbit Room",
  entryScene: "scene/doom-sprite-orbit-room",
  itemDefinitions: structuredClone(doom.itemDefinitions),
  assets: [
    ...doom.assets.filter((asset) => asset.spriteAtlas == null),
    sprite,
    ...(doom.assets.some((asset) => asset.id === columnMaterial.id)
      ? []
      : [columnMaterial]),
    column,
    ...(doom.assets.some((asset) => asset.id === floorMaterial.id)
      ? []
      : [floorMaterial]),
    floorStrip,
  ],
  scenes: [
    {
      id: "scene/doom-sprite-orbit-room",
      name: "Doom Directional Sprite Orbit Room",
      voxelEnvironment: {
        kind: "material",
        voxelSize: 0.25,
        chunkSize: 16,
        materialVoxels: floorVoxels,
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
    "run",
    "--quiet",
    "--locked",
    "-p",
    "loading-bay-game",
    "--bin",
    "project-store",
    "--",
    "--input",
    outputPath,
    "--output",
    canonical,
  ],
  { cwd: root, encoding: "utf8" },
);
if (admitted.status !== 0)
  throw new Error(`${admitted.stderr}${admitted.stdout}`);
writeFileSync(outputPath, readFileSync(canonical));
unlinkSync(canonical);
console.log(`Wrote ${outputPath}`);
