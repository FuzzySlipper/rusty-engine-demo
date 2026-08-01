import { createHash } from "node:crypto";
import { access, readFile } from "node:fs/promises";
import { resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");
const PROJECT_PATH = resolve(ROOT, "content/projects/loading-bay.project.json");
const EVIDENCE_PATH = resolve(
  ROOT,
  "docs/evidence/voxel-level-brush-authoring.json",
);
const BROWSER_EVIDENCE_PATH = resolve(
  ROOT,
  "docs/evidence/voxel-level-brush-studio-browser.json",
);
const ACTOR_EVIDENCE_PATH = resolve(
  ROOT,
  "docs/evidence/actor-kit-authoring.json",
);
const VISUAL_BINDING_EVIDENCE_PATH = resolve(
  ROOT,
  "docs/evidence/visual-bindings.json",
);
const GROUNDING_EVIDENCE_PATH = resolve(
  ROOT,
  "docs/evidence/renderable-grounding.json",
);

function invariant(condition, message) {
  if (!condition) {
    throw new Error(`brush-level invariant failed: ${message}`);
  }
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

const projectBytes = await readFile(PROJECT_PATH);
const project = JSON.parse(projectBytes);
const evidence = JSON.parse(await readFile(EVIDENCE_PATH, "utf8"));
const actorEvidence = JSON.parse(await readFile(ACTOR_EVIDENCE_PATH, "utf8"));
const visualBindingEvidence = JSON.parse(
  await readFile(VISUAL_BINDING_EVIDENCE_PATH, "utf8"),
);
const groundingEvidence = JSON.parse(
  await readFile(GROUNDING_EVIDENCE_PATH, "utf8"),
);
const scene = project.scenes.find(({ id }) => id === "scene/loading-bay");
const brushAssets = project.assets.filter(({ voxelObject }) =>
  voxelObject?.assetId.startsWith("voxel-object/brush-"),
);
const allInstances = scene?.voxelObjectInstances ?? [];
const levelInstances = allInstances.filter(({ instanceId }) =>
  instanceId.startsWith("level-"),
);

invariant(scene !== undefined, "Loading Bay scene must exist");
invariant(
  evidence.projectSha256 === evidence.projectHashAfter &&
    (sha256(projectBytes) === evidence.projectHashAfter ||
      (actorEvidence.project.startingHash === evidence.projectHashAfter &&
        actorEvidence.project.finalHash === sha256(projectBytes)) ||
      (visualBindingEvidence.project.startingHash ===
        actorEvidence.project.finalHash &&
        visualBindingEvidence.project.finalHash === sha256(projectBytes)) ||
      (groundingEvidence.project.startingHash ===
        visualBindingEvidence.project.finalHash &&
        groundingEvidence.project.finalHash === sha256(projectBytes))),
  "current project bytes must match the batch publication or its recorded actor descendant",
);
invariant(
  scene.voxelEnvironment?.gameplayProxy === true &&
    evidence.gameplayProxy === true,
  "the original collision/navigation voxels must be an explicit gameplay proxy",
);
invariant(
  brushAssets.length === 9 && evidence.definitionsReused === 9,
  "the visible level must reuse the nine reviewed canonical brush definitions",
);
invariant(
  evidence.structuralProjectionBytes === 739_471 &&
    evidence.structuralProjectionBytes < 2 * 1024 * 1024,
  "the exact complete structural projection must remain below 2 MiB",
);
invariant(
  levelInstances.length === evidence.placementCount &&
    evidence.placementCount === 342,
  "the complete visual level must retain all 342 batch-authored placements",
);
invariant(
  allInstances.length === 367,
  "the full scene must retain the 25 review instances plus the visual level",
);

const levelIds = new Set(levelInstances.map(({ instanceId }) => instanceId));
const ownerIds = new Set(
  levelInstances.map(({ ownerEntityId }) => ownerEntityId),
);
invariant(
  levelIds.size === levelInstances.length,
  "level instance identities must be unique",
);
invariant(
  ownerIds.size === levelInstances.length,
  "level owner identities must be unique",
);

const entities = new Map(scene.entities.map((entity) => [entity.id, entity]));
const assetIds = new Set(
  brushAssets.map(({ voxelObject }) => voxelObject.assetId),
);
for (const instance of levelInstances) {
  invariant(
    assetIds.has(instance.voxelObjectAssetId),
    `${instance.instanceId} must reference a reviewed shared definition`,
  );
  const owner = entities.get(instance.ownerEntityId);
  invariant(owner !== undefined, `${instance.instanceId} owner must exist`);
  for (const forbidden of [
    "bounds",
    "collision",
    "kinematic",
    "trigger",
    "door",
    "switch",
    "hazard",
    "secret",
    "pickup",
    "enemy",
  ]) {
    invariant(
      owner[forbidden] === undefined,
      `${instance.instanceId} decorative owner must not define ${forbidden}`,
    );
  }
}

const placementsByAsset = Object.fromEntries(
  [...assetIds]
    .sort()
    .map((assetId) => [
      assetId,
      levelInstances.filter(
        ({ voxelObjectAssetId }) => voxelObjectAssetId === assetId,
      ).length,
    ]),
);
invariant(
  JSON.stringify(placementsByAsset) ===
    JSON.stringify(evidence.placementsByAsset),
  "per-definition repeat counts must match the authored project",
);
for (const required of [
  "level-floor-",
  "level-ceiling-",
  "level-wall-",
  "level-doorway-",
  "level-corner-",
  "level-column-",
  "level-relay-",
]) {
  invariant(
    levelInstances.some(({ instanceId }) => instanceId.startsWith(required)),
    `visible level must contain ${required} placements`,
  );
}

const doorOwners = scene.entities.filter(({ door }) => door !== undefined);
const doorwayInstances = levelInstances.filter(
  ({ voxelObjectAssetId }) =>
    voxelObjectAssetId === "voxel-object/brush-doorway",
);
invariant(
  doorwayInstances.length === doorOwners.length,
  "every canonical door must have one decorative doorway brush",
);
for (const door of doorOwners) {
  invariant(
    doorwayInstances.some(
      ({ instanceId }) => instanceId === `level-doorway-owner-${door.id}`,
    ),
    `door ${door.id} must retain its aligned decorative doorway`,
  );
}

invariant(
  evidence.protocolVersion === 13 &&
    evidence.batchLimit === 32 &&
    evidence.batchCount === 11 &&
    evidence.oneRequestPerBatch === true,
  "level publication must use one bounded protocol-12 request per batch",
);
invariant(
  evidence.batches.reduce((total, batch) => total + batch.placements, 0) ===
    levelInstances.length,
  "batch receipt counts must cover the complete visual level",
);
let expectedOffset = 0;
let priorOwner = 0;
for (const batch of evidence.batches) {
  invariant(
    batch.offset === expectedOffset &&
      batch.placements > 0 &&
      batch.placements <= evidence.batchLimit &&
      batch.ownerEntityIds.length === batch.placements,
    `batch at offset ${batch.offset} must be bounded and contiguous`,
  );
  for (const ownerEntityId of batch.ownerEntityIds) {
    invariant(
      ownerEntityId > priorOwner && ownerIds.has(ownerEntityId),
      `owner ${ownerEntityId} must be deterministic, ordered, and published`,
    );
    priorOwner = ownerEntityId;
  }
  expectedOffset += batch.placements;
}
invariant(
  evidence.canonicalRereadMatched && evidence.freshAdapterMatched,
  "canonical reread and fresh adapter reconstruction must match publication",
);

let browser;
try {
  browser = JSON.parse(await readFile(BROWSER_EVIDENCE_PATH, "utf8"));
} catch (error) {
  if (error?.code !== "ENOENT") throw error;
}
if (browser !== undefined) {
  invariant(
    browser.projectHash === evidence.projectHashAfter,
    "browser proof must load the exact batch-authored project",
  );
  invariant(
    browser.canvasCount === 1 &&
      browser.viewport.status === "ready" &&
      browser.viewport.rendererError === "",
    "Studio must retain one healthy shared renderer surface",
  );
  invariant(
    browser.viewport.definitions === 9 &&
      browser.viewport.instances === allInstances.length &&
      browser.viewport.placementGhosts === 0,
    "Studio must reconstruct the complete durable brush scene with no candidate",
  );
  invariant(
    browser.viewport.selectedRenderHandle !== null,
    "repeated brush picking must resolve a retained render handle",
  );
  invariant(
    browser.capture.rendererSubmission.engineRevision ===
      "5a42db2feac72788b25eedf8d5efbc0fb2ec2afd",
    "browser proof must use the reviewed projection-staging provider",
  );
  const first = browser.submissions.denseSelection.latest.submission.statistics;
  const second =
    browser.submissions.conservativeSelection.latest.submission.statistics;
  invariant(
    first.geometryResourceCount.value === second.geometryResourceCount.value &&
      first.materialResourceCount.value === second.materialResourceCount.value,
    "selecting a second repeated definition must not allocate new resources",
  );
  invariant(
    browser.lifecycle.disposed.canvasCount === 0 &&
      browser.lifecycle.remounted.canvasCount === 1 &&
      browser.lifecycle.reloaded.canvasCount === 1,
    "close, remount, and reload must prove shared-surface lifecycle ownership",
  );
  for (const screenshot of browser.capture.screenshots) {
    await access(resolve(ROOT, screenshot));
  }
}

process.stdout.write(
  `BRUSH_LEVEL_OK definitions=${brushAssets.length} levelInstances=${levelInstances.length} totalInstances=${allInstances.length} batches=${evidence.batchCount} hash=${evidence.projectHashAfter}\n`,
);
