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
const weaponRoom = JSON.parse(
  readFileSync(
    resolve(root, "content/projects/doom-weapon-room.project.json"),
    "utf8",
  ),
);
const loading = JSON.parse(
  readFileSync(
    resolve(root, "content/projects/loading-bay.project.json"),
    "utf8",
  ),
);
const manifest = JSON.parse(
  readFileSync(
    resolve(root, "content/doom-e1m1/sprites/manifest.json"),
    "utf8",
  ),
);
const outputPath = resolve(
  root,
  "content/projects/doom-pickup-room.project.json",
);
const presentationScale =
  manifest.contract.scale.presentationDoomUnitsPerEngineUnit;
if (presentationScale !== 28)
  throw new Error(`unexpected Doom presentation scale ${presentationScale}`);

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

function pickupTriggerHalfExtent(spec) {
  const family = familyByPrefix.get(spec.prefix);
  const radius = family?.dimensionsDoomUnits?.radius;
  if (family?.role !== "item" || radius !== 20)
    throw new Error(`unexpected ${spec.prefix} pickup trigger radius ${radius}`);
  return radius / presentationScale;
}

const pickupSpecs = [
  {
    prefix: "SHOT",
    label: "SHOTGUN",
    item: "weapon/shotgun",
    quantity: 1,
    starterAmmunition: { item: "ammo/shells", quantity: 8 },
  },
  { prefix: "CLIP", label: "BULLET CLIP", item: "ammo/bullets", quantity: 10 },
  { prefix: "SHEL", label: "SHELLS", item: "ammo/shells", quantity: 4 },
  { prefix: "AMMO", label: "BULLET BOX", item: "ammo/bullets", quantity: 50 },
  { prefix: "SBOX", label: "SHELL BOX", item: "ammo/shells", quantity: 20 },
  { prefix: "STIM", label: "STIMPACK", item: "supply/stimpack", quantity: 1 },
  { prefix: "MEDI", label: "MEDIKIT", item: "supply/medikit", quantity: 1 },
  {
    prefix: "BON1",
    label: "HEALTH BONUS",
    item: "supply/health-bonus",
    quantity: 1,
  },
  { prefix: "BON2", label: "ARMOR BONUS", item: "armor/bonus", quantity: 1 },
  { prefix: "ARM1", label: "GREEN ARMOR", item: "armor/green", quantity: 1 },
  { prefix: "ARM2", label: "BLUE ARMOR", item: "armor/blue", quantity: 1 },
];

function familySources(prefix) {
  const sources = sourceFrames.filter((frame) => frame.family === prefix);
  if (sources.length === 0)
    throw new Error(`missing canonical ${prefix} frames`);
  return sources;
}

function sourceFor(family, frameName) {
  const directional = family.directionalFrames.find(
    (candidate) => candidate.frame === frameName,
  );
  const selected = directional?.rotations.find(
    (candidate) => candidate.rotation === 0,
  );
  const source = selected && sourceByName.get(selected.sourceLump);
  if (!selected || !source)
    throw new Error(`missing canonical ${family.prefix}${frameName}0`);
  return source;
}

function localizedSpriteAsset(spec) {
  const sources = familySources(spec.prefix);
  const first = sources[0];
  if (
    sources.some(
      (source) =>
        source.atlasFile !== first.atlasFile ||
        source.textureId !== first.textureId ||
        source.atlasSha256 !== first.atlasSha256,
    )
  ) {
    throw new Error(`${spec.prefix} must resolve to one source atlas`);
  }
  return {
    id: `sprite/doom-pickup-${spec.prefix.toLowerCase()}`,
    catalog: {
      version: 1,
      hash: `sha256:${first.atlasSha256}`,
      sourcePath: `content/doom-e1m1/sprites/${first.atlasFile}`,
      label: `Doom ${spec.prefix} pickup sprites`,
    },
    spriteAtlas: {
      id: `sprite/doom-pickup-${spec.prefix.toLowerCase()}`,
      texture: first.textureId,
      frames: sources.map((source, frame) => ({
        frame,
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

function expandedAvailable(spec) {
  const family = familyByPrefix.get(spec.prefix);
  const clip = family?.clips.find((candidate) => candidate.id === "available");
  if (!family || !clip)
    throw new Error(`missing ${spec.prefix} available clip`);
  const sources = familySources(spec.prefix);
  const localFrameBySource = new Map(
    sources.map((source, index) => [source.name, index]),
  );
  const frames = [];
  const offsets = [];
  let sourceTicks = 0;
  let runtimeTicks = 0;
  for (const step of clip.steps) {
    const source = sourceFor(family, step.frame);
    const localFrame = localFrameBySource.get(source.name);
    const duration =
      step.tics < 0
        ? 1
        : Math.max(
            1,
            Math.round(
              ((sourceTicks += step.tics) * 60) / manifest.contract.tickRateHz,
            ) - runtimeTicks,
          );
    if (step.tics >= 0) runtimeTicks += duration;
    const size = [
      source.pixelSize[0] / presentationScale,
      source.pixelSize[1] / presentationScale,
    ];
    const offset = [
      (0.5 - source.pivot[0]) * size[0],
      (0.5 - source.pivot[1]) * size[1],
    ];
    for (let tick = 0; tick < duration; tick += 1) {
      frames.push(localFrame);
      offsets.push(offset);
    }
  }
  const directionalViews = Array.from({ length: 8 }, (_, index) => ({
    rotation: index + 1,
    frames: [...frames],
    mirrored: false,
    sourceOriginOffsets: offsets.map((offset) => [...offset]),
  }));
  return { frames, directionalViews };
}

const assets = pickupSpecs.map(localizedSpriteAsset);
const assetByPrefix = new Map(
  pickupSpecs.map((spec, index) => [spec.prefix, assets[index]]),
);

function pickupEntity(
  spec,
  id,
  translation,
  { visible = true, localY = 0 } = {},
) {
  const clip = expandedAvailable(spec);
  const triggerHalfExtent = pickupTriggerHalfExtent(spec);
  const pickup = { item: spec.item, quantity: spec.quantity };
  if (spec.starterAmmunition) pickup.starterAmmunition = spec.starterAmmunition;
  const state = (name) => ({
    state: name,
    kind: "spriteFrames",
    frames: clip.frames,
    ticksPerFrame: 1,
    loopMode: "repeat",
    directionalViews: clip.directionalViews,
  });
  return {
    id,
    name: `doom-pickup-room-${spec.prefix.toLowerCase()}-${spec.label.toLowerCase().replaceAll(" ", "-")}`,
    translation,
    bounds: {
      min: [-triggerHalfExtent, -0.35, -triggerHalfExtent],
      max: [triggerHalfExtent, 0.65, triggerHalfExtent],
    },
    renderable: {
      asset: assetByPrefix.get(spec.prefix).id,
      visible,
      localTransform: {
        translation: [0, localY, 0],
        rotation: [0, 0, 0, 1],
        scale: [1, 1, 1],
      },
      visualBinding: {
        version: 2,
        states: [state("dormant"), state("available"), state("collected")],
      },
    },
    pickup,
  };
}

const player = structuredClone(
  weaponRoom.scenes[0].entities.find((entity) => entity.playerController),
);
player.id = 1;
player.name = "doom-pickup-room-player";
player.translation = [0, 0.5, -8];
player.playerController.initialYawDegrees = 180;
player.playerController.initialPitchDegrees = -15;
player.health.startingHealth = 75;
player.renderable.visible = false;
player.inventory.startingStacks = [
  { item: "weapon/fist", quantity: 1 },
  { item: "weapon/pistol", quantity: 1 },
  { item: "ammo/bullets", quantity: 20 },
];

const pickupEntities = pickupSpecs.map((spec, index) =>
  pickupEntity(spec, 10 + index, [
    -5 + (index % 6) * 2,
    0.25,
    index < 6 ? -5 : -1,
  ]),
);

const target = structuredClone(
  weaponRoom.scenes[0].entities.find(
    (entity) => entity.name === "doom-weapon-room-stationary-target",
  ),
);
target.id = 30;
target.name = "doom-pickup-room-drop-zombieman";
target.translation = [0, 1.25, 6];
target.health.max = 20;
target.health.startingHealth = 20;
target.defeatDrop = { pickup: 31 };
const drop = pickupEntity(
  pickupSpecs.find((spec) => spec.prefix === "CLIP"),
  31,
  target.translation,
  { visible: false, localY: -1 },
);
drop.name = "doom-pickup-room-zombieman-clip-drop";

const cloneLoadingAsset = (id) =>
  structuredClone(loading.assets.find((asset) => asset.id === id));
const floorStrip = cloneLoadingAsset("mesh/floor-strip");
const floor = {
  id: 40,
  name: "doom-pickup-room-floor",
  translation: [0, 0, 0],
  bounds: { min: [-14, -0.25, -11], max: [14, 0.25, 13] },
  collision: { enabled: true, staticCollider: true },
  kinematic: { halfExtents: [14, 0.25, 12], velocity: [0, 0, 0] },
  renderable: {
    asset: floorStrip.id,
    visible: true,
    localTransform: {
      translation: [-14, 0, -11],
      rotation: [0, 0, 0, 1],
      scale: [14, 12, 1],
    },
  },
};

const project = {
  schemaVersion: doom.schemaVersion,
  projectId: "doom-pickup-room",
  name: "Doom Pickup and Item Clean Room",
  entryScene: "scene/doom-pickup-room",
  itemDefinitions: structuredClone(doom.itemDefinitions),
  assets: [
    ...doom.assets.filter(
      (asset) =>
        asset.spriteAtlas == null && asset.id !== "voxel-volume/doom-e1m1",
    ),
    ...assets,
    ...weaponRoom.assets.filter(
      (asset) =>
        asset.id === target.renderable.asset &&
        !assets.some((candidate) => candidate.id === asset.id),
    ),
    ...(doom.assets.some((asset) => asset.id === floorStrip.id)
      ? []
      : [floorStrip]),
  ],
  scenes: [
    {
      id: "scene/doom-pickup-room",
      name: "Doom Pickup and Item Clean Room",
      voxelEnvironment: {
        kind: "material",
        voxelSize: 0.25,
        chunkSize: 16,
        materialVoxels: [],
        gameplayProxy: true,
      },
      entities: [player, ...pickupEntities, target, drop, floor],
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
