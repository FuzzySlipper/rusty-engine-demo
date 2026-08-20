type RenderFrameDiff = Readonly<Record<string, unknown>>;
type RenderMaterialDescriptor = Readonly<Record<string, unknown>>;
type StaticMeshAsset = Readonly<Record<string, unknown>>;

export interface RuntimeApplicationResource {
  readonly identity: string;
  readonly contentHash: string;
  readonly mediaType: "application/octet-stream" | "image/png";
  readonly byteLength: number;
  readonly resourceUrl: string;
}

export interface RuntimeApplicationContent {
  readonly frame: RenderFrameDiff;
  readonly resources: readonly RuntimeApplicationResource[];
}

export type RuntimeVisualState =
  | "default"
  | "open"
  | "opening"
  | "closed"
  | "closing"
  | "active"
  | "inactive"
  | "standby"
  | "available"
  | "dormant"
  | "collected"
  | "cooling"
  | "completed";

export interface RuntimeProjectionNode {
  readonly id: number;
  readonly name: string;
  readonly asset: string;
  readonly translation: readonly [number, number, number] | null;
  readonly rotation?: readonly [number, number, number, number];
  readonly scale?: readonly [number, number, number];
  readonly visible: boolean;
  readonly visualState: RuntimeVisualState;
}

export type RuntimeBoundVisualState =
  | RuntimeVisualState
  | "idle"
  | "moving"
  | "alert"
  | "attacking"
  | "hit"
  | "defeated";

export interface RuntimeAnimatedMeshResource {
  readonly asset: string;
  readonly resourceUrl: string;
  readonly [field: string]: unknown;
}

export interface RuntimeVisualBindingResource {
  readonly entity: number;
  readonly binding: {
    readonly version: 1;
    readonly states: readonly RuntimeVisualBindingState[];
  };
}

export type RuntimeVisualBindingState =
  | {
      readonly state: RuntimeBoundVisualState;
      readonly kind: "material";
      readonly textureTint: readonly [number, number, number, number];
      readonly emissionColor: readonly [number, number, number];
      readonly emissionIntensity: number;
    }
  | {
      readonly state: RuntimeBoundVisualState;
      readonly kind: "animation";
      readonly clip: string;
      readonly loopMode: "once" | "repeat";
      readonly speed: number;
      readonly fadeSeconds: number | null;
    };

export interface RuntimeEnemyState {
  readonly id: number;
  readonly name: string;
  readonly state: "alive" | "defeated";
  readonly position: readonly [number, number, number];
  readonly currentHealth: number;
  readonly maxHealth: number;
  readonly combatPosture:
    | "sleeping"
    | "alert"
    | "pursuing"
    | "attacking"
    | "dead"
    | null;
  readonly attackKind: "melee" | "rangedHitscan" | "projectile" | null;
}

export interface RuntimePlayerBindings {
  readonly moveForward: string;
  readonly moveBackward: string;
  readonly moveLeft: string;
  readonly moveRight: string;
  readonly mouseLook: string;
  readonly primaryFire: string;
  readonly jump?: string | null;
  readonly selectWeapon: readonly string[];
}

export interface RuntimePlayerState {
  readonly id: number;
  readonly position: readonly [number, number, number];
  readonly yawDegrees: number;
  readonly pitchDegrees: number;
  readonly moveStepSeconds: number;
  readonly lookDegreesPerUnit: number;
  readonly grounded?: boolean;
  readonly verticalVelocity?: number;
  readonly eyeHeight?: number;
  readonly bindings: RuntimePlayerBindings;
  readonly currentHealth: number;
  readonly maxHealth: number;
  readonly armor: number;
  readonly maxArmor: number;
  readonly vitalityState: "alive" | "dead";
}

export interface RuntimeWeaponState {
  readonly item: string;
  readonly presentation: string;
  readonly damage: number;
  readonly ammunition: string;
  readonly ammunitionCost: number;
  readonly ammoRemaining: number;
  readonly ammoCapacity: number;
  readonly readyAtTick: number;
}

export interface RuntimeInventoryStack {
  readonly item: string;
  readonly quantity: number;
}

export interface RuntimeInventoryWeapon {
  readonly slot: number;
  readonly item: string;
  readonly owned: boolean;
  readonly selected: boolean;
  readonly ammunition: string;
  readonly ammunitionQuantity: number;
}

export interface RuntimeInventoryState {
  readonly owner: number;
  readonly capacitySlots: number;
  readonly stacks: readonly RuntimeInventoryStack[];
  readonly equippedWeapon: string | null;
  readonly weapons: readonly RuntimeInventoryWeapon[];
}

export interface RuntimePickupState {
  readonly id: number;
  readonly item: string;
  readonly quantity: number;
  readonly state: "dormant" | "available" | "collected";
  readonly collectedBy: number | null;
  readonly collectedAtTick: number | null;
  readonly collectionCause: "overlap" | "interaction" | null;
}

export interface RuntimeHazardState {
  readonly id: number;
  readonly damage: number;
  readonly cooldownTicks: number;
  readonly readyAtTick: number;
}

export interface RuntimeRestartState {
  readonly authoredBaselineAvailable: boolean;
  readonly checkpointAvailable: boolean;
}

export interface RuntimeInputSessionState {
  readonly connectionGeneration: number;
  readonly connected: boolean;
  readonly paused: boolean;
  readonly acknowledgedSequence: number;
  readonly consumedSequence: number;
  readonly queuedEdgeCommands: number;
}

export interface RuntimeExtractionBeaconState {
  readonly id: number;
  readonly state: "standby" | "active";
  readonly activationRadius: number;
  readonly activatedBy: number | null;
  readonly activatedAtTick: number | null;
}

export interface RuntimeDoorAccessState {
  readonly id: number;
  readonly state: "closed" | "opening" | "open" | "closing";
  readonly requiredKey: string;
  readonly keyPolicy: "retain" | "consume";
  readonly activationRadius: number;
  readonly deniedPresentation: string;
}

export interface RuntimeSecretRegionState {
  readonly id: number;
  readonly state: "undiscovered" | "discovered";
  readonly presentation: string;
}

export interface RuntimeFloorActionState {
  readonly id: number;
  readonly targetPlatform: number;
  readonly state: "armed" | "lowering" | "lowered";
  readonly motionElapsedTicks: number;
  readonly prompt: string;
  readonly presentation: string;
  readonly source: string;
}

export interface RuntimeLiftState {
  readonly id: number;
  readonly targetPlatform: number;
  readonly state: "raised" | "lowering" | "waiting" | "raising";
  readonly motionElapsedTicks: number;
  readonly waitElapsedTicks: number;
  readonly prompt: string;
  readonly presentation: string;
  readonly source: string;
}

export interface RuntimeLevelExitState {
  readonly id: number;
  readonly state: "available" | "completed";
  readonly activationRadius: number;
  readonly presentation: string;
  readonly completedBy: number | null;
  readonly completedAtTick: number | null;
}

export interface RuntimeInteractionState {
  readonly target: number;
  readonly prompt: string;
}

export interface DerivedCameraPose {
  readonly position: readonly [number, number, number];
  readonly yawDegrees: number;
  readonly pitchDegrees: number;
}

export interface RuntimeVoxelMeshGroup {
  readonly materialSlot: number;
  readonly start: number;
  readonly count: number;
}

export interface RuntimeVoxelMeshChunk {
  readonly chunk: readonly [number, number, number];
  readonly contentHash: string;
  readonly translation: readonly [number, number, number];
  readonly positions: readonly number[];
  readonly normals: readonly number[];
  readonly indices: readonly number[];
  readonly groups: readonly RuntimeVoxelMeshGroup[];
  readonly boundsMin: readonly [number, number, number];
  readonly boundsMax: readonly [number, number, number];
}

export interface RuntimeGeneratedEnvironment {
  readonly seed: number;
  readonly outputHash: string;
  readonly solidVoxels: number;
  readonly meshVertices: number;
  readonly meshQuads: number;
}

export type RuntimeAuthoredLightDefinition =
  | {
      readonly kind: "ambient";
      readonly color: readonly [number, number, number];
      readonly intensity: number;
      readonly enabled: boolean;
      readonly shadows: boolean;
    }
  | {
      readonly kind: "directional";
      readonly color: readonly [number, number, number];
      readonly intensity: number;
      readonly enabled: boolean;
      readonly shadows: boolean;
    }
  | {
      readonly kind: "point";
      readonly color: readonly [number, number, number];
      readonly intensity: number;
      readonly enabled: boolean;
      readonly range: number | null;
      readonly decay: number;
      readonly shadows: boolean;
    }
  | {
      readonly kind: "spot";
      readonly color: readonly [number, number, number];
      readonly intensity: number;
      readonly enabled: boolean;
      readonly range: number | null;
      readonly decay: number;
      readonly outerAngleRadians: number;
      readonly penumbra: number;
      readonly shadows: boolean;
    };

export interface RuntimeAuthoredLight {
  readonly id: number;
  readonly translation: readonly [number, number, number] | null;
  readonly rotation: readonly [number, number, number, number];
  readonly light: RuntimeAuthoredLightDefinition;
}

export interface RuntimeAnimationState {
  readonly entity: number;
  readonly posture:
    | "idle"
    | "moving"
    | "alert"
    | "attacking"
    | "defeated"
    | "open"
    | "opening"
    | "closed"
    | "closing"
    | "standby"
    | "active";
}

export type RuntimeFeedbackCue =
  | {
      readonly kind: "movement";
      readonly entity: number;
      readonly from: readonly [number, number, number];
      readonly to: readonly [number, number, number];
    }
  | { readonly kind: "movementBlocked"; readonly entity: number }
  | {
      readonly kind: "attack";
      readonly attacker: number;
      readonly weapon: string;
      readonly presentation: string;
      readonly attackMode: "hitscan" | "spread";
      readonly rayCount: number;
      readonly origin: readonly [number, number, number];
      readonly direction: readonly [number, number, number];
    }
  | {
      readonly kind: "dryFire";
      readonly attacker: number;
      readonly weapon: string;
      readonly presentation: string;
    }
  | {
      readonly kind: "attackHit";
      readonly attacker: number;
      readonly target: number;
    }
  | {
      readonly kind: "attackMissed";
      readonly attacker: number;
      readonly reason: "noTarget" | "worldBlocked";
    }
  | {
      readonly kind: "damage";
      readonly attacker: number;
      readonly target: number;
      readonly amount: number;
      readonly remaining: number;
      readonly direction: "front" | "left" | "right";
    }
  | {
      readonly kind: "enemyAlert";
      readonly entity: number;
      readonly target: number;
      readonly cause: "sight" | "hearing";
    }
  | {
      readonly kind: "enemyAttack";
      readonly attacker: number;
      readonly target: number;
      readonly attackKind: "melee" | "rangedHitscan" | "projectile";
      readonly presentation: string;
      readonly origin: readonly [number, number, number];
      readonly targetPosition: readonly [number, number, number];
    }
  | {
      readonly kind: "enemyAttackMissed";
      readonly attacker: number;
      readonly target: number;
      readonly reason: "worldBlocked" | "targetOutOfRange" | "targetDead";
    }
  | {
      readonly kind: "defeat";
      readonly attacker: number | null;
      readonly entity: number;
    }
  | {
      readonly kind: "enemyDropMaterialized";
      readonly enemy: number;
      readonly pickup: number;
      readonly item: string;
      readonly quantity: number;
      readonly position: readonly [number, number, number];
    }
  | {
      readonly kind: "encounterActivated";
      readonly entity: number;
      readonly player: number;
    }
  | {
      readonly kind: "doorChanged";
      readonly entity: number;
      readonly state: "open" | "closed";
    }
  | {
      readonly kind: "switchActivated";
      readonly entity: number;
      readonly actor: number;
    }
  | {
      readonly kind: "checkpoint";
      readonly player: number;
      readonly action: "saved" | "restored";
    }
  | {
      readonly kind: "extractionBeaconActivated";
      readonly entity: number;
      readonly actor: number;
    }
  | {
      readonly kind: "pickupCollected";
      readonly entity: number;
      readonly actor: number;
      readonly item: string;
      readonly quantity: number;
    }
  | {
      readonly kind: "doorAccessGranted";
      readonly entity: number;
      readonly actor: number;
      readonly requiredKey: string;
      readonly keyConsumed: boolean;
    }
  | {
      readonly kind: "doorAccessDenied";
      readonly entity: number;
      readonly requiredKey: string;
      readonly presentation: string;
    }
  | {
      readonly kind: "secretDiscovered";
      readonly entity: number;
      readonly actor: number;
      readonly presentation: string;
    }
  | {
      readonly kind: "levelCompleted";
      readonly entity: number;
      readonly actor: number;
      readonly presentation: string;
    };

export interface RuntimePresentationState {
  readonly animationStates: readonly RuntimeAnimationState[];
  readonly cues: readonly RuntimeFeedbackCue[];
}

export interface RuntimeDoomSpriteInspectionState {
  readonly entity: number;
  readonly family: string;
  readonly clip: string;
  readonly label: string;
  readonly sequenceIndex: number;
  readonly sequenceCount: number;
  readonly elapsedTicks: number;
  readonly displayTicks: number;
  readonly frame: number;
  readonly frameIndex: number;
  readonly frameCount: number;
  readonly loopMode: "once" | "repeat";
}

export type RuntimeSaveSlotId = "checkpoint" | "slot1" | "slot2" | "slot3";

export type RuntimeSaveSlotCompatibility =
  | "empty"
  | "available"
  | "corrupt"
  | "incompatible";

export interface RuntimeSaveGameMetadata {
  readonly revision: number;
  readonly savedAtUnixMilliseconds: number;
  readonly displayName: string;
  readonly tick: number;
  readonly snapshotSchemaVersion: number;
  readonly playerState: "alive" | "dead" | "unavailable";
  readonly levelComplete: boolean;
}

export interface RuntimeSaveProjectIdentity {
  readonly projectId: string;
  readonly entryScene: string;
  readonly playerEntity: number;
  readonly projectSchemaVersion: number;
  readonly contentRevision: string;
}

export interface RuntimeSaveSlotSummary {
  readonly slot: RuntimeSaveSlotId;
  readonly compatibility: RuntimeSaveSlotCompatibility;
  readonly storageRevision: string | null;
  readonly metadata: RuntimeSaveGameMetadata | null;
  readonly project: RuntimeSaveProjectIdentity | null;
  readonly diagnostic: string | null;
}

/** Read-only bounded description of Loading Bay's admitted closed programs. */
export interface RuntimeGameplayPrograms {
  readonly programs: readonly RuntimeGameplayProgramShape[];
  readonly bindings: readonly RuntimeGameplayProgramBinding[];
}

export interface RuntimeGameplayProgramShape {
  readonly id: string;
  readonly steps: readonly string[];
}

export interface RuntimeGameplayProgramBinding {
  readonly item: string;
  readonly programId: string;
}

/** Static, family-specific pickup catalog; it is not a live control path. */
export interface RuntimePickupPrograms {
  readonly programs: readonly RuntimePickupProgramShape[];
  readonly bindings: readonly RuntimePickupProgramBinding[];
}

export interface RuntimePickupProgramShape {
  readonly id: string;
  readonly steps: readonly string[];
}

export interface RuntimePickupProgramBinding {
  readonly pickup: number;
  readonly programId: string;
}

/** Static player initialization catalog; it is never a live control surface. */
export interface RuntimePlayerSetupPrograms {
  readonly programs: readonly RuntimePlayerSetupProgramShape[];
  readonly bindings: readonly RuntimePlayerSetupProgramBinding[];
}

export interface RuntimePlayerSetupProgramShape {
  readonly id: string;
  readonly operations: readonly RuntimePlayerSetupOperation[];
}

export type RuntimePlayerSetupOperation =
  | { readonly kind: "grantItem"; readonly item: string; readonly quantity: number }
  | { readonly kind: "equipInitialWeapon"; readonly item: string };

export interface RuntimePlayerSetupProgramBinding {
  readonly player: number;
  readonly programId: string;
}

/** Static, family-specific enemy program catalog; never a live control path. */
export interface RuntimeEnemyPrograms {
  readonly attack: RuntimeEnemyAttackPrograms;
  readonly defeat: RuntimeEnemyDefeatPrograms;
}

/** Static closed hazard programs and placed trigger bindings. */
export interface RuntimeHazardPrograms {
  readonly programs: readonly RuntimeEnvironmentalProgramShape[];
  readonly bindings: readonly RuntimeHazardProgramBinding[];
}

/** Static closed explosive-prop programs and placed prop bindings. */
export interface RuntimeExplosivePropPrograms {
  readonly programs: readonly RuntimeEnvironmentalProgramShape[];
  readonly bindings: readonly RuntimeExplosivePropProgramBinding[];
}

/** Static closed encounter lifecycle programs and explicit encounter bindings. */
export interface RuntimeEncounterPrograms {
  readonly programs: readonly RuntimeEncounterProgramShape[];
  readonly bindings: readonly RuntimeEncounterProgramBinding[];
}

export interface RuntimeEncounterProgramShape {
  readonly id: string;
  readonly activationSteps: readonly string[];
  readonly clearSteps: readonly string[];
}

export interface RuntimeEncounterProgramBinding {
  readonly encounter: number;
  readonly programId: string;
}

export interface RuntimeSwitchPrograms {
  readonly programs: readonly RuntimeSwitchProgramShape[];
  readonly bindings: readonly RuntimeSwitchProgramBinding[];
}

/** Static closed floor-action programs and trigger bindings. */
export interface RuntimeFloorActionPrograms {
  readonly programs: readonly RuntimeEnvironmentalProgramShape[];
  readonly bindings: readonly RuntimeFloorActionProgramBinding[];
}
export interface RuntimeFloorActionProgramBinding { readonly floorAction: number; readonly programId: string; }

/** Static closed lift-cycle programs and trigger bindings. */
export interface RuntimeLiftPrograms {
  readonly programs: readonly RuntimeEnvironmentalProgramShape[];
  readonly bindings: readonly RuntimeLiftProgramBinding[];
}
export interface RuntimeLiftProgramBinding { readonly lift: number; readonly programId: string; }

/** Static closed secret-discovery programs and region bindings. */
export interface RuntimeSecretPrograms {
  readonly programs: readonly RuntimeEnvironmentalProgramShape[];
  readonly bindings: readonly RuntimeSecretProgramBinding[];
}
export interface RuntimeSecretProgramBinding { readonly secret: number; readonly programId: string; }

/** Static closed level-exit completion programs and exit bindings. */
export interface RuntimeLevelExitPrograms {
  readonly programs: readonly RuntimeEnvironmentalProgramShape[];
  readonly bindings: readonly RuntimeLevelExitProgramBinding[];
}
export interface RuntimeLevelExitProgramBinding { readonly exit: number; readonly programId: string; }

export interface RuntimeSwitchProgramShape {
  readonly id: string;
  readonly steps: readonly string[];
}

export interface RuntimeSwitchProgramBinding {
  readonly switch: number;
  readonly programId: string;
}

export interface RuntimeEnvironmentalProgramShape {
  readonly id: string;
  readonly steps: readonly string[];
}

export interface RuntimeHazardProgramBinding {
  readonly hazard: number;
  readonly programId: string;
}

export interface RuntimeExplosivePropProgramBinding {
  readonly explosiveProp: number;
  readonly programId: string;
}

export interface RuntimeEnemyAttackPrograms {
  readonly programs: readonly RuntimeEnemyProgramShape[];
  readonly bindings: readonly RuntimeEnemyProgramBinding[];
}

export interface RuntimeEnemyDefeatPrograms {
  readonly programs: readonly RuntimeEnemyProgramShape[];
  readonly bindings: readonly RuntimeEnemyProgramBinding[];
}

export interface RuntimeEnemyProgramShape {
  readonly id: string;
  readonly steps: readonly string[];
}

export interface RuntimeEnemyProgramBinding {
  readonly enemy: number;
  readonly programId: string;
}

export interface RuntimeGameplayOutcome {
  readonly programId: string;
  readonly status: "applied" | "rejected";
  readonly plannedOperations: readonly string[];
  readonly executedOperations: readonly string[];
  readonly effects: readonly string[];
  readonly rejectionReason?: string;
}

export interface RuntimeBrowserState {
  readonly hostSessionId: string;
  readonly projectId: string;
  readonly tick: number;
  readonly entityRevision: number;
  readonly gameplayFrame: RenderFrameDiff;
  readonly voxelRevision: number;
  readonly voxelAuthorityHash: string;
  readonly voxelSolidCount: number;
  readonly voxelNavigationHash: string;
  readonly voxelProbePathLength: number;
  readonly projection: readonly RuntimeProjectionNode[];
  readonly doorState: "closed" | "opening" | "open" | "closing";
  readonly encounterState: "dormant" | "active" | "cleared";
  readonly motionState: "moving" | "blocked";
  readonly navigationState: "following" | "arrived" | "blocked" | "unreachable";
  readonly playerMotionState: "idle" | "moved" | "blocked";
  readonly combatState: "ready" | "hit" | "missed";
  readonly enemyAwarenessEnabled: boolean;
  readonly input: RuntimeInputSessionState;
  readonly player: RuntimePlayerState;
  readonly weapon: RuntimeWeaponState;
  readonly inventory: RuntimeInventoryState | null;
  readonly pickups: readonly RuntimePickupState[];
  readonly hazards: readonly RuntimeHazardState[];
  readonly restart: RuntimeRestartState;
  readonly saveSlots: readonly RuntimeSaveSlotSummary[];
  readonly extractionBeacon: RuntimeExtractionBeaconState | null;
  readonly doorAccess: readonly RuntimeDoorAccessState[];
  readonly secretRegions: readonly RuntimeSecretRegionState[];
  readonly floorActions: readonly RuntimeFloorActionState[];
  readonly lifts: readonly RuntimeLiftState[];
  readonly levelExits: readonly RuntimeLevelExitState[];
  readonly levelComplete: boolean;
  readonly interaction: RuntimeInteractionState | null;
  readonly voxelEnvironmentRole: "visible" | "gameplayProxy" | "none";
  readonly voxelMeshes: readonly RuntimeVoxelMeshChunk[];
  readonly voxelObjectFrame: RenderFrameDiff;
  readonly lights: readonly RuntimeAuthoredLight[];
  readonly renderMaterials: readonly RenderMaterialDescriptor[];
  readonly staticMeshes: readonly StaticMeshAsset[];
  readonly animatedMeshes: readonly RuntimeAnimatedMeshResource[];
  readonly visualBindings: readonly RuntimeVisualBindingResource[];
  readonly generatedEnvironment: RuntimeGeneratedEnvironment | null;
  readonly applicationContent?: RuntimeApplicationContent | null;
  readonly gameplayPrograms: RuntimeGameplayPrograms;
  readonly pickupPrograms: RuntimePickupPrograms;
  readonly playerSetupPrograms: RuntimePlayerSetupPrograms;
  readonly enemyPrograms: RuntimeEnemyPrograms;
  readonly hazardPrograms: RuntimeHazardPrograms;
  readonly explosivePropPrograms: RuntimeExplosivePropPrograms;
  readonly encounterPrograms: RuntimeEncounterPrograms;
  readonly switchPrograms: RuntimeSwitchPrograms;
  readonly floorActionPrograms: RuntimeFloorActionPrograms;
  readonly liftPrograms: RuntimeLiftPrograms;
  readonly secretPrograms: RuntimeSecretPrograms;
  readonly levelExitPrograms: RuntimeLevelExitPrograms;
  readonly enemies: readonly RuntimeEnemyState[];
  readonly presentation: RuntimePresentationState;
  readonly doomSpriteInspection: RuntimeDoomSpriteInspectionState | null;
  readonly gameplayOutcome: RuntimeGameplayOutcome | null;
  readonly lastEvents: readonly string[];
  readonly voxelEditReceipt?: RuntimeVoxelEditReceipt;
}

export interface RuntimeVoxelEditReceipt {
  readonly revisionBefore: number;
  readonly acceptedRevision: number;
  readonly changedVoxels: number;
  readonly changedMin: readonly [number, number, number];
  readonly changedMaxInclusive: readonly [number, number, number];
  readonly authorityHash: string;
  readonly persistedToProject: boolean;
}

/** Legacy gameplay eye height above the accepted Rust player origin. */
export const GAMEPLAY_CAMERA_EYE_HEIGHT = 1.2;

/** Presentation-only first-person camera rebuilt from the accepted Rust player pose. */
export function derivePlayerCameraPose(
  player: RuntimePlayerState,
  eyeHeight = player.eyeHeight ?? GAMEPLAY_CAMERA_EYE_HEIGHT,
): DerivedCameraPose {
  return {
    position: [
      player.position[0],
      player.position[1] + eyeHeight,
      player.position[2],
    ],
    yawDegrees: player.yawDegrees,
    pitchDegrees: player.pitchDegrees,
  };
}

const ENTITY_HANDLE_OFFSET = 100_000;
const LIGHT_HANDLE_OFFSET = 400_000;
const FIRST_VOXEL_MESH_HANDLE = 800_000;
