import { existsSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
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
  "content/projects/doom-fx-room.project.json",
);

const presentationScale =
  manifest.contract.scale.presentationDoomUnitsPerEngineUnit;
if (presentationScale !== 28) {
  throw new Error(`unexpected Doom presentation scale ${presentationScale}`);
}

const sourceFrames = manifest.atlases.flatMap((atlas) =>
  atlas.frames.map((frame) => ({
    ...frame,
    textureId: atlas.textureId,
    atlasFile: atlas.file,
    atlasSha256: atlas.pngSha256,
  })),
);
const sourceByName = new Map(sourceFrames.map((frame) => [frame.name, frame]));
const familyByPrefix = new Map(
  manifest.contract.families.map((family) => [family.prefix, family]),
);

const sourceFor = (family, frameName, rotation) => {
  const directional = family.directionalFrames.find(
    (candidate) => candidate.frame === frameName,
  );
  const selected =
    directional?.rotations.find((candidate) => candidate.rotation === rotation) ??
    directional?.rotations.find((candidate) => candidate.rotation === 0);
  const source = selected && sourceByName.get(selected.sourceLump);
  if (!selected || !source) {
    throw new Error(`missing canonical ${family.prefix}${frameName} rotation ${rotation}`);
  }
  return { selected, source };
};

const familySources = (prefix) => {
  const sources = sourceFrames.filter((frame) => frame.family === prefix);
  if (sources.length === 0) throw new Error(`missing canonical ${prefix} frames`);
  return sources;
};

function expandedClip(prefix, clipId) {
  const family = familyByPrefix.get(prefix);
  if (!family) throw new Error(`missing canonical ${prefix} family`);
  const clip = family.clips.find((candidate) => candidate.id === clipId);
  if (!clip) throw new Error(`missing canonical ${prefix} ${clipId} clip`);
  const localFrameBySource = new Map(
    familySources(prefix).map((frame, index) => [frame.name, index]),
  );
  const views = Array.from({ length: 8 }, (_, index) => ({
    rotation: index + 1,
    frames: [],
    mirrored: false,
    sourceOriginOffsets: [],
  }));
  let sourceTicks = 0;
  let runtimeTicks = 0;
  for (const step of clip.steps) {
    const duration = (() => {
      if (step.tics < 0) return 1;
      sourceTicks += step.tics;
      const nextRuntimeTicks = Math.round(
        (sourceTicks * 60) / manifest.contract.tickRateHz,
      );
      const ticks = Math.max(1, nextRuntimeTicks - runtimeTicks);
      runtimeTicks = nextRuntimeTicks;
      return ticks;
    })();
    for (const view of views) {
      const { selected, source } = sourceFor(family, step.frame, view.rotation);
      const localFrame = localFrameBySource.get(source.name);
      if (localFrame === undefined) {
        throw new Error(`missing local frame ${source.name}`);
      }
      const size = [
        source.pixelSize[0] / presentationScale,
        source.pixelSize[1] / presentationScale,
      ];
      const sourceOriginOffset = [
        (0.5 - source.pivot[0]) * size[0],
        (0.5 - source.pivot[1]) * size[1],
      ];
      for (let tick = 0; tick < duration; tick += 1) {
        view.frames.push(localFrame);
        view.sourceOriginOffsets.push(sourceOriginOffset);
      }
      if (selected.mirrored) view.mirrored = true;
    }
  }
  return {
    clip,
    frames: views[0].frames,
    ticksPerFrame: 1,
    loopMode: clip.loopMode,
    directionalViews: views,
  };
}

const spriteSpecs = [
  ["POSS", "sprite/doom-zombieman", "Doom POSS sprites"],
  ["TROO", "sprite/doom-imp", "Doom TROO sprites"],
  ["BLUD", "sprite/doom-blood", "Doom BLUD sprites"],
  ["PUFF", "sprite/doom-puff", "Doom PUFF sprites"],
  ["BAL1", "sprite/doom-imp-fireball", "Doom BAL1 sprites"],
];

function localizedSpriteAsset(prefix, id, label) {
  const sources = familySources(prefix);
  const atlasFile = sources[0].atlasFile;
  const textureId = sources[0].textureId;
  if (
    sources.some(
      (source) =>
        source.atlasFile !== atlasFile ||
        source.textureId !== textureId ||
        source.atlasSha256 !== sources[0].atlasSha256,
    )
  ) {
    throw new Error(`${prefix} must resolve to one canonical source atlas`);
  }
  return {
    id,
    catalog: {
      version: 1,
      hash: `sha256:${sources[0].atlasSha256}`,
      sourcePath: `content/doom-e1m1/sprites/${atlasFile}`,
      label,
    },
    spriteAtlas: {
      id,
      texture: textureId,
      frames: sources.map((source, index) => ({
        frame: index,
        uvMin: source.uv.min,
        uvMax: source.uv.max,
        size: [
          source.pixelSize[0] / presentationScale,
          source.pixelSize[1] / presentationScale,
        ],
      })),
    },
  };
}

const localizedAssets = spriteSpecs.map(([prefix, id, label]) =>
  localizedSpriteAsset(prefix, id, label),
);
const assetById = new Map(localizedAssets.map((asset) => [asset.id, asset]));

function bindingFor(prefix, clipIds) {
  const states = Object.entries(clipIds).map(([state, clipId]) => {
    const expanded = expandedClip(prefix, clipId);
    return {
      state,
      kind: "spriteFrames",
      frames: expanded.frames,
      ticksPerFrame: expanded.ticksPerFrame,
      loopMode: expanded.loopMode,
      directionalViews: expanded.directionalViews,
    };
  });
  return { version: 2, states };
}

const actorStates = {
  idle: "idle",
  moving: "walk",
  alert: "idle",
  attacking: "attack",
  hit: "pain",
  defeated: "death",
};

const doomScene = doom.scenes[0];
const sourcePlayer = doomScene.entities.find(
  (entity) => entity.playerController != null,
);
const sourceImp = doomScene.entities.find((entity) => entity.name === "doom-imp-1");
const sourceZombieman = doomScene.entities.find(
  (entity) => entity.name === "doom-zombieman-12",
);
const sourceDrop = doomScene.entities.find(
  (entity) => entity.name === "doom-drop-zombieman-12",
);
if (!sourcePlayer || !sourceImp || !sourceZombieman || !sourceDrop) {
  throw new Error("canonical Doom player, Imp, Zombieman, or drop is missing");
}

function liveActor(source, id, name, prefix, assetId, translation) {
  const actor = structuredClone(source);
  actor.id = id;
  actor.name = name;
  actor.translation = translation;
  actor.rotation = [0, 0, 0, 1];
  actor.health.hitboxHalfExtents = [0.45, 1, 0.45];
  actor.kinematic.halfExtents = [0.45, 1, 0.45];
  actor.navigation.goal = translation;
  actor.navigation.speedUnitsPerSecond = 3;
  actor.renderable = {
    asset: assetId,
    visible: true,
    localTransform: {
      translation: [0, -1, 0],
      rotation: [0, 0, 0, 1],
      scale: [1, 1, 1],
    },
    visualBinding: bindingFor(prefix, actorStates),
  };
  return actor;
}

const bloodEnemy = liveActor(
  sourceZombieman,
  2,
  "doom-fx-blood-lane-zombieman",
  "POSS",
  "sprite/doom-zombieman",
  [-12, 1.25, 5],
);
const projectileEnemy = liveActor(
  sourceImp,
  3,
  "doom-fx-projectile-lane-imp",
  "TROO",
  "sprite/doom-imp",
  [12, 1.25, 5],
);

const drop = structuredClone(sourceDrop);
drop.id = 4;
drop.name = "doom-fx-blood-lane-ammo-drop";
drop.translation = bloodEnemy.translation;
bloodEnemy.defeatDrop = { pickup: drop.id };

function effectTemplate(id, name, prefix, clipId, assetId) {
  const expanded = expandedClip(prefix, clipId);
  return {
    id,
    name,
    translation: [0, 0, 0],
    renderable: {
      asset: assetId,
      visible: false,
      localTransform: {
        translation: [0, 0, 0],
        rotation: [0, 0, 0, 1],
        scale: [1, 1, 1],
      },
      visualBinding: {
        version: 2,
        states: [
          {
            state: "default",
            kind: "spriteFrames",
            frames: expanded.frames,
            ticksPerFrame: expanded.ticksPerFrame,
            loopMode: expanded.loopMode,
            directionalViews: expanded.directionalViews,
          },
        ],
      },
    },
  };
}

const effectTemplates = [
  effectTemplate(
    10,
    "doom-fx-template-blood",
    "BLUD",
    "hit",
    assetById.get("sprite/doom-blood").id,
  ),
  effectTemplate(
    11,
    "doom-fx-template-bullet-puff",
    "PUFF",
    "impact",
    assetById.get("sprite/doom-puff").id,
  ),
  effectTemplate(
    12,
    "doom-fx-template-projectile-flight",
    "BAL1",
    "flight",
    assetById.get("sprite/doom-imp-fireball").id,
  ),
  effectTemplate(
    13,
    "doom-fx-template-projectile-impact",
    "BAL1",
    "impact",
    assetById.get("sprite/doom-imp-fireball").id,
  ),
];

const cloneLoadingAsset = (id) => {
  const asset = loading.assets.find((candidate) => candidate.id === id);
  if (!asset) throw new Error(`missing loading-bay reference asset ${id}`);
  return structuredClone(asset);
};
const column = cloneLoadingAsset("mesh/column");
const floorStrip = cloneLoadingAsset("mesh/floor-strip");

const floorXMin = -80;
const floorXMax = 80;
const floorZMin = -64;
const floorZMax = 64;
const targetWallXMin = -8;
const targetWallXMax = 8;
const targetWallYMin = 1;
const targetWallYMax = 16;
const targetWallZ = 20;
const spawnCoverXMin = -24;
const spawnCoverXMax = 24;
const spawnCoverZ = -16;
const materialVoxels = [];
for (let x = floorXMin; x <= floorXMax; x += 1) {
  for (let z = floorZMin; z <= floorZMax; z += 1) {
    materialVoxels.push({ address: [x, 0, z], materialSlot: 1 });
  }
}
for (let x = targetWallXMin; x <= targetWallXMax; x += 1) {
  for (let y = targetWallYMin; y <= targetWallYMax; y += 1) {
    materialVoxels.push({ address: [x, y, targetWallZ], materialSlot: 1 });
  }
}
for (let x = spawnCoverXMin; x <= spawnCoverXMax; x += 1) {
  for (let y = 1; y <= 12; y += 1) {
    materialVoxels.push({ address: [x, y, spawnCoverZ], materialSlot: 1 });
  }
}

const targetWall = {
  id: 20,
  name: "doom-fx-target-wall",
  translation: [0, 0.25, 5],
  renderable: {
    asset: column.id,
    visible: true,
    localTransform: {
      translation: [-0.1875, 0, -0.1875],
      rotation: [0, 0, 0, 1],
      scale: [8, 4, 1],
    },
  },
};
const floor = {
  id: 21,
  name: "doom-fx-room-floor",
  translation: [-20, 0, -16],
  renderable: {
    asset: floorStrip.id,
    visible: true,
    localTransform: {
      translation: [0, 0, 0],
      rotation: [0, 0, 0, 1],
      scale: [20, 16, 1],
    },
  },
};
const spawnCover = {
  id: 22,
  name: "doom-fx-spawn-cover",
  translation: [0, 0.25, -4],
  renderable: {
    asset: column.id,
    visible: true,
    localTransform: {
      translation: [-0.1875, 0, -0.1875],
      rotation: [0, 0, 0, 1],
      scale: [32, 3, 1],
    },
  },
};

const player = structuredClone(sourcePlayer);
player.id = 1;
player.name = "doom-fx-room-player";
player.translation = [0, 0.5, -10];
player.renderable.visible = false;
player.playerController.initialYawDegrees = 180;
player.playerController.initialPitchDegrees = -5;
player.playerController.traversal.maxStepHeight = 0.5;
player.playerController.traversal.eyeHeight = 1;

const assets = [
  ...doom.assets.filter(
    (asset) => asset.spriteAtlas == null && asset.id !== "voxel-volume/doom-e1m1",
  ),
  ...localizedAssets,
  ...(doom.assets.some((asset) => asset.id === column.id) ? [] : [column]),
  ...(doom.assets.some((asset) => asset.id === floorStrip.id) ? [] : [floorStrip]),
];
const project = {
  schemaVersion: doom.schemaVersion,
  projectId: "doom-fx-room",
  name: "Doom Combat FX Clean Room",
  entryScene: "scene/doom-fx-room",
  itemDefinitions: structuredClone(doom.itemDefinitions),
  assets,
  scenes: [
    {
      id: "scene/doom-fx-room",
      name: "Doom Combat FX Clean Room",
      voxelEnvironment: {
        kind: "material",
        voxelSize: 0.25,
        chunkSize: 16,
        materialVoxels,
        gameplayProxy: true,
      },
      entities: [
        player,
        bloodEnemy,
        projectileEnemy,
        drop,
        ...effectTemplates,
        targetWall,
        floor,
        spawnCover,
      ],
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
if (admitted.status !== 0) {
  if (existsSync(canonical)) unlinkSync(canonical);
  throw new Error(`${admitted.stderr}${admitted.stdout}`);
}
writeFileSync(outputPath, readFileSync(canonical));
unlinkSync(canonical);
console.log(`Wrote ${outputPath}`);
