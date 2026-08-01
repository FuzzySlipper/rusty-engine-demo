import { spawn } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { dirname, relative, resolve } from "node:path";
import { createInterface } from "node:readline";
import { performance } from "node:perf_hooks";

const ROOT = resolve(import.meta.dirname, "..");
const PROTOCOL_VERSION = 14;
const PROJECT =
  process.argv[2] === undefined
    ? resolve(ROOT, "content/projects/loading-bay.project.json")
    : resolve(process.argv[2]);
const EVIDENCE =
  process.argv[3] === undefined
    ? resolve(ROOT, "docs/evidence/actor-kit-authoring.json")
    : resolve(process.argv[3]);
const PROJECT_ROOT =
  process.argv[4] === undefined
    ? dirname(dirname(dirname(PROJECT)))
    : resolve(process.argv[4]);
const MANIFEST = JSON.parse(
  await readFile(
    resolve(ROOT, "content/assets/actor-kit/source-manifest.json"),
  ),
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

  async sendRaw(request) {
    const started = performance.now();
    const line = await new Promise((resolveLine, reject) => {
      this.#pending.push({ resolve: resolveLine, reject });
      this.#child.stdin.write(`${JSON.stringify(request)}\n`);
    });
    return {
      response: JSON.parse(line),
      elapsedMs: Number((performance.now() - started).toFixed(3)),
      responseBytes: Buffer.byteLength(line),
    };
  }

  async send(request) {
    const result = await this.sendRaw(request);
    if (result.response.type === "rejected") {
      throw new Error(
        `${request.type} rejected: ${JSON.stringify(result.response.error)}`,
      );
    }
    return result;
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

function projectDocument(response) {
  return JSON.parse(response.project.canonical.projectJson);
}

function importedAsset(response, assetId) {
  return projectDocument(response).assets.find((asset) => asset.id === assetId);
}

const root = PROJECT_ROOT;
const adapter = new Adapter();
let current = (
  await adapter.send({
    type: "openProject",
    protocolVersion: PROTOCOL_VERSION,
    requestId: "actor-kit-open",
    root,
    projectFile: relative(root, PROJECT),
  })
).response;
const startingHash = projectHash(current);
const stale = await adapter.sendRaw({
  type: "prepareAssetImport",
  protocolVersion: PROTOCOL_VERSION,
  requestId: "actor-kit-stale-import",
  expectedProjectHash:
    "0000000000000000000000000000000000000000000000000000000000000000",
  source: {
    scope: "project",
    path: "content/assets/actor-kit/arc-warden.glb",
  },
  settings: {
    scale: 1,
    generateCollision: false,
    materialNamespace: "actor-kit",
  },
});
if (
  stale.response.type !== "rejected" ||
  stale.response.error.code !== "project.staleHash"
) {
  throw new Error(
    `stale actor import did not fail closed: ${JSON.stringify(stale)}`,
  );
}
if (projectHash(current) !== startingHash) {
  throw new Error("stale actor import changed the accepted project");
}

const imports = [];
for (const variant of MANIFEST.variants) {
  const existing = importedAsset(current, variant.assetId);
  const sourcePath = `content/assets/actor-kit/${variant.file}`;
  let prepared;
  if (
    existing?.import?.sourceHash === variant.sha256 &&
    existing.import.source?.path === sourcePath
  ) {
    imports.push({
      assetId: variant.assetId,
      sourceHash: variant.sha256,
      sourceBytes: variant.bytes,
      generatedAssetIds: existing.import.generatedAssetIds,
      reimportKind: "alreadyCurrent",
      prepareMs: 0,
      applyMs: 0,
      applyResponseBytes: 0,
    });
    continue;
  }
  if (existing?.import === undefined) {
    prepared = await adapter.send({
      type: "prepareAssetImport",
      protocolVersion: PROTOCOL_VERSION,
      requestId: `actor-kit-import-${variant.file}`,
      expectedProjectHash: projectHash(current),
      source: { scope: "project", path: sourcePath },
      settings: {
        scale: 1,
        generateCollision: false,
        materialNamespace: null,
      },
    });
  } else {
    prepared = await adapter.send({
      type: "prepareAssetReimport",
      protocolVersion: PROTOCOL_VERSION,
      requestId: `actor-kit-reimport-${variant.file}`,
      expectedProjectHash: projectHash(current),
      assetId: variant.assetId,
    });
  }
  if (prepared.response.plan.meshAssetId !== variant.assetId) {
    throw new Error(
      `${variant.file} prepared ${prepared.response.plan.meshAssetId}, ` +
        `expected ${variant.assetId}`,
    );
  }
  const applied = await adapter.send({
    type: "applyAssetImport",
    protocolVersion: PROTOCOL_VERSION,
    requestId: `actor-kit-apply-${variant.file}`,
    expectedProjectHash: projectHash(current),
    planId: prepared.response.plan.planId,
    expectedPlanHash: prepared.response.plan.planHash,
  });
  current = applied.response;
  imports.push({
    assetId: variant.assetId,
    sourceHash: prepared.response.plan.sourceHash,
    sourceBytes: prepared.response.plan.sourceByteCount,
    generatedAssetIds: prepared.response.plan.generatedAssetIds,
    reimportKind: prepared.response.plan.reimportKind ?? "initialImport",
    prepareMs: prepared.elapsedMs,
    applyMs: applied.elapsedMs,
    applyResponseBytes: applied.responseBytes,
  });
}

const firstAssetId = MANIFEST.variants[0].assetId;
const noop = await adapter.send({
  type: "prepareAssetReimport",
  protocolVersion: PROTOCOL_VERSION,
  requestId: "actor-kit-noop-reimport",
  expectedProjectHash: projectHash(current),
  assetId: firstAssetId,
});
if (noop.response.plan.reimportKind !== "noop") {
  throw new Error(
    `unchanged actor source prepared ${noop.response.plan.reimportKind}, expected noop`,
  );
}

const finalHash = projectHash(current);
const canonical = (
  await adapter.send({
    type: "readProject",
    protocolVersion: PROTOCOL_VERSION,
    requestId: "actor-kit-read",
  })
).response;
await adapter.close();

const freshAdapter = new Adapter();
const fresh = (
  await freshAdapter.send({
    type: "openProject",
    protocolVersion: PROTOCOL_VERSION,
    requestId: "actor-kit-fresh-open",
    root,
    projectFile: relative(root, PROJECT),
  })
).response;
await freshAdapter.close();
if (projectHash(canonical) !== finalHash || projectHash(fresh) !== finalHash) {
  throw new Error("canonical reread or fresh adapter lost the actor library");
}

const finalProject = projectDocument(fresh);
const assets = MANIFEST.variants.map((variant) => {
  const asset = finalProject.assets.find(
    (candidate) => candidate.id === variant.assetId,
  );
  if (
    asset?.animatedMesh?.asset !== variant.assetId ||
    asset.import?.sourceHash !== variant.sha256
  ) {
    throw new Error(`${variant.assetId} was not durably imported`);
  }
  return {
    assetId: variant.assetId,
    sourceHash: asset.import.sourceHash,
    sourceBytes: asset.import.sourceByteCount,
    clipIds: asset.animatedMesh.clips.map(({ id }) => id),
    bounds: asset.animatedMesh.bounds,
    materialCount: asset.animatedMesh.materialSlots.length,
  };
});

const evidence = {
  schemaVersion: 1,
  authority:
    "Studio imports serialized actors; Rust admission owns durable project state; gameplay posture binding is deferred to VC8",
  project: {
    path: relative(ROOT, PROJECT),
    startingHash,
    finalHash,
    schemaVersion: finalProject.schemaVersion,
    assetCount: finalProject.assets.length,
  },
  staleImport: {
    code: stale.response.error.code,
    projectHash: startingHash,
    nonMutation: true,
  },
  imports,
  assets,
  noopReimport: {
    assetId: firstAssetId,
    kind: noop.response.plan.reimportKind,
    sourceHash: noop.response.plan.sourceHash,
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
    importedActors: imports.length,
    staleImport: stale.response.error.code,
    noopReimport: noop.response.plan.reimportKind,
  }),
);
