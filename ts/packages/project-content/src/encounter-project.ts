import type {
  EntityDefinition,
  PlayerInputBindingsDefinition,
  ProjectContent,
  StoredAssetDefinition,
  StoredProjectContent,
  Vec3,
} from "./schema.js";

export const ENCOUNTER_IDS = {
  actor: 1,
  encounter: 2,
  exit: 3,
  firstEnemy: 4,
  doorControl: 6,
  extractionBeacon: 7,
  motionProbe: 10,
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
  const entitiesWithInventory = entities.map((entity) => {
    if (entity.id === ENCOUNTER_IDS.actor) {
      return {
        ...playerWithoutLegacyWeapon,
        playerController: {
          ...playerController,
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
            { item: LOADING_BAY_ITEM_IDS.energyCell, quantity: 40 },
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
    if (entity.enemy === true) {
      const melee = entity.id === ENCOUNTER_IDS.firstEnemy;
      return {
        ...entity,
        kinematic: entity.kinematic ?? {
          halfExtents: [0.25, 0.5, 0.25] as const,
          velocity: [0, 0, 0] as const,
        },
        navigation: entity.navigation ?? {
          goal: entity.translation ?? ([1.5, 1.5, 2.5] as const),
          speedUnitsPerSecond: 2.5,
          maxVisited: 512,
        },
        enemyCombat: {
          sightRange: melee ? 7 : 8,
          hearingRange: 2.5,
          attack: melee
            ? {
                kind: "melee" as const,
                damage: 12,
                range: 1.25,
                cooldownTicks: 45,
                originOffset: [0, 0, 0] as const,
                presentation: "sentry-strike",
              }
            : {
                kind: "rangedHitscan" as const,
                damage: 4,
                range: 7,
                cooldownTicks: 120,
                originOffset: [0, 0.25, 0] as const,
                presentation: "sentry-pulse",
              },
        },
      };
    }
    return entity;
  });
  const probe = entitiesWithInventory.at(-1);
  if (
    probe?.id !== ENCOUNTER_IDS.motionProbe ||
    legacy.generatedVoxelEnvironment === undefined
  ) {
    throw new Error("loading-bay source composition is incomplete");
  }

  return {
    schemaVersion: 18,
    projectId: "loading-bay",
    name: "Loading Bay",
    entryScene: "scene/loading-bay",
    assets: [
      ANIMATED_CHARACTER_ASSET,
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
      { id: "mesh/security-sentry" },
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
          kind: "generatedRoom",
          ...legacy.generatedVoxelEnvironment,
        },
        entities: [
          ...entitiesWithInventory.slice(0, -1),
          {
            id: ENCOUNTER_IDS.doorControl,
            name: "door-control",
            translation: [2.5, 1.5, 7.5],
            renderable: { asset: "mesh/control-panel", visible: true },
            switch: {
              controls: [ENCOUNTER_IDS.exit],
              loadingBayInterlock: {
                closeDoor: ENCOUNTER_IDS.keyedBulkhead,
                openDoor: ENCOUNTER_IDS.exit,
              },
            },
          },
          {
            id: ENCOUNTER_IDS.extractionBeacon,
            name: "extraction-beacon",
            translation: [
              GENERATED_EXIT.centerX,
              1.5,
              GENERATED_EXIT.wallZ + 1.5,
            ],
            renderable: { asset: "mesh/extraction-beacon", visible: true },
            extractionBeacon: {
              activationRadius: options.beaconActivationRadius ?? 16,
            },
          },
          probe,
          pickupEntity(
            ENCOUNTER_IDS.energyFillPickup,
            "energy-cell-cache",
            "mesh/pickup-ammunition",
            [2.5, 1.5, 2.5],
            LOADING_BAY_ITEM_IDS.energyCell,
            160,
          ),
          pickupEntity(
            ENCOUNTER_IDS.energyOverflowPickup,
            "energy-cell-overflow-probe",
            "mesh/pickup-ammunition",
            [3.5, 1.5, 2.5],
            LOADING_BAY_ITEM_IDS.energyCell,
            1,
          ),
          pickupEntity(
            ENCOUNTER_IDS.ammunitionPickup,
            "scatter-shell-cache",
            "mesh/pickup-ammunition",
            [4.5, 1.5, 2.5],
            LOADING_BAY_ITEM_IDS.scatterShell,
            12,
          ),
          pickupEntity(
            ENCOUNTER_IDS.weaponPickup,
            "breach-scattergun-pickup",
            "mesh/pickup-weapon",
            [5.5, 1.5, 2.5],
            LOADING_BAY_ITEM_IDS.breachScattergun,
            1,
            {
              item: LOADING_BAY_ITEM_IDS.scatterShell,
              quantity: 8,
            },
          ),
          pickupEntity(
            ENCOUNTER_IDS.healthPickup,
            "med-patch-pickup",
            "mesh/pickup-health",
            [6.5, 1.5, 2.5],
            LOADING_BAY_ITEM_IDS.medPatch,
            1,
          ),
          pickupEntity(
            ENCOUNTER_IDS.armorPickup,
            "impact-vest-pickup",
            "mesh/pickup-armor",
            [6.5, 1.5, 3.5],
            LOADING_BAY_ITEM_IDS.impactVest,
            1,
          ),
          pickupEntity(
            ENCOUNTER_IDS.keyPickup,
            "maintenance-pass-pickup",
            "mesh/pickup-key",
            [2.5, 1.5, 3.5],
            LOADING_BAY_ITEM_IDS.maintenancePass,
            1,
          ),
          {
            id: ENCOUNTER_IDS.hazard,
            name: "coolant-leak",
            translation: [1.5, 1.5, 4.5],
            bounds: {
              min: [-0.45, -0.45, -0.45],
              max: [0.45, 0.45, 0.45],
            },
            renderable: { asset: "mesh/hazard-pad", visible: true },
            hazard: { damage: 20, cooldownTicks: 60 },
          },
          pickupEntity(
            ENCOUNTER_IDS.automaticWeaponPickup,
            "rivet-carbine-pickup",
            "mesh/pickup-weapon",
            [2.5, 1.5, 3.5],
            LOADING_BAY_ITEM_IDS.rivetCarbine,
            1,
          ),
          {
            id: ENCOUNTER_IDS.keyedBulkhead,
            name: "maintenance-bulkhead",
            translation: [GENERATED_EXIT.centerX, 1.5, 5.5],
            collision: { enabled: true, staticCollider: true },
            renderable: { asset: "mesh/security-door", visible: true },
            kinematic: {
              halfExtents: [3.2, 1.5, 0.275],
              velocity: [0, 0, 0],
            },
            door: {
              openTranslation: [GENERATED_EXIT.centerX, 4.5, 5.5],
              autoCloseAfterTicks: 90,
              access: {
                requiredKey: LOADING_BAY_ITEM_IDS.maintenancePass,
                keyPolicy: "retain",
                activationRadius: 3,
                deniedPresentation: "Maintenance pass required",
              },
            },
          },
          {
            id: ENCOUNTER_IDS.secretRegion,
            name: "overlook-secret",
            translation: [6.5, 1.5, 8.5],
            bounds: {
              min: [-0.6, -0.6, -0.6],
              max: [0.6, 0.6, 0.6],
            },
            secretRegion: {
              presentation: "Secret overlook discovered",
            },
          },
          {
            id: ENCOUNTER_IDS.levelExit,
            name: "loading-bay-level-exit",
            translation: [
              GENERATED_EXIT.centerX,
              1.5,
              GENERATED_EXIT.wallZ + 1.5,
            ],
            renderable: { asset: "mesh/level-exit", visible: true },
            levelExit: {
              activationRadius: 2,
              presentation: "Loading Bay complete",
            },
          },
        ],
      },
    ],
  };
}

/** A second arrangement composed entirely from the demo's settled authored meanings. */
export function relayAnnexStoredProject(): StoredProjectContent {
  const source = loadingBayStoredProject({
    generationSeed: 17,
    navigationGoal: [1.5, 1.5, 6.5],
    navigationSpeedUnitsPerSecond: 2.5,
    enemyHealth: 80,
    weaponDamage: 40,
    beaconActivationRadius: 4,
  });
  const sourceScene = source.scenes[0];
  if (sourceScene === undefined) {
    throw new Error("loading-bay source scene is missing");
  }

  const entities = sourceScene.entities.flatMap(
    (entity): readonly EntityDefinition[] => {
      if (entity.pickup !== undefined) {
        const positions: Readonly<Record<number, Vec3>> = {
          [ENCOUNTER_IDS.energyFillPickup]: [2.5, 1.5, 2.5],
          [ENCOUNTER_IDS.energyOverflowPickup]: [3.5, 1.5, 2.5],
          [ENCOUNTER_IDS.ammunitionPickup]: [4.5, 1.5, 2.5],
          [ENCOUNTER_IDS.weaponPickup]: [5.5, 1.5, 2.5],
          [ENCOUNTER_IDS.automaticWeaponPickup]: [2.5, 1.5, 3.5],
          [ENCOUNTER_IDS.healthPickup]: [5.5, 1.5, 3.5],
          [ENCOUNTER_IDS.armorPickup]: [4.5, 1.5, 3.5],
          [ENCOUNTER_IDS.keyPickup]: [3.5, 1.5, 3.5],
        };
        const translation = positions[entity.id];
        if (translation === undefined) {
          throw new Error(`unexpected relay pickup ${entity.id}`);
        }
        return [{ ...entity, translation }];
      }
      switch (entity.id) {
        case ENCOUNTER_IDS.actor:
          return [{ ...entity, translation: [2.5, 1.5, 2.5] }];
        case ENCOUNTER_IDS.encounter:
          return [
            {
              ...entity,
              name: "relay-annex-encounter",
              encounter: {
                members: [ENCOUNTER_IDS.firstEnemy],
                exit: ENCOUNTER_IDS.exit,
              },
            },
          ];
        case ENCOUNTER_IDS.exit:
          return [
            {
              ...entity,
              name: "relay-annex-exit",
              translation: [3.5, 1, 9],
              door: { openTranslation: [3.5, 4, 9], autoCloseAfterTicks: null },
            },
          ];
        case ENCOUNTER_IDS.firstEnemy:
          return [
            {
              ...entity,
              name: "relay-warden",
              translation: [5.5, 1.5, 6.5],
            },
          ];
        case ENCOUNTER_IDS.firstEnemy + 1:
          return [];
        case ENCOUNTER_IDS.doorControl:
          return [
            {
              ...entity,
              name: "annex-door-control",
              translation: [5.5, 1.5, 8.5],
            },
          ];
        case ENCOUNTER_IDS.keyedBulkhead:
          return [
            {
              ...entity,
              translation: [3.5, 1.5, 4.5],
              door: {
                ...entity.door!,
                openTranslation: [3.5, 4.5, 4.5],
              },
            },
          ];
        case ENCOUNTER_IDS.secretRegion:
          return [{ ...entity, translation: [4.5, 1.5, 6.5] }];
        case ENCOUNTER_IDS.levelExit:
          return [{ ...entity, translation: [3.5, 1.5, 10.5] }];
        case ENCOUNTER_IDS.extractionBeacon:
          return [
            { ...entity, name: "relay-beacon", translation: [3.5, 1.5, 4.5] },
          ];
        case ENCOUNTER_IDS.motionProbe:
          return [
            {
              ...entity,
              name: "relay-pulse-probe",
              translation: [1.5, 1.5, 7.5],
              kinematic: {
                halfExtents: [0.25, 0.25, 0.25],
                velocity: [3, 0, 0],
              },
            },
          ];
        case ENCOUNTER_IDS.hazard:
          return [{ ...entity, translation: [2.5, 1.5, 4.5] }];
        default:
          throw new Error(`unexpected loading-bay entity ${entity.id}`);
      }
    },
  );

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
        voxelEnvironment: {
          kind: "generatedRoom",
          seed: 17,
          voxelSize: 1,
          chunkSize: 16,
          width: 5,
          height: 4,
          length: 8,
        },
        entities,
      },
    ],
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
