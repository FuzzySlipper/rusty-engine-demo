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
const outputPath = resolve(root, "content/projects/doom-combat-room.project.json");

const presentationScale =
  manifest.contract.scale.presentationDoomUnitsPerEngineUnit;
const family = manifest.contract.families.find(
  (candidate) => candidate.prefix === "POSS",
);
const atlas = manifest.atlases.find(
  (candidate) => candidate.id === "sprite/doom-e1m1-actors",
);
if (!family || !atlas) throw new Error("canonical Zombieman sprite contract is missing");

const sourceByName = new Map(atlas.frames.map((frame) => [frame.name, frame]));
const familySources = atlas.frames.filter((frame) => frame.family === family.prefix);
const localFrameBySource = new Map(
  familySources.map((frame, index) => [frame.name, index]),
);

const sourceFor = (frameName, rotation) => {
  const directional = family.directionalFrames.find(
    (candidate) => candidate.frame === frameName,
  );
  const authored = directional?.rotations.find(
    (candidate) => candidate.rotation === rotation,
  );
  const invariant = directional?.rotations.find(
    (candidate) => candidate.rotation === 0,
  );
  const selected = authored ?? invariant;
  const source = selected && sourceByName.get(selected.sourceLump);
  if (!selected || !source) {
    throw new Error(`missing canonical POSS${frameName} rotation ${rotation}`);
  }
  return { selected, source };
};

function expandedClip(clipId) {
  const clip = family.clips.find((candidate) => candidate.id === clipId);
  if (!clip) throw new Error(`missing canonical POSS ${clipId} clip`);
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
      const result = Math.max(1, nextRuntimeTicks - runtimeTicks);
      runtimeTicks = nextRuntimeTicks;
      return result;
    })();
    for (const view of views) {
      const { selected, source } = sourceFor(step.frame, view.rotation);
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
      for (let tick = 0; tick < duration; tick += 1) {
        view.frames.push(localFrame);
        view.sourceOriginOffsets.push(offset);
      }
      if (selected.mirrored) view.mirrored = true;
    }
  }
  return {
    state: clipId,
    kind: "spriteFrames",
    frames: views[0].frames,
    ticksPerFrame: 1,
    loopMode: clip.loopMode,
    directionalViews: views,
  };
}

const sprite = {
  id: "sprite/combat-room-zombieman",
  catalog: {
    version: 1,
    hash: `sha256:${atlas.pngSha256}`,
    sourcePath: `content/doom-e1m1/sprites/${atlas.file}`,
    label: "Combat room Zombieman canonical directional animations",
  },
  spriteAtlas: {
    id: "sprite/combat-room-zombieman",
    texture: atlas.textureId,
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
};

const sourceScene = doom.scenes[0];
const player = structuredClone(
  sourceScene.entities.find((entity) => entity.playerController != null),
);
const enemy = structuredClone(
  sourceScene.entities.find((entity) => entity.name === "doom-zombieman-12"),
);
const drop = structuredClone(
  sourceScene.entities.find((entity) => entity.name === "doom-drop-zombieman-12"),
);
if (!player || !enemy || !drop) throw new Error("canonical combat owners are missing");

player.name = "combat-room-player";
player.translation = [0, 0.5, -6];
player.renderable.visible = false;
player.playerController.initialYawDegrees = 180;
player.playerController.initialPitchDegrees = -5;
player.playerController.traversal.maxStepHeight = 0.5;
player.playerController.traversal.eyeHeight = 1;

enemy.name = "combat-room-zombieman · LOGICAL FACING SOUTH";
enemy.translation = [0, 1.25, 4];
enemy.rotation = [0, 0, 0, 1];
enemy.renderable = {
  asset: sprite.id,
  visible: true,
  localTransform: {
    translation: [0, -1, 0],
    rotation: [0, 0, 0, 1],
    scale: [1, 1, 1],
  },
  visualBinding: {
    version: 2,
    states: [
      { ...expandedClip("idle"), state: "idle" },
      { ...expandedClip("walk"), state: "moving" },
      { ...expandedClip("idle"), state: "alert" },
      { ...expandedClip("attack"), state: "attacking" },
      { ...expandedClip("pain"), state: "hit" },
      { ...expandedClip("death"), state: "defeated" },
    ],
  },
};
enemy.health.hitboxHalfExtents = [0.45, 1, 0.45];
enemy.kinematic.halfExtents = [0.45, 1, 0.45];
enemy.navigation.goal = enemy.translation;
enemy.navigation.speedUnitsPerSecond = 3;

drop.name = "combat-room-zombieman-bullet-drop";
drop.translation = enemy.translation;

const cloneAsset = (id) =>
  structuredClone(loading.assets.find((asset) => asset.id === id));
const column = cloneAsset("mesh/column");
const columnMaterial = cloneAsset("material/brush-kit/column");
const floorStrip = cloneAsset("mesh/floor-strip");
const floorMaterial = cloneAsset("material/brush-kit/floor-strip");
if (!column || !columnMaterial || !floorStrip || !floorMaterial) {
  throw new Error("combat room reference geometry is missing");
}

const entities = [player, enemy, drop];
entities.push({
  id: 40,
  name: "combat-room-floor",
  translation: [-10, 0, -10],
  renderable: {
    asset: floorStrip.id,
    visible: true,
    localTransform: {
      translation: [0, 0, 0],
      rotation: [0, 0, 0, 1],
      scale: [10, 1, 10],
    },
  },
});
for (let index = 0; index < 7; index += 1) {
  entities.push({
    id: 50 + index,
    name: `combat-room-sight-blocker-${index + 1}`,
    translation: [-5.5 + index, 0.25, -1],
    renderable: {
      asset: column.id,
      visible: true,
      localTransform: {
        translation: [-0.1875, 0, -0.1875],
        rotation: [0, 0, 0, 1],
        scale: [2, 2, 1],
      },
    },
  });
}

const materialVoxels = [];
for (let x = -40; x < 40; x += 1) {
  for (let z = -40; z < 40; z += 1) {
    materialVoxels.push({ address: [x, 0, z], materialSlot: 1 });
  }
}
for (let x = -24; x <= 4; x += 1) {
  for (let y = 1; y <= 8; y += 1) {
    materialVoxels.push({ address: [x, y, -4], materialSlot: 1 });
  }
}

const assets = [
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
];
const project = {
  schemaVersion: doom.schemaVersion,
  projectId: "doom-combat-room",
  name: "Doom Single-Enemy Combat Room",
  entryScene: "scene/doom-combat-room",
  itemDefinitions: structuredClone(doom.itemDefinitions),
  assets,
  scenes: [
    {
      id: "scene/doom-combat-room",
      name: "Doom Single-Enemy Combat Room",
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
