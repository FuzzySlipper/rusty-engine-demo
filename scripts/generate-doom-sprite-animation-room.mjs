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
const manifest = JSON.parse(
  readFileSync(resolve(root, "content/doom-e1m1/sprites/manifest.json"), "utf8"),
);
const outputPath = resolve(
  root,
  "content/projects/doom-sprite-animation-room.project.json",
);

const requestedClips = [
  ...["POSS", "SPOS", "TROO"].flatMap((family) =>
    ["idle", "walk", "attack", "pain", "death"].map((clip) => ({ family, clip })),
  ),
  { family: "BAL1", clip: "flight" },
  { family: "BAL1", clip: "impact" },
  { family: "BLUD", clip: "hit" },
];
const actorLabels = new Map([
  ["POSS", "ZOMBIEMAN"],
  ["SPOS", "SHOTGUN GUY"],
  ["TROO", "IMP"],
  ["BAL1", "IMP FIREBALL"],
  ["BLUD", "BLOOD / HIT FX"],
]);
const presentationScale =
  manifest.contract.scale.presentationDoomUnitsPerEngineUnit;
const sourceFrames = manifest.atlases.flatMap((atlas) =>
  atlas.frames.map((frame) => ({
    ...frame,
    textureId: atlas.textureId,
    atlasFile: atlas.file,
    atlasSha256: atlas.pngSha256,
  })),
);
const sourceByName = new Map(sourceFrames.map((frame) => [frame.name, frame]));
const contractByFamily = new Map(
  manifest.contract.families.map((family) => [family.prefix, family]),
);

function frontSource(family, frameName) {
  const directional = family.directionalFrames.find(
    (candidate) => candidate.frame === frameName,
  );
  const rotation = directional?.rotations.find(
    (candidate) => candidate.rotation === 1 || candidate.rotation === 0,
  );
  const source = rotation && sourceByName.get(rotation.sourceLump);
  if (!source) {
    throw new Error(`missing canonical ${family.prefix}${frameName} front frame`);
  }
  return source;
}

function expandedClip(family, clipId, localFrameBySource) {
  const clip = family.clips.find((candidate) => candidate.id === clipId);
  if (!clip) throw new Error(`missing canonical ${family.prefix} ${clipId} clip`);
  const frames = [];
  const offsets = [];
  let sourceTics = 0;
  let runtimeTicks = 0;
  for (const step of clip.steps) {
    const source = frontSource(family, step.frame);
    const localFrame = localFrameBySource.get(source.name);
    if (localFrame === undefined) throw new Error(`missing local frame ${source.name}`);
    const size = [
      source.pixelSize[0] / presentationScale,
      source.pixelSize[1] / presentationScale,
    ];
    const offset = [
      (0.5 - source.pivot[0]) * size[0],
      (0.5 - source.pivot[1]) * size[1],
    ];
    const duration = (() => {
      if (step.tics < 0) return 1;
      sourceTics += step.tics;
      const nextRuntimeTicks = Math.round((sourceTics * 60) / manifest.contract.tickRateHz);
      const ticks = Math.max(1, nextRuntimeTicks - runtimeTicks);
      runtimeTicks = nextRuntimeTicks;
      return ticks;
    })();
    for (let tick = 0; tick < duration; tick += 1) {
      frames.push(localFrame);
      offsets.push(offset);
    }
  }
  return { clip, frames, offsets };
}

const requiredFamilies = [...new Set(requestedClips.map(({ family }) => family))];
const localizedAssets = [];
const localFramesByFamily = new Map();
for (const prefix of requiredFamilies) {
  const family = contractByFamily.get(prefix);
  if (!family) throw new Error(`missing canonical ${prefix} family`);
  const familySources = sourceFrames.filter((frame) => frame.family === prefix);
  const textureIds = new Set(familySources.map((frame) => frame.textureId));
  if (familySources.length === 0 || textureIds.size !== 1) {
    throw new Error(`${prefix} must resolve to one populated source atlas`);
  }
  const localFrameBySource = new Map(
    familySources.map((frame, index) => [frame.name, index]),
  );
  localFramesByFamily.set(prefix, localFrameBySource);
  localizedAssets.push({
    id: `sprite/animation-room-${prefix.toLowerCase()}`,
    catalog: {
      version: 1,
      hash: `sha256:${familySources[0].atlasSha256}`,
      sourcePath: `content/doom-e1m1/sprites/${familySources[0].atlasFile}`,
      label: `${actorLabels.get(prefix)} canonical animation frames`,
    },
    spriteAtlas: {
      id: `sprite/animation-room-${prefix.toLowerCase()}`,
      texture: [...textureIds][0],
      frames: familySources.map((source, index) => ({
        frame: index,
        uvMin: source.uv.min,
        uvMax: source.uv.max,
        size: [
          source.pixelSize[0] / presentationScale,
          source.pixelSize[1] / presentationScale,
        ],
      })),
    },
  });
}

const doomPlayer = doom.scenes[0].entities.find(
  (entity) => entity.playerController != null,
);
if (!doomPlayer) throw new Error("canonical Doom player is missing");
const entities = [
  {
    id: 1,
    name: "animation-room-player",
    translation: [0, 0.5, -6],
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
      initialPitchDegrees: -3,
      traversal: {
        maxStepHeight: 0.5,
        gravityUnitsPerSecondSquared: 18,
        jumpImpulseUnitsPerSecond: 6,
        groundProbeDistance: 0.3,
        eyeHeight: 1,
        manualJumpEnabled: false,
        maxAirJumps: 0,
      },
      bindings: structuredClone(doomPlayer.playerController.bindings),
    },
    inventory: structuredClone(doomPlayer.inventory),
  },
];

requestedClips.forEach(({ family: prefix, clip: clipId }, sequenceOrder) => {
  const family = contractByFamily.get(prefix);
  const expanded = expandedClip(
    family,
    clipId,
    localFramesByFamily.get(prefix),
  );
  const displayTicks =
    expanded.clip.loopMode === "once"
      ? expanded.frames.length + 75
      : Math.max(150, expanded.frames.length * 3);
  entities.push({
    id: 100 + sequenceOrder,
    name: `animation-fixture-${prefix.toLowerCase()}-${clipId}`,
    translation: [0, prefix === "BAL1" || prefix === "BLUD" ? 1.2 : 0.25, 0],
    renderable: {
      asset: `sprite/animation-room-${prefix.toLowerCase()}`,
      visible: false,
      visualBinding: {
        version: 2,
        states: [
          {
            state: "default",
            kind: "spriteFrames",
            frames: expanded.frames,
            ticksPerFrame: 1,
            loopMode: expanded.clip.loopMode,
            directionalViews: Array.from({ length: 8 }, (_, index) => ({
              rotation: index + 1,
              frames: expanded.frames,
              mirrored: false,
              sourceOriginOffsets: expanded.offsets,
            })),
          },
        ],
      },
    },
    doomSpriteInspection: {
      family: prefix,
      clip: clipId,
      label: `${actorLabels.get(prefix)} · ${clipId.toUpperCase()}`,
      sequenceOrder,
      displayTicks,
    },
  });
});

const cloneLoadingAsset = (id) =>
  structuredClone(loading.assets.find((asset) => asset.id === id));
const column = cloneLoadingAsset("mesh/column");
const columnMaterial = cloneLoadingAsset("material/brush-kit/column");
if (!column || !columnMaterial) throw new Error("room reference assets are missing");
entities.push({
  id: 50,
  name: "two-unit-reference-column",
  translation: [2.5, 0.25, 0],
  renderable: {
    asset: column.id,
    visible: true,
    localTransform: {
      translation: [-0.1875, 0, -0.1875],
      rotation: [0, 0, 0, 1],
      scale: [0.5, 2, 0.5],
    },
  },
});

const floorVoxels = [];
for (let x = -32; x < 32; x += 1) {
  for (let z = -32; z < 32; z += 1) {
    floorVoxels.push({ address: [x, 0, z], materialSlot: 1 });
  }
}

const project = {
  schemaVersion: doom.schemaVersion,
  projectId: "doom-sprite-animation-room",
  name: "Doom Sprite Animation Inspection Room",
  entryScene: "scene/doom-sprite-animation-room",
  itemDefinitions: structuredClone(doom.itemDefinitions),
  assets: [
    ...doom.assets.filter((asset) => asset.spriteAtlas == null),
    ...localizedAssets,
    ...(doom.assets.some((asset) => asset.id === columnMaterial.id)
      ? []
      : [columnMaterial]),
    column,
  ],
  scenes: [
    {
      id: "scene/doom-sprite-animation-room",
      name: "Doom Sprite Animation Inspection Room",
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
if (admitted.status !== 0) throw new Error(`${admitted.stderr}${admitted.stdout}`);
writeFileSync(outputPath, readFileSync(canonical));
unlinkSync(canonical);
console.log(`Wrote ${outputPath}`);
