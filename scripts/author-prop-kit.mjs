import { spawn } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { dirname, relative, resolve } from "node:path";
import { createInterface } from "node:readline";
import { performance } from "node:perf_hooks";

const ROOT = resolve(import.meta.dirname, "..");
const PROTOCOL_VERSION = 15;
const PROJECT =
  process.argv[2] === undefined
    ? resolve(ROOT, "content/projects/loading-bay.project.json")
    : resolve(process.argv[2]);
const EVIDENCE =
  process.argv[3] === undefined
    ? resolve(ROOT, "docs/evidence/prop-kit-authoring.json")
    : resolve(process.argv[3]);
const PROFILE = process.argv[4] ?? "loading-bay";
const PROJECT_ROOT =
  process.argv[5] === undefined
    ? dirname(dirname(dirname(PROJECT)))
    : resolve(process.argv[5]);
const MANIFEST = JSON.parse(
  await readFile(resolve(ROOT, "content/assets/prop-kit/source-manifest.json")),
);

const loadingBayAppearances = [
  [3, "security-door"],
  [6, "control-panel"],
  [7, "extraction-beacon"],
  [10, "status-runner"],
  [11, "security-door"],
  [12, "security-door"],
  [13, "security-door"],
  [20, "energy-cell"],
  [21, "med-patch"],
  [22, "scatter-shells"],
  [23, "breach-scattergun"],
  [24, "med-patch"],
  [25, "impact-vest"],
  [26, "maintenance-pass"],
  [27, "hazard-marker"],
  [28, "rivet-carbine"],
  [30, "security-door"],
  [32, "level-exit"],
  [33, "med-patch"],
  [34, "energy-cell"],
  [60, "med-patch"],
  [61, "energy-cell"],
  [62, "med-patch"],
  [63, "energy-cell"],
  [64, "med-patch"],
  [65, "energy-cell"],
];

const loadingBayLandmarks = [
  {
    entityId: 8,
    legacyEntityId: 1_000,
    name: "loading-bay-overhead-crane",
    asset: "mesh/prop-kit/landmark-crane",
    translation: [16.5, 5.75, 39],
    rotation: [0, 0, 0, 1],
    scale: [1, 1, 1],
  },
  {
    entityId: 9,
    legacyEntityId: 1_001,
    name: "generator-coolant-tank",
    asset: "mesh/prop-kit/landmark-tank",
    translation: [28, 3.5, 27],
    rotation: [0, 0, 0, 1],
    scale: [1, 1, 1],
  },
];

const profiles = {
  "loading-bay": {
    appearances: loadingBayAppearances,
    landmarks: loadingBayLandmarks,
    assetNames: null,
  },
  "converted-wall": {
    appearances: [
      [3, "security-door"],
      [4, "status-runner"],
      [5, "status-runner"],
      [6, "control-panel"],
      [10, "status-runner"],
    ],
    landmarks: [],
    assetNames: new Set(["security-door", "control-panel", "status-runner"]),
  },
};
const profile = profiles[PROFILE];
if (profile === undefined) {
  throw new Error(`unknown prop-kit authoring profile ${PROFILE}`);
}
const appearances = new Map(profile.appearances);
const landmarks = profile.landmarks;

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
    return {
      response,
      elapsedMs: Number(elapsedMs.toFixed(3)),
      responseBytes: Buffer.byteLength(line),
    };
  }

  async close() {
    await new Promise((resolveExit, reject) => {
      this.#child.once("exit", (code) => {
        if (code === 0) resolveExit();
        else
          reject(
            new Error(`Studio adapter exited ${String(code)}\n${this.#stderr}`),
          );
      });
      this.#child.stdin.end();
    });
  }
}

function projectHash(response) {
  return response.project.identity.projectHash;
}

function sceneRevision(response) {
  return response.project.identity.sceneRevision;
}

function projectDocument(response) {
  return JSON.parse(response.project.canonical.projectJson);
}

function entryScene(response) {
  const project = projectDocument(response);
  return project.scenes.find((scene) => scene.id === project.entryScene);
}

function importedAsset(response, assetId) {
  return projectDocument(response).assets.find((asset) => asset.id === assetId);
}

const adapter = new Adapter();
const root = PROJECT_ROOT;
let current = (
  await adapter.send({
    type: "openProject",
    protocolVersion: PROTOCOL_VERSION,
    requestId: "prop-open",
    root,
    projectFile: relative(root, PROJECT),
  })
).response;
const startingHash = projectHash(current);
const imports = [];

for (const asset of MANIFEST.assets.filter(
  (candidate) =>
    profile.assetNames === null ||
    profile.assetNames.has(candidate.assetId.split("/").at(-1)),
)) {
  const assetId = asset.assetId;
  const existing = importedAsset(current, assetId);
  const requestId = assetId.replaceAll("/", "-");
  if (
    existing?.import?.sourceHash === asset.contentSha256 &&
    existing.import.source?.path === asset.importSourcePath
  ) {
    imports.push({
      assetId,
      sourceHash: asset.contentSha256,
      sourceBytes: asset.byteCount,
      generatedAssetIds: existing.import.generatedAssetIds,
      reimportKind: "alreadyCurrent",
      prepareMs: 0,
      applyMs: 0,
      applyResponseBytes: 0,
    });
    continue;
  }
  const prepared =
    existing?.import === undefined
      ? await adapter.send({
          type: "prepareAssetImport",
          protocolVersion: PROTOCOL_VERSION,
          requestId: `prop-import-prepare-${requestId}`,
          expectedProjectHash: projectHash(current),
          source: { scope: "project", path: asset.importSourcePath },
          settings: {
            scale: 1,
            generateCollision: false,
            materialNamespace: "prop-kit",
          },
        })
      : await adapter.send({
          type: "prepareAssetReimport",
          protocolVersion: PROTOCOL_VERSION,
          requestId: `prop-reimport-prepare-${requestId}`,
          expectedProjectHash: projectHash(current),
          assetId,
        });
  if (prepared.response.plan.meshAssetId !== assetId) {
    throw new Error(
      `Studio import produced ${prepared.response.plan.meshAssetId} for ${assetId}`,
    );
  }
  const applied = await adapter.send({
    type: "applyAssetImport",
    protocolVersion: PROTOCOL_VERSION,
    requestId: `prop-import-apply-${requestId}`,
    expectedProjectHash: projectHash(current),
    planId: prepared.response.plan.planId,
    expectedPlanHash: prepared.response.plan.planHash,
  });
  current = applied.response;
  imports.push({
    assetId,
    sourceHash: prepared.response.plan.sourceHash,
    sourceBytes: prepared.response.plan.sourceByteCount,
    generatedAssetIds: prepared.response.plan.generatedAssetIds,
    reimportKind: prepared.response.plan.reimportKind ?? "initialImport",
    prepareMs: prepared.elapsedMs,
    applyMs: applied.elapsedMs,
    applyResponseBytes: applied.responseBytes,
  });
}

for (const [entityId, assetName] of appearances) {
  const entity = entryScene(current).entities.find(
    (candidate) => candidate.id === entityId,
  );
  if (entity === undefined) {
    throw new Error(
      `project is missing mapped prop entity ${String(entityId)}`,
    );
  }
  const asset = `mesh/prop-kit/${assetName}`;
  if (entity.renderable?.asset === asset) {
    continue;
  }
  current = (
    await adapter.send({
      type: "setSceneObjectAppearance",
      protocolVersion: PROTOCOL_VERSION,
      requestId: `prop-appearance-${String(entityId)}`,
      expectedProjectHash: projectHash(current),
      expectedSceneRevision: sceneRevision(current),
      entityId,
      appearance: {
        kind: "staticMesh",
        asset,
        visible: entity.renderable.visible,
      },
    })
  ).response;
}

for (const landmark of landmarks) {
  const legacy = entryScene(current).entities.find(
    (entity) => entity.id === landmark.legacyEntityId,
  );
  if (legacy !== undefined) {
    if (
      legacy.name !== landmark.name ||
      legacy.renderable?.asset !== landmark.asset
    ) {
      throw new Error(
        `legacy landmark entity ${String(landmark.legacyEntityId)} has unexpected authored content`,
      );
    }
    current = (
      await adapter.send({
        type: "deleteSceneObject",
        protocolVersion: PROTOCOL_VERSION,
        requestId: `prop-landmark-retire-${String(landmark.legacyEntityId)}`,
        expectedProjectHash: projectHash(current),
        expectedSceneRevision: sceneRevision(current),
        entityId: landmark.legacyEntityId,
      })
    ).response;
  }
  const existing = entryScene(current).entities.find(
    (entity) => entity.id === landmark.entityId,
  );
  if (existing !== undefined) {
    if (
      existing.renderable?.asset !== landmark.asset ||
      JSON.stringify(existing.translation) !==
        JSON.stringify(landmark.translation)
    ) {
      throw new Error(
        `landmark entity ${String(landmark.entityId)} already has different authored content`,
      );
    }
    continue;
  }
  current = (
    await adapter.send({
      type: "createSceneObject",
      protocolVersion: PROTOCOL_VERSION,
      requestId: `prop-landmark-${String(landmark.entityId)}`,
      expectedProjectHash: projectHash(current),
      expectedSceneRevision: sceneRevision(current),
      object: {
        entityId: landmark.entityId,
        name: landmark.name,
        parentEntityId: null,
        childOrder: landmark.entityId,
        transform: {
          translation: landmark.translation,
          rotation: landmark.rotation,
          scale: landmark.scale,
        },
        appearance: {
          kind: "staticMesh",
          asset: landmark.asset,
          visible: true,
        },
        collision: null,
        kinematic: null,
      },
    })
  ).response;
}

const securityDoorReimport = await adapter.send({
  type: "prepareAssetReimport",
  protocolVersion: PROTOCOL_VERSION,
  requestId: "prop-security-door-reimport",
  expectedProjectHash: projectHash(current),
  assetId: "mesh/prop-kit/security-door",
});
if (securityDoorReimport.response.plan.reimportKind !== "noop") {
  current = (
    await adapter.send({
      type: "applyAssetImport",
      protocolVersion: PROTOCOL_VERSION,
      requestId: "prop-security-door-reimport-apply",
      expectedProjectHash: projectHash(current),
      planId: securityDoorReimport.response.plan.planId,
      expectedPlanHash: securityDoorReimport.response.plan.planHash,
    })
  ).response;
}

const finalHash = projectHash(current);
const canonical = (
  await adapter.send({
    type: "readProject",
    protocolVersion: PROTOCOL_VERSION,
    requestId: "prop-canonical-read",
  })
).response;
await adapter.close();

const freshAdapter = new Adapter();
const fresh = (
  await freshAdapter.send({
    type: "openProject",
    protocolVersion: PROTOCOL_VERSION,
    requestId: "prop-fresh-open",
    root,
    projectFile: relative(root, PROJECT),
  })
).response;
await freshAdapter.close();

if (projectHash(canonical) !== finalHash || projectHash(fresh) !== finalHash) {
  throw new Error(
    "canonical reread or fresh adapter did not retain prop-kit hash",
  );
}

const finalProject = projectDocument(fresh);
const finalScene = finalProject.scenes.find(
  (scene) => scene.id === finalProject.entryScene,
);
const evidence = {
  schemaVersion: 1,
  project: {
    path: relative(ROOT, PROJECT),
    startingHash,
    finalHash,
    schemaVersion: finalProject.schemaVersion,
    assetCount: finalProject.assets.length,
    entityCount: finalScene.entities.length,
  },
  importCount: imports.length,
  imports,
  appearanceMappings: [...appearances].map(([entityId, assetName]) => ({
    entityId,
    assetId: `mesh/prop-kit/${assetName}`,
  })),
  landmarks,
  reimport: {
    assetId: "mesh/prop-kit/security-door",
    kind: securityDoorReimport.response.plan.reimportKind,
    sourceHash: securityDoorReimport.response.plan.sourceHash,
  },
  reload: {
    canonicalHash: projectHash(canonical),
    freshProcessHash: projectHash(fresh),
    passed: true,
  },
};
await writeFile(EVIDENCE, `${JSON.stringify(evidence, null, 2)}\n`);
console.log(
  JSON.stringify({
    finalHash,
    importedAssets: imports.length,
    mappedEntities: appearances.size,
    landmarks: landmarks.length,
  }),
);
