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

export interface RenderableDefinition {
  readonly asset: string;
  readonly visible: boolean;
}

export interface DoorDefinition {
  readonly openTranslation: Vec3;
  readonly autoCloseAfterTicks: number | null;
  readonly access?: DoorAccessDefinition;
}

export interface DoorAccessDefinition {
  readonly requiredKey: string;
  readonly keyPolicy: "retain" | "consume";
  readonly activationRadius: number;
  readonly deniedPresentation: string;
}

export interface SwitchDefinition {
  readonly controls: readonly number[];
  readonly loadingBayInterlock?: {
    readonly closeDoor: number;
    readonly openDoor: number;
  };
}

export interface SecretRegionDefinition {
  readonly presentation: string;
}

export interface LevelExitDefinition {
  readonly activationRadius: number;
  readonly presentation: string;
}

export interface EncounterDefinition {
  readonly members: readonly number[];
  readonly exit: number;
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
  readonly hitboxHalfExtents: Vec3;
  readonly maxArmor?: number;
  readonly armorAbsorptionPercent?: number;
}

export interface EnemyCombatDefinition {
  readonly sightRange: number;
  readonly hearingRange: number;
  readonly attack: {
    readonly kind: "melee" | "rangedHitscan";
    readonly damage: number;
    readonly range: number;
    readonly cooldownTicks: number;
    readonly originOffset: Vec3;
    readonly presentation: string;
  };
}

export interface HazardDefinition {
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
  readonly selectWeapon?: readonly string[];
}

export interface PlayerControllerDefinition {
  readonly moveSpeedUnitsPerSecond: number;
  readonly moveStepSeconds: number;
  readonly lookDegreesPerUnit: number;
  readonly initialYawDegrees: number;
  readonly initialPitchDegrees: number;
  readonly bindings: PlayerInputBindingsDefinition;
}

export interface WeaponDefinition {
  readonly damage: number;
  readonly maxDistance: number;
  readonly cooldownTicks: number;
  readonly ammoCapacity: number;
  readonly muzzleOffset: Vec3;
}

export type ItemKindDefinition =
  | {
      readonly kind: "weapon";
      readonly attackMode: "hitscan" | "automatic";
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
  | { readonly kind: "healthSupply"; readonly restoreHealth: number }
  | { readonly kind: "armor"; readonly protection: number };

export interface ItemDefinition {
  readonly id: string;
  readonly maxQuantity: number;
  readonly kind: ItemKindDefinition;
}

export interface InventoryStackDefinition {
  readonly item: string;
  readonly quantity: number;
}

export interface InventoryDefinition {
  readonly capacitySlots: number;
  readonly startingStacks: readonly InventoryStackDefinition[];
  readonly initiallyEquippedWeapon: string | null;
  readonly weaponSlots: readonly string[];
}

export interface PickupDefinition {
  readonly item: string;
  readonly quantity: number;
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

export interface EntityDefinition {
  readonly id: number;
  readonly name: string;
  readonly parent?: number;
  readonly childOrder?: number;
  readonly translation?: Vec3;
  readonly rotation?: Quat;
  readonly scale?: Vec3;
  readonly light?: LightDefinition;
  readonly bounds?: BoundsDefinition;
  readonly collision?: CollisionDefinition;
  readonly renderable?: RenderableDefinition;
  readonly door?: DoorDefinition;
  readonly switch?: SwitchDefinition;
  readonly enemy?: true;
  readonly enemyCombat?: EnemyCombatDefinition;
  readonly defeatDrop?: EnemyDropDefinition;
  readonly health?: HealthDefinition;
  readonly hazard?: HazardDefinition;
  readonly encounter?: EncounterDefinition;
  readonly extractionBeacon?: ExtractionBeaconDefinition;
  readonly kinematic?: KinematicDefinition;
  readonly navigation?: NavigationDefinition;
  readonly playerController?: PlayerControllerDefinition;
  readonly inventory?: InventoryDefinition;
  readonly pickup?: PickupDefinition;
  readonly weapon?: WeaponDefinition;
  readonly secretRegion?: SecretRegionDefinition;
  readonly levelExit?: LevelExitDefinition;
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

export interface ProjectContent {
  readonly schemaVersion: 6;
  readonly entities: readonly EntityDefinition[];
  readonly voxelCollision?: VoxelCollisionDefinition;
  readonly generatedVoxelEnvironment?: GeneratedVoxelEnvironmentDefinition;
}

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

export interface StoredSceneDefinition {
  readonly id: string;
  readonly name: string;
  readonly voxelEnvironment?: StoredVoxelEnvironmentDefinition;
  readonly voxelObjectInstances?: readonly StoredVoxelObjectInstanceDefinition[];
  readonly entities: readonly EntityDefinition[];
}

export interface StoredProjectContent {
  readonly schemaVersion: 22;
  readonly projectId: string;
  readonly name: string;
  readonly entryScene: string;
  readonly assets: readonly StoredAssetDefinition[];
  readonly itemDefinitions: readonly ItemDefinition[];
  readonly scenes: readonly StoredSceneDefinition[];
}
