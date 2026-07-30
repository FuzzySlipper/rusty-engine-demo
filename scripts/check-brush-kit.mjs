import { createHash } from "node:crypto";
import { access, readFile } from "node:fs/promises";
import { resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");
const PROJECT_PATH = resolve(ROOT, "content/projects/loading-bay.project.json");
const EVIDENCE_PATH = resolve(
  ROOT,
  "docs/evidence/voxel-brush-kit-authoring.json",
);
const BROWSER_EVIDENCE_PATH = resolve(
  ROOT,
  "docs/evidence/voxel-brush-kit-studio-browser.json",
);

const expectedModules = new Set([
  "wall-conservative",
  "wall-dense",
  "corner",
  "doorway",
  "vent-panel",
  "column",
  "floor-strip",
  "ceiling-strip",
  "landmark-relay",
]);

function invariant(condition, message) {
  if (!condition) throw new Error(`brush-kit invariant failed: ${message}`);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function resolvedCellCount(voxelObject) {
  const representation = voxelObject.defaultFrame.representation;
  invariant(
    representation.kind === "sparseRuns",
    `${voxelObject.assetId} must use canonical sparse runs`,
  );
  return representation.sparseRuns.reduce(
    (total, run) => total + run.length,
    0,
  );
}

const projectBytes = await readFile(PROJECT_PATH);
const project = JSON.parse(projectBytes);
const evidence = JSON.parse(await readFile(EVIDENCE_PATH, "utf8"));
const browser = JSON.parse(await readFile(BROWSER_EVIDENCE_PATH, "utf8"));
const scene = project.scenes.find(
  ({ id }) => id === evidence.proofRoom.sceneId,
);

invariant(scene !== undefined, "proof-room scene must exist");
invariant(
  sha256(projectBytes) === evidence.finalHash,
  "evidence must name the exact canonical project bytes",
);
invariant(
  evidence.finalHash === evidence.readbackHash,
  "same-process readback hash must match publication",
);
invariant(
  evidence.reconstruction.readbackHashMatches &&
    evidence.reconstruction.freshProcessHashMatches,
  "fresh-process reconstruction must match publication",
);

const moduleNames = new Set(evidence.modules.map(({ name }) => name));
invariant(
  moduleNames.size === expectedModules.size,
  "module names must be unique",
);
for (const name of expectedModules) {
  invariant(moduleNames.has(name), `missing evidence for ${name}`);
}

const voxelAssets = project.assets.filter(({ voxelObject }) =>
  voxelObject?.assetId.startsWith("voxel-object/brush-"),
);
const instances = scene.voxelObjectInstances.filter(({ instanceId }) =>
  instanceId.startsWith("brush-proof-"),
);
invariant(
  voxelAssets.length === 9,
  "canonical project must contain nine brush definitions",
);
invariant(
  instances.length === 25,
  "proof room must contain 25 brush instances",
);
invariant(
  evidence.proofRoom.definitionCount === voxelAssets.length &&
    evidence.proofRoom.placementCount === instances.length,
  "evidence definition and placement counts must match the project",
);

const assetIds = new Set(
  voxelAssets.map(({ voxelObject }) => voxelObject.assetId),
);
for (const instance of instances) {
  invariant(
    assetIds.has(instance.voxelObjectAssetId),
    `${instance.instanceId} must reference one shared canonical brush definition`,
  );
}
invariant(
  new Set(instances.map(({ voxelObjectAssetId }) => voxelObjectAssetId))
    .size === voxelAssets.length,
  "every accepted brush definition must be reused by the proof room",
);

const owners = new Map(scene.entities.map((entity) => [entity.id, entity]));
for (const instance of instances) {
  const owner = owners.get(instance.ownerEntityId);
  invariant(owner !== undefined, `${instance.instanceId} owner must exist`);
  for (const forbidden of [
    "bounds",
    "collision",
    "kinematic",
    "trigger",
    "hazard",
    "secret",
  ]) {
    invariant(
      owner[forbidden] === undefined,
      `${instance.instanceId} decorative owner must not define ${forbidden}`,
    );
  }
}
invariant(
  instances.some(({ materialOverrides }) => materialOverrides.length > 0),
  "proof room must exercise an instance material override",
);
invariant(
  instances.some(({ rotation }) =>
    rotation.some((value, index) => value !== [0, 0, 0, 1][index]),
  ),
  "proof room must exercise rotated instances",
);
invariant(
  instances.some(({ scale }) => scale.some((value) => value !== 1)),
  "proof room must exercise normalized instance scaling",
);

let projectCells = 0;
for (const asset of voxelAssets) {
  const voxelObject = asset.voxelObject;
  const module = evidence.modules.find(
    ({ name }) => voxelObject.assetId === `voxel-object/brush-${name}`,
  );
  invariant(
    module !== undefined,
    `${voxelObject.assetId} must have metric evidence`,
  );
  invariant(
    voxelObject.grid.cellSize === module.cellSize,
    `${module.name} cell size must match the canonical definition`,
  );
  const cells = resolvedCellCount(voxelObject);
  invariant(
    cells === module.conversion.resolvedCells,
    `${module.name} resolved cells must match the canonical sparse runs`,
  );
  projectCells += cells;
  const sourcePath = resolve(ROOT, voxelObject.provenance.sourcePath);
  const sourceBytes = await readFile(sourcePath);
  invariant(
    sourceBytes.byteLength === voxelObject.provenance.sourceByteCount,
    `${module.name} source byte count must match provenance`,
  );
  invariant(
    `sha256:${sha256(sourceBytes)}` === voxelObject.provenance.sourceSha256,
    `${module.name} source hash must match provenance`,
  );
  await access(resolve(ROOT, module.source.meshJsonPath));
}
invariant(
  projectCells === evidence.aggregate.resolvedCells,
  "aggregate resolved-cell evidence must match the canonical project",
);

const conservative = evidence.modules.find(
  ({ name }) => name === "wall-conservative",
);
const dense = evidence.modules.find(({ name }) => name === "wall-dense");
invariant(
  JSON.stringify(conservative.dimensions) === JSON.stringify(dense.dimensions),
  "conservative and dense walls must have equal intended world dimensions",
);
invariant(
  JSON.stringify(conservative.conversion.placedDimensions) ===
    JSON.stringify(dense.conversion.placedDimensions),
  "conservative and dense walls must place at equal world dimensions",
);
invariant(
  dense.cellSize === conservative.cellSize / 2 &&
    dense.conversion.resolvedCells > conservative.conversion.resolvedCells &&
    dense.conversion.estimatedMeshBytes >
      conservative.conversion.estimatedMeshBytes,
  "dense wall must be a genuine equal-size higher-resolution stress treatment",
);

invariant(
  browser.projectHash === evidence.finalHash,
  "browser proof must load the exact project",
);
invariant(browser.canvasCount === 1, "Studio proof must use one shared canvas");
invariant(browser.viewport.status === "ready", "Studio renderer must be ready");
invariant(
  browser.viewport.definitions === 9,
  "Studio must reconstruct nine definitions",
);
invariant(
  browser.viewport.instances === 25,
  "Studio must reconstruct 25 instances",
);
invariant(
  browser.viewport.placementGhosts === 0,
  "no private placement candidate may remain",
);
invariant(
  browser.viewport.rendererError === "",
  "Studio renderer must report no error",
);
invariant(
  browser.viewport.selectedRenderHandle !== null,
  "Studio picking must resolve a retained brush render handle",
);
for (const screenshot of browser.capture.screenshots) {
  await access(resolve(ROOT, screenshot));
}

process.stdout.write(
  `BRUSH_KIT_OK definitions=${voxelAssets.length} instances=${instances.length} cells=${projectCells} hash=${evidence.finalHash}\n`,
);
