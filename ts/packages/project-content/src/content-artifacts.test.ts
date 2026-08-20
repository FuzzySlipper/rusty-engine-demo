import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  CANONICAL_PROJECT_FILES,
  readCanonicalProject,
} from "./content-artifacts.js";

test("Doom E1M1 is the sole canonical authored project", () => {
  assert.deepEqual(CANONICAL_PROJECT_FILES, ["doom-e1m1.project.json"]);
});

test("canonical project reads decode without rewriting authored bytes", () => {
  const root = mkdtempSync(join(tmpdir(), "doom-e1m1-canonical-"));
  try {
    const project = canonicalProject();
    const path = join(root, "doom-e1m1.project.json");
    const canonical = `${JSON.stringify(project, null, 2)}\n`;
    writeFileSync(path, canonical);

    assert.deepEqual(
      readCanonicalProject(root, "doom-e1m1.project.json"),
      project,
    );
    assert.equal(readFileSync(path, "utf8"), canonical);

    const compact = JSON.stringify(project);
    writeFileSync(path, compact);
    assert.deepEqual(
      readCanonicalProject(root, "doom-e1m1.project.json"),
      project,
    );
    assert.equal(readFileSync(path, "utf8"), compact);

    writeFileSync(path, "{");
    assert.throws(
      () => readCanonicalProject(root, "doom-e1m1.project.json"),
      SyntaxError,
    );
    assert.equal(readFileSync(path, "utf8"), "{");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

function canonicalProject() {
  return {
    schemaVersion: 26,
    projectId: "doom-e1m1",
    name: "Doom E1M1",
    entryScene: "scene/doom-e1m1",
    assets: [],
    itemDefinitions: [],
    gameplayPrograms: [],
    pickupPrograms: [],
    playerSetupPrograms: [],
    enemyAttackPrograms: [],
    enemyDefeatPrograms: [],
    hazardPrograms: [],
    explosivePropPrograms: [],
    encounterPrograms: [],
    switchPrograms: [],
    floorActionPrograms: [],
    liftPrograms: [],
    secretPrograms: [],
    levelExitPrograms: [],
    weaponEntities: [],
    scenes: [
      {
        id: "scene/doom-e1m1",
        name: "E1M1",
        entities: [],
      },
    ],
  };
}
