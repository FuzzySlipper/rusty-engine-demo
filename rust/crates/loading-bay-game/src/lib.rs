//! Loading Bay's explicit Rust service-owned game runtime over [`entity_state`].
//!
//! Game components remain mostly data; named services own live behavior;
//! TypeScript-authored content is admitted before the session starts; and the
//! runtime owns event order, scheduling, projection, persistence, and lifecycle.

#![forbid(unsafe_code)]

mod application_content_projection;
mod combat;
mod content;
mod definition;
mod doom_e1m1_materials;
mod door;
mod encounter;
mod enemy_combat;
mod enemy_drop;
mod explosive_prop;
mod extraction_beacon;
mod floor_action;
mod game_loop;
mod hazard;
mod interaction;
mod inventory;
mod lift;
mod mechanics;
mod navigation;
mod pickup;
mod player;
mod progression;
mod project_admission;
mod project_codec;
mod project_store;
mod projectile;
mod runtime;
mod runtime_records;
mod save_game;
mod scheduler;
mod session;
mod snapshot;
mod stored_project;
mod studio_adapter;
mod vitality;
mod voxel_object_projection;
mod voxel_volume_projection;
mod weapon_authoring;

pub use application_content_projection::{
    doom_texture_projection, externalize_frame_meshes, project_doom_e1m1_application_content,
    GameplayApplicationProjector, ProjectedApplicationContent,
};
pub use combat::{
    CombatFact, CombatMissReason, CombatReceipt, CombatRejectionReason, EnemyComponent, EnemyState,
    EnemyView, ResolvedAttackAction, WeaponConfig, WeaponState, WeaponView, MAX_WEAPON_AMMO,
    MAX_WEAPON_COOLDOWN_TICKS, MAX_WEAPON_DAMAGE, MAX_WEAPON_MUZZLE_OFFSET, MAX_WEAPON_RANGE,
};
pub use content::{
    decode_project_content, AdmittedProject, ProjectContentError, PROJECT_CONTENT_SCHEMA_VERSION,
};
pub use definition::{GameEntityDefinition, GameEntityDefinitionError};
pub use doom_e1m1_materials::{
    doom_asset_catalog, doom_manifest_path, doom_stored_assets,
    doom_stored_assets_include_textures, doom_stored_material_assets, load_doom_manifest,
    validate_doom_palette_closure, verify_doom_texture_files, DoomMaterialBinding, DOOM_FLAT_COUNT,
    DOOM_MATERIAL_COUNT, DOOM_WALL_COUNT,
};
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
pub use game_loop::{
    EdgeCommandRejection, GameLoopAdvanceReceipt, GameLoopEdgeCommand, GameLoopEdgeCommandKind,
    GameLoopFact, GameLoopPhase, GameLoopTickReceipt, GameRestartMode, InputCommandDisposition,
    InputCommandReceipt, InputCommandRejection, LoadingBayGameLoop, PlayerInputCommand,
    PlayerInputIntent, PlayerInputSessionView, FIXED_SIMULATION_HZ, FIXED_STEP_DURATION,
    FIXED_STEP_SECONDS, FIXED_TICK_PHASE_ORDER, MAX_ACCUMULATED_LOOK_UNITS, MAX_CATCH_UP_TICKS,
    MAX_EDGE_COMMANDS, MAX_INPUT_AGE_TICKS, MAX_PENDING_GAME_LOOP_FACTS,
    MAX_RETAINED_COMMAND_SEQUENCES,
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
    ResolvedPlayerAction, MAX_GROUND_PROBE_DISTANCE, MAX_INPUT_CONTROL_LENGTH,
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
pub use project_store::{
    LoadedProjectSource, ProjectSaveMode, ProjectStore, ProjectStoreError,
    DEFAULT_MAX_PROJECT_FILE_BYTES,
};
pub use projectile::{ProjectileError, ProjectileFact, ProjectilePhaseReceipt};
pub use runtime::{
    GameRuntime, RuntimeError, WalkTriggerPhaseReceipt, MAX_EVENT_WAVE, MAX_TICK_ADVANCE,
};
pub use runtime_records::{GameEvent, JournalEntry, RuntimeReadout, RuntimeReceipt};
pub use rusty_engine::engine_spatial::{
    MaterialVoxel, VoxelEdit, VoxelEditApplyError, VoxelEditFact, VoxelEditReceipt,
    VoxelEditRejection, VoxelEditTransaction, VoxelProjectionRevisions, VoxelSourceRevision,
};
pub use rusty_engine::engine_spatial::{MotionAxis, MotionFact, MotionPhaseReceipt};
pub use save_game::{
    LoadedSaveGame, SaveGameError, SaveGameMetadata, SaveGameStore, SaveLoadRequest,
    SavePlayerState, SaveProjectIdentity, SaveSlotCompatibility, SaveSlotId, SaveSlotSummary,
    SaveWriteRequest, MAX_SAVE_GAME_BYTES, MAX_SAVE_SLOTS, SAVE_GAME_SCHEMA_VERSION,
};
pub use scheduler::{ScheduledIntent, ScheduledIntentKind, Scheduler};
pub use session::GameSession;
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
pub use studio_adapter::{
    AdapterDescription, AdapterRejection, CanonicalOwnerContent, EntityTranslationReceipt,
    OwnerInspections, PathSafetyError, ProjectLocation, ProjectionDiagnosticReadout,
    ProjectionReadout, StudioAdapterRequest, StudioAdapterResponse, StudioAdapterService,
    StudioEntityComponentReference, StudioEntityInspectorContractIdentity, StudioProjectIdentity,
    StudioProjectReadout, MAX_PROJECT_PATH_BYTES, MAX_REQUEST_ID_BYTES, MAX_ROOT_PATH_BYTES,
    MAX_STUDIO_ADAPTER_REQUEST_BYTES, MAX_STUDIO_ADAPTER_RESPONSE_BYTES,
    MAX_STUDIO_ENTITY_COMPONENTS_PER_OWNER, MAX_STUDIO_ENTITY_COMPONENT_REFERENCES,
    MAX_STUDIO_ENTITY_INSPECTOR_CONTRACTS, STUDIO_ADAPTER_PROTOCOL_VERSION,
    VOXEL_OBJECT_COMPONENT_TYPE_ID, VOXEL_OBJECT_INSPECTOR_CONTRACT_ID,
    VOXEL_OBJECT_INSPECTOR_CONTRACT_VERSION,
};
pub use vitality::{
    DamageCommand, DamageDisposition, DamageService, DamageSource, HealthConfig, HealthView,
    VitalityFact, VitalityReceipt, VitalityRejection, VitalityState, MAX_ARMOR,
    MAX_COMBAT_HITBOX_HALF_EXTENT, MAX_DAMAGE, MAX_HEALTH,
};
pub(crate) use voxel_object_projection::project_stored_voxel_objects_with;
pub use voxel_object_projection::{project_stored_voxel_objects, StoredVoxelObjectProjectionError};
pub use voxel_volume_projection::{project_stored_voxel_volume, StoredVoxelVolumeProjectionError};
pub use weapon_authoring::{
    decode_loading_bay_weapon_authoring_request, encode_loading_bay_weapon_authoring_response,
    LoadingBayProjectileAuthoringConfig, LoadingBayWeaponAuthoringAttackMode,
    LoadingBayWeaponAuthoringBinding, LoadingBayWeaponAuthoringCandidate,
    LoadingBayWeaponAuthoringCodecError, LoadingBayWeaponAuthoringReceipt,
    LoadingBayWeaponAuthoringRejection, LoadingBayWeaponAuthoringRejectionCode,
    LoadingBayWeaponAuthoringRequest, LoadingBayWeaponAuthoringResponse,
    LoadingBayWeaponAuthoringService, LoadingBayWeaponAuthoringWeapon,
    LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID, LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION,
    LOADING_BAY_WEAPON_COMPONENT_TYPE_ID, MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES,
    MAX_LOADING_BAY_WEAPON_AUTHORING_RESPONSE_BYTES,
};
