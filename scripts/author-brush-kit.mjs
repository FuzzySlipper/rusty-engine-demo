import { spawn } from "node:child_process";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { dirname, relative, resolve } from "node:path";
import { createInterface } from "node:readline";
import { performance } from "node:perf_hooks";

const ROOT = resolve(import.meta.dirname, "..");
const PROJECT =
  process.argv[2] === undefined
    ? resolve(ROOT, "content/projects/loading-bay.project.json")
    : resolve(process.argv[2]);
const EVIDENCE =
  process.argv[3] === undefined
    ? resolve(ROOT, "docs/evidence/voxel-brush-kit-authoring.json")
    : resolve(process.argv[3]);

const modules = [
  {
    name: "wall-conservative",
    role: "straight wall, conservative treatment",
    resolution: [32, 32, 4],
    cellSize: 1 / 16,
    dimensions: [2, 2, 0.25],
    snap: [2, 2, 0.25],
    repeat: 6,
  },
  {
    name: "wall-dense",
    role: "straight wall, dense equal-dimension treatment",
    resolution: [64, 64, 8],
    cellSize: 1 / 32,
    dimensions: [2, 2, 0.25],
    snap: [2, 2, 0.25],
    repeat: 2,
  },
  {
    name: "corner",
    role: "compatible inside corner",
    resolution: [32, 32, 32],
    cellSize: 1 / 16,
    dimensions: [2, 2, 2],
    snap: [2, 2, 2],
    repeat: 4,
  },
  {
    name: "doorway",
    role: "doorway surround",
    resolution: [48, 40, 4],
    cellSize: 1 / 16,
    dimensions: [3, 2.5, 0.25],
    snap: [0.5, 0.5, 0.25],
    repeat: 1,
  },
  {
    name: "vent-panel",
    role: "inset panel and vent variation",
    resolution: [32, 32, 6],
    cellSize: 1 / 16,
    dimensions: [2, 2, 0.375],
    snap: [2, 2, 0.125],
    repeat: 1,
  },
  {
    name: "column",
    role: "structural column",
    resolution: [12, 32, 12],
    cellSize: 1 / 16,
    dimensions: [0.75, 2, 0.75],
    snap: [0.25, 2, 0.25],
    repeat: 4,
  },
  {
    name: "floor-strip",
    role: "floor channel tile",
    resolution: [32, 6, 32],
    cellSize: 1 / 16,
    dimensions: [2, 0.375, 2],
    snap: [2, 0.125, 2],
    repeat: 4,
  },
  {
    name: "ceiling-strip",
    role: "ceiling structure tile",
    resolution: [32, 8, 32],
    cellSize: 1 / 16,
    dimensions: [2, 0.5, 2],
    snap: [2, 0.125, 2],
    repeat: 4,
  },
  {
    name: "landmark-relay",
    role: "relay-frame landmark",
    resolution: [32, 32, 16],
    cellSize: 1 / 16,
    dimensions: [2, 2, 1],
    snap: [2, 2, 1],
    repeat: 1,
  },
];

const placements = [
  ["floor-strip", "floor-nw", [32, 0, 4], 0],
  ["floor-strip", "floor-ne", [34, 0, 4], 0],
  ["floor-strip", "floor-sw", [32, 0, 6], 0],
  ["floor-strip", "floor-se", [34, 0, 6], 0],
  ["ceiling-strip", "ceiling-nw", [32, 2.5, 4], 0],
  ["ceiling-strip", "ceiling-ne", [34, 2.5, 4], 0],
  ["ceiling-strip", "ceiling-sw", [32, 2.5, 6], 0],
  ["ceiling-strip", "ceiling-se", [34, 2.5, 6], 0],
  ["wall-conservative", "wall-north-west", [32, 0, 3], 0],
  ["wall-dense", "wall-north-east", [34, 0, 3], 0],
  ["wall-conservative", "wall-south-west", [32, 0, 7], Math.PI],
  ["wall-dense", "wall-south-east", [34, 0, 7], Math.PI],
  ["wall-conservative", "wall-west-north", [31, 0, 4], Math.PI / 2],
  ["vent-panel", "wall-west-south", [31, 0, 6], Math.PI / 2],
  ["wall-conservative", "wall-east-north", [35, 0, 4], -Math.PI / 2],
  ["doorway", "door-east-south", [35, 0, 6], -Math.PI / 2],
  ["corner", "corner-nw", [31, 0, 3], 0],
  ["corner", "corner-ne", [35, 0, 3], -Math.PI / 2],
  ["corner", "corner-sw", [31, 0, 7], Math.PI / 2],
  ["corner", "corner-se", [35, 0, 7], Math.PI],
  ["column", "column-nw", [31.5, 0, 3.5], 0],
  ["column", "column-ne", [34.5, 0, 3.5], 0],
  ["column", "column-sw", [31.5, 0, 6.5], 0],
  ["column", "column-se", [34.5, 0, 6.5], 0],
  ["landmark-relay", "landmark", [33, 0, 6], 0],
];

class Adapter {
  #child;
  #lines;
  #pending = [];
  #stderr = "";

  constructor() {
    this.#child = spawn(
      "cargo",
      [
        "run",
        "--locked",
        "--quiet",
        "-p",
        "loading-bay-game",
        "--bin",
        "studio-adapter",
      ],
      { cwd: ROOT, stdio: ["pipe", "pipe", "pipe"] },
    );
    this.#child.stderr.setEncoding("utf8");
    this.#child.stderr.on("data", (chunk) => {
      this.#stderr += chunk;
    });
    this.#lines = createInterface({ input: this.#child.stdout });
    this.#lines.on("line", (line) => {
      const pending = this.#pending.shift();
      if (pending === undefined) {
        throw new Error(`unexpected Studio adapter response: ${line}`);
      }
      pending.resolve(line);
    });
    this.#child.on("exit", (code) => {
      if (code !== 0 && this.#pending.length > 0) {
        const error = new Error(
          `Studio adapter exited ${String(code)}\n${this.#stderr}`,
        );
        for (const pending of this.#pending.splice(0)) {
          pending.reject(error);
        }
      }
    });
  }

  async send(request) {
    const started = performance.now();
    const line = await new Promise((resolveLine, reject) => {
      this.#pending.push({ resolve: resolveLine, reject });
      this.#child.stdin.write(`${JSON.stringify(request)}\n`);
    });
    const elapsedMs = performance.now() - started;
    const response = JSON.parse(line);
    if (response.type === "rejected") {
      throw new Error(
        `${request.type} rejected: ${JSON.stringify(response.error)}`,
      );
    }
    return { response, elapsedMs, responseBytes: Buffer.byteLength(line) };
  }

  async close() {
    this.#child.stdin.end();
    await new Promise((resolveExit, reject) => {
      this.#child.once("exit", (code) => {
        if (code === 0) resolveExit();
        else
          reject(
            new Error(`Studio adapter exited ${String(code)}\n${this.#stderr}`),
          );
      });
    });
  }
}

function projectHash(response) {
  return response.project.identity.projectHash;
}

function quaternion(yaw) {
  return [0, Math.sin(yaw / 2), 0, Math.cos(yaw / 2)];
}

function materialId(name) {
  return `material/brush-kit/${name}`;
}

function meshId(name) {
  return `mesh/${name}`;
}

function objectId(name) {
  return `voxel-object/brush-${name}`;
}

function metric(preview, key, fallback = null) {
  return preview[key] ?? preview.selectedFrame?.[key] ?? fallback;
}

const adapter = new Adapter();
const root = dirname(dirname(dirname(PROJECT)));
let current = (
  await adapter.send({
    type: "openProject",
    protocolVersion: 11,
    requestId: "brush-open",
    root,
    projectFile: relative(root, PROJECT),
  })
).response;
const startingHash = projectHash(current);
const authored = [];

for (const module of modules) {
  const sourcePath = `content/assets/brush-kit/${module.name}.mesh.json`;
  const glbPath = `content/assets/brush-kit/${module.name}.glb`;
  const preparedImport = (
    await adapter.send({
      type: "prepareAssetImport",
      protocolVersion: 11,
      requestId: `brush-import-prepare-${module.name}`,
      expectedProjectHash: projectHash(current),
      source: { scope: "project", path: sourcePath },
      settings: {
        scale: 1,
        generateCollision: false,
        materialNamespace: "brush-kit",
      },
    })
  ).response;
  if (preparedImport.plan.meshAssetId !== meshId(module.name)) {
    throw new Error(
      `unexpected source identity ${preparedImport.plan.meshAssetId}`,
    );
  }
  current = (
    await adapter.send({
      type: "applyAssetImport",
      protocolVersion: 11,
      requestId: `brush-import-apply-${module.name}`,
      expectedProjectHash: projectHash(current),
      planId: preparedImport.plan.planId,
      expectedPlanHash: preparedImport.plan.planHash,
    })
  ).response;

  const inspected = await adapter.send({
    type: "inspectVoxelObjectSource",
    protocolVersion: 11,
    requestId: `brush-inspect-${module.name}`,
    expectedProjectHash: projectHash(current),
    sourceKind: "static",
    sourceAssetId: meshId(module.name),
    source: { scope: "project", path: glbPath },
    meshPrimitive: "group/0",
  });
  const conversion = await adapter.send({
    type: "prepareVoxelObjectConversion",
    protocolVersion: 11,
    requestId: `brush-convert-prepare-${module.name}`,
    expectedProjectHash: projectHash(current),
    sourceKind: "static",
    sourceAssetId: meshId(module.name),
    source: { scope: "project", path: glbPath },
    targetAssetId: objectId(module.name),
    meshPrimitive: "group/0",
    settings: {
      mesh: {
        conversion: {
          resolution: module.resolution,
          cellSize: module.cellSize,
          chunkSize: 16,
          origin: [0, 0, 0],
          fitPolicy: "contain",
          originPolicy: "sourceOrigin",
          mode: "surface",
          materialPalette: [
            {
              materialSlot: 1,
              materialAssetId: materialId(module.name),
              displayName: module.role,
            },
          ],
          materialMap: [
            {
              sourceMaterialSlot: 0,
              sourceMaterialName: module.name,
              voxelMaterialSlot: 1,
            },
          ],
          maxOutputVoxels: 20_000,
        },
        transform: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1],
        materialPolicy: { textureAssets: [], textureBindings: [] },
      },
      pivot: [0, 0, 0],
      anchorPolicy: { kind: "preserveSourceSpace" },
    },
    clips: [],
    frame: { kind: "default" },
    maxPreviewSamples: 64,
  });
  const preview = conversion.response.preview;
  const previewDefinition = conversion.response.projection.ops.find(
    (operation) =>
      operation.op === "defineVoxelObject" &&
      operation.asset.asset === objectId(module.name),
  )?.asset;
  if (previewDefinition === undefined) {
    throw new Error(`private preview omitted ${objectId(module.name)}`);
  }
  const previewBounds = previewDefinition.meshes.reduce(
    (bounds, mesh) => ({
      min: bounds.min.map((value, axis) =>
        Math.min(value, mesh.payload.bounds.min[axis]),
      ),
      max: bounds.max.map((value, axis) =>
        Math.max(value, mesh.payload.bounds.max[axis]),
      ),
    }),
    {
      min: [
        Number.POSITIVE_INFINITY,
        Number.POSITIVE_INFINITY,
        Number.POSITIVE_INFINITY,
      ],
      max: [
        Number.NEGATIVE_INFINITY,
        Number.NEGATIVE_INFINITY,
        Number.NEGATIVE_INFINITY,
      ],
    },
  );
  module.placementScale = module.dimensions.map(
    (dimension, axis) =>
      dimension / (previewBounds.max[axis] - previewBounds.min[axis]),
  );
  current = (
    await adapter.send({
      type: "applyVoxelObjectConversion",
      protocolVersion: 11,
      requestId: `brush-convert-apply-${module.name}`,
      expectedProjectHash: projectHash(current),
      planId: conversion.response.plan.planId,
      expectedPlanHash: conversion.response.plan.planHash,
      expectedOutputHash: preview.outputHash,
    })
  ).response;
  const source = await stat(resolve(root, glbPath));
  authored.push({
    ...module,
    source: {
      meshJsonPath: sourcePath,
      glbPath,
      glbBytes: source.size,
      inspectedGroups: inspected.response.inspection.metadata.groups.length,
    },
    conversion: {
      outputHash: preview.outputHash,
      storedFrames: preview.storedFrameCount,
      aggregateVoxels: preview.aggregateVoxels,
      resolvedCells: metric(preview, "resolvedCells"),
      worstCaseFaces: metric(preview, "worstCaseFaces"),
      vertices: metric(preview, "vertexCount"),
      indices: metric(preview, "indexCount"),
      groups: metric(preview, "groupCount"),
      prepareMs: Number(conversion.elapsedMs.toFixed(3)),
      prepareResponseBytes: conversion.responseBytes,
    },
  });
}

const attachmentSamples = [];
for (const [name, instanceId, translation, yaw] of placements) {
  const module = authored.find((entry) => entry.name === name);
  await adapter.send({
    type: "prepareVoxelObjectPlacement",
    protocolVersion: 11,
    requestId: `brush-place-prepare-${instanceId}`,
    expectedProjectHash: projectHash(current),
    assetId: objectId(name),
    expectedObjectContentHash: module.conversion.outputHash,
  });
  const attached = await adapter.send({
    type: "attachVoxelObjectInstance",
    protocolVersion: 11,
    requestId: `brush-place-attach-${instanceId}`,
    expectedProjectHash: projectHash(current),
    sceneId: "scene/loading-bay",
    instance: {
      instanceId: `brush-proof-${instanceId}`,
      voxelObjectAssetId: objectId(name),
      frame: { kind: "default" },
      translation,
      rotation: quaternion(yaw),
      scale: module.placementScale,
      materialOverrides:
        instanceId === "wall-south-west"
          ? [
              {
                materialSlot: 1,
                materialAssetId: materialId("wall-dense"),
              },
            ]
          : [],
    },
  });
  current = attached.response;
  attachmentSamples.push({
    instanceId: `brush-proof-${instanceId}`,
    elapsedMs: Number(attached.elapsedMs.toFixed(3)),
    responseBytes: attached.responseBytes,
  });
}

const finalHash = projectHash(current);
const reopened = (
  await adapter.send({
    type: "readProject",
    protocolVersion: 11,
    requestId: "brush-read",
  })
).response;
await adapter.close();
const freshAdapter = new Adapter();
const fresh = (
  await freshAdapter.send({
    type: "openProject",
    protocolVersion: 11,
    requestId: "brush-fresh-open",
    root,
    projectFile: relative(root, PROJECT),
  })
).response;
await freshAdapter.close();

const canonical = JSON.parse(await readFile(PROJECT, "utf8"));
for (const module of authored) {
  const stored = canonical.assets.find(
    (asset) => asset.id === objectId(module.name),
  )?.voxelObject;
  const definition = reopened.project.projection.ops.find(
    (operation) =>
      operation.op === "defineVoxelObject" &&
      operation.asset.asset === objectId(module.name),
  )?.asset;
  if (stored === undefined || definition === undefined) {
    throw new Error(`missing retained metrics for ${module.name}`);
  }
  const resolvedCells = stored.defaultFrame.representation.sparseRuns.reduce(
    (total, run) => total + run.length,
    0,
  );
  const meshes = definition.meshes;
  const vertices = meshes.reduce(
    (total, mesh) => total + mesh.payload.layout.vertexCount,
    0,
  );
  const indices = meshes.reduce(
    (total, mesh) => total + mesh.payload.layout.indexCount,
    0,
  );
  const groups = meshes.reduce(
    (total, mesh) => total + mesh.payload.groups.length,
    0,
  );
  const physicalBounds = {
    min: stored.bounds.min.map(
      (value, index) =>
        (value - stored.grid.pivot[index]) * stored.grid.cellSize,
    ),
    max: stored.bounds.max.map(
      (value, index) =>
        (value + 1 - stored.grid.pivot[index]) * stored.grid.cellSize,
    ),
  };
  Object.assign(module.conversion, {
    resolvedCells,
    sparseRuns: stored.defaultFrame.representation.sparseRuns.length,
    worstCaseFaces: resolvedCells * 6,
    vertices,
    indices,
    groups,
    meshChunks: meshes.length,
    estimatedMeshBytes: vertices * 24 + indices * 4,
    serializedObjectBytes: Buffer.byteLength(JSON.stringify(stored)),
    physicalBounds,
    placedDimensions: physicalBounds.max.map(
      (value, axis) =>
        (value - physicalBounds.min[axis]) * module.placementScale[axis],
    ),
  });
}

const projectBytes = (await stat(PROJECT)).size;
const evidence = {
  schemaVersion: 1,
  task: 6356,
  project: relative(ROOT, PROJECT),
  startingHash,
  finalHash,
  readbackHash: projectHash(reopened),
  projectBytes,
  authoringPath:
    "Studio protocol 11 import -> inspect -> private conversion preview -> atomic apply -> placement prepare -> attach",
  authority:
    "voxel-object instances are decorative; existing coarse material-voxel environment and explicit entity colliders remain gameplay collision/navigation truth",
  modules: authored,
  proofRoom: {
    sceneId: "scene/loading-bay",
    placementCount: placements.length,
    definitionCount: modules.length,
    reusedDefinitionCount: new Set(placements.map(([name]) => name)).size,
    origin: [31, 0, 3],
    extent: [35, 3, 7],
    materialOverrideInstance: "brush-proof-wall-south-west",
    attachmentSamples,
    attachmentTotalMs: Number(
      attachmentSamples
        .reduce((total, sample) => total + sample.elapsedMs, 0)
        .toFixed(3),
    ),
    attachmentMaxMs: Math.max(
      ...attachmentSamples.map((sample) => sample.elapsedMs),
    ),
  },
  reconstruction: {
    readbackHashMatches: projectHash(reopened) === finalHash,
    freshProcessHashMatches: projectHash(fresh) === finalHash,
    assetCount: reopened.project.voxelObjectAuthoring.assets.length,
    instanceCount: reopened.project.voxelObjectAuthoring.instances.length,
    freshAssetCount: fresh.project.voxelObjectAuthoring.assets.length,
    freshInstanceCount: fresh.project.voxelObjectAuthoring.instances.length,
  },
  aggregate: {
    resolvedCells: authored.reduce(
      (total, module) => total + module.conversion.resolvedCells,
      0,
    ),
    worstCaseFaces: authored.reduce(
      (total, module) => total + module.conversion.worstCaseFaces,
      0,
    ),
    vertices: authored.reduce(
      (total, module) => total + module.conversion.vertices,
      0,
    ),
    indices: authored.reduce(
      (total, module) => total + module.conversion.indices,
      0,
    ),
    estimatedMeshBytes: authored.reduce(
      (total, module) => total + module.conversion.estimatedMeshBytes,
      0,
    ),
  },
};
await mkdir(dirname(EVIDENCE), { recursive: true });
await writeFile(EVIDENCE, `${JSON.stringify(evidence, null, 2)}\n`);

if (
  !canonical.assets.some((asset) => asset.voxelObject !== undefined) ||
  !canonical.scenes[0].voxelObjectInstances?.length
) {
  throw new Error(
    "canonical project did not retain brush assets and instances",
  );
}

console.log(
  `authored ${modules.length} brush definitions and ${placements.length} instances; ${startingHash} -> ${finalHash}`,
);
