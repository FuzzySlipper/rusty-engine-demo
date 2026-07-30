import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { readCanonicalProject } from "./content-artifacts.js";
import {
  ENCOUNTER_IDS,
  LOADING_BAY_ITEM_IDS,
  encounterGateProject,
} from "./encounter-project.js";

const projectDirectory = fileURLToPath(
  new URL("../../../../content/projects/", import.meta.url),
);

function loadingBayProject() {
  return readCanonicalProject(projectDirectory, "loading-bay.project.json");
}

function relayAnnexProject() {
  return readCanonicalProject(projectDirectory, "relay-annex.project.json");
}

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

test("schema-six migration fixture keeps predecessor entity ownership explicit", () => {
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

test("the Loading Bay project is read directly from its canonical artifact", () => {
  const project = loadingBayProject();

  assert.equal(project.projectId, "loading-bay");
  assert.equal(project.entryScene, "scene/loading-bay");
  assert.equal(project.schemaVersion, 22);
});

test("stored item definitions and starting inventory remain immutable authored data", () => {
  const project = loadingBayProject();
  const player = project.scenes[0]?.entities.find(
    (entity) => entity.id === ENCOUNTER_IDS.actor,
  );

  assert.deepEqual(player?.inventory, {
    capacitySlots: 8,
    startingStacks: [
      { item: LOADING_BAY_ITEM_IDS.arcPistol, quantity: 1 },
      { item: LOADING_BAY_ITEM_IDS.energyCell, quantity: 18 },
      { item: LOADING_BAY_ITEM_IDS.medPatch, quantity: 1 },
    ],
    initiallyEquippedWeapon: LOADING_BAY_ITEM_IDS.arcPistol,
    weaponSlots: [
      LOADING_BAY_ITEM_IDS.arcPistol,
      LOADING_BAY_ITEM_IDS.breachScattergun,
      LOADING_BAY_ITEM_IDS.rivetCarbine,
    ],
  });
  assert.deepEqual(
    project.itemDefinitions.find(
      (definition) => definition.id === LOADING_BAY_ITEM_IDS.arcPistol,
    ),
    {
      id: LOADING_BAY_ITEM_IDS.arcPistol,
      maxQuantity: 1,
      kind: {
        kind: "weapon",
        attackMode: "hitscan",
        damage: 60,
        maxDistance: 20,
        cooldownTicks: 2,
        ammunition: LOADING_BAY_ITEM_IDS.energyCell,
        ammunitionCost: 1,
        muzzleOffset: [0, 0, 0],
        presentation: "arc-pistol",
      },
    },
  );
  assert.deepEqual(
    project.itemDefinitions.find(
      (definition) => definition.id === LOADING_BAY_ITEM_IDS.breachScattergun,
    )?.kind,
    {
      kind: "weapon",
      attackMode: "spread",
      pelletCount: 7,
      spreadDegrees: 7,
      damage: 14,
      maxDistance: 12,
      cooldownTicks: 36,
      ammunition: LOADING_BAY_ITEM_IDS.scatterShell,
      ammunitionCost: 1,
      muzzleOffset: [0, 0, 0],
      presentation: "breach-scattergun",
    },
  );
  assert.deepEqual(
    project.itemDefinitions.find(
      (definition) => definition.id === LOADING_BAY_ITEM_IDS.rivetCarbine,
    )?.kind,
    {
      kind: "weapon",
      attackMode: "automatic",
      damage: 18,
      maxDistance: 25,
      cooldownTicks: 4,
      ammunition: LOADING_BAY_ITEM_IDS.energyCell,
      ammunitionCost: 1,
      muzzleOffset: [0, 0, 0],
      presentation: "rivet-carbine",
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
      (stack) =>
        (stack.item as string) === LOADING_BAY_ITEM_IDS.inertInspectionTag,
    ),
    false,
  );
  assert.deepEqual(player?.playerController?.bindings.selectWeapon, [
    "Digit1",
    "Digit2",
    "Digit3",
  ]);
  assert.deepEqual(
    project.scenes[0]?.entities.find(
      (entity) => entity.id === ENCOUNTER_IDS.weaponPickup,
    )?.pickup,
    {
      item: LOADING_BAY_ITEM_IDS.breachScattergun,
      quantity: 1,
      starterAmmunition: {
        item: LOADING_BAY_ITEM_IDS.scatterShell,
        quantity: 8,
      },
    },
  );
  assert.deepEqual(
    project.scenes[0]?.entities.find(
      (entity) => entity.id === ENCOUNTER_IDS.automaticWeaponPickup,
    )?.pickup,
    {
      item: LOADING_BAY_ITEM_IDS.rivetCarbine,
      quantity: 1,
    },
  );
});

test("the extraction beacon is game-owned data on its responsible entity", () => {
  const project = loadingBayProject();
  const beacon = project.scenes[0]?.entities.find(
    (entity) => entity.id === ENCOUNTER_IDS.extractionBeacon,
  );

  assert.deepEqual(beacon, {
    id: ENCOUNTER_IDS.extractionBeacon,
    name: "extraction-beacon",
    translation: [18.5, 1.5, 46.5],
    renderable: {
      asset: "mesh/prop-kit/extraction-beacon",
      visible: true,
    },
    extractionBeacon: { activationRadius: 3 },
  });
});

test("loading bay material voxels are deterministic and preserve all authored gates", () => {
  const first = loadingBayProject();
  const second = loadingBayProject();
  const environment = first.scenes[0]?.voxelEnvironment;

  assert.equal(environment?.kind, "material");
  assert.deepEqual(environment, second.scenes[0]?.voxelEnvironment);
  if (environment?.kind !== "material") {
    assert.fail("loading bay material environment is missing");
  }
  assert.equal(environment.voxelSize, 1);
  assert.equal(environment.chunkSize, 16);
  assert.equal(environment.materialVoxels.length, 3_931);
  assert.deepEqual(
    [
      ...new Set(environment.materialVoxels.map((voxel) => voxel.materialSlot)),
    ].sort(),
    [1, 2, 3],
  );
  const addresses = new Set(
    environment.materialVoxels.map((voxel) => voxel.address.join(",")),
  );
  for (const opening of [
    "4,1,17",
    "11,1,17",
    "23,1,30",
    "21,1,35",
    "21,1,49",
  ]) {
    assert.equal(
      addresses.has(opening),
      false,
      `${opening} remains traversable`,
    );
  }
  for (const wall of ["2,1,17", "7,1,17", "19,1,35", "24,1,35"]) {
    assert.equal(addresses.has(wall), true, `${wall} remains solid`);
  }
});

test("loading bay route composes encounters, upgrades, key loop, secret, and exit as entity data", () => {
  const scene = loadingBayProject().scenes[0];
  assert.ok(scene);
  const byId = (id: number) =>
    scene.entities.find((entity) => entity.id === id);

  assert.equal(
    scene.entities.filter((entity) => entity.enemy === true).length,
    8,
  );
  assert.deepEqual(byId(ENCOUNTER_IDS.encounter)?.encounter, {
    members: [ENCOUNTER_IDS.firstEnemy],
    exit: ENCOUNTER_IDS.cargoDoor,
    activationRadius: 6,
  });
  assert.deepEqual(byId(ENCOUNTER_IDS.generatorEncounter)?.encounter, {
    members: [
      ENCOUNTER_IDS.firstEnemy + 1,
      ENCOUNTER_IDS.generatorMelee,
      ENCOUNTER_IDS.generatorRanged,
    ],
    exit: ENCOUNTER_IDS.generatorDoor,
    activationRadius: 8,
  });
  assert.deepEqual(byId(ENCOUNTER_IDS.finalEncounter)?.encounter, {
    members: [
      ENCOUNTER_IDS.finalMeleeOne,
      ENCOUNTER_IDS.finalRangedOne,
      ENCOUNTER_IDS.finalMeleeTwo,
      ENCOUNTER_IDS.finalRangedTwo,
    ],
    exit: ENCOUNTER_IDS.exit,
    activationRadius: 5,
  });
  assert.deepEqual(byId(ENCOUNTER_IDS.doorControl)?.switch, {
    controls: [ENCOUNTER_IDS.extractionGate],
    loadingBayInterlock: {
      closeDoor: ENCOUNTER_IDS.generatorDoor,
      openDoor: ENCOUNTER_IDS.extractionGate,
    },
  });
  assert.equal(
    byId(ENCOUNTER_IDS.keyedBulkhead)?.door?.access?.requiredKey,
    LOADING_BAY_ITEM_IDS.maintenancePass,
  );
  assert.equal(
    byId(ENCOUNTER_IDS.weaponPickup)?.pickup?.item,
    LOADING_BAY_ITEM_IDS.breachScattergun,
  );
  assert.equal(
    byId(ENCOUNTER_IDS.automaticWeaponPickup)?.pickup?.item,
    LOADING_BAY_ITEM_IDS.rivetCarbine,
  );
  assert.deepEqual(
    byId(ENCOUNTER_IDS.secretRegion)?.translation,
    [3.5, 1.5, 24.5],
  );
  assert.deepEqual(byId(ENCOUNTER_IDS.generatorMelee)?.enemyCombat?.attack, {
    kind: "melee",
    damage: 8,
    range: 1.25,
    cooldownTicks: 120,
    originOffset: [0, 0, 0],
    presentation: "sentry-strike",
  });
  assert.deepEqual(
    byId(ENCOUNTER_IDS.levelExit)?.translation,
    [21.5, 1.5, 50.5],
  );
  assert.equal(
    scene.entities.filter((entity) => entity.light !== undefined).length,
    8,
  );
});

test("relay annex is a distinct content-only composition with settled demo meanings", () => {
  const project = relayAnnexProject();
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
  assert.equal(scene?.voxelEnvironment?.kind, "material");
  assert.notDeepEqual(
    scene?.voxelEnvironment,
    loadingBayProject().scenes[0]?.voxelEnvironment,
  );
  assert.deepEqual(encounter?.encounter?.members, [ENCOUNTER_IDS.firstEnemy]);
  assert.equal(
    scene?.entities.some(
      (entity) => entity.id === ENCOUNTER_IDS.firstEnemy + 1,
    ),
    true,
  );
  assert.deepEqual(player?.translation, [6.5, 1.5, 3.5]);
  assert.deepEqual(beacon?.translation, [18.5, 1.5, 46.5]);
  assert.deepEqual(beacon?.extractionBeacon, { activationRadius: 4 });
});

test("settled items, weapon tuning, enemy tuning, and layout remain content-local", () => {
  const loadingBay = loadingBayProject();
  const relayAnnex = relayAnnexProject();
  const loadingBayScene = loadingBay.scenes[0];
  const relayAnnexScene = relayAnnex.scenes[0];
  assert.ok(loadingBayScene);
  assert.ok(relayAnnexScene);

  const definition = (project: typeof loadingBay, id: string) =>
    project.itemDefinitions.find((candidate) => candidate.id === id);
  const entity = (scene: typeof loadingBayScene, id: number) =>
    scene.entities.find((candidate) => candidate.id === id);

  assert.deepEqual(
    definition(loadingBay, LOADING_BAY_ITEM_IDS.inertInspectionTag),
    definition(relayAnnex, LOADING_BAY_ITEM_IDS.inertInspectionTag),
  );
  assert.equal(
    definition(loadingBay, LOADING_BAY_ITEM_IDS.arcPistol)?.kind.kind,
    "weapon",
  );
  assert.equal(
    definition(relayAnnex, LOADING_BAY_ITEM_IDS.arcPistol)?.kind.kind,
    "weapon",
  );
  assert.notDeepEqual(
    definition(loadingBay, LOADING_BAY_ITEM_IDS.arcPistol),
    definition(relayAnnex, LOADING_BAY_ITEM_IDS.arcPistol),
  );
  assert.deepEqual(entity(loadingBayScene, ENCOUNTER_IDS.firstEnemy)?.health, {
    max: 100,
    hitboxHalfExtents: [0.55, 0.9, 0.55],
  });
  assert.deepEqual(entity(relayAnnexScene, ENCOUNTER_IDS.firstEnemy)?.health, {
    max: 80,
    hitboxHalfExtents: [0.55, 0.9, 0.55],
  });
  assert.notDeepEqual(
    loadingBayScene.voxelEnvironment,
    relayAnnexScene.voxelEnvironment,
  );
  assert.notDeepEqual(
    entity(loadingBayScene, ENCOUNTER_IDS.actor)?.translation,
    entity(relayAnnexScene, ENCOUNTER_IDS.actor)?.translation,
  );
});

test("the Relay Annex project is read directly from its canonical artifact", () => {
  const project = relayAnnexProject();

  assert.equal(project.projectId, "relay-annex");
  assert.equal(project.entryScene, "scene/relay-annex");
  assert.equal(project.schemaVersion, 22);
});
