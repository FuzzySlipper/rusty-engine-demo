import { createHash } from "node:crypto";
import { createInterface } from "node:readline";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
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
const PROTOCOL_VERSION = 14;
const BATCH_LIMIT = 32;
const SURFACE_TILE_LIMIT = 8;
const SCENE_ID = "scene/loading-bay";
const COLLISION_BACKED_COLUMNS = [
  [3, 8],
  [11, 8],
  [17, 38],
  [18, 22],
  [27, 46],
  [28, 29],
];
const COLUMN_PROXY_VOXELS_REQUIRED = COLLISION_BACKED_COLUMNS.length * 3;
const COLUMN_PROXY_VOXELS_PREEXISTING = 4;
const COLLISION_BACKED_COLUMN_KEYS = new Set(
  COLLISION_BACKED_COLUMNS.map(([x, z]) => `${String(x)},${String(z)}`),
);

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

async function reservePort() {
  const server = createServer();
  await new Promise((resolveListen, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolveListen);
  });
  const address = server.address();
  if (address === null || typeof address === "string") {
    server.close();
    throw new Error("could not reserve a loopback authoring port");
  }
  await new Promise((resolveClose) => server.close(resolveClose));
  return address.port;
}

async function waitForHealth(address, child, output) {
  const deadline = Date.now() + 120_000;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) {
      throw new Error(
        `browser host exited ${String(child.exitCode)} before authoring\n${output()}`,
      );
    }
    try {
      const response = await fetch(`http://${address}/health`);
      if (response.ok) return;
    } catch {
      // The Rust host is still starting.
    }
    await new Promise((resolveDelay) => setTimeout(resolveDelay, 100));
  }
  throw new Error(`browser host did not become healthy\n${output()}`);
}

async function stopHost(child) {
  if (child.exitCode !== null) return;
  child.kill("SIGTERM");
  await new Promise((resolveExit) => child.once("exit", resolveExit));
}

async function ensureColumnGameplayProxies(project) {
  const scene = project.scenes.find((entry) => entry.id === SCENE_ID);
  const existing = new Map(
    scene.voxelEnvironment.materialVoxels.map((voxel) => [
      voxel.address.join(","),
      voxel.materialSlot,
    ]),
  );
  const required = COLLISION_BACKED_COLUMNS.flatMap(([x, z]) =>
    [1, 2, 3].map((y) => ({
      kind: "set",
      address: [x, y, z],
      material_slot: 1,
    })),
  );
  const missing = required.filter(
    (edit) => existing.get(edit.address.join(",")) === undefined,
  );
  if (missing.length === 0) {
    return { changedVoxels: 0, persistedToProject: false };
  }

  const port = await reservePort();
  const address = `127.0.0.1:${String(port)}`;
  const hostRoot = await mkdtemp(
    resolve(tmpdir(), "rusty-engine-demo-wall-proxy-host-"),
  );
  const hostDist = resolve(hostRoot, "dist");
  const hostSaves = resolve(hostRoot, "saves");
  await mkdir(hostDist);
  await writeFile(
    resolve(hostDist, "index.html"),
    "<!doctype html><title>authoring</title>\n",
  );
  const child = spawn(
    "cargo",
    [
      "run",
      "--locked",
      "--quiet",
      "-p",
      "loading-bay-game",
      "--bin",
      "browser-host",
      "--",
      "--addr",
      address,
      "--dist",
      hostDist,
      "--project",
      PROJECT,
      "--save-root",
      hostSaves,
    ],
    { cwd: ROOT, stdio: ["ignore", "pipe", "pipe"] },
  );
  let hostOutput = "";
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    hostOutput += chunk;
  });
  child.stderr.on("data", (chunk) => {
    hostOutput += chunk;
  });
  try {
    await waitForHealth(address, child, () => hostOutput);
    const beforeResponse = await fetch(`http://${address}/api/state`);
    const before = await beforeResponse.json();
    if (!beforeResponse.ok) {
      throw new Error(
        `could not read voxel authoring state: ${JSON.stringify(before)}`,
      );
    }
    const response = await fetch(`http://${address}/api/voxel-edit`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        expectedRevision: before.voxelRevision,
        persistToProject: true,
        edits: missing,
      }),
    });
    const receipt = await response.json();
    if (
      !response.ok ||
      receipt.voxelEditReceipt?.changedVoxels !== missing.length ||
      receipt.voxelEditReceipt?.persistedToProject !== true
    ) {
      throw new Error(
        `column proxy authoring failed: ${JSON.stringify(receipt)}`,
      );
    }
    return receipt.voxelEditReceipt;
  } finally {
    await stopHost(child);
    await rm(hostRoot, { recursive: true, force: true });
  }
}

function sameInstance(left, right) {
  return (
    JSON.stringify({
      instanceId: left.instanceId,
      voxelObjectAssetId: left.voxelObjectAssetId,
      frame: left.frame,
      translation: left.translation,
      rotation: left.rotation,
      scale: left.scale,
      materialOverrides: left.materialOverrides,
    }) === JSON.stringify(right)
  );
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
    .filter(
      (voxel) =>
        voxel.address[1] === 1 &&
        !COLLISION_BACKED_COLUMN_KEYS.has(
          `${String(voxel.address[0])},${String(voxel.address[2])}`,
        ),
    )
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
      const z = Math.round(entity.translation[2]);
      const solid = new Set(
        voxels
          .filter((voxel) => voxel.address[1] === 1 && voxel.address[2] === z)
          .map((voxel) => voxel.address[0]),
      );
      let openingStart = Math.floor(entity.translation[0]);
      while (!solid.has(openingStart - 1)) openingStart -= 1;
      let openingEnd = Math.floor(entity.translation[0]) + 1;
      while (!solid.has(openingEnd)) openingEnd += 1;
      const width = openingEnd - openingStart;
      return {
        sceneId: SCENE_ID,
        instance: {
          instanceId: `level-doorway-owner-${String(entity.id)}`,
          voxelObjectAssetId: "voxel-object/brush-doorway",
          frame: { kind: "default" },
          translation: [openingStart, 0, z],
          rotation: [0, 0, 0, 1],
          scale: scaleFor(project, "voxel-object/brush-doorway", [width, 4, 1]),
          materialOverrides: [],
        },
      };
    });

  const accents = [
    ["corner-nw", "voxel-object/brush-corner", [0, 0, 0], [1, 4, 1]],
    ["corner-ne", "voxel-object/brush-corner", [30, 0, 0], [1, 4, 1]],
    ["corner-sw", "voxel-object/brush-corner", [0, 0, 49], [1, 4, 1]],
    ["corner-se", "voxel-object/brush-corner", [30, 0, 49], [1, 4, 1]],
    ["column-arrival-a", "voxel-object/brush-column", [3, 0, 8], [1, 4, 1]],
    ["column-arrival-b", "voxel-object/brush-column", [11, 0, 8], [1, 4, 1]],
    ["column-generator-a", "voxel-object/brush-column", [18, 0, 22], [1, 4, 1]],
    ["column-generator-b", "voxel-object/brush-column", [28, 0, 29], [1, 4, 1]],
    ["column-dock-a", "voxel-object/brush-column", [17, 0, 38], [1, 4, 1]],
    ["column-dock-b", "voxel-object/brush-column", [27, 0, 46], [1, 4, 1]],
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
let project = JSON.parse(sourceBytes);
const proxyEditReceipt = await ensureColumnGameplayProxies(project);
if (proxyEditReceipt.changedVoxels > 0) {
  project = JSON.parse(await readFile(PROJECT));
}
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
  if (existingIds.size === expectedIds.size) {
    const currentInstances = new Map(
      project.scenes
        .flatMap((scene) => scene.voxelObjectInstances ?? [])
        .filter((instance) => instance.instanceId.startsWith("level-"))
        .map((instance) => [instance.instanceId, instance]),
    );
    const exact = placements.every(({ instance }) => {
      const current = currentInstances.get(instance.instanceId);
      return current !== undefined && sameInstance(current, instance);
    });
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
  proxyEdit: {
    requiredColumnCount: COLLISION_BACKED_COLUMNS.length,
    requiredVoxelCount: COLUMN_PROXY_VOXELS_REQUIRED,
    preexistingVoxelCount: COLUMN_PROXY_VOXELS_PREEXISTING,
    addedVoxelCount:
      COLUMN_PROXY_VOXELS_REQUIRED - COLUMN_PROXY_VOXELS_PREEXISTING,
    changedThisRun: proxyEditReceipt.changedVoxels,
    persistedThisRun: proxyEditReceipt.persistedToProject,
  },
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
