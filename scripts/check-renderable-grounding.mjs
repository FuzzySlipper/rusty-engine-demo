import { createHash } from "node:crypto";
import { access, readFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const projectBytes = await readFile(
  resolve(root, "content/projects/loading-bay.project.json"),
);
const project = JSON.parse(projectBytes);
const evidence = JSON.parse(
  await readFile(resolve(root, "docs/evidence/renderable-grounding.json"), "utf8"),
);

function invariant(condition, message) {
  if (!condition) throw new Error(`renderable-grounding invariant failed: ${message}`);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function sameVector(left, right) {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function nearlyEqual(left, right, tolerance = 0.000_01) {
  return Math.abs(left - right) <= tolerance;
}

invariant(project.schemaVersion === 24, "canonical project must use schema 24");
invariant(evidence.protocolVersion === 14, "Studio proof must use protocol 14");
invariant(
  projectBytes.byteLength === evidence.project.finalBytes
    && sha256(projectBytes) === evidence.project.finalHash,
  "canonical bytes must match the recorded visual-local descendant",
);
const scene = project.scenes.find(({ id }) => id === project.entryScene);
invariant(scene !== undefined, "entry scene must exist");

for (const alignment of evidence.alignments) {
  const entity = scene.entities.find(({ id }) => id === alignment.entityId);
  invariant(entity !== undefined, `entity ${alignment.entityId} must exist`);
  invariant(entity.renderable?.asset === alignment.asset, `${alignment.entityId} asset drifted`);
  invariant(
    sameVector(entity.translation, alignment.worldTranslation)
      && sameVector(entity.scale ?? [1, 1, 1], alignment.worldScale),
    `${alignment.entityId} world transform drifted`,
  );
  invariant(
    sameVector(entity.renderable.localTransform?.translation ?? [], alignment.visualTranslation),
    `${alignment.entityId} visual-local transform drifted`,
  );
  const projectedLowerBound = entity.translation[1]
    + (entity.scale?.[1] ?? 1)
      * (entity.renderable.localTransform.translation[1] + alignment.sourceLowerBound);
  invariant(
    nearlyEqual(projectedLowerBound, alignment.expectedContactPlaneY),
    `${alignment.entityId} no longer meets the contact plane`,
  );
}

const alignedEntityIds = new Set(evidence.alignments.map(({ entityId }) => entityId));
const enemyEntityIds = scene.entities.filter(({ enemy }) => enemy === true).map(({ id }) => id);
invariant(
  enemyEntityIds.every((entityId) => alignedEntityIds.has(entityId)),
  "every enemy must carry an inspected visual-local grounding transform",
);
for (const entityId of [6, 7, 27]) {
  invariant(
    alignedEntityIds.has(entityId),
    `floor-standing prop ${entityId} must carry an inspected visual-local transform`,
  );
}

invariant(
  Object.keys(evidence.conventions).sort().join(",") ===
    "floorStanding,intentionallyHovering,suspended,wallMounted",
  "grounding conventions must keep the four explicit asset classes",
);
const assets = new Map(project.assets.map((asset) => [asset.id, asset]));
for (const inspection of evidence.inspections) {
  const entity = scene.entities.find(({ id }) => id === inspection.entityId);
  invariant(entity !== undefined, `inspection entity ${inspection.entityId} must exist`);
  invariant(
    entity.renderable?.asset === inspection.asset,
    `inspection entity ${inspection.entityId} asset drifted`,
  );
  const bounds = assets.get(inspection.asset)?.staticMesh?.payload?.bounds;
  invariant(bounds !== undefined, `${inspection.asset} must retain static-mesh bounds`);
  invariant(
    nearlyEqual(bounds.min[1], inspection.sourceLowerBound),
    `${inspection.asset} source lower bound drifted`,
  );
  const localY = entity.renderable.localTransform?.translation?.[1] ?? 0;
  const worldLower = entity.translation[1]
    + (entity.scale?.[1] ?? 1) * (localY + bounds.min[1]);
  invariant(
    nearlyEqual(worldLower, inspection.worldLowerBound),
    `${inspection.entityId} inspected world lower bound drifted`,
  );
  invariant(
    nearlyEqual(worldLower - inspection.contactPlaneY, inspection.clearance),
    `${inspection.entityId} inspected clearance drifted`,
  );
  if (inspection.classification === "floorStanding") {
    invariant(
      nearlyEqual(inspection.clearance, 0),
      `${inspection.entityId} floor-standing clearance must be zero`,
    );
  } else {
    invariant(
      inspection.clearance > 0,
      `${inspection.entityId} explicit non-grounded exception must retain clearance`,
    );
  }
}

for (const viewport of evidence.viewportEvidence) {
  invariant(
    viewport.viewport.length === 2 && viewport.viewport.every((value) => value > 0),
    `${viewport.path} viewport must be explicit`,
  );
  invariant(
    scene.entities.some(({ id }) => id === viewport.selectedEntityId),
    `${viewport.path} selected entity must exist`,
  );
  await access(resolve(root, viewport.path));
}

invariant(
  evidence.ownership.worldTransformUnchanged
    && evidence.ownership.collisionUnchanged
    && evidence.ownership.navigationUnchanged
    && evidence.ownership.gameplayUnchanged,
  "authority nonclaims must remain explicit",
);

console.log(
  `renderable grounding passed: ${String(evidence.alignments.length)} identities hash=${evidence.project.finalHash}`,
);
