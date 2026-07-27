import type {
  EntityDefinition,
  MaterialVoxelEnvironmentDefinition,
  PlayerInputBindingsDefinition,
  ProjectContent,
  StoredAssetDefinition,
  StoredProjectContent,
  Vec3,
  VoxelAddress,
} from "./schema.js";

export const ENCOUNTER_IDS = {
  actor: 1,
  encounter: 2,
  exit: 3,
  firstEnemy: 4,
  doorControl: 6,
  extractionBeacon: 7,
  motionProbe: 10,
  cargoDoor: 11,
  extractionGate: 12,
  generatorDoor: 13,
  energyFillPickup: 20,
  energyOverflowPickup: 21,
  ammunitionPickup: 22,
  weaponPickup: 23,
  healthPickup: 24,
  armorPickup: 25,
  keyPickup: 26,
  hazard: 27,
  automaticWeaponPickup: 28,
  keyedBulkhead: 30,
  secretRegion: 31,
  levelExit: 32,
  meleeDropPickup: 33,
  rangedDropPickup: 34,
  generatorEncounter: 40,
  generatorMelee: 41,
  generatorRanged: 42,
  finalEncounter: 50,
  finalMeleeOne: 51,
  finalRangedOne: 52,
  finalMeleeTwo: 53,
  finalRangedTwo: 54,
  generatorMeleeDrop: 60,
  generatorRangedDrop: 61,
  finalMeleeOneDrop: 62,
  finalRangedOneDrop: 63,
  finalMeleeTwoDrop: 64,
  finalRangedTwoDrop: 65,
  ambientLight: 80,
  cargoLight: 81,
  storageLight: 82,
  generatorLight: 83,
  controlLight: 84,
  gantryLight: 85,
  extractionLight: 86,
  exitLight: 87,
} as const;

export const LOADING_BAY_ITEM_IDS = {
  arcPistol: "weapon/arc-pistol",
  breachScattergun: "weapon/breach-scattergun",
  rivetCarbine: "weapon/rivet-carbine",
  energyCell: "ammo/energy-cell",
  scatterShell: "ammo/scatter-shell",
  maintenancePass: "key/maintenance-pass",
  medPatch: "supply/med-patch",
  impactVest: "armor/impact-vest",
  inertInspectionTag: "key/inert-inspection-tag",
} as const;

export interface EncounterProjectOptions {
  readonly navigationGoal?: Vec3;
  readonly navigationSpeedUnitsPerSecond?: number;
  readonly playerBindings?: PlayerInputBindingsDefinition;
  readonly generationSeed?: number;
  readonly enemyHealth?: number;
  readonly weaponDamage?: number;
  readonly weaponCooldownTicks?: number;
  readonly beaconActivationRadius?: number;
  readonly levelVariant?: "campaign" | "relayAnnex";
}

const GENERATED_ROOM = {
  voxelSize: 1,
  chunkSize: 16,
  width: 7,
  height: 4,
  length: 10,
} as const;
const GENERATED_EXIT = {
  centerX: (GENERATED_ROOM.width + 2) / 2,
  centerY: 1,
  wallZ: GENERATED_ROOM.length + 1,
  collisionHalfExtents: [1.2, 1.5, 0.275] as const,
} as const;

const ANIMATED_CHARACTER_ASSET = {
  id: "mesh-animation/kenney-retro-character-medium",
  catalog: {
    version: 1,
    hash: "c71255a41c0373f0d2ef52593369d5fd9d2f6220ae548aff8cd6bf5edb403674",
    sourcePath: "content/assets/kenney-retro-character-medium.glb",
    label: "Kenney Retro Character",
    dependencies: [],
  },
  animatedMesh: {
    asset: "mesh-animation/kenney-retro-character-medium",
    runtimeFormat: "glb",
    contentHash:
      "sha256:c71255a41c0373f0d2ef52593369d5fd9d2f6220ae548aff8cd6bf5edb403674",
    clips: [
      { id: "idle", name: "Idle", durationSeconds: 1.04166662693024 },
      { id: "run", name: "Run", durationSeconds: 0.666666686534882 },
      { id: "jump", name: "Jump", durationSeconds: 0.5 },
    ],
    defaultClip: "idle",
    materialSlots: [],
    bounds: {
      min: [-0.02, -0.01, 0],
      max: [0.02, 0.01, 0.04],
    },
  },
} as const satisfies StoredAssetDefinition;

/**
 * Schema-6 migration fixture retained to prove predecessor admission.
 * New game content should use the current stored-project composer below.
 */
export function encounterGateProject(
  enemyNames: readonly string[],
  options: EncounterProjectOptions = {},
): ProjectContent {
  if (enemyNames.length === 0) {
    throw new Error("an encounter gate requires at least one enemy");
  }
  const normalizedNames = enemyNames.map((name) => name.trim());
  if (normalizedNames.some((name) => name.length === 0)) {
    throw new Error("enemy names must not be empty");
  }

  const enemies: EntityDefinition[] = normalizedNames.map((name, index) => ({
    id: ENCOUNTER_IDS.firstEnemy + index,
    name,
    translation: index === 0 ? [1.5, 1.5, 6.5] : [6.5, 1.5, 2.5],
    collision: { enabled: true, staticCollider: false },
    renderable: { asset: "mesh/security-sentry", visible: true },
    enemy: true,
    health: {
      max: options.enemyHealth ?? 100,
      hitboxHalfExtents: [0.55, 0.9, 0.55],
    },
    ...(index === 0
      ? {
          kinematic: { halfExtents: [0.25, 0.25, 0.25], velocity: [0, 0, 0] },
          navigation: {
            goal: options.navigationGoal ?? [7.5, 1.5, 6.5],
            speedUnitsPerSecond: options.navigationSpeedUnitsPerSecond ?? 4,
            maxVisited: 512,
          },
        }
      : {}),
  }));
  const members = enemies.map((enemy) => enemy.id);

  return {
    schemaVersion: 6,
    entities: [
      {
        id: ENCOUNTER_IDS.actor,
        name: "player",
        translation: [1.5, 1.5, 2.5],
        collision: { enabled: true, staticCollider: false },
        renderable: { asset: "primitive/player-marker", visible: true },
        kinematic: { halfExtents: [0.25, 0.25, 0.25], velocity: [0, 0, 0] },
        playerController: {
          moveSpeedUnitsPerSecond: 4,
          moveStepSeconds: 0.1,
          lookDegreesPerUnit: 12,
          initialYawDegrees: 0,
          initialPitchDegrees: -10,
          bindings: options.playerBindings ?? {
            moveForward: "KeyW",
            moveBackward: "KeyS",
            moveLeft: "KeyA",
            moveRight: "KeyD",
            mouseLook: "pointer",
            primaryFire: "Mouse0",
          },
        },
        weapon: {
          damage: options.weaponDamage ?? 60,
          maxDistance: 20,
          cooldownTicks: options.weaponCooldownTicks ?? 2,
          ammoCapacity: 8,
          muzzleOffset: [0, 0, 0],
        },
      },
      {
        id: ENCOUNTER_IDS.encounter,
        name: "loading-bay-encounter",
        encounter: { members, exit: ENCOUNTER_IDS.exit },
      },
      {
        id: ENCOUNTER_IDS.exit,
        name: "loading-bay-exit",
        translation: [
          GENERATED_EXIT.centerX,
          GENERATED_EXIT.centerY,
          GENERATED_EXIT.wallZ,
        ],
        collision: { enabled: true, staticCollider: true },
        renderable: { asset: "mesh/security-door", visible: true },
        kinematic: {
          halfExtents: GENERATED_EXIT.collisionHalfExtents,
          velocity: [0, 0, 0],
        },
        door: {
          openTranslation: [
            GENERATED_EXIT.centerX,
            GENERATED_EXIT.centerY + 3,
            GENERATED_EXIT.wallZ,
          ],
          autoCloseAfterTicks: null,
        },
      },
      ...enemies,
      {
        id: ENCOUNTER_IDS.motionProbe,
        name: "spatial-probe",
        translation: [1.5, 1.5, 8.5],
        renderable: { asset: "primitive/spatial-probe", visible: true },
        kinematic: { halfExtents: [0.25, 0.25, 0.25], velocity: [5, 0, 0] },
      },
    ],
    generatedVoxelEnvironment: {
      seed: options.generationSeed ?? 4,
      ...GENERATED_ROOM,
    },
  };
}

export const generatedEncounterProjects = {
  "encounter-gate.project.json": encounterGateProject([
    "sentry-alpha",
    "sentry-beta",
  ]),
  "encounter-gate-solo.project.json": encounterGateProject(["sentry-alpha"]),
} as const;

/** Optional authoring frontend for the same candidate admitted canonically by Rust. */
export function loadingBayStoredProject(
  options: EncounterProjectOptions = {},
): StoredProjectContent {
  const levelVariant = options.levelVariant ?? "campaign";
  const legacy = encounterGateProject(["sentry-alpha", "sentry-beta"], options);
  const entities = legacy.entities.map((entity) => {
    const renderable = entity.renderable;
    if (
      renderable === undefined ||
      !renderable.asset.startsWith("primitive/")
    ) {
      return entity;
    }
    return {
      ...entity,
      renderable: {
        asset: `mesh/${renderable.asset.slice("primitive/".length)}`,
        visible: renderable.visible,
      },
    };
  });
  const player = entities.find((entity) => entity.id === ENCOUNTER_IDS.actor);
  if (player === undefined) {
    throw new Error("loading-bay player source is missing");
  }
  const { weapon: legacyWeapon, ...playerWithoutLegacyWeapon } = player;
  if (legacyWeapon === undefined || player.playerController === undefined) {
    throw new Error("loading-bay player weapon/controller source is missing");
  }
  const playerController = player.playerController;
  const entitiesWithInventory = entities.map((entity): EntityDefinition => {
    if (entity.id === ENCOUNTER_IDS.actor) {
      return {
        ...playerWithoutLegacyWeapon,
        translation:
          levelVariant === "relayAnnex"
            ? ([6.5, 1.5, 3.5] as const)
            : ([5.5, 1.5, 3.5] as const),
        playerController: {
          ...playerController,
          moveSpeedUnitsPerSecond: 6,
          initialYawDegrees: 180,
          initialPitchDegrees: -6,
          bindings: {
            ...playerController.bindings,
            selectWeapon: ["Digit1", "Digit2", "Digit3"],
          },
        },
        bounds: {
          min: [-0.25, -0.25, -0.25] as const,
          max: [0.25, 0.25, 0.25] as const,
        },
        health: {
          max: 100,
          hitboxHalfExtents: [0.25, 0.5, 0.25] as const,
          maxArmor: 100,
          armorAbsorptionPercent: 50,
        },
        inventory: {
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
        },
      };
    }
    if (entity.id === ENCOUNTER_IDS.encounter) {
      return {
        ...entity,
        name: "cargo-floor-contact",
        translation: [7.5, 1.5, 11.5] as const,
        encounter: {
          members: [ENCOUNTER_IDS.firstEnemy],
          exit: ENCOUNTER_IDS.cargoDoor,
          activationRadius: 6,
        },
      };
    }
    if (entity.id === ENCOUNTER_IDS.exit) {
      return {
        ...entity,
        name: "extraction-pressure-door",
        translation: [21.5, 1.5, 49],
        bounds: {
          min: [-1.2, -1.5, -0.275] as const,
          max: [1.2, 1.5, 0.275] as const,
        },
        door: {
          openTranslation: [21.5, 5.5, 49],
          autoCloseAfterTicks: null,
        },
      };
    }
    if (entity.enemy === true) {
      if (entity.id === ENCOUNTER_IDS.firstEnemy) {
        return enemyEntity({
          id: entity.id,
          name: "cargo-loader-arrival",
          translation:
            levelVariant === "relayAnnex" ? [9.5, 1.5, 12.5] : [7.5, 1.5, 12.5],
          kind: "melee",
          drop: ENCOUNTER_IDS.meleeDropPickup,
          health: options.enemyHealth,
          navigationGoal: options.navigationGoal,
          navigationSpeed: options.navigationSpeedUnitsPerSecond,
        });
      }
      return enemyEntity({
        id: entity.id,
        name: "gantry-sentry-generator",
        translation: [20.5, 1.5, 21.5],
        kind: "ranged",
        drop: ENCOUNTER_IDS.rangedDropPickup,
        health: options.enemyHealth,
      });
    }
    return entity;
  });
  const probe = entitiesWithInventory.at(-1);
  if (probe?.id !== ENCOUNTER_IDS.motionProbe) {
    throw new Error("loading-bay source composition is incomplete");
  }

  return {
    schemaVersion: 20,
    projectId: "loading-bay",
    name: "Loading Bay",
    entryScene: "scene/loading-bay",
    assets: [
      ANIMATED_CHARACTER_ASSET,
      { id: "mesh/arc-warden" },
      { id: "mesh/bay-rusher" },
      { id: "mesh/control-panel" },
      { id: "mesh/extraction-beacon" },
      { id: "mesh/hazard-pad" },
      { id: "mesh/level-exit" },
      { id: "mesh/pickup-ammunition" },
      { id: "mesh/pickup-armor" },
      { id: "mesh/pickup-health" },
      { id: "mesh/pickup-key" },
      { id: "mesh/pickup-weapon" },
      { id: "mesh/player-marker" },
      { id: "mesh/security-door" },
      { id: "mesh/spatial-probe" },
    ],
    itemDefinitions: [
      {
        id: LOADING_BAY_ITEM_IDS.energyCell,
        maxQuantity: 200,
        kind: { kind: "ammunition" },
      },
      {
        id: LOADING_BAY_ITEM_IDS.scatterShell,
        maxQuantity: 50,
        kind: { kind: "ammunition" },
      },
      {
        id: LOADING_BAY_ITEM_IDS.impactVest,
        maxQuantity: 1,
        kind: { kind: "armor", protection: 100 },
      },
      {
        id: LOADING_BAY_ITEM_IDS.inertInspectionTag,
        maxQuantity: 1,
        kind: { kind: "accessKey" },
      },
      {
        id: LOADING_BAY_ITEM_IDS.maintenancePass,
        maxQuantity: 1,
        kind: { kind: "accessKey" },
      },
      {
        id: LOADING_BAY_ITEM_IDS.medPatch,
        maxQuantity: 5,
        kind: { kind: "healthSupply", restoreHealth: 25 },
      },
      {
        id: LOADING_BAY_ITEM_IDS.arcPistol,
        maxQuantity: 1,
        kind: {
          kind: "weapon",
          attackMode: "hitscan",
          damage: options.weaponDamage ?? 60,
          maxDistance: 20,
          cooldownTicks: options.weaponCooldownTicks ?? 2,
          ammunition: LOADING_BAY_ITEM_IDS.energyCell,
          ammunitionCost: 1,
          muzzleOffset: [0, 0, 0],
          presentation: "arc-pistol",
        },
      },
      {
        id: LOADING_BAY_ITEM_IDS.breachScattergun,
        maxQuantity: 1,
        kind: {
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
      },
      {
        id: LOADING_BAY_ITEM_IDS.rivetCarbine,
        maxQuantity: 1,
        kind: {
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
      },
    ],
    scenes: [
      {
        id: "scene/loading-bay",
        name: "Loading Bay",
        voxelEnvironment: {
          kind: "material",
          ...loadingBayMaterialEnvironment(levelVariant),
        },
        entities: (
          [
            ...entitiesWithInventory.slice(0, -1),
            {
              id: ENCOUNTER_IDS.doorControl,
              name: "generator-interlock-control",
              translation: [11.5, 1.5, 21.5],
              renderable: { asset: "mesh/control-panel", visible: true },
              switch: {
                controls: [ENCOUNTER_IDS.extractionGate],
                loadingBayInterlock: {
                  closeDoor: ENCOUNTER_IDS.generatorDoor,
                  openDoor: ENCOUNTER_IDS.extractionGate,
                },
              },
            },
            {
              id: ENCOUNTER_IDS.extractionBeacon,
              name: "extraction-beacon",
              translation: [18.5, 1.5, 46.5],
              renderable: { asset: "mesh/extraction-beacon", visible: true },
              extractionBeacon: {
                activationRadius: options.beaconActivationRadius ?? 3,
              },
            },
            {
              ...probe,
              name: "gantry-status-runner",
              translation: [18.5, 1.5, 32.5],
              kinematic: {
                halfExtents: [0.25, 0.25, 0.25],
                velocity: [2, 0, 0],
              },
            },
            pickupEntity(
              ENCOUNTER_IDS.energyFillPickup,
              "arrival-energy-cache",
              "mesh/pickup-ammunition",
              [4.5, 1.5, 9.5],
              LOADING_BAY_ITEM_IDS.energyCell,
              12,
            ),
            pickupEntity(
              ENCOUNTER_IDS.energyOverflowPickup,
              "side-storage-med-patch",
              "mesh/pickup-health",
              [2.5, 1.5, 20.5],
              LOADING_BAY_ITEM_IDS.medPatch,
              1,
            ),
            pickupEntity(
              ENCOUNTER_IDS.ammunitionPickup,
              "side-storage-shell-cache",
              "mesh/pickup-ammunition",
              [3.5, 1.5, 21.5],
              LOADING_BAY_ITEM_IDS.scatterShell,
              6,
            ),
            pickupEntity(
              ENCOUNTER_IDS.weaponPickup,
              "side-storage-breach-scattergun",
              "mesh/pickup-weapon",
              [5.5, 1.5, 22.5],
              LOADING_BAY_ITEM_IDS.breachScattergun,
              1,
              {
                item: LOADING_BAY_ITEM_IDS.scatterShell,
                quantity: 8,
              },
            ),
            pickupEntity(
              ENCOUNTER_IDS.healthPickup,
              "generator-med-patch",
              "mesh/pickup-health",
              [19.5, 1.5, 24.5],
              LOADING_BAY_ITEM_IDS.medPatch,
              2,
            ),
            pickupEntity(
              ENCOUNTER_IDS.armorPickup,
              "secret-manifest-impact-vest",
              "mesh/pickup-armor",
              [3.5, 1.5, 24.5],
              LOADING_BAY_ITEM_IDS.impactVest,
              1,
            ),
            pickupEntity(
              ENCOUNTER_IDS.keyPickup,
              "generator-maintenance-pass",
              "mesh/pickup-key",
              [27.5, 1.5, 28.5],
              LOADING_BAY_ITEM_IDS.maintenancePass,
              1,
            ),
            {
              id: ENCOUNTER_IDS.hazard,
              name: "generator-coolant-leak",
              translation: [23.5, 1.5, 25.5],
              bounds: {
                min: [-1.45, -0.45, -1.45],
                max: [1.45, 0.45, 1.45],
              },
              renderable: { asset: "mesh/hazard-pad", visible: true },
              hazard: { damage: 12, cooldownTicks: 60 },
            },
            pickupEntity(
              ENCOUNTER_IDS.automaticWeaponPickup,
              "generator-rivet-carbine",
              "mesh/pickup-weapon",
              [18.5, 1.5, 28.5],
              LOADING_BAY_ITEM_IDS.rivetCarbine,
              1,
            ),
            doorEntity(
              ENCOUNTER_IDS.cargoDoor,
              "cargo-floor-pressure-door",
              [4.5, 1.5, 17],
              1.2,
            ),
            doorEntity(
              ENCOUNTER_IDS.extractionGate,
              "extraction-gate",
              [21.5, 1.5, 35],
              1.7,
            ),
            doorEntity(
              ENCOUNTER_IDS.generatorDoor,
              "generator-pressure-door",
              [23.5, 1.5, 30],
              1.7,
            ),
            doorEntity(
              ENCOUNTER_IDS.keyedBulkhead,
              "maintenance-bulkhead",
              [11.5, 1.5, 17],
              3.2,
              {
                requiredKey: LOADING_BAY_ITEM_IDS.maintenancePass,
                keyPolicy: "retain",
                activationRadius: 3,
                deniedPresentation: "Maintenance pass required",
              },
            ),
            {
              id: ENCOUNTER_IDS.secretRegion,
              name: "sealed-manifest-secret",
              translation: [3.5, 1.5, 24.5],
              bounds: {
                min: [-1.1, -0.6, -0.8],
                max: [1.1, 0.6, 0.8],
              },
              secretRegion: {
                presentation: "Sealed manifest cache discovered",
              },
            },
            {
              id: ENCOUNTER_IDS.levelExit,
              name: "loading-bay-level-exit",
              translation: [21.5, 1.5, 50.5],
              renderable: { asset: "mesh/level-exit", visible: true },
              levelExit: {
                activationRadius: 2,
                presentation: "Loading Bay complete",
              },
            },
            encounterEntity(
              ENCOUNTER_IDS.generatorEncounter,
              "generator-floor-contact",
              [23.5, 1.5, 24.5],
              [
                ENCOUNTER_IDS.firstEnemy + 1,
                ENCOUNTER_IDS.generatorMelee,
                ENCOUNTER_IDS.generatorRanged,
              ],
              ENCOUNTER_IDS.generatorDoor,
              8,
            ),
            enemyEntity({
              id: ENCOUNTER_IDS.generatorMelee,
              name: "generator-loader",
              translation: [24.5, 1.5, 23.5],
              kind: "melee",
              drop: ENCOUNTER_IDS.generatorMeleeDrop,
              health: options.enemyHealth,
            }),
            enemyEntity({
              id: ENCOUNTER_IDS.generatorRanged,
              name: "generator-warden",
              translation: [26.5, 1.5, 27.5],
              kind: "ranged",
              drop: ENCOUNTER_IDS.generatorRangedDrop,
              health: options.enemyHealth,
            }),
            encounterEntity(
              ENCOUNTER_IDS.finalEncounter,
              "extraction-dock-contact",
              [22.5, 1.5, 42.5],
              [
                ENCOUNTER_IDS.finalMeleeOne,
                ENCOUNTER_IDS.finalRangedOne,
                ENCOUNTER_IDS.finalMeleeTwo,
                ENCOUNTER_IDS.finalRangedTwo,
              ],
              ENCOUNTER_IDS.exit,
              5,
            ),
            enemyEntity({
              id: ENCOUNTER_IDS.finalMeleeOne,
              name: "dock-loader-west",
              translation: [18.5, 1.5, 39.5],
              kind: "melee",
              drop: ENCOUNTER_IDS.finalMeleeOneDrop,
              health: options.enemyHealth,
            }),
            enemyEntity({
              id: ENCOUNTER_IDS.finalRangedOne,
              name: "dock-warden-west",
              translation: [24.5, 1.5, 39.5],
              kind: "ranged",
              drop: ENCOUNTER_IDS.finalRangedOneDrop,
              health: options.enemyHealth,
            }),
            enemyEntity({
              id: ENCOUNTER_IDS.finalMeleeTwo,
              name: "dock-loader-east",
              translation: [20.5, 1.5, 43.5],
              kind: "melee",
              drop: ENCOUNTER_IDS.finalMeleeTwoDrop,
              health: options.enemyHealth,
            }),
            enemyEntity({
              id: ENCOUNTER_IDS.finalRangedTwo,
              name: "dock-warden-east",
              translation: [26.5, 1.5, 44.5],
              kind: "ranged",
              drop: ENCOUNTER_IDS.finalRangedTwoDrop,
              health: options.enemyHealth,
            }),
            hiddenDropEntity(
              ENCOUNTER_IDS.meleeDropPickup,
              "cargo-loader-field-drop",
              LOADING_BAY_ITEM_IDS.medPatch,
              1,
              0,
            ),
            hiddenDropEntity(
              ENCOUNTER_IDS.rangedDropPickup,
              "gantry-sentry-field-drop",
              LOADING_BAY_ITEM_IDS.energyCell,
              10,
              1,
            ),
            hiddenDropEntity(
              ENCOUNTER_IDS.generatorMeleeDrop,
              "generator-loader-field-drop",
              LOADING_BAY_ITEM_IDS.medPatch,
              1,
              2,
            ),
            hiddenDropEntity(
              ENCOUNTER_IDS.generatorRangedDrop,
              "generator-warden-field-drop",
              LOADING_BAY_ITEM_IDS.energyCell,
              10,
              3,
            ),
            hiddenDropEntity(
              ENCOUNTER_IDS.finalMeleeOneDrop,
              "dock-loader-west-field-drop",
              LOADING_BAY_ITEM_IDS.medPatch,
              1,
              4,
            ),
            hiddenDropEntity(
              ENCOUNTER_IDS.finalRangedOneDrop,
              "dock-warden-west-field-drop",
              LOADING_BAY_ITEM_IDS.energyCell,
              10,
              5,
            ),
            hiddenDropEntity(
              ENCOUNTER_IDS.finalMeleeTwoDrop,
              "dock-loader-east-field-drop",
              LOADING_BAY_ITEM_IDS.medPatch,
              1,
              6,
            ),
            hiddenDropEntity(
              ENCOUNTER_IDS.finalRangedTwoDrop,
              "dock-warden-east-field-drop",
              LOADING_BAY_ITEM_IDS.energyCell,
              10,
              7,
            ),
            ambientLightEntity(ENCOUNTER_IDS.ambientLight),
            pointLightEntity(
              ENCOUNTER_IDS.cargoLight,
              "cargo-floor-work-light",
              [7.5, 3.3, 9.5],
              [0.98, 0.68, 0.35],
              1.3,
              15,
            ),
            pointLightEntity(
              ENCOUNTER_IDS.storageLight,
              "side-storage-work-light",
              [4.5, 3.2, 22.5],
              [0.3, 0.62, 1],
              1,
              11,
            ),
            pointLightEntity(
              ENCOUNTER_IDS.generatorLight,
              "generator-warning-light",
              [23.5, 3.2, 25.5],
              [1, 0.25, 0.12],
              1.5,
              14,
            ),
            pointLightEntity(
              ENCOUNTER_IDS.controlLight,
              "control-room-light",
              [11.5, 3.2, 21.5],
              [0.25, 0.9, 0.72],
              1.2,
              10,
            ),
            pointLightEntity(
              ENCOUNTER_IDS.gantryLight,
              "return-gantry-light",
              [18.5, 3.2, 32.5],
              [0.55, 0.72, 1],
              1,
              14,
            ),
            pointLightEntity(
              ENCOUNTER_IDS.extractionLight,
              "extraction-dock-light",
              [22.5, 3.2, 42.5],
              [0.35, 0.75, 1],
              1.35,
              18,
            ),
            pointLightEntity(
              ENCOUNTER_IDS.exitLight,
              "exit-marker-light",
              [21.5, 3.2, 48],
              [0.3, 1, 0.45],
              1.5,
              9,
            ),
          ] satisfies EntityDefinition[]
        ).sort((left, right) => left.id - right.id),
      },
    ],
  };
}

/** A second arrangement composed entirely from the demo's settled authored meanings. */
export function relayAnnexStoredProject(): StoredProjectContent {
  const source = loadingBayStoredProject({
    levelVariant: "relayAnnex",
    navigationGoal: [9.5, 1.5, 12.5],
    navigationSpeedUnitsPerSecond: 2.5,
    enemyHealth: 80,
    weaponDamage: 40,
    beaconActivationRadius: 4,
  });
  const sourceScene = source.scenes[0];
  if (sourceScene === undefined) {
    throw new Error("loading-bay source scene is missing");
  }

  return {
    ...source,
    projectId: "relay-annex",
    name: "Relay Annex",
    entryScene: "scene/relay-annex",
    scenes: [
      {
        ...sourceScene,
        id: "scene/relay-annex",
        name: "Relay Annex",
      },
    ],
  };
}

function enemyEntity(options: {
  readonly id: number;
  readonly name: string;
  readonly translation: Vec3;
  readonly kind: "melee" | "ranged";
  readonly drop: number;
  readonly health?: number | undefined;
  readonly navigationGoal?: Vec3 | undefined;
  readonly navigationSpeed?: number | undefined;
}): EntityDefinition {
  const melee = options.kind === "melee";
  return {
    id: options.id,
    name: options.name,
    translation: options.translation,
    collision: { enabled: true, staticCollider: false },
    renderable: {
      asset: melee ? "mesh/bay-rusher" : "mesh/arc-warden",
      visible: true,
    },
    enemy: true,
    health: {
      max: options.health ?? 100,
      hitboxHalfExtents: [0.55, 0.9, 0.55],
    },
    scale: melee ? [1.15, 0.85, 1.15] : [0.75, 1.35, 0.75],
    kinematic: {
      halfExtents: melee ? [0.45, 0.25, 0.45] : [0.3, 0.5, 0.3],
      velocity: [0, 0, 0],
    },
    navigation: {
      goal: options.navigationGoal ?? options.translation,
      speedUnitsPerSecond: options.navigationSpeed ?? (melee ? 4 : 2.5),
      maxVisited: 512,
    },
    enemyCombat: {
      sightRange: melee ? 7 : 8,
      hearingRange: 2.5,
      attack: melee
        ? {
            kind: "melee",
            damage: 8,
            range: 1.25,
            cooldownTicks: 120,
            originOffset: [0, 0, 0],
            presentation: "sentry-strike",
          }
        : {
            kind: "rangedHitscan",
            damage: 4,
            range: 7,
            cooldownTicks: 120,
            originOffset: [0, 0.25, 0],
            presentation: "sentry-pulse",
          },
    },
    defeatDrop: { pickup: options.drop },
  };
}

function encounterEntity(
  id: number,
  name: string,
  translation: Vec3,
  members: readonly number[],
  exit: number,
  activationRadius: number,
): EntityDefinition {
  return {
    id,
    name,
    translation,
    encounter: { members, exit, activationRadius },
  };
}

function doorEntity(
  id: number,
  name: string,
  translation: Vec3,
  halfWidth: number,
  access?: NonNullable<EntityDefinition["door"]>["access"],
): EntityDefinition {
  return {
    id,
    name,
    translation,
    bounds: {
      min: [-halfWidth, -1.5, -0.275],
      max: [halfWidth, 1.5, 0.275],
    },
    collision: { enabled: true, staticCollider: true },
    renderable: { asset: "mesh/security-door", visible: true },
    kinematic: {
      halfExtents: [halfWidth, 1.5, 0.275],
      velocity: [0, 0, 0],
    },
    door: {
      openTranslation: [translation[0], 5.5, translation[2]],
      autoCloseAfterTicks: null,
      ...(access === undefined ? {} : { access }),
    },
  };
}

function hiddenDropEntity(
  id: number,
  name: string,
  item: string,
  quantity: number,
  index: number,
): EntityDefinition {
  const asset =
    item === LOADING_BAY_ITEM_IDS.medPatch
      ? "mesh/pickup-health"
      : "mesh/pickup-ammunition";
  return {
    ...pickupEntity(id, name, asset, [-32, -32, -32 - index], item, quantity),
    renderable: { asset, visible: false },
  };
}

function ambientLightEntity(id: number): EntityDefinition {
  return {
    id,
    name: "loading-bay-ambient-light",
    light: {
      kind: "ambient",
      color: [0.16, 0.2, 0.28],
      intensity: 0.6,
      enabled: true,
      shadows: false,
    },
  };
}

function pointLightEntity(
  id: number,
  name: string,
  translation: Vec3,
  color: Vec3,
  intensity: number,
  range: number,
): EntityDefinition {
  return {
    id,
    name,
    translation,
    light: {
      kind: "point",
      color,
      intensity,
      enabled: true,
      range,
      decay: 2,
      shadows: false,
    },
  };
}

function loadingBayMaterialEnvironment(
  variant: "campaign" | "relayAnnex",
): MaterialVoxelEnvironmentDefinition {
  const cells = new Map<
    string,
    { readonly address: VoxelAddress; readonly materialSlot: number }
  >();
  const set = (x: number, y: number, z: number, materialSlot: number): void => {
    cells.set(`${x},${y},${z}`, {
      address: [x, y, z],
      materialSlot,
    });
  };
  const box = (
    min: VoxelAddress,
    max: VoxelAddress,
    materialSlot: number,
  ): void => {
    for (let x = min[0]; x <= max[0]; x += 1) {
      for (let y = min[1]; y <= max[1]; y += 1) {
        for (let z = min[2]; z <= max[2]; z += 1) {
          set(x, y, z, materialSlot);
        }
      }
    }
  };
  const wallZ = (
    z: number,
    fromX: number,
    toX: number,
    openings: readonly (readonly [number, number])[] = [],
  ): void => {
    for (let x = fromX; x <= toX; x += 1) {
      if (openings.some(([from, to]) => x >= from && x <= to)) {
        continue;
      }
      box([x, 1, z], [x, 3, z], 1);
    }
  };
  const wallX = (
    x: number,
    fromZ: number,
    toZ: number,
    openings: readonly (readonly [number, number])[] = [],
  ): void => {
    for (let z = fromZ; z <= toZ; z += 1) {
      if (openings.some(([from, to]) => z >= from && z <= to)) {
        continue;
      }
      box([x, 1, z], [x, 3, z], 1);
    }
  };

  box([0, 0, 0], [30, 0, 51], 2);
  box([0, 4, 0], [30, 4, 49], 1);
  wallZ(0, 0, 30);
  wallZ(49, 0, 30, [[20, 22]]);
  wallX(0, 0, 49);
  wallX(30, 0, 49);

  wallZ(17, 1, 29, [
    [3, 5],
    [8, 14],
  ]);
  wallX(8, 18, 25);
  wallX(16, 18, 30, [[27, 29]]);
  wallZ(23, 1, 7, [[6, 7]]);
  wallZ(30, 16, 29, [[22, 25]]);
  wallZ(35, 1, 29, [[20, 23]]);

  box([10, 1, 8], [12, 2, 9], 3);
  box([20, 1, 11], [22, 2, 12], 3);
  box([25, 1, 6], [27, 2, 7], variant === "campaign" ? 3 : 4);
  box([18, 1, 20], [19, 2, 21], 3);
  box([27, 1, 24], [28, 2, 25], 3);
  box([17, 1, 40], [18, 2, 41], 3);
  box([26, 1, 45], [27, 2, 46], variant === "campaign" ? 3 : 4);
  box(
    variant === "campaign" ? [2, 1, 6] : [4, 1, 7],
    variant === "campaign" ? [3, 2, 7] : [5, 2, 8],
    3,
  );

  return {
    voxelSize: 1,
    chunkSize: 16,
    materialVoxels: [...cells.values()].sort(
      (left, right) =>
        left.address[0] - right.address[0] ||
        left.address[1] - right.address[1] ||
        left.address[2] - right.address[2] ||
        left.materialSlot - right.materialSlot,
    ),
  };
}

function pickupEntity(
  id: number,
  name: string,
  asset: string,
  translation: Vec3,
  item: string,
  quantity: number,
  starterAmmunition?: {
    readonly item: string;
    readonly quantity: number;
  },
): EntityDefinition {
  return {
    id,
    name,
    translation,
    bounds: {
      min: [-0.35, -0.35, -0.35],
      max: [0.35, 0.35, 0.35],
    },
    renderable: { asset, visible: true },
    pickup: {
      item,
      quantity,
      ...(starterAmmunition === undefined ? {} : { starterAmmunition }),
    },
  };
}
