import { existsSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";

const root = resolve(import.meta.dirname, "..");
const doom = JSON.parse(
  readFileSync(
    resolve(root, "content/projects/doom-e1m1.project.json"),
    "utf8",
  ),
);
const loading = JSON.parse(
  readFileSync(
    resolve(root, "content/projects/loading-bay.project.json"),
    "utf8",
  ),
);
const outputPath = resolve(
  root,
  "content/projects/doom-player-hurt-room.project.json",
);

const cloneAsset = (source, id) => {
  const asset = source.assets.find((candidate) => candidate.id === id);
  if (!asset) throw new Error(`missing reference asset ${id}`);
  return structuredClone(asset);
};

const sourcePlayer = doom.scenes[0].entities.find(
  (entity) => entity.playerController != null,
);
const sourceHazard = loading.scenes[0].entities.find(
  (entity) => entity.name === "generator-coolant-leak",
);
if (!sourcePlayer || !sourceHazard) {
  throw new Error(
    "canonical Doom player or loading-bay hazard marker is missing",
  );
}

const player = structuredClone(sourcePlayer);
player.name = "doom-player-hurt-room-player";
player.translation = [0, 0.5, -6];
player.renderable.visible = false;
player.playerController.initialYawDegrees = 180;
player.playerController.initialPitchDegrees = -15;
player.health.startingHealth = 100;

const hazard = structuredClone(sourceHazard);
hazard.id = 2;
hazard.name = "doom-player-hurt-room-staged-hazard";
hazard.translation = [0, 0.5, -4];
hazard.bounds = {
  min: [-1.45, -0.45, -1.45],
  max: [1.45, 0.45, 1.45],
};
hazard.renderable = {
  asset: "mesh/prop-kit/hazard-marker",
  visible: true,
  localTransform: {
    translation: [0, -0.38, 0],
    rotation: [0, 0, 0, 1],
    scale: [1, 1, 1],
  },
  visualBinding: structuredClone(sourceHazard.renderable.visualBinding),
};
hazard.hazard = { damage: 10, cooldownTicks: 180 };

const terminalHazard = structuredClone(hazard);
terminalHazard.id = 3;
terminalHazard.name = "doom-player-hurt-room-terminal-hazard";
terminalHazard.translation = [4, 0.5, -4];
terminalHazard.hazard = { damage: 100, cooldownTicks: 600 };

const floorMaterial = cloneAsset(loading, "material/brush-kit/floor-strip");
const floorStrip = cloneAsset(loading, "mesh/floor-strip");
const markerMaterial = cloneAsset(
  loading,
  "material/prop-kit/hazard-marker-surface",
);
const marker = cloneAsset(loading, "mesh/prop-kit/hazard-marker");
const assetsById = new Map(
  doom.assets
    .filter(
      (asset) =>
        asset.spriteAtlas == null && asset.id !== "voxel-volume/doom-e1m1",
    )
    .map((asset) => [asset.id, structuredClone(asset)]),
);
for (const asset of [floorMaterial, floorStrip, markerMaterial, marker]) {
  assetsById.set(asset.id, asset);
}
const assets = [...assetsById.values()].sort((left, right) =>
  left.id.localeCompare(right.id),
);

const materialVoxels = [];
for (let x = -40; x < 40; x += 1) {
  for (let z = -40; z < 40; z += 1) {
    materialVoxels.push({ address: [x, 0, z], materialSlot: 1 });
  }
}

const floor = {
  id: 40,
  name: "doom-player-hurt-room-floor",
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
};

const project = {
  schemaVersion: doom.schemaVersion,
  projectId: "doom-player-hurt-room",
  name: "Doom Player Hurt Clean Room",
  entryScene: "scene/doom-player-hurt-room",
  itemDefinitions: structuredClone(doom.itemDefinitions),
  assets,
  scenes: [
    {
      id: "scene/doom-player-hurt-room",
      name: "Doom Player Hurt Clean Room",
      voxelEnvironment: {
        kind: "material",
        voxelSize: 0.25,
        chunkSize: 16,
        materialVoxels,
        gameplayProxy: true,
      },
      entities: [player, hazard, terminalHazard, floor].sort(
        (left, right) => left.id - right.id,
      ),
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
