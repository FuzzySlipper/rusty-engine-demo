//! Loading Bay gameplay crate: the game's Rust-owned gameplay semantics,
//! state owners, and the authored-to-canonical admission compiler.
//!
//! Downstream gameplay owner per the Rusty Engine downstream-adoption guide:
//! domain services and their component state, the session/runtime state
//! owners, and the semantic compiler that turns authored project content
//! into canonical game definitions. The product shell (loading-bay-game)
//! owns the fixed-tick loop, persistence, projections, hosts, and the
//! studio adapter.

#![forbid(unsafe_code)]

pub mod combat;
pub mod combat_resolution;
pub mod content;
pub mod definition;
mod doom_e1m1_materials;
pub mod door;
pub mod encounter;
pub mod enemy_combat;
pub mod enemy_drop;
pub mod explosive_prop;
pub mod extraction_beacon;
pub mod floor_action;
pub mod hazard;
pub mod interaction;
pub mod inventory;
pub mod lift;
pub mod mechanics;
pub mod navigation;
pub mod pickup;
pub mod player;
pub mod progression;
pub mod project_admission;
pub mod project_codec;
pub mod projectile;
pub mod runtime;
pub mod runtime_records;
pub mod scheduler;
pub mod session;
pub mod snapshot;
pub mod stored_project;
pub mod vitality;

pub use combat::{
    CombatFact, CombatImpactKind, CombatMissReason, CombatReceipt, CombatRejectionReason,
    EnemyComponent, EnemyState, EnemyView, ResolvedAttackAction, WeaponConfig, WeaponState,
    WeaponView, MAX_WEAPON_AMMO, MAX_WEAPON_COOLDOWN_TICKS, MAX_WEAPON_DAMAGE,
    MAX_WEAPON_MUZZLE_OFFSET, MAX_WEAPON_RANGE,
};
pub use content::{
    decode_project_content, AdmittedProject, ProjectContentError, PROJECT_CONTENT_SCHEMA_VERSION,
};
pub use definition::{GameEntityDefinition, GameEntityDefinitionError};
pub use door::{
    security_door_definitions, DoorComponent, DoorConfig, DoorState, DoorView, SecurityDoorIds,
};
pub use encounter::{
    EncounterComponent, EncounterConfig, EncounterState, EncounterView,
    MAX_ENCOUNTER_ACTIVATION_RADIUS,
};
pub use enemy_combat::{
    EnemyAttackConfig, EnemyAttackKind, EnemyAttackMissReason, EnemyAttackPhaseReceipt,
    EnemyCombatComponent, EnemyCombatConfig, EnemyCombatFact, EnemyCombatPosture, EnemyCombatState,
    EnemyCombatView, EnemyIntentAndMotionReceipt, EnemyIntentPhaseReceipt, EnemyPerceptionCause,
    EnemyPerceptionConfig, MAX_ENEMY_ATTACK_COOLDOWN_TICKS, MAX_ENEMY_ATTACK_DAMAGE,
    MAX_ENEMY_ATTACK_RANGE, MAX_ENEMY_PERCEPTION_RANGE, MAX_ENEMY_PRESENTATION_BYTES,
};
pub use enemy_drop::{
    EnemyDropComponent, EnemyDropConfig, EnemyDropFact, EnemyDropRejection, EnemyDropState,
    EnemyDropView,
};
pub use explosive_prop::{
    ExplosivePropComponent, ExplosivePropConfig, ExplosivePropError, ExplosivePropFact,
    ExplosivePropPhaseReceipt, ExplosivePropState, ExplosivePropView, MAX_EXPLOSION_RADIUS,
};
pub use extraction_beacon::{
    ExtractionBeaconComponent, ExtractionBeaconConfig, ExtractionBeaconFact,
    ExtractionBeaconReceipt, ExtractionBeaconState, ExtractionBeaconView,
    MAX_EXTRACTION_BEACON_ACTIVATION_RADIUS,
};
pub use floor_action::{
    FloorActionActivation, FloorActionComponent, FloorActionConfig, FloorActionPhaseReceipt,
    FloorActionRejection, FloorActionService, FloorActionState, FloorActionView,
    DEFAULT_FLOOR_ACTION_MOTION_DURATION_TICKS, DEFAULT_FLOOR_ACTION_PRESENTATION,
    DEFAULT_FLOOR_ACTION_PROMPT, DEFAULT_FLOOR_ACTION_SOURCE, FLOOR_ACTION_TRIGGER_SCOPE,
    MAX_FLOOR_ACTION_MOTION_TICKS, MAX_FLOOR_ACTION_OVERLAP_SUBJECTS,
    MAX_FLOOR_ACTION_PRESENTATION_BYTES, MAX_FLOOR_ACTION_SOURCE_BYTES,
};
pub use hazard::{
    HazardComponent, HazardConfig, HazardFact, HazardPhaseReceipt, HazardRejection, HazardService,
    HazardView, HAZARD_TRIGGER_SCOPE, MAX_HAZARD_COOLDOWN_TICKS, MAX_HAZARD_OVERLAP_SUBJECTS,
};
pub use interaction::{
    SwitchComponent, SwitchConfig, SwitchEffect, SwitchView, DEFAULT_SWITCH_ACTIVATION_RADIUS,
    DEFAULT_SWITCH_PROMPT, DEFAULT_SWITCH_REPEATABLE, DEFAULT_SWITCH_UNAVAILABLE_PRESENTATION,
    MAX_SWITCH_ACTIVATION_RADIUS, MAX_SWITCH_EFFECTS, MAX_SWITCH_PRESENTATION_BYTES,
};
pub use inventory::{
    ArmorGrantMode, ArmorTransition, InventoryAction, InventoryAdmissionError, InventoryCommand,
    InventoryConfig, InventoryFact, InventoryReceipt, InventoryRejection, InventoryService,
    InventoryStack, InventoryView, ItemDefinition, ItemDefinitionId, ItemDefinitionIdError,
    ItemDefinitionView, ItemKind, ProjectileDefinition, WeaponAttackMode, WeaponDefinition,
    MAX_INVENTORY_SLOTS, MAX_ITEM_DEFINITION_ID_BYTES, MAX_ITEM_QUANTITY,
    MAX_PROJECTILE_GRAVITY_SCALE, MAX_PROJECTILE_IMPULSE, MAX_PROJECTILE_LIFETIME_TICKS,
    MAX_PROJECTILE_MASS, MAX_PROJECTILE_RADIUS, MAX_PROJECTILE_RESTITUTION,
};
pub use lift::{
    LiftActivation, LiftComponent, LiftConfig, LiftPhaseReceipt, LiftRejection, LiftService,
    LiftState, LiftView, DEFAULT_LIFT_MOTION_DURATION_TICKS, DEFAULT_LIFT_PROMPT,
    DEFAULT_LIFT_SOURCE, DEFAULT_LIFT_WAIT_TICKS, LIFT_TRIGGER_SCOPE, MAX_LIFT_MOTION_TICKS,
    MAX_LIFT_OVERLAP_SUBJECTS, MAX_LIFT_PRESENTATION_BYTES, MAX_LIFT_SOURCE_BYTES,
    MAX_LIFT_WAIT_TICKS,
};
pub use navigation::{
    NavigationComponent, NavigationConfig, NavigationFact, NavigationFailure,
    NavigationPhaseReceipt, NavigationState, NavigationView, MAX_NAVIGATION_QUERY_BUDGET,
    MAX_NAVIGATION_SPEED_UNITS_PER_SECOND,
};
pub use pickup::{
    PickupCollectionCause, PickupCollectionCommand, PickupComponent, PickupConfig,
    PickupDisposition, PickupFact, PickupPhaseReceipt, PickupPresentationCue, PickupReceipt,
    PickupRejectedAttempt, PickupRejection, PickupService, PickupState, PickupView,
    MAX_PICKUP_OVERLAP_SUBJECTS, PICKUP_TRIGGER_SCOPE,
};
pub use player::{
    PlayerControlFact, PlayerControlReceipt, PlayerControllerComponent, PlayerControllerConfig,
    PlayerControllerState, PlayerControllerView, PlayerInputBindings, PlayerTraversalConfig,
    ResolvedPlayerAction, ResolvedPlayerFrame, MAX_GROUND_PROBE_DISTANCE, MAX_INPUT_CONTROL_LENGTH,
    MAX_PLAYER_EYE_HEIGHT, MAX_PLAYER_GRAVITY, MAX_PLAYER_JUMP_IMPULSE,
    MAX_PLAYER_LOOK_DEGREES_PER_UNIT, MAX_PLAYER_SPEED_UNITS_PER_SECOND, MAX_PLAYER_STEP_HEIGHT,
};
pub use progression::{
    DoorAccessConfig, DoorAccessReceipt, DoorAccessRejection, DoorAccessView, LevelExitComponent,
    LevelExitConfig, LevelExitRejection, LevelExitState, LevelExitView, LoadingBayInterlockConfig,
    LoadingBayInterlockRejection, LoadingBayInterlockView, ProgressionFact, RequiredKeyPolicy,
    SecretPhaseReceipt, SecretRegionComponent, SecretRegionConfig, SecretRegionState,
    SecretRegionView, SecretRejection, LOADING_BAY_INTERLOCK_ACTIVATION_RADIUS,
    MAX_PROGRESSION_ACTIVATION_RADIUS, MAX_PROGRESSION_PRESENTATION_BYTES,
    MAX_SECRET_OVERLAP_SUBJECTS, SECRET_TRIGGER_SCOPE,
};
pub use project_admission::{
    admit_stored_project, admit_stored_project_with_document, decode_and_admit_stored_project,
    materialize_stored_project_voxels, AdmittedStoredProject,
};
pub use project_codec::{
    decode_project_document, encode_project_document, DecodedProjectDocument,
    MIGRATED_V6_PROJECT_ID, MIGRATED_V6_SCENE_ID,
};
pub use projectile::{ProjectileError, ProjectileFact, ProjectilePhaseReceipt};
pub use runtime::{
    GameRuntime, RuntimeError, WalkTriggerPhaseReceipt, MAX_EVENT_WAVE, MAX_TICK_ADVANCE,
};
pub use runtime_records::{GameEvent, JournalEntry, RuntimeReadout, RuntimeReceipt};
pub use scheduler::{ScheduledIntent, ScheduledIntentKind, Scheduler};
pub use session::GameSession;
pub use stored_project::{
    decode_stored_project, diagnostic_code, ProjectDiagnostic, StoredArmorGrantMode,
    StoredArmorTransition, StoredAsset, StoredAssetCatalogMetadata, StoredAssetImport,
    StoredBounds, StoredCollision, StoredDirectionalSpriteView, StoredDoor, StoredDoorAccess,
    StoredEncounter, StoredEnemyAttack, StoredEnemyAttackKind, StoredEnemyCombat, StoredEnemyDrop,
    StoredEntityDefinition, StoredExplosiveProp, StoredExtractionBeacon, StoredFloorAction,
    StoredGeneratedVoxelEnvironment, StoredHazard, StoredHealth, StoredImportSource,
    StoredInventory, StoredInventoryStack, StoredItemDefinition, StoredItemKind, StoredKinematic,
    StoredLevelExit, StoredLift, StoredLight, StoredLoadingBayInterlock, StoredMaterialVoxel,
    StoredMaterialVoxelEnvironment, StoredNavigation, StoredPickup, StoredPlayerController,
    StoredPlayerInputBindings, StoredProject, StoredProjectError, StoredRenderable,
    StoredRenderableTransform, StoredRequiredKeyPolicy, StoredScene, StoredSecretRegion,
    StoredSolidVoxelEnvironment, StoredSwitch, StoredSwitchEffect, StoredVisualAnimationLoopMode,
    StoredVisualBinding, StoredVisualBindingState, StoredVisualPresentation, StoredVisualState,
    StoredVoxelEnvironment, StoredVoxelInstance, StoredVoxelObjectFrameSelection,
    StoredVoxelObjectInstance, StoredVoxelObjectMaterialOverride, StoredVoxelObjectSurfaceMode,
    StoredWeapon, StoredWeaponAttackMode, MAX_PROJECT_VOXEL_OBJECTS,
    MAX_PROJECT_VOXEL_OBJECT_FRAMES, MAX_PROJECT_VOXEL_OBJECT_INSTANCES,
    MAX_PROJECT_VOXEL_OBJECT_MESH_FACE_WORK, MAX_PROJECT_VOXEL_OBJECT_RESOLVED_CELLS,
    MAX_STORED_VISUAL_BINDING_STATES, STORED_PROJECT_SCHEMA_VERSION, STORED_VISUAL_BINDING_VERSION,
};
pub use vitality::{
    DamageCommand, DamageDisposition, DamageService, DamageSource, HealthConfig, HealthView,
    VitalityFact, VitalityReceipt, VitalityRejection, VitalityState, MAX_ARMOR,
    MAX_COMBAT_HITBOX_HALF_EXTENT, MAX_DAMAGE, MAX_HEALTH,
};

pub use doom_e1m1_materials::{
    doom_asset_catalog, doom_manifest_path, doom_stored_assets,
    doom_stored_assets_include_textures, doom_stored_material_assets, load_doom_manifest,
    validate_doom_palette_closure, verify_doom_texture_files, DoomMaterialBinding, DOOM_FLAT_COUNT,
    DOOM_MATERIAL_COUNT, DOOM_WALL_COUNT,
};
pub use snapshot::{
    decode_game_snapshot, encode_game_snapshot, EncounterSnapshot, EnemyCombatSnapshot,
    EnemyDropSnapshot, EnemySnapshot, ExtractionBeaconSnapshot, FloorActionSnapshot, GameSnapshot,
    GameSnapshotError, GeneratedRoomSnapshot, HazardSnapshot, HealthSnapshot, InventorySnapshot,
    InventoryStackSnapshot, ItemDefinitionSnapshot, LiftSnapshot, MaterialVoxelSnapshot,
    NavigationSnapshot, PickupSnapshot, PlayerControllerSnapshot, PlayerInputBindingsSnapshot,
    SnapshotArmorGrantMode, SnapshotArmorTransition, SnapshotEncounterState,
    SnapshotEnemyAttackKind, SnapshotEnemyCombatPosture, SnapshotEnemyDropState,
    SnapshotEnemyState, SnapshotExtractionBeaconState, SnapshotFloorActionState, SnapshotItemKind,
    SnapshotLiftState, SnapshotNavigationState, SnapshotPickupCollectionCause, SnapshotPickupState,
    SnapshotVitalityState, SnapshotWeaponAttackMode, VoxelCollisionSnapshot,
    WeaponCooldownSnapshot, WeaponEntitySnapshot, WeaponSnapshot, GAME_SNAPSHOT_SCHEMA_VERSION,
};
