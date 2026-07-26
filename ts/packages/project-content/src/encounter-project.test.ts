import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  ENCOUNTER_IDS,
  LOADING_BAY_ITEM_IDS,
  encounterGateProject,
  loadingBayStoredProject,
  relayAnnexStoredProject,
} from "./encounter-project.js";

test("encounter membership and exit relationships are explicit authored content", () => {
  const project = encounterGateProject(["alpha", "beta"]);
  const encounter = project.entities.find(
    (entity) => entity.id === ENCOUNTER_IDS.encounter,
  );
  assert.deepEqual(encounter?.encounter, {
    members: [ENCOUNTER_IDS.firstEnemy, ENCOUNTER_IDS.firstEnemy + 1],
    exit: ENCOUNTER_IDS.exit,
  });
});

test("enemy count is a content-only variation", () => {
  const project = encounterGateProject(["only-enemy"]);
  assert.equal(
    project.entities.filter((entity) => entity.enemy === true).length,
    1,
  );
  assert.deepEqual(
    project.entities.find((entity) => entity.id === ENCOUNTER_IDS.encounter)
      ?.encounter?.members,
    [ENCOUNTER_IDS.firstEnemy],
  );
});

test("loading bay composes a kinematic probe over one generated voxel environment", () => {
  const project = encounterGateProject(["only-enemy"]);
  const probe = project.entities.find(
    (entity) => entity.id === ENCOUNTER_IDS.motionProbe,
  );

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
  const player = project.entities.find(
    (entity) => entity.id === ENCOUNTER_IDS.actor,
  );

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
  const player = project.entities.find(
    (entity) => entity.id === ENCOUNTER_IDS.actor,
  );

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
  const player = project.entities.find(
    (entity) => entity.id === ENCOUNTER_IDS.actor,
  );
  const enemy = project.entities.find(
    (entity) => entity.id === ENCOUNTER_IDS.firstEnemy,
  );

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
  const navigator = project.entities.find(
    (entity) => entity.id === ENCOUNTER_IDS.firstEnemy,
  );

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
  const navigator = project.entities.find(
    (entity) => entity.id === ENCOUNTER_IDS.firstEnemy,
  );

  assert.deepEqual(navigator?.navigation, {
    goal: [1.5, 0.5, 7.5],
    speedUnitsPerSecond: 2,
    maxVisited: 512,
  });
});

test("optional TypeScript authoring materializes the checked-in stored-project candidate", () => {
  const artifact = JSON.parse(
    readFileSync(
      new URL(
        "../../../../content/projects/loading-bay.project.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );

  assert.deepEqual(loadingBayStoredProject(), artifact);
});

test("stored item definitions and starting inventory remain immutable authored data", () => {
  const project = loadingBayStoredProject();
  const player = project.scenes[0]?.entities.find(
    (entity) => entity.id === ENCOUNTER_IDS.actor,
  );

  assert.deepEqual(player?.inventory, {
    capacitySlots: 8,
    startingStacks: [
      { item: LOADING_BAY_ITEM_IDS.arcPistol, quantity: 1 },
      { item: LOADING_BAY_ITEM_IDS.energyCell, quantity: 40 },
      { item: LOADING_BAY_ITEM_IDS.medPatch, quantity: 1 },
    ],
    initiallyEquippedWeapon: LOADING_BAY_ITEM_IDS.arcPistol,
  });
  assert.deepEqual(
    project.itemDefinitions.find(
      (definition) => definition.id === LOADING_BAY_ITEM_IDS.arcPistol,
    ),
    {
      id: LOADING_BAY_ITEM_IDS.arcPistol,
      maxQuantity: 1,
      kind: { kind: "weapon", ammunition: LOADING_BAY_ITEM_IDS.energyCell },
    },
  );
  assert.equal(
    project.itemDefinitions.some(
      (definition) => definition.id === LOADING_BAY_ITEM_IDS.inertInspectionTag,
    ),
    true,
  );
  assert.equal(
    player?.inventory?.startingStacks.some(
      (stack) => (stack.item as string) === LOADING_BAY_ITEM_IDS.inertInspectionTag,
    ),
    false,
  );
});

test("the extraction beacon is game-owned data on its responsible entity", () => {
  const project = loadingBayStoredProject({ beaconActivationRadius: 3 });
  const beacon = project.scenes[0]?.entities.find(
    (entity) => entity.id === ENCOUNTER_IDS.extractionBeacon,
  );

  assert.deepEqual(beacon, {
    id: ENCOUNTER_IDS.extractionBeacon,
    name: "extraction-beacon",
    translation: [4.5, 1.5, 12.5],
    renderable: { asset: "mesh/extraction-beacon", visible: true },
    extractionBeacon: { activationRadius: 3 },
  });
});

test("stored-project seed remains a candidate-only variation", () => {
  const first = loadingBayStoredProject({ generationSeed: 4 });
  const second = loadingBayStoredProject({ generationSeed: 9 });

  assert.equal(first.scenes[0]?.voxelEnvironment?.kind, "generatedRoom");
  assert.equal(second.scenes[0]?.voxelEnvironment?.kind, "generatedRoom");
  assert.notDeepEqual(
    first.scenes[0]?.voxelEnvironment,
    second.scenes[0]?.voxelEnvironment,
  );
  assert.deepEqual(first.scenes[0]?.entities, second.scenes[0]?.entities);
});

test("relay annex is a distinct content-only composition with settled demo meanings", () => {
  const project = relayAnnexStoredProject();
  const scene = project.scenes[0];
  const encounter = scene?.entities.find(
    (entity) => entity.id === ENCOUNTER_IDS.encounter,
  );
  const player = scene?.entities.find(
    (entity) => entity.id === ENCOUNTER_IDS.actor,
  );
  const beacon = scene?.entities.find(
    (entity) => entity.id === ENCOUNTER_IDS.extractionBeacon,
  );

  assert.equal(project.projectId, "relay-annex");
  assert.deepEqual(scene?.voxelEnvironment, {
    kind: "generatedRoom",
    seed: 17,
    voxelSize: 1,
    chunkSize: 16,
    width: 5,
    height: 4,
    length: 8,
  });
  assert.deepEqual(encounter?.encounter?.members, [ENCOUNTER_IDS.firstEnemy]);
  assert.equal(
    scene?.entities.some(
      (entity) => entity.id === ENCOUNTER_IDS.firstEnemy + 1,
    ),
    false,
  );
  assert.deepEqual(player?.translation, [2.5, 1.5, 2.5]);
  assert.deepEqual(beacon?.translation, [3.5, 1.5, 4.5]);
  assert.deepEqual(beacon?.extractionBeacon, { activationRadius: 4 });
});

test("relay annex authoring materializes its checked project artifact", () => {
  const artifact = JSON.parse(
    readFileSync(
      new URL(
        "../../../../content/projects/relay-annex.project.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );

  assert.deepEqual(relayAnnexStoredProject(), artifact);
});
