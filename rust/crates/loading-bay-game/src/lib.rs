//! Loading Bay's explicit Rust service-owned game runtime over [`entity_state`].
//!
//! Game components remain mostly data; named services own live behavior;
//! TypeScript-authored content is admitted before the session starts; and the
//! runtime owns event order, scheduling, projection, persistence, and lifecycle.

#![forbid(unsafe_code)]

mod combat;
mod content;
mod definition;
mod door;
mod encounter;
mod enemy_combat;
mod enemy_drop;
mod extraction_beacon;
mod game_loop;
mod hazard;
mod interaction;
mod inventory;
mod mechanics;
mod navigation;
mod pickup;
mod player;
mod progression;
mod project_admission;
mod project_codec;
mod project_store;
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
mod weapon_authoring;

pub use combat::{
    CombatFact, CombatMissReason, CombatReceipt, CombatRejectionReason, EnemyComponent, EnemyState,
    EnemyView, ResolvedAttackAction, WeaponConfig, WeaponState, WeaponView, MAX_WEAPON_AMMO,
    MAX_WEAPON_COOLDOWN_TICKS, MAX_WEAPON_DAMAGE, MAX_WEAPON_MUZZLE_OFFSET, MAX_WEAPON_RANGE,
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
pub use engine_spatial::{
    MaterialVoxel, VoxelEdit, VoxelEditApplyError, VoxelEditFact, VoxelEditReceipt,
    VoxelEditRejection, VoxelEditTransaction, VoxelProjectionRevisions, VoxelSourceRevision,
};
pub use engine_spatial::{MotionAxis, MotionFact, MotionPhaseReceipt};
pub use extraction_beacon::{
    ExtractionBeaconComponent, ExtractionBeaconConfig, ExtractionBeaconFact,
    ExtractionBeaconReceipt, ExtractionBeaconState, ExtractionBeaconView,
    MAX_EXTRACTION_BEACON_ACTIVATION_RADIUS,
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
pub use interaction::{SwitchComponent, SwitchView};
pub use inventory::{
    InventoryAction, InventoryAdmissionError, InventoryCommand, InventoryConfig, InventoryFact,
    InventoryReceipt, InventoryRejection, InventoryService, InventoryStack, InventoryView,
    ItemDefinition, ItemDefinitionId, ItemDefinitionIdError, ItemDefinitionView, ItemKind,
    WeaponAttackMode, WeaponDefinition, MAX_INVENTORY_SLOTS, MAX_ITEM_DEFINITION_ID_BYTES,
    MAX_ITEM_QUANTITY,
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
    PlayerControllerState, PlayerControllerView, PlayerInputBindings, ResolvedPlayerAction,
    MAX_INPUT_CONTROL_LENGTH, MAX_PLAYER_LOOK_DEGREES_PER_UNIT, MAX_PLAYER_SPEED_UNITS_PER_SECOND,
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
pub use runtime::{GameRuntime, RuntimeError, MAX_EVENT_WAVE, MAX_TICK_ADVANCE};
pub use runtime_records::{GameEvent, JournalEntry, RuntimeReadout, RuntimeReceipt};
pub use save_game::{
    LoadedSaveGame, SaveGameError, SaveGameMetadata, SaveGameStore, SaveLoadRequest,
    SavePlayerState, SaveProjectIdentity, SaveSlotCompatibility, SaveSlotId, SaveSlotSummary,
    SaveWriteRequest, MAX_SAVE_GAME_BYTES, MAX_SAVE_SLOTS, SAVE_GAME_SCHEMA_VERSION,
};
pub use scheduler::{ScheduledIntent, ScheduledIntentKind, Scheduler};
pub use session::GameSession;
pub use snapshot::{
    decode_game_snapshot, encode_game_snapshot, EncounterSnapshot, EnemyCombatSnapshot,
    EnemyDropSnapshot, EnemySnapshot, ExtractionBeaconSnapshot, GameSnapshot, GameSnapshotError,
    GeneratedRoomSnapshot, HazardSnapshot, HealthSnapshot, InventorySnapshot,
    InventoryStackSnapshot, ItemDefinitionSnapshot, MaterialVoxelSnapshot, NavigationSnapshot,
    PickupSnapshot, PlayerControllerSnapshot, PlayerInputBindingsSnapshot, SnapshotEncounterState,
    SnapshotEnemyAttackKind, SnapshotEnemyCombatPosture, SnapshotEnemyDropState,
    SnapshotEnemyState, SnapshotExtractionBeaconState, SnapshotItemKind, SnapshotNavigationState,
    SnapshotPickupCollectionCause, SnapshotPickupState, SnapshotVitalityState,
    SnapshotWeaponAttackMode, VoxelCollisionSnapshot, WeaponCooldownSnapshot, WeaponEntitySnapshot,
    WeaponSnapshot, GAME_SNAPSHOT_SCHEMA_VERSION,
};
pub use stored_project::{
    decode_stored_project, diagnostic_code, ProjectDiagnostic, StoredAsset,
    StoredAssetCatalogMetadata, StoredAssetImport, StoredBounds, StoredCollision, StoredDoor,
    StoredDoorAccess, StoredEncounter, StoredEnemyAttack, StoredEnemyAttackKind, StoredEnemyCombat,
    StoredEnemyDrop, StoredEntityDefinition, StoredExtractionBeacon,
    StoredGeneratedVoxelEnvironment, StoredHazard, StoredHealth, StoredImportSource,
    StoredInventory, StoredInventoryStack, StoredItemDefinition, StoredItemKind, StoredKinematic,
    StoredLevelExit, StoredLight, StoredLoadingBayInterlock, StoredMaterialVoxel,
    StoredMaterialVoxelEnvironment, StoredNavigation, StoredPickup, StoredPlayerController,
    StoredPlayerInputBindings, StoredProject, StoredProjectError, StoredRenderable,
    StoredRequiredKeyPolicy, StoredScene, StoredSecretRegion, StoredSolidVoxelEnvironment,
    StoredSwitch, StoredVoxelEnvironment, StoredVoxelInstance, StoredVoxelObjectFrameSelection,
    StoredVoxelObjectInstance, StoredVoxelObjectMaterialOverride, StoredWeapon,
    StoredWeaponAttackMode, MAX_PROJECT_VOXEL_OBJECTS, MAX_PROJECT_VOXEL_OBJECT_FRAMES,
    MAX_PROJECT_VOXEL_OBJECT_INSTANCES, MAX_PROJECT_VOXEL_OBJECT_MESH_FACE_WORK,
    MAX_PROJECT_VOXEL_OBJECT_RESOLVED_CELLS, STORED_PROJECT_SCHEMA_VERSION,
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
pub use weapon_authoring::{
    decode_loading_bay_weapon_authoring_request, encode_loading_bay_weapon_authoring_response,
    LoadingBayWeaponAuthoringAttackMode, LoadingBayWeaponAuthoringBinding,
    LoadingBayWeaponAuthoringCandidate, LoadingBayWeaponAuthoringCodecError,
    LoadingBayWeaponAuthoringReceipt, LoadingBayWeaponAuthoringRejection,
    LoadingBayWeaponAuthoringRejectionCode, LoadingBayWeaponAuthoringRequest,
    LoadingBayWeaponAuthoringResponse, LoadingBayWeaponAuthoringService,
    LoadingBayWeaponAuthoringWeapon, LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID,
    LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION, LOADING_BAY_WEAPON_COMPONENT_TYPE_ID,
    MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES,
    MAX_LOADING_BAY_WEAPON_AUTHORING_RESPONSE_BYTES,
};
