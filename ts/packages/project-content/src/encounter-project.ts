import type {
  EntityDefinition,
  PlayerInputBindingsDefinition,
  ProjectContent,
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

/**
 * Schema-6 migration fixture retained to prove predecessor admission.
 * Current game projects are durable Studio-owned JSON artifacts.
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
