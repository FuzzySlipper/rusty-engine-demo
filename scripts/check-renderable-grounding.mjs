import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
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

invariant(project.schemaVersion === 24, "canonical project must use schema 24");
invariant(evidence.protocolVersion === 13, "Studio proof must use protocol 13");
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
    Math.abs(projectedLowerBound - alignment.expectedContactPlaneY) < 0.000_01,
    `${alignment.entityId} no longer meets the contact plane`,
  );
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
