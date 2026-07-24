import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  ENCOUNTER_IDS,
  encounterGateProject,
  loadingBayStoredProject,
} from "./encounter-project.js";

test("encounter membership and exit relationships are explicit authored content", () => {
  const project = encounterGateProject(["alpha", "beta"]);
  const encounter = project.entities.find((entity) => entity.id === ENCOUNTER_IDS.encounter);
  assert.deepEqual(encounter?.encounter, {
    members: [ENCOUNTER_IDS.firstEnemy, ENCOUNTER_IDS.firstEnemy + 1],
    exit: ENCOUNTER_IDS.exit,
  });
});

test("enemy count is a content-only variation", () => {
  const project = encounterGateProject(["only-enemy"]);
  assert.equal(project.entities.filter((entity) => entity.enemy === true).length, 1);
  assert.deepEqual(
    project.entities.find((entity) => entity.id === ENCOUNTER_IDS.encounter)?.encounter?.members,
    [ENCOUNTER_IDS.firstEnemy],
  );
});

test("loading bay composes a kinematic probe over one generated voxel environment", () => {
  const project = encounterGateProject(["only-enemy"]);
  const probe = project.entities.find((entity) => entity.id === ENCOUNTER_IDS.motionProbe);

  assert.deepEqual(probe?.kinematic, {
    halfExtents: [0.25, 0.25, 0.25],
    velocity: [5, 0, 0],
  });
  assert.deepEqual(project.generatedVoxelEnvironment, {
    seed: 4,
    voxelSize: 1,
    chunkSize: 16,
    width: 7,
    height: 4,
    length: 10,
  });
  assert.equal(project.voxelCollision, undefined);
});

test("player controller and physical bindings are explicit content", () => {
  const project = encounterGateProject(["guard"]);
  const player = project.entities.find((entity) => entity.id === ENCOUNTER_IDS.actor);

  assert.deepEqual(player?.playerController, {
    moveSpeedUnitsPerSecond: 4,
    moveStepSeconds: 0.1,
    lookDegreesPerUnit: 12,
    initialYawDegrees: 0,
    initialPitchDegrees: -10,
    bindings: {
      moveForward: "KeyW",
      moveBackward: "KeyS",
      moveLeft: "KeyA",
      moveRight: "KeyD",
      mouseLook: "pointer",
      primaryFire: "Mouse0",
    },
  });
  assert.deepEqual(player?.kinematic?.velocity, [0, 0, 0]);
});

test("keyboard bindings vary as content without changing controller behavior", () => {
  const bindings = {
    moveForward: "ArrowUp",
    moveBackward: "ArrowDown",
    moveLeft: "ArrowLeft",
    moveRight: "ArrowRight",
    mouseLook: "pointer",
    primaryFire: "Space",
  } as const;
  const project = encounterGateProject(["guard"], { playerBindings: bindings });
  const player = project.entities.find((entity) => entity.id === ENCOUNTER_IDS.actor);

  assert.deepEqual(player?.playerController?.bindings, bindings);
  assert.equal(player?.playerController?.moveSpeedUnitsPerSecond, 4);
  assert.equal(player?.playerController?.lookDegreesPerUnit, 12);
});

test("health and weapon configuration stay on their responsible entities", () => {
  const project = encounterGateProject(["guard"], {
    enemyHealth: 140,
    weaponDamage: 35,
    weaponCooldownTicks: 3,
  });
  const player = project.entities.find((entity) => entity.id === ENCOUNTER_IDS.actor);
  const enemy = project.entities.find((entity) => entity.id === ENCOUNTER_IDS.firstEnemy);

  assert.deepEqual(enemy?.health, {
    max: 140,
    hitboxHalfExtents: [0.55, 0.9, 0.55],
  });
  assert.deepEqual(player?.weapon, {
    damage: 35,
    maxDistance: 20,
    cooldownTicks: 3,
    ammoCapacity: 8,
    muzzleOffset: [0, 0, 0],
  });
});

test("autonomous navigation is explicit data on the responsible enemy", () => {
  const project = encounterGateProject(["pathfinder", "guard"]);
  const navigator = project.entities.find((entity) => entity.id === ENCOUNTER_IDS.firstEnemy);

  assert.deepEqual(navigator?.navigation, {
    goal: [7.5, 1.5, 6.5],
    speedUnitsPerSecond: 4,
    maxVisited: 512,
  });
  assert.deepEqual(navigator?.kinematic?.velocity, [0, 0, 0]);
});

test("generation seed is a content-only environment variation", () => {
  const first = encounterGateProject(["guard"], { generationSeed: 4 });
  const second = encounterGateProject(["guard"], { generationSeed: 9 });

  assert.equal(first.generatedVoxelEnvironment?.seed, 4);
  assert.equal(second.generatedVoxelEnvironment?.seed, 9);
  assert.deepEqual(first.entities, second.entities);
});

test("navigation target and speed are content-only variations", () => {
  const project = encounterGateProject(["pathfinder"], {
    navigationGoal: [1.5, 0.5, 7.5],
    navigationSpeedUnitsPerSecond: 2,
  });
  const navigator = project.entities.find((entity) => entity.id === ENCOUNTER_IDS.firstEnemy);

  assert.deepEqual(navigator?.navigation, {
    goal: [1.5, 0.5, 7.5],
    speedUnitsPerSecond: 2,
    maxVisited: 512,
  });
});

test("optional TypeScript authoring materializes the checked-in stored-project candidate", () => {
  const artifact = JSON.parse(
    readFileSync(
      new URL("../../../../content/projects/loading-bay.project.json", import.meta.url),
      "utf8",
    ),
  );

  assert.deepEqual(loadingBayStoredProject(), artifact);
});

test("stored-project seed remains a candidate-only variation", () => {
  const first = loadingBayStoredProject({ generationSeed: 4 });
  const second = loadingBayStoredProject({ generationSeed: 9 });

  assert.equal(first.scenes[0]?.voxelEnvironment?.kind, "generatedRoom");
  assert.equal(second.scenes[0]?.voxelEnvironment?.kind, "generatedRoom");
  assert.notDeepEqual(first.scenes[0]?.voxelEnvironment, second.scenes[0]?.voxelEnvironment);
  assert.deepEqual(first.scenes[0]?.entities, second.scenes[0]?.entities);
});
