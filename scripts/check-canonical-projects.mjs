import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const canonicalProject = {
  path: "content/projects/doom-e1m1.project.json",
  sha256: "08d069726cdeaf1fddf1181eb3e75d63bad11e5262a4a5b46b4cf9a3bf5ae31b",
  schemaVersion: 28,
  assets: 155,
  entities: 152,
  semanticArrays: {
    itemDefinitions: 11,
    gameplayPrograms: 4,
    pickupPrograms: 4,
    playerSetupPrograms: 2,
    enemyAttackPrograms: 2,
    enemyDefeatPrograms: 2,
    hazardPrograms: 1,
    explosivePropPrograms: 1,
    encounterPrograms: 1,
    switchPrograms: 1,
    floorActionPrograms: 1,
    liftPrograms: 1,
    secretPrograms: 1,
    levelExitPrograms: 1,
  },
};

const path = resolve(repoRoot, canonicalProject.path);
const source = readFileSync(path);
const sha256 = createHash("sha256").update(source).digest("hex");
if (sha256 !== canonicalProject.sha256) {
  throw new Error(
    `${canonicalProject.path} is not the retained E1M1 content closure: expected sha256=${canonicalProject.sha256}, got sha256=${sha256}`,
  );
}

const project = JSON.parse(source.toString("utf8"));
if (
  project.schemaVersion !== canonicalProject.schemaVersion ||
  project.projectId !== "doom-e1m1" ||
  !Array.isArray(project.assets) ||
  project.assets.length !== canonicalProject.assets ||
  !Array.isArray(project.scenes) ||
  project.scenes.length !== 1 ||
  !Array.isArray(project.scenes[0]?.entities) ||
  project.scenes[0].entities.length !== canonicalProject.entities
) {
  throw new Error(`${canonicalProject.path} no longer has the retained E1M1 structure`);
}

for (const [field, expectedLength] of Object.entries(
  canonicalProject.semanticArrays,
)) {
  if (!Array.isArray(project[field]) || project[field].length !== expectedLength) {
    throw new Error(
      `${canonicalProject.path} lost retained E1M1 semantics at ${field}; expected ${expectedLength} entries`,
    );
  }
}

console.log(
  `retained E1M1 content passed: ${canonicalProject.path} sha256=${sha256} assets=${project.assets.length} entities=${project.scenes[0].entities.length}`,
);
