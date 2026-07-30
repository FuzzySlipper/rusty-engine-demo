import { createHash } from "node:crypto";
import { createInterface } from "node:readline";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import { performance } from "node:perf_hooks";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const PROJECT = resolve(ROOT, "content/projects/loading-bay.project.json");
const EVIDENCE = resolve(
  ROOT,
  "docs/evidence/voxel-level-brush-authoring.json",
);
const PROTOCOL_VERSION = 12;
const BATCH_LIMIT = 32;
const SURFACE_TILE_LIMIT = 8;
const SCENE_ID = "scene/loading-bay";

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
    const response = JSON.parse(line);
    if (response.type === "rejected") {
      throw new Error(
        `${request.type} rejected: ${JSON.stringify(response.error)}`,
      );
    }
    return {
      response,
      elapsedMs: performance.now() - started,
      responseBytes: Buffer.byteLength(line),
    };
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

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function projectHash(response) {
  return response.project.identity.projectHash;
}

function objectDimensions(project, assetId) {
  const object = project.assets.find(
    (asset) => asset.id === assetId,
  )?.voxelObject;
  if (object === undefined) throw new Error(`missing voxel object ${assetId}`);
  return object.bounds.min.map(
    (minimum, axis) =>
      (object.bounds.max[axis] - minimum + 1) * object.grid.cellSize,
  );
}

function scaleFor(project, assetId, targetDimensions) {
  const dimensions = objectDimensions(project, assetId);
  return targetDimensions.map((target, axis) => target / dimensions[axis]);
}

function key(x, z) {
  return `${String(x)},${String(z)}`;
}

function tileLayer(cells, y, prefix, assetId, worldY, project) {
  const remaining = new Set(
    cells
      .filter((voxel) => voxel.address[1] === y)
      .map((voxel) => key(voxel.address[0], voxel.address[2])),
  );
  const ordered = [...remaining]
    .map((entry) => entry.split(",").map(Number))
    .sort(([ax, az], [bx, bz]) => az - bz || ax - bx);
  const placements = [];
  for (const [x, z] of ordered) {
    if (!remaining.has(key(x, z))) continue;
    let width = 1;
    while (width < SURFACE_TILE_LIMIT && remaining.has(key(x + width, z))) {
      width += 1;
    }
    let depth = 1;
    depthLoop: while (depth < SURFACE_TILE_LIMIT) {
      for (let dx = 0; dx < width; dx += 1) {
        if (!remaining.has(key(x + dx, z + depth))) break depthLoop;
      }
      depth += 1;
    }
    for (let dz = 0; dz < depth; dz += 1) {
      for (let dx = 0; dx < width; dx += 1) {
        remaining.delete(key(x + dx, z + dz));
      }
    }
    placements.push({
      sceneId: SCENE_ID,
      instance: {
        instanceId: `level-${prefix}-x${String(x).padStart(2, "0")}-z${String(z).padStart(2, "0")}-${String(width)}x${String(depth)}`,
        voxelObjectAssetId: assetId,
        frame: { kind: "default" },
        translation: [x, worldY, z],
        rotation: [0, 0, 0, 1],
        scale: scaleFor(project, assetId, [
          width,
          prefix === "floor" ? 0.375 : 0.5,
          depth,
        ]),
        materialOverrides: [],
      },
    });
  }
  return placements;
}

function buildPlacements(project) {
  const scene = project.scenes.find((entry) => entry.id === SCENE_ID);
  if (scene?.voxelEnvironment?.gameplayProxy !== true) {
    throw new Error(
      "Loading Bay scene must retain the explicit gameplay proxy",
    );
  }
  const voxels = scene.voxelEnvironment.materialVoxels;
  const floor = tileLayer(
    voxels,
    0,
    "floor",
    "voxel-object/brush-floor-strip",
    0,
    project,
  );
  const ceiling = tileLayer(
    voxels,
    4,
    "ceiling",
    "voxel-object/brush-ceiling-strip",
    4,
    project,
  );
  const walls = voxels
    .filter((voxel) => voxel.address[1] === 1)
    .sort(
      (left, right) =>
        left.address[2] - right.address[2] ||
        left.address[0] - right.address[0],
    )
    .map((voxel, index) => {
      const [x, , z] = voxel.address;
      const assetId =
        voxel.materialSlot === 3
          ? "voxel-object/brush-wall-dense"
          : index % 13 === 0
            ? "voxel-object/brush-vent-panel"
            : "voxel-object/brush-wall-conservative";
      return {
        sceneId: SCENE_ID,
        instance: {
          instanceId: `level-wall-x${String(x).padStart(2, "0")}-z${String(z).padStart(2, "0")}`,
          voxelObjectAssetId: assetId,
          frame: { kind: "default" },
          translation: [x, 0, z],
          rotation: [0, 0, 0, 1],
          scale: scaleFor(project, assetId, [1, 4, 1]),
          materialOverrides: [],
        },
      };
    });

  const doorways = scene.entities
    .filter((entity) => entity.door !== undefined)
    .map((entity) => {
      const width = entity.bounds.max[0] - entity.bounds.min[0];
      return {
        sceneId: SCENE_ID,
        instance: {
          instanceId: `level-doorway-owner-${String(entity.id)}`,
          voxelObjectAssetId: "voxel-object/brush-doorway",
          frame: { kind: "default" },
          translation: [
            entity.translation[0] - width / 2,
            0,
            entity.translation[2] - 0.125,
          ],
          rotation: [0, 0, 0, 1],
          scale: scaleFor(project, "voxel-object/brush-doorway", [
            width,
            4,
            0.25,
          ]),
          materialOverrides: [],
        },
      };
    });

  const accents = [
    ["corner-nw", "voxel-object/brush-corner", [0, 0, 0], [1, 4, 1]],
    ["corner-ne", "voxel-object/brush-corner", [30, 0, 0], [1, 4, 1]],
    ["corner-sw", "voxel-object/brush-corner", [0, 0, 51], [1, 4, 1]],
    ["corner-se", "voxel-object/brush-corner", [30, 0, 51], [1, 4, 1]],
    [
      "column-arrival-a",
      "voxel-object/brush-column",
      [3, 0, 8],
      [0.75, 4, 0.75],
    ],
    [
      "column-arrival-b",
      "voxel-object/brush-column",
      [11, 0, 8],
      [0.75, 4, 0.75],
    ],
    [
      "column-generator-a",
      "voxel-object/brush-column",
      [18, 0, 22],
      [0.75, 4, 0.75],
    ],
    [
      "column-generator-b",
      "voxel-object/brush-column",
      [28, 0, 29],
      [0.75, 4, 0.75],
    ],
    [
      "column-dock-a",
      "voxel-object/brush-column",
      [17, 0, 38],
      [0.75, 4, 0.75],
    ],
    [
      "column-dock-b",
      "voxel-object/brush-column",
      [27, 0, 46],
      [0.75, 4, 0.75],
    ],
    [
      "relay-generator",
      "voxel-object/brush-landmark-relay",
      [22, 0, 24],
      [2, 2, 1],
    ],
    [
      "relay-extraction",
      "voxel-object/brush-landmark-relay",
      [20, 0, 42],
      [2, 2, 1],
    ],
  ].map(([name, assetId, translation, targetDimensions]) => ({
    sceneId: SCENE_ID,
    instance: {
      instanceId: `level-${name}`,
      voxelObjectAssetId: assetId,
      frame: { kind: "default" },
      translation,
      rotation: [0, 0, 0, 1],
      scale: scaleFor(project, assetId, targetDimensions),
      materialOverrides: [],
    },
  }));

  return [...floor, ...ceiling, ...walls, ...doorways, ...accents];
}

const sourceBytes = await readFile(PROJECT);
const project = JSON.parse(sourceBytes);
const placements = buildPlacements(project);
const expectedIds = new Set(
  placements.map((placement) => placement.instance.instanceId),
);
const existingIds = new Set(
  project.scenes
    .flatMap((scene) => scene.voxelObjectInstances ?? [])
    .map((instance) => instance.instanceId)
    .filter((instanceId) => instanceId.startsWith("level-")),
);
let resetEvidence = null;
if (existingIds.size > 0) {
  const prefix = placements
    .slice(0, existingIds.size)
    .map((placement) => placement.instance.instanceId);
  if (existingIds.size === expectedIds.size) {
    const exact = prefix.every((instanceId) => existingIds.has(instanceId));
    if (exact) {
      console.log(
        `Loading Bay brush level is already canonical (${String(existingIds.size)} instances).`,
      );
      process.exit(0);
    }
  }

  const existingLevelInstances = project.scenes.flatMap((scene) =>
    (scene.voxelObjectInstances ?? []).filter((instance) =>
      instance.instanceId.startsWith("level-"),
    ),
  );
  const levelOwnerIds = new Set(
    existingLevelInstances.map((instance) => instance.ownerEntityId),
  );
  const candidate = structuredClone(project);
  for (const scene of candidate.scenes) {
    scene.voxelObjectInstances = (scene.voxelObjectInstances ?? []).filter(
      (instance) => !instance.instanceId.startsWith("level-"),
    );
    scene.entities = scene.entities.filter(
      (entity) => !levelOwnerIds.has(entity.id),
    );
  }

  const temporaryRoot = await mkdtemp(
    resolve(tmpdir(), "rusty-engine-demo-brush-level-"),
  );
  const candidatePath = resolve(temporaryRoot, "loading-bay.project.json");
  try {
    await writeFile(candidatePath, `${JSON.stringify(candidate, null, 2)}\n`);
    await new Promise((resolveExit, reject) => {
      const child = spawn(
        "cargo",
        [
          "run",
          "--locked",
          "--quiet",
          "-p",
          "loading-bay-game",
          "--bin",
          "project-store",
          "--",
          "--input",
          candidatePath,
          "--output",
          PROJECT,
          "--replace",
        ],
        { cwd: ROOT, stdio: ["ignore", "pipe", "pipe"] },
      );
      let stderr = "";
      child.stderr.setEncoding("utf8");
      child.stderr.on("data", (chunk) => {
        stderr += chunk;
      });
      child.once("exit", (code) => {
        if (code === 0) resolveExit();
        else
          reject(
            new Error(
              `Rust ProjectStore reset exited ${String(code)}\n${stderr}`,
            ),
          );
      });
    });
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
  resetEvidence = {
    removedInstances: existingLevelInstances.length,
    removedOwners: levelOwnerIds.size,
    publication: "rust-project-store-admitted-replacement",
  };
}

const adapter = new Adapter();
let current = (
  await adapter.send({
    type: "openProject",
    protocolVersion: PROTOCOL_VERSION,
    requestId: "level-open",
    root: ROOT,
    projectFile: relative(ROOT, PROJECT),
  })
).response;
const startingHash = projectHash(current);
const batches = [];
for (
  let offset = resetEvidence === null ? existingIds.size : 0;
  offset < placements.length;
  offset += BATCH_LIMIT
) {
  const batch = placements.slice(offset, offset + BATCH_LIMIT);
  const applied = await adapter.send({
    type: "attachVoxelObjectInstances",
    protocolVersion: PROTOCOL_VERSION,
    requestId: `level-batch-${String(offset / BATCH_LIMIT).padStart(2, "0")}`,
    expectedProjectHash: projectHash(current),
    placements: batch,
  });
  current = applied.response;
  const receipt = current.receipt;
  if (
    receipt.kind !== "voxelObjectInstancesAttached" ||
    receipt.placements.length !== batch.length
  ) {
    throw new Error(`batch receipt mismatch at offset ${String(offset)}`);
  }
  batches.push({
    offset,
    placements: batch.length,
    elapsedMs: Number(applied.elapsedMs.toFixed(3)),
    responseBytes: applied.responseBytes,
    ownerEntityIds: receipt.placements.map(
      (placement) => placement.ownerEntityId,
    ),
  });
}
const reopened = (
  await adapter.send({
    type: "readProject",
    protocolVersion: PROTOCOL_VERSION,
    requestId: "level-read",
  })
).response;
await adapter.close();

const freshAdapter = new Adapter();
const fresh = (
  await freshAdapter.send({
    type: "openProject",
    protocolVersion: PROTOCOL_VERSION,
    requestId: "level-fresh-open",
    root: ROOT,
    projectFile: relative(ROOT, PROJECT),
  })
).response;
await freshAdapter.close();

const authoredIds = new Set(
  fresh.project.voxelObjectAuthoring.instances
    .map((entry) => entry.instance.instanceId)
    .filter((instanceId) => instanceId.startsWith("level-")),
);
if (
  authoredIds.size !== expectedIds.size ||
  [...expectedIds].some((instanceId) => !authoredIds.has(instanceId))
) {
  throw new Error("fresh adapter did not reconstruct the complete brush level");
}
if (
  projectHash(reopened) !== projectHash(current) ||
  projectHash(fresh) !== projectHash(current)
) {
  throw new Error("canonical reread/fresh-adapter project hashes diverged");
}

const finalBytes = await readFile(PROJECT);
const byAsset = Object.fromEntries(
  [
    ...new Set(
      placements.map((placement) => placement.instance.voxelObjectAssetId),
    ),
  ]
    .sort()
    .map((assetId) => [
      assetId,
      placements.filter(
        (placement) => placement.instance.voxelObjectAssetId === assetId,
      ).length,
    ]),
);
const evidence = {
  schemaVersion: 1,
  protocolVersion: PROTOCOL_VERSION,
  project: relative(ROOT, PROJECT),
  projectHashBefore: startingHash,
  projectHashAfter: projectHash(current),
  projectSha256: sha256(finalBytes),
  gameplayProxy: true,
  placementCount: placements.length,
  surfaceTileLimit: SURFACE_TILE_LIMIT,
  batchLimit: BATCH_LIMIT,
  batchCount: batches.length,
  oneRequestPerBatch: true,
  definitionsReused: Object.keys(byAsset).length,
  structuralProjectionBytes: Buffer.byteLength(
    JSON.stringify(fresh.project.projection),
  ),
  placementsByAsset: byAsset,
  batches,
  canonicalRereadMatched: true,
  freshAdapterMatched: true,
  reset: resetEvidence,
};
await writeFile(EVIDENCE, `${JSON.stringify(evidence, null, 2)}\n`);
console.log(JSON.stringify(evidence, null, 2));
