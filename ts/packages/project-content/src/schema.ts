export type Vec3 = readonly [number, number, number];
export type Quat = readonly [number, number, number, number];
export type VoxelAddress = readonly [number, number, number];

export interface CollisionDefinition {
  readonly enabled: boolean;
  readonly staticCollider: boolean;
}

export interface BoundsDefinition {
  readonly min: Vec3;
  readonly max: Vec3;
}

/** Presentation overrides for a node's Engine-declared renderable. */
export interface RenderableDefinition {
  readonly visible?: false;
  readonly initialClip?: string;
  readonly visualBinding?: VisualBindingDefinition;
}

export type VisualStateDefinition =
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
  | "completed"
  | "idle"
  | "moving"
  | "alert"
  | "attacking"
  | "hit"
  | "defeated";

export interface VisualBindingDefinition {
  readonly version: 1 | 2;
  readonly states: readonly VisualBindingStateDefinition[];
}

export type VisualBindingStateDefinition =
  | {
      readonly state: VisualStateDefinition;
      readonly kind: "material";
      readonly textureTint: readonly [number, number, number, number];
      readonly emissionColor: Vec3;
      readonly emissionIntensity: number;
    }
  | {
      readonly state: VisualStateDefinition;
      readonly kind: "animation";
      readonly clip: string;
      readonly loopMode: "once" | "repeat";
      readonly speed: number;
      readonly fadeSeconds: number | null;
    }
  | {
      readonly state: VisualStateDefinition;
      readonly kind: "spriteFrames";
      readonly frames: readonly number[];
      readonly ticksPerFrame: number;
      readonly loopMode: "once" | "repeat";
      readonly directionalViews?: readonly DirectionalSpriteViewDefinition[];
    };

export interface DirectionalSpriteViewDefinition {
  readonly rotation: 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8;
  readonly frames: readonly number[];
  readonly mirrored: boolean;
  readonly sourceOriginOffsets: readonly (readonly [number, number])[];
}

export interface DoorDefinition {
  readonly openTranslation: Vec3;
  readonly autoCloseAfterTicks: number | null;
  readonly motionDurationTicks?: number;
  readonly source?: string;
  readonly openPresentation?: string;
  readonly closePresentation?: string;
  readonly openSound?: string;
  readonly closeSound?: string;
  readonly access?: DoorAccessDefinition;
}

export interface DoorAccessDefinition {
  readonly requiredKey: string;
  readonly keyPolicy: "retain" | "consume";
  readonly activationRadius: number;
  readonly deniedPresentation: string;
}

export interface SwitchDefinition {
  readonly program: string;
  readonly controls: readonly number[];
  readonly activationRadius?: number;
  readonly prompt?: string;
  readonly unavailablePresentation?: string;
  readonly repeatable?: boolean;
  readonly effects?: readonly (
    | { readonly kind: "openDoor"; readonly door: number }
    | { readonly kind: "closeDoor"; readonly door: number }
  )[];
  readonly loadingBayInterlock?: {
    readonly closeDoor: number;
    readonly openDoor: number;
  };
}

export interface SecretRegionDefinition {
  readonly program: string;
  readonly presentation: string;
  readonly source?: string;
}

export interface FloorActionDefinition {
  readonly program: string;
  readonly targetPlatform: number;
  readonly upperTranslation: Vec3;
  readonly loweredTranslation: Vec3;
  readonly motionDurationTicks?: number;
  readonly prompt?: string;
  readonly presentation?: string;
  readonly source?: string;
}

export interface LiftDefinition {
  readonly program: string;
  readonly targetPlatform: number;
  readonly raisedTranslation: Vec3;
  readonly loweredTranslation: Vec3;
  readonly motionDurationTicks?: number;
  readonly loweredWaitTicks?: number;
  readonly prompt?: string;
  readonly presentation?: string;
  readonly source?: string;
}

export interface LevelExitDefinition {
  readonly program: string;
  readonly activationRadius: number;
  readonly presentation: string;
  readonly source?: string;
}

export interface EncounterDefinition {
  readonly members: readonly number[];
  /** Closed encounter lifecycle catalog id; Rust executes it against these explicit members. */
  readonly program: string;
  readonly exit?: number;
  readonly activationRadius?: number;
}

export interface EnemyDropDefinition {
  readonly pickup: number;
}

export interface ExtractionBeaconDefinition {
  readonly activationRadius: number;
}

export interface HealthDefinition {
  readonly max: number;
  readonly startingHealth?: number;
  readonly hitboxHalfExtents: Vec3;
  readonly maxArmor?: number;
  readonly armorAbsorptionPercent?: number;
}

export interface ExplosivePropDefinition {
  /** Closed Rust explosive-prop program selected for this placed prop. */
  readonly program: string;
  readonly damage: number;
  readonly radius: number;
}

export interface EnemyCombatDefinition {
  readonly sightRange: number;
  readonly hearingRange: number;
  readonly painDurationTicks?: number;
  /** Closed enemy-attack catalog id; Rust evaluates the admitted program. */
  readonly attackProgram: string;
  /** Closed enemy-defeat catalog id; Rust owns death and applies consequences. */
  readonly defeatProgram: string;
  readonly attack: {
    readonly kind: "melee" | "rangedHitscan" | "projectile";
    readonly damage: number;
    readonly range: number;
    readonly cooldownTicks: number;
    readonly originOffset: Vec3;
    readonly presentation: string;
    readonly projectile?: {
      readonly mass: number;
      readonly radius: number;
      readonly impulse: number;
      readonly gravityScale: number;
      readonly lifetimeTicks: number;
      readonly restitution: number;
      readonly visualAsset?: string;
    };
  };
}

export interface HazardDefinition {
  /** Closed Rust hazard program selected for this placed trigger. */
  readonly program: string;
  readonly damage: number;
  readonly cooldownTicks: number;
}

export interface KinematicDefinition {
  readonly halfExtents: Vec3;
  readonly velocity: Vec3;
}

export interface NavigationDefinition {
  readonly goal: Vec3;
  readonly speedUnitsPerSecond: number;
  readonly maxVisited: number;
}

export interface PlayerInputBindingsDefinition {
  readonly moveForward: string;
  readonly moveBackward: string;
  readonly moveLeft: string;
  readonly moveRight: string;
  readonly mouseLook: string;
  readonly primaryFire: string;
  readonly jump?: string;
  readonly selectWeapon?: readonly string[];
}

export interface PlayerTraversalDefinition {
  readonly maxStepHeight: number;
  readonly gravityUnitsPerSecondSquared: number;
  readonly jumpImpulseUnitsPerSecond: number;
  readonly groundProbeDistance: number;
  readonly eyeHeight: number;
  readonly manualJumpEnabled: boolean;
  readonly maxAirJumps?: number;
}

export interface PlayerControllerDefinition {
  readonly moveSpeedUnitsPerSecond: number;
  readonly moveStepSeconds: number;
  readonly lookDegreesPerUnit: number;
  readonly initialYawDegrees: number;
  readonly initialPitchDegrees: number;
  readonly traversal?: PlayerTraversalDefinition;
  readonly bindings: PlayerInputBindingsDefinition;
}

export type ItemKindDefinition =
  | {
      readonly kind: "weapon";
      readonly attackMode: "hitscan";
      readonly repeatWhileHeld?: boolean;
      readonly damageRolls?: number;
      readonly pelletCount?: never;
      readonly spreadDegrees?: never;
      readonly damage: number;
      readonly maxDistance: number;
      readonly cooldownTicks: number;
      readonly ammunition: string;
      readonly ammunitionCost: number;
      readonly muzzleOffset: Vec3;
      readonly presentation: string;
    }
  | {
      readonly kind: "weapon";
      readonly attackMode: "spread";
      readonly repeatWhileHeld?: boolean;
      readonly damageRolls?: number;
      readonly pelletCount: number;
      readonly spreadDegrees: number;
      readonly damage: number;
      readonly maxDistance: number;
      readonly cooldownTicks: number;
      readonly ammunition: string;
      readonly ammunitionCost: number;
      readonly muzzleOffset: Vec3;
      readonly presentation: string;
    }
  | { readonly kind: "ammunition" }
  | { readonly kind: "accessKey" }
  | {
      readonly kind: "healthSupply";
      readonly restoreHealth: number;
      readonly maximumHealth?: number;
      readonly automaticUse?: boolean;
      readonly consumeAtCap?: boolean;
    }
  | {
      readonly kind: "armor";
      readonly protection: number;
      readonly maximumArmor?: number;
      readonly absorptionPercent?: number;
      readonly absorptionDivisor?: number;
      readonly grantMode?: "add" | "setMinimum";
      readonly transition?: "rejectDifferent" | "preserve" | "replace";
      readonly consumeAtCap?: boolean;
    };

export interface ItemDefinition {
  readonly id: string;
  readonly maxQuantity: number;
  readonly program?: string;
  readonly kind: ItemKindDefinition;
}

export interface InventoryStackDefinition {
  readonly item: string;
  readonly quantity: number;
}

export interface InventoryDefinition {
  readonly capacitySlots: number;
  /** Closed player setup program; Rust owns its admission and execution. */
  readonly setupProgram: string;
  readonly weaponSlots: readonly string[];
}

export interface PickupDefinition {
  readonly item: string;
  readonly quantity: number;
  /** Closed pickup catalog id; Rust owns its execution against this pickup. */
  readonly program: string;
  readonly starterAmmunition?: InventoryStackDefinition;
}

export interface VoxelCollisionDefinition {
  readonly voxelSize: number;
  readonly chunkSize: number;
  readonly solidVoxels: readonly VoxelAddress[];
  readonly gameplayProxy?: boolean;
}

export interface GeneratedVoxelEnvironmentDefinition {
  readonly seed: number;
  readonly voxelSize: number;
  readonly chunkSize: number;
  readonly width: number;
  readonly height: number;
  readonly length: number;
  readonly gameplayProxy?: boolean;
}

export interface MaterialVoxelDefinition {
  readonly address: VoxelAddress;
  readonly materialSlot: number;
}

export interface MaterialVoxelEnvironmentDefinition {
  readonly voxelSize: number;
  readonly chunkSize: number;
  readonly materialVoxels: readonly MaterialVoxelDefinition[];
  readonly voxelAssets?: readonly string[];
  readonly gameplayProxy?: boolean;
}

/**
 * Downstream binding record for one authored scene node. Generic scene
 * structure (label, hierarchy, transform, renderable asset, light) lives
 * only in the Engine `authoredScene` document keyed by the same node id.
 */
export interface EntityDefinition {
  readonly id: number;
  readonly bounds?: BoundsDefinition;
  readonly collision?: CollisionDefinition;
  readonly renderable?: RenderableDefinition;
  readonly door?: DoorDefinition;
  readonly switch?: SwitchDefinition;
  readonly floorAction?: FloorActionDefinition;
  readonly lift?: LiftDefinition;
  readonly enemy?: true;
  readonly enemyCombat?: EnemyCombatDefinition;
  readonly defeatDrop?: EnemyDropDefinition;
  readonly health?: HealthDefinition;
  readonly explosiveProp?: ExplosivePropDefinition;
  readonly hazard?: HazardDefinition;
  readonly encounter?: EncounterDefinition;
  readonly extractionBeacon?: ExtractionBeaconDefinition;
  readonly kinematic?: KinematicDefinition;
  readonly navigation?: NavigationDefinition;
  readonly playerController?: PlayerControllerDefinition;
  readonly inventory?: InventoryDefinition;
  readonly pickup?: PickupDefinition;
  readonly secretRegion?: SecretRegionDefinition;
  readonly levelExit?: LevelExitDefinition;
  readonly doomSpriteInspection?: DoomSpriteInspectionDefinition;
}

export interface DoomSpriteInspectionDefinition {
  readonly family: string;
  readonly clip: string;
  readonly label: string;
  readonly sequenceOrder: number;
  readonly displayTicks: number;
}

export type LightDefinition =
  | {
      readonly kind: "ambient";
      readonly color: Vec3;
      readonly intensity: number;
      readonly enabled: boolean;
      readonly shadows: boolean;
    }
  | {
      readonly kind: "directional";
      readonly color: Vec3;
      readonly intensity: number;
      readonly enabled: boolean;
      readonly shadows: boolean;
    }
  | {
      readonly kind: "point";
      readonly color: Vec3;
      readonly intensity: number;
      readonly enabled: boolean;
      readonly range: number | null;
      readonly decay: number;
      readonly shadows: boolean;
    }
  | {
      readonly kind: "spot";
      readonly color: Vec3;
      readonly intensity: number;
      readonly enabled: boolean;
      readonly range: number | null;
      readonly decay: number;
      readonly outerAngleRadians: number;
      readonly penumbra: number;
      readonly shadows: boolean;
    };

export interface StoredAssetDefinition {
  readonly id: string;
  readonly catalog?: {
    readonly version: 1;
    readonly hash: string;
    readonly sourcePath: string;
    readonly label: string | null;
    readonly dependencies: readonly string[];
  };
  readonly animatedMesh?: {
    readonly asset: string;
    readonly runtimeFormat: "glb";
    readonly contentHash: string | null;
    readonly clips: readonly {
      readonly id: string;
      readonly name: string | null;
      readonly durationSeconds: number | null;
    }[];
    readonly defaultClip: string | null;
    readonly materialSlots: readonly {
      readonly slot: number;
      readonly material: string;
    }[];
    readonly bounds: {
      readonly min: Vec3;
      readonly max: Vec3;
    };
  };
  readonly spriteAtlas?: {
    readonly id: string;
    readonly texture: string;
    readonly frames: readonly {
      readonly frame: number;
      readonly uvMin: readonly [number, number];
      readonly uvMax: readonly [number, number];
      readonly size?: readonly [number, number];
    }[];
  };
  readonly voxelObject?: StoredVoxelObjectAssetDefinition;
}

export interface StoredVoxelObjectAssetDefinition {
  readonly schemaVersion: 1;
  readonly assetId: string;
  readonly grid: {
    readonly coordinateSystem: "rightHandedYUp";
    readonly cellSize: number;
    readonly chunkSize: number;
    readonly pivot: Vec3;
  };
  readonly bounds: {
    readonly min: readonly [number, number, number];
    readonly max: readonly [number, number, number];
  };
  readonly defaultFrame: StoredVoxelObjectFrameDefinition;
  readonly clips: readonly {
    readonly id: string;
    readonly name?: string;
    readonly framesPerSecond: number;
    readonly frames: readonly {
      readonly durationSeconds?: number;
      readonly frame: StoredVoxelObjectFrameDefinition;
    }[];
  }[];
  readonly defaultClip?: string;
  readonly materialPalette: readonly {
    readonly materialSlot: number;
    readonly materialAssetId: string;
    readonly displayName?: string;
  }[];
  readonly materialMap: readonly {
    readonly sourceMaterialSlot: number;
    readonly sourceMaterialName?: string;
    readonly voxelMaterialSlot: number;
  }[];
  readonly provenance: {
    readonly kind: "authored" | "convertedStaticMesh" | "convertedAnimatedMesh";
    readonly sourcePath: string;
    readonly sourceSha256: string;
    readonly sourceByteCount: number;
    readonly converter: string;
    readonly settingsSha256: string;
    readonly licensePath?: string;
    readonly sourceClips?: readonly {
      readonly outputClipId: string;
      readonly sourceClipName: string;
      readonly sourceAnimationIndex: number;
      readonly startMicroseconds: number;
      readonly endMicroseconds: number;
      readonly sampleRateHz: number;
      readonly includedClipEnd: boolean;
    }[];
  };
  readonly contentHash: string;
}

export interface StoredVoxelObjectFrameDefinition {
  readonly bounds: {
    readonly min: readonly [number, number, number];
    readonly max: readonly [number, number, number];
  };
  readonly representation: {
    readonly kind: "sparseRuns";
    readonly sparseRuns: readonly {
      readonly start: readonly [number, number, number];
      readonly length: number;
      readonly materialSlot: number;
    }[];
  };
  readonly voxelDataHash: string;
}

export interface StoredVoxelObjectInstanceDefinition {
  readonly ownerEntityId: number;
  readonly instanceId: string;
  readonly voxelObjectAssetId: string;
  readonly frame:
    | { readonly kind: "default" }
    | {
        readonly kind: "clip";
        readonly clipId: string;
        readonly frameIndex: number;
      };
  readonly translation: Vec3;
  readonly rotation: readonly [number, number, number, number];
  readonly scale: Vec3;
  readonly materialOverrides: readonly {
    readonly materialSlot: number;
    readonly materialAssetId: string;
  }[];
}

export type StoredVoxelEnvironmentDefinition =
  | ({ readonly kind: "solid" } & VoxelCollisionDefinition)
  | ({ readonly kind: "material" } & MaterialVoxelEnvironmentDefinition)
  | ({ readonly kind: "generatedRoom" } & GeneratedVoxelEnvironmentDefinition);

export interface AuthoredSceneTransform {
  readonly translation: Vec3;
  readonly rotation: Quat;
  readonly scale: Vec3;
}

export interface AuthoredAssetReference {
  readonly id: string;
  readonly version: { readonly req: "any" };
  readonly hash: null;
}

export interface AuthoredSceneMetadata {
  readonly name: string | null;
  readonly authoringFormatVersion: number;
}

export type AuthoredSceneLight =
  | {
      readonly kind: "ambient";
      readonly color: Vec3;
      readonly intensity: number;
      readonly enabled: boolean;
      readonly shadowIntent: "disabled" | "requested";
    }
  | {
      readonly kind: "directional";
      readonly color: Vec3;
      readonly intensity: number;
      readonly enabled: boolean;
      readonly shadowIntent: "disabled" | "requested";
    }
  | {
      readonly kind: "point";
      readonly color: Vec3;
      readonly intensity: number;
      readonly enabled: boolean;
      readonly range: number | null;
      readonly decay: number;
      readonly shadowIntent: "disabled" | "requested";
    }
  | {
      readonly kind: "spot";
      readonly color: Vec3;
      readonly intensity: number;
      readonly enabled: boolean;
      readonly range: number | null;
      readonly decay: number;
      readonly outerAngleRadians: number;
      readonly penumbra: number;
      readonly shadowIntent: "disabled" | "requested";
    };

export type AuthoredSceneKind =
  | { readonly kind: "light"; readonly light: AuthoredSceneLight }
  | { readonly kind: "staticMesh"; readonly asset: AuthoredAssetReference }
  | { readonly kind: "sprite"; readonly asset: AuthoredAssetReference }
  | { readonly kind: "emptyGroup" };

export interface AuthoredSceneNode {
  readonly id: number;
  readonly parent: number | null;
  readonly childOrder: number;
  readonly label: string | null;
  readonly tags: readonly string[];
  readonly transform: AuthoredSceneTransform;
  readonly renderableTransform?: AuthoredSceneTransform;
  readonly kind: AuthoredSceneKind;
}

export interface AuthoredSceneDocument {
  readonly schemaVersion: 5;
  readonly id: number;
  readonly revision: number;
  readonly metadata: AuthoredSceneMetadata;
  readonly dependencies: readonly AuthoredAssetReference[];
  readonly nodes: readonly AuthoredSceneNode[];
}

export interface StoredSceneDefinition {
  readonly id: string;
  readonly name: string;
  readonly voxelEnvironment?: StoredVoxelEnvironmentDefinition;
  readonly voxelObjectInstances?: readonly StoredVoxelObjectInstanceDefinition[];
  readonly authoredScene: AuthoredSceneDocument;
  readonly entities: readonly EntityDefinition[];
}

export type GameplayProgramDefinition =
  | { readonly kind: "sequence"; readonly steps: readonly GameplayProgramDefinition[] }
  | {
      readonly kind: "when";
      readonly predicate: "impactIsHit";
      readonly thenProgram: GameplayProgramDefinition;
      readonly otherwiseProgram?: GameplayProgramDefinition;
    }
  | {
      readonly kind: "operation";
      readonly operation:
        | "recordFired"
        | "consumeAmmo"
        | "applyHit"
        | "applyMiss"
        | "applySpreadImpacts"
        | "setCooldown"
        | "useHealthSupply";
    };
export interface GameplayProgramCatalogEntry {
  readonly id: string;
  readonly program: GameplayProgramDefinition;
}

export type PickupProgramDefinition =
  | { readonly kind: "sequence"; readonly steps: readonly PickupProgramDefinition[] }
  | {
      readonly kind: "when";
      readonly predicate: "weaponAlreadyOwnedWithStarterAmmunition";
      readonly thenProgram: PickupProgramDefinition;
      readonly otherwiseProgram?: PickupProgramDefinition;
    }
  | {
      readonly kind: "operation";
      readonly operation:
        | "grantPickedItem"
        | "grantStarterAmmunition"
        | "useGrantedHealthSupply"
        | "applyGrantedArmor"
        | "consumePickup";
    };

export interface PickupProgramCatalogEntry {
  readonly id: string;
  readonly program: PickupProgramDefinition;
}

export type PlayerSetupProgramOperation =
  | { readonly kind: "grantItem"; readonly item: string; readonly quantity: number }
  | { readonly kind: "equipInitialWeapon"; readonly item: string };

export interface PlayerSetupProgramCatalogEntry {
  readonly id: string;
  readonly program: readonly PlayerSetupProgramOperation[];
}

export type EnemyAttackProgramDefinition =
  | { readonly kind: "sequence"; readonly steps: readonly EnemyAttackProgramDefinition[] }
  | {
      readonly kind: "when";
      readonly predicate: "impactIsHit";
      readonly thenProgram: EnemyAttackProgramDefinition;
      readonly otherwiseProgram?: EnemyAttackProgramDefinition;
    }
  | {
      readonly kind: "operation";
      readonly operation:
        | "recordEnemyAttack"
        | "applyEnemyHit"
        | "applyEnemyMiss"
        | "spawnEnemyProjectile"
        | "setEnemyCooldown";
    };

export interface EnemyAttackProgramCatalogEntry {
  readonly id: string;
  readonly program: EnemyAttackProgramDefinition;
}

export type EnemyDefeatProgramDefinition =
  | { readonly kind: "sequence"; readonly steps: readonly EnemyDefeatProgramDefinition[] }
  | {
      readonly kind: "operation";
      readonly operation: "recordEnemyDefeat" | "activateBoundDrop";
    };

export interface EnemyDefeatProgramCatalogEntry {
  readonly id: string;
  readonly program: EnemyDefeatProgramDefinition;
}

export type HazardProgramDefinition =
  | { readonly kind: "sequence"; readonly steps: readonly HazardProgramDefinition[] }
  | {
      readonly kind: "when";
      readonly predicate: "playerOverlapping" | "playerEligible" | "cooldownReady";
      readonly thenProgram: HazardProgramDefinition;
      readonly otherwiseProgram?: HazardProgramDefinition;
    }
  | {
      readonly kind: "operation";
      readonly operation: "applyHazardDamage" | "scheduleHazardCooldown";
    };

export interface HazardProgramCatalogEntry {
  readonly id: string;
  readonly program: HazardProgramDefinition;
}

export type ExplosivePropProgramDefinition =
  | {
      readonly kind: "sequence";
      readonly steps: readonly ExplosivePropProgramDefinition[];
    }
  | {
      readonly kind: "when";
      readonly predicate: "explosionPending";
      readonly thenProgram: ExplosivePropProgramDefinition;
      readonly otherwiseProgram?: ExplosivePropProgramDefinition;
    }
  | {
      readonly kind: "operation";
      readonly operation: "selectRadialTargets" | "applyScaledDamage" | "resolveExplosion";
    };

export interface ExplosivePropProgramCatalogEntry {
  readonly id: string;
  readonly program: ExplosivePropProgramDefinition;
}

export type SwitchProgramDefinition =
  | { readonly kind: "sequence"; readonly steps: readonly SwitchProgramDefinition[] }
  | {
      readonly kind: "when";
      readonly predicate: "switchAvailable";
      readonly thenProgram: SwitchProgramDefinition;
      readonly otherwiseProgram?: SwitchProgramDefinition;
    }
  | {
      readonly kind: "operation";
      readonly operation:
        | "recordActivation"
        | "requestOpenBoundDoor"
        | "requestCloseBoundDoor"
        | "emitInteractionFeedback";
    };

export interface SwitchProgramCatalogEntry {
  readonly id: string;
  readonly program: SwitchProgramDefinition;
}

export type FloorActionProgramDefinition =
  | { readonly kind: "sequence"; readonly steps: readonly FloorActionProgramDefinition[] }
  | { readonly kind: "when"; readonly predicate: "activationEntered" | "loweringMotionTick"; readonly thenProgram: FloorActionProgramDefinition; readonly otherwiseProgram?: FloorActionProgramDefinition }
  | { readonly kind: "operation"; readonly operation: "recordActivation" | "requestLowerBoundPlatform" | "advanceLowering" | "emitFloorFeedback" };
export interface FloorActionProgramCatalogEntry { readonly id: string; readonly program: FloorActionProgramDefinition; }

export type LiftProgramDefinition =
  | { readonly kind: "sequence"; readonly steps: readonly LiftProgramDefinition[] }
  | { readonly kind: "when"; readonly predicate: "activationEntered" | "loweringMotionTick" | "waitingTick" | "raisingMotionTick"; readonly thenProgram: LiftProgramDefinition; readonly otherwiseProgram?: LiftProgramDefinition }
  | { readonly kind: "operation"; readonly operation: "recordActivation" | "requestLowerBoundPlatform" | "advanceLowering" | "advanceWait" | "advanceRaising" | "emitLiftFeedback" };
export interface LiftProgramCatalogEntry { readonly id: string; readonly program: LiftProgramDefinition; }

export type EncounterActivationProgramDefinition =
  | { readonly kind: "sequence"; readonly steps: readonly EncounterActivationProgramDefinition[] }
  | { readonly kind: "when"; readonly predicate: "activationEligible"; readonly thenProgram: EncounterActivationProgramDefinition; readonly otherwiseProgram?: EncounterActivationProgramDefinition }
  | { readonly kind: "operation"; readonly operation: "recordEncounterActivation" | "activateBoundMembers" | "emitEncounterFeedback" };
export type EncounterClearProgramDefinition =
  | { readonly kind: "sequence"; readonly steps: readonly EncounterClearProgramDefinition[] }
  | { readonly kind: "when"; readonly predicate: "membersDefeated"; readonly thenProgram: EncounterClearProgramDefinition; readonly otherwiseProgram?: EncounterClearProgramDefinition }
  | { readonly kind: "operation"; readonly operation: "recordEncounterCleared" | "openBoundExit" };
export interface EncounterProgramCatalogEntry {
  readonly id: string;
  readonly activation: EncounterActivationProgramDefinition;
  readonly clear: EncounterClearProgramDefinition;
}

export type SecretProgramDefinition =
  | { readonly kind: "sequence"; readonly steps: readonly SecretProgramDefinition[] }
  | { readonly kind: "when"; readonly predicate: "secretRegionEntered" | "secretUndiscovered"; readonly thenProgram: SecretProgramDefinition; readonly otherwiseProgram?: SecretProgramDefinition }
  | { readonly kind: "operation"; readonly operation: "recordDiscovery" | "emitSecretPresentation" };
export interface SecretProgramCatalogEntry { readonly id: string; readonly program: SecretProgramDefinition; }

export type LevelExitProgramDefinition =
  | { readonly kind: "sequence"; readonly steps: readonly LevelExitProgramDefinition[] }
  | { readonly kind: "when"; readonly predicate: "exitAvailable"; readonly thenProgram: LevelExitProgramDefinition; readonly otherwiseProgram?: LevelExitProgramDefinition }
  | { readonly kind: "operation"; readonly operation: "recordCompletion" | "emitCompletionPresentation" };
export interface LevelExitProgramCatalogEntry { readonly id: string; readonly program: LevelExitProgramDefinition; }

export interface StoredProjectContent {
  readonly schemaVersion: 28;
  readonly projectId: string;
  readonly name: string;
  readonly entryScene: string;
  readonly assets: readonly StoredAssetDefinition[];
  readonly itemDefinitions: readonly ItemDefinition[];
  readonly gameplayPrograms: readonly GameplayProgramCatalogEntry[];
  readonly pickupPrograms: readonly PickupProgramCatalogEntry[];
  readonly playerSetupPrograms: readonly PlayerSetupProgramCatalogEntry[];
  readonly enemyAttackPrograms: readonly EnemyAttackProgramCatalogEntry[];
  readonly enemyDefeatPrograms: readonly EnemyDefeatProgramCatalogEntry[];
  readonly hazardPrograms: readonly HazardProgramCatalogEntry[];
  readonly explosivePropPrograms: readonly ExplosivePropProgramCatalogEntry[];
  readonly encounterPrograms: readonly EncounterProgramCatalogEntry[];
  readonly switchPrograms: readonly SwitchProgramCatalogEntry[];
  readonly floorActionPrograms: readonly FloorActionProgramCatalogEntry[];
  readonly liftPrograms: readonly LiftProgramCatalogEntry[];
  readonly secretPrograms: readonly SecretProgramCatalogEntry[];
  readonly levelExitPrograms: readonly LevelExitProgramCatalogEntry[];
  readonly scenes: readonly StoredSceneDefinition[];
}
