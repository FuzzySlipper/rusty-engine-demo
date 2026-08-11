import { createHash } from "node:crypto";
import { access, readFile } from "node:fs/promises";
import { resolve } from "node:path";

import { certifySchemaOnlyMigration } from "./project-schema-lineage.mjs";

const ROOT = resolve(import.meta.dirname, "..");
const PROJECT_PATH = resolve(ROOT, "content/projects/loading-bay.project.json");
const EVIDENCE_PATH = resolve(
  ROOT,
  "docs/evidence/voxel-level-brush-authoring.json",
);
const BROWSER_EVIDENCE_PATH = resolve(
  ROOT,
  "docs/evidence/wall-proxy-studio-browser.json",
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
const PHYSICS_EVIDENCE_PATH = resolve(
  ROOT,
  "docs/evidence/physics-projectile-consumer.json",
);
const SCHEMA_MIGRATION_EVIDENCE_PATH = resolve(
  ROOT,
  "docs/evidence/loading-bay-schema-25-migration.json",
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
const physicsEvidence = JSON.parse(
  await readFile(PHYSICS_EVIDENCE_PATH, "utf8"),
);
const schemaMigrationEvidence = JSON.parse(
  await readFile(SCHEMA_MIGRATION_EVIDENCE_PATH, "utf8"),
);
const schemaOnlyMigration = certifySchemaOnlyMigration(
  projectBytes,
  project,
  schemaMigrationEvidence,
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
        groundingEvidence.project.finalHash === sha256(projectBytes)) ||
      (physicsEvidence.project.startingHash === groundingEvidence.project.finalHash &&
        (physicsEvidence.project.finalHash === sha256(projectBytes) ||
          (physicsEvidence.project.finalHash === schemaMigrationEvidence.startingHash &&
            schemaOnlyMigration)))),
  "current project bytes must match the batch publication or its recorded actor descendant",
);
invariant(
  scene.voxelEnvironment?.gameplayProxy === true &&
    evidence.gameplayProxy === true,
  "the original collision/navigation voxels must be an explicit gameplay proxy",
);
invariant(
  brushAssets.length === 9 && evidence.definitionsReused === 8,
  "the visible level must retain nine reviewed definitions and reuse the eight needed by the route",
);
invariant(
  evidence.structuralProjectionBytes === 772_551 &&
    evidence.structuralProjectionBytes < 2 * 1024 * 1024,
  "the exact complete structural projection must remain below 2 MiB",
);
invariant(
  levelInstances.length === evidence.placementCount &&
    evidence.placementCount === 340,
  "the complete visual level must retain all 340 collision-aligned placements",
);
invariant(
  allInstances.length === 365,
  "the full scene must retain the 25 review instances plus the visual level",
);
invariant(
  evidence.proxyEdit.requiredColumnCount === 6 &&
    evidence.proxyEdit.requiredVoxelCount === 18 &&
    evidence.proxyEdit.preexistingVoxelCount === 4 &&
    evidence.proxyEdit.addedVoxelCount === 14 &&
    evidence.proxyEdit.changedThisRun === 0 &&
    evidence.proxyEdit.persistedThisRun === false,
  "the canonical proof must retain the supported 14-voxel column-proxy publication",
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
    .map((assetId) => {
      const count = levelInstances.filter(
        ({ voxelObjectAssetId }) => voxelObjectAssetId === assetId,
      ).length;
      return [assetId, count];
    })
    .filter(([, count]) => count > 0),
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
const doorwayInstances = levelInstances.filter(({ instanceId }) =>
  instanceId.startsWith("level-doorway-owner-"),
);
invariant(
  doorwayInstances.length === doorOwners.length,
  "every canonical door must have one decorative overhead surround",
);
for (const door of doorOwners) {
  const doorway = doorwayInstances.find(
    ({ instanceId }) => instanceId === `level-doorway-owner-${door.id}`,
  );
  invariant(
    doorway !== undefined &&
      doorway.voxelObjectAssetId === "voxel-object/brush-wall-conservative" &&
      doorway.translation[1] === 3,
    `door ${door.id} must retain its collision-safe overhead surround`,
  );
}

invariant(
  evidence.protocolVersion === 14 &&
    evidence.batchLimit === 32 &&
    evidence.batchCount === 11 &&
    evidence.oneRequestPerBatch === true,
  "level publication must use one bounded protocol-14 request per batch",
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
      "f1baabcd55b7075fff40b93b1bae0a16ef88ab0e",
    "browser proof must use the exact reviewed Engine provider",
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
