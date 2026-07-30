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
const PROP_EVIDENCE_PATH = resolve(
  ROOT,
  "docs/evidence/prop-kit-authoring.json",
);
const LEVEL_EVIDENCE_PATH = resolve(
  ROOT,
  "docs/evidence/voxel-level-brush-authoring.json",
);
const ACTOR_EVIDENCE_PATH = resolve(
  ROOT,
  "docs/evidence/actor-kit-authoring.json",
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
const propEvidence = JSON.parse(await readFile(PROP_EVIDENCE_PATH, "utf8"));
const levelEvidence = JSON.parse(await readFile(LEVEL_EVIDENCE_PATH, "utf8"));
const actorEvidence = JSON.parse(await readFile(ACTOR_EVIDENCE_PATH, "utf8"));
const scene = project.scenes.find(
  ({ id }) => id === evidence.proofRoom.sceneId,
);
const currentProjectHash = sha256(projectBytes);

invariant(scene !== undefined, "proof-room scene must exist");
invariant(
  currentProjectHash === evidence.finalHash ||
    (propEvidence.project.startingHash === evidence.finalHash &&
      propEvidence.project.finalHash === currentProjectHash) ||
    currentProjectHash === levelEvidence.projectHashAfter ||
    (actorEvidence.project.startingHash === levelEvidence.projectHashAfter &&
      actorEvidence.project.finalHash === currentProjectHash),
  "current project must retain the exact brush proof in a recorded descendant",
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
invariant(
  browser.capture.rendererSubmission.engineRevision ===
    "70808ba1b74b908c47edfbf3b1282fb2eb5f192d",
  "browser proof must use the exact reviewed Studio submission provider",
);
invariant(
  browser.capture.rendererSubmission.event ===
    "rusty_studio_viewport_frame_submitted.v1" &&
    browser.capture.rendererSubmission.outlet ===
      "StudioShellComponent.frameSubmitted",
  "browser proof must consume the public shell-level submission event",
);

const completeSubmission = browser.submissions.initial.latest;
const richSubmission = browser.submissions.denseSelection.latest;
invariant(
  completeSubmission.kind === "rusty_studio_viewport_frame_submitted.v1" &&
    completeSubmission.updateKind === "complete",
  "initial browser reconstruction must emit one complete submission",
);
invariant(
  richSubmission.kind === "rusty_studio_viewport_frame_submitted.v1" &&
    richSubmission.updateKind === "presentation",
  "real brush selection must emit one presentation submission",
);
for (const { label, event } of [
  { label: "complete", event: completeSubmission },
  { label: "rich presentation", event: richSubmission },
]) {
  const { submission } = event;
  invariant(
    submission.source === "explicit" &&
      submission.backendSubmissionDurationStatus === "available" &&
      submission.backendSubmissionDurationMs > 0,
    `${label} must be the explicit shared-surface submission with backend timing`,
  );
  for (const [name, counter] of Object.entries(submission.statistics).filter(
    ([name]) => name !== "schemaVersion",
  )) {
    invariant(
      counter.status === "available",
      `${label} ${name} must be renderer-owned and available`,
    );
  }
}

const completeStatistics = completeSubmission.submission.statistics;
const richStatistics = richSubmission.submission.statistics;
invariant(
  completeStatistics.drawCallCount.scope === "perSubmission" &&
    completeStatistics.drawCallCount.value === 9 &&
    completeStatistics.triangleCount.scope === "perSubmission" &&
    completeStatistics.triangleCount.value === 96,
  "initial default-camera complete submission must retain its exact draw and triangle counts",
);
invariant(
  richStatistics.drawCallCount.scope === "perSubmission" &&
    richStatistics.drawCallCount.value === 28 &&
    richStatistics.triangleCount.scope === "perSubmission" &&
    richStatistics.triangleCount.value === 310_852,
  "focused content-rich submission must retain its exact draw and triangle counts",
);
for (const [name, expected] of [
  ["renderHandleCount", 62],
  ["geometryResourceCount", 32],
  ["materialResourceCount", 33],
  ["textureResourceCount", 0],
  ["animatedInstanceCount", 0],
]) {
  const counter = richStatistics[name];
  invariant(
    counter.scope === "liveResident" && counter.value === expected,
    `rich presentation ${name} must match the exact live-resident sample`,
  );
}
invariant(
  browser.submissions.conservativeSelection.latest.submission.statistics
    .geometryResourceCount.value === 32,
  "selecting a second repeated brush must not allocate another geometry resource",
);

for (const resize of browser.lifecycle.resize) {
  invariant(
    resize.canvasCount === 1 &&
      resize.rendererStatus === "ready" &&
      resize.rendererError === "",
    `${resize.viewport.join("x")} resize must preserve one healthy shared canvas`,
  );
}
invariant(
  browser.lifecycle.disposed.canvasCount === 0 &&
    browser.lifecycle.remounted.canvasCount === 1 &&
    browser.lifecycle.reloaded.canvasCount === 1,
  "close, remount, and reload must prove shared-canvas disposal and reconstruction",
);
for (const lifecycle of [
  browser.lifecycle.remounted,
  browser.lifecycle.reloaded,
]) {
  invariant(
    lifecycle.projectHash === evidence.finalHash &&
      lifecycle.rendererStatus === "ready" &&
      lifecycle.rendererError === "",
    "remount and reload must reconstruct the exact project on a healthy renderer",
  );
}
invariant(
  browser.submissions.remounted.latest.updateKind === "complete" &&
    browser.submissions.reloaded.latest.updateKind === "complete" &&
    browser.submissions.reloadedSelection.latest.updateKind === "presentation",
  "remount and fresh reload must re-emit complete then presentation samples",
);
for (const screenshot of browser.capture.screenshots) {
  await access(resolve(ROOT, screenshot));
}

process.stdout.write(
  `BRUSH_KIT_OK definitions=${voxelAssets.length} instances=${instances.length} cells=${projectCells} hash=${evidence.finalHash}\n`,
);
