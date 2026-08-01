import assert from "node:assert/strict";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  readCanonicalProject,
  synchronizeGeneratedProjects,
} from "./content-artifacts.js";

test("fixture generation cannot overwrite canonical project artifacts", () => {
  const root = mkdtempSync(join(tmpdir(), "loading-bay-content-"));
  try {
    const generated = join(root, "generated");
    const projects = join(root, "projects");
    mkdirSync(projects);
    const canonical = `${JSON.stringify(canonicalProject(), null, 2)}\n`;
    const canonicalPath = join(projects, "loading-bay.project.json");
    writeFileSync(canonicalPath, canonical);

    synchronizeGeneratedProjects(
      generated,
      { "fixture.project.json": { schemaVersion: 6, entities: [] } },
      "write",
    );

    assert.equal(readFileSync(canonicalPath, "utf8"), canonical);
    assert.equal(
      readFileSync(join(generated, "fixture.project.json"), "utf8"),
      '{\n  "schemaVersion": 6,\n  "entities": []\n}\n',
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("canonical project reads decode without rewriting Studio-owned bytes", () => {
  const root = mkdtempSync(join(tmpdir(), "loading-bay-canonical-"));
  try {
    const project = canonicalProject();
    const path = join(root, "loading-bay.project.json");
    const canonical = `${JSON.stringify(project, null, 2)}\n`;
    writeFileSync(path, canonical);

    assert.deepEqual(
      readCanonicalProject(root, "loading-bay.project.json"),
      project,
    );
    assert.equal(readFileSync(path, "utf8"), canonical);

    const compact = JSON.stringify(project);
    writeFileSync(path, compact);
    assert.deepEqual(
      readCanonicalProject(root, "loading-bay.project.json"),
      project,
    );
    assert.equal(readFileSync(path, "utf8"), compact);

    writeFileSync(path, "{");
    assert.throws(
      () => readCanonicalProject(root, "loading-bay.project.json"),
      SyntaxError,
    );
    assert.equal(readFileSync(path, "utf8"), "{");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

function canonicalProject() {
  return {
    schemaVersion: 24,
    projectId: "loading-bay",
    name: "Loading Bay",
    entryScene: "scene/loading-bay",
    assets: [],
    itemDefinitions: [],
    weaponEntities: [],
    scenes: [
      {
        id: "scene/loading-bay",
        name: "Loading Bay",
        entities: [],
      },
    ],
  };
}
