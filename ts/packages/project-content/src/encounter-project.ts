import type {
  EntityDefinition,
  PlayerInputBindingsDefinition,
  ProjectContent,
  StoredProjectContent,
  Vec3,
} from "./schema.js";

export const ENCOUNTER_IDS = {
  actor: 1,
  encounter: 2,
  exit: 3,
  firstEnemy: 4,
  motionProbe: 10,
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
        translation: [GENERATED_EXIT.centerX, GENERATED_EXIT.centerY, GENERATED_EXIT.wallZ],
        collision: { enabled: true, staticCollider: true },
        renderable: { asset: "mesh/security-door", visible: true },
        kinematic: { halfExtents: GENERATED_EXIT.collisionHalfExtents, velocity: [0, 0, 0] },
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
  "encounter-gate.project.json": encounterGateProject(["sentry-alpha", "sentry-beta"]),
  "encounter-gate-solo.project.json": encounterGateProject(["sentry-alpha"]),
} as const;

/** Optional authoring frontend for the same candidate admitted canonically by Rust. */
export function loadingBayStoredProject(
  options: EncounterProjectOptions = {},
): StoredProjectContent {
  const legacy = encounterGateProject(["sentry-alpha", "sentry-beta"], options);
  const entities = legacy.entities.map((entity) => {
    const renderable = entity.renderable;
    if (renderable === undefined || !renderable.asset.startsWith("primitive/")) {
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
  const probe = entities.at(-1);
  if (probe?.id !== ENCOUNTER_IDS.motionProbe || legacy.generatedVoxelEnvironment === undefined) {
    throw new Error("loading-bay source composition is incomplete");
  }

  return {
    schemaVersion: 7,
    projectId: "loading-bay",
    name: "Loading Bay",
    entryScene: "scene/loading-bay",
    assets: [
      { id: "mesh/control-panel" },
      { id: "mesh/player-marker" },
      { id: "mesh/security-door" },
      { id: "mesh/security-sentry" },
      { id: "mesh/spatial-probe" },
    ],
    scenes: [
      {
        id: "scene/loading-bay",
        name: "Loading Bay",
        voxelEnvironment: { kind: "generatedRoom", ...legacy.generatedVoxelEnvironment },
        entities: [
          ...entities.slice(0, -1),
          {
            id: 6,
            name: "door-control",
            translation: [2.5, 1.5, 10.5],
            renderable: { asset: "mesh/control-panel", visible: true },
            switch: { controls: [ENCOUNTER_IDS.exit] },
          },
          probe,
        ],
      },
    ],
  };
}
