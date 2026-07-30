import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");
const PROJECT_PATH = resolve(ROOT, "content/projects/loading-bay.project.json");
const MANIFEST_PATH = resolve(
  ROOT,
  "content/assets/prop-kit/source-manifest.json",
);
const EVIDENCE_PATH = resolve(ROOT, "docs/evidence/prop-kit-authoring.json");

function invariant(condition, message) {
  if (!condition) throw new Error(`prop-kit invariant failed: ${message}`);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

const projectBytes = await readFile(PROJECT_PATH);
const project = JSON.parse(projectBytes);
const manifest = JSON.parse(await readFile(MANIFEST_PATH, "utf8"));
const evidence = JSON.parse(await readFile(EVIDENCE_PATH, "utf8"));
const scene = project.scenes.find(({ id }) => id === project.entryScene);

invariant(scene !== undefined, "entry scene must exist");
invariant(manifest.assets.length === 17, "manifest must own 17 mesh assets");
invariant(
  new Set(manifest.assets.map(({ assetId }) => assetId)).size === 17,
  "manifest asset identities must be unique",
);
invariant(evidence.importCount === 17, "Studio evidence must cover 17 imports");
invariant(
  evidence.appearanceMappings.length === 26,
  "Studio evidence must cover 26 gameplay-object appearances",
);
invariant(
  evidence.landmarks.length === 2,
  "Studio evidence must cover two decorative landmarks",
);
invariant(
  sha256(projectBytes) === evidence.project.finalHash,
  "evidence must name exact canonical project bytes",
);
invariant(
  evidence.reload.passed &&
    evidence.reload.canonicalHash === evidence.project.finalHash &&
    evidence.reload.freshProcessHash === evidence.project.finalHash,
  "fresh adapter reconstruction must match canonical publication",
);
invariant(
  evidence.reimport.kind === "noop",
  "same-source Studio reimport must be a typed no-op",
);

const projectAssets = new Map(project.assets.map((asset) => [asset.id, asset]));
for (const license of manifest.licenses) {
  const bytes = await readFile(resolve(ROOT, license.path));
  invariant(
    sha256(bytes) === license.sha256,
    `${license.path} license hash must match provenance`,
  );
}
for (const described of manifest.assets) {
  const importBytes = await readFile(resolve(ROOT, described.importSourcePath));
  invariant(
    sha256(importBytes) === described.contentSha256,
    `${described.assetId} import derivative hash must match`,
  );
  const sourceBytes = await readFile(resolve(ROOT, described.sourcePath));
  invariant(
    sha256(sourceBytes) === described.sourceSha256,
    `${described.assetId} source hash must match`,
  );
  const stored = projectAssets.get(described.assetId);
  invariant(
    stored?.staticMesh !== undefined,
    `${described.assetId} is missing`,
  );
  invariant(
    stored.staticMesh.asset === described.assetId,
    `${described.assetId} retained identity must match`,
  );
  invariant(
    stored.staticMesh.collision.kind === "visualOnly",
    `${described.assetId} must not create gameplay collision`,
  );
  invariant(
    stored.import?.source.path === described.importSourcePath &&
      stored.import.sourceHash === described.contentSha256 &&
      stored.import.sourceByteCount === importBytes.byteLength,
    `${described.assetId} Studio import provenance must match source bytes`,
  );
  invariant(
    stored.staticMesh.payload.layout.vertexCount === described.vertexCount &&
      stored.staticMesh.payload.layout.indexCount ===
        described.triangleCount * 3,
    `${described.assetId} mesh metrics must match the source manifest`,
  );
  invariant(
    JSON.stringify(stored.staticMesh.payload.bounds) ===
      JSON.stringify(described.bounds),
    `${described.assetId} admitted bounds must match the derivative`,
  );
}

const entities = new Map(scene.entities.map((entity) => [entity.id, entity]));
for (const mapping of evidence.appearanceMappings) {
  const entity = entities.get(mapping.entityId);
  invariant(
    entity !== undefined,
    `mapped entity ${mapping.entityId} must exist`,
  );
  invariant(
    entity.renderable?.asset === mapping.assetId,
    `mapped entity ${mapping.entityId} must use ${mapping.assetId}`,
  );
}
for (const landmark of evidence.landmarks) {
  const entity = entities.get(landmark.entityId);
  invariant(
    entity?.renderable?.asset === landmark.asset,
    `landmark ${landmark.entityId} must retain its serialized asset`,
  );
  for (const forbidden of [
    "bounds",
    "collision",
    "kinematic",
    "trigger",
    "hazard",
    "pickup",
  ]) {
    invariant(
      entity[forbidden] === undefined,
      `visual landmark ${landmark.entityId} must not define ${forbidden}`,
    );
  }
}

const expectedViewmodels = [
  "mesh/prop-kit/arc-pistol",
  "mesh/prop-kit/breach-scattergun",
  "mesh/prop-kit/rivet-carbine",
  "mesh/prop-kit/muzzle-flash",
];
for (const assetId of expectedViewmodels) {
  invariant(projectAssets.has(assetId), `missing viewmodel asset ${assetId}`);
}

const gameplayProps = [...entities.values()].filter(
  (entity) =>
    entity.door !== undefined ||
    entity.switch !== undefined ||
    entity.pickup !== undefined ||
    entity.hazard !== undefined ||
    entity.extractionBeacon !== undefined ||
    entity.levelExit !== undefined,
);
for (const entity of gameplayProps) {
  invariant(
    entity.renderable?.asset.startsWith("mesh/prop-kit/"),
    `gameplay prop ${entity.id} still uses a placeholder appearance`,
  );
}

console.log(
  JSON.stringify({
    projectHash: evidence.project.finalHash,
    assets: manifest.assets.length,
    mappedGameplayProps: evidence.appearanceMappings.length,
    landmarks: evidence.landmarks.length,
    vertices: manifest.assets.reduce(
      (total, asset) => total + asset.vertexCount,
      0,
    ),
    triangles: manifest.assets.reduce(
      (total, asset) => total + asset.triangleCount,
      0,
    ),
  }),
);
