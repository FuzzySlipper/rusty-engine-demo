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
mod extraction_beacon;
mod game_loop;
mod interaction;
mod inventory;
mod navigation;
mod pickup;
mod player;
mod project_admission;
mod project_codec;
mod project_store;
mod runtime;
mod runtime_records;
mod scheduler;
mod session;
mod snapshot;
mod stored_project;
mod studio_adapter;

pub use combat::{
    CombatFact, CombatMissReason, CombatReceipt, CombatRejectionReason, EnemyComponent, EnemyState,
    EnemyView, HealthComponent, HealthConfig, HealthView, ResolvedAttackAction, WeaponConfig,
    WeaponState, WeaponView, MAX_COMBAT_HITBOX_HALF_EXTENT, MAX_HEALTH, MAX_WEAPON_AMMO,
    MAX_WEAPON_COOLDOWN_TICKS, MAX_WEAPON_DAMAGE, MAX_WEAPON_MUZZLE_OFFSET, MAX_WEAPON_RANGE,
};
pub use content::{
    decode_project_content, AdmittedProject, ProjectContentError, PROJECT_CONTENT_SCHEMA_VERSION,
};
pub use definition::{GameEntityDefinition, GameEntityDefinitionError};
pub use door::{
    security_door_definitions, DoorComponent, DoorConfig, DoorState, DoorView, SecurityDoorIds,
};
pub use encounter::{EncounterComponent, EncounterConfig, EncounterState, EncounterView};
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
    GameLoopFact, GameLoopPhase, GameLoopTickReceipt, InputCommandDisposition, InputCommandReceipt,
    InputCommandRejection, LoadingBayGameLoop, PlayerInputCommand, PlayerInputIntent,
    PlayerInputSessionView, FIXED_SIMULATION_HZ, FIXED_STEP_DURATION, FIXED_STEP_SECONDS,
    FIXED_TICK_PHASE_ORDER, MAX_ACCUMULATED_LOOK_UNITS, MAX_CATCH_UP_TICKS, MAX_EDGE_COMMANDS,
    MAX_INPUT_AGE_TICKS, MAX_PENDING_GAME_LOOP_FACTS, MAX_RETAINED_COMMAND_SEQUENCES,
};
pub use interaction::{SwitchComponent, SwitchView};
pub use inventory::{
    InventoryAction, InventoryAdmissionError, InventoryCommand, InventoryComponent,
    InventoryConfig, InventoryFact, InventoryReceipt, InventoryRejection, InventoryService,
    InventoryStack, InventoryView, ItemDefinition, ItemDefinitionId, ItemDefinitionIdError,
    ItemDefinitionView, ItemKind, WeaponAttackMode, WeaponDefinition, MAX_INVENTORY_SLOTS,
    MAX_ITEM_DEFINITION_ID_BYTES, MAX_ITEM_QUANTITY,
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
pub use scheduler::{ScheduledIntent, ScheduledIntentKind, Scheduler};
pub use session::GameSession;
pub use snapshot::{
    decode_game_snapshot, encode_game_snapshot, EncounterSnapshot, EnemySnapshot,
    ExtractionBeaconSnapshot, GameSnapshot, GameSnapshotError, GeneratedRoomSnapshot,
    HealthSnapshot, InventorySnapshot, InventoryStackSnapshot, ItemDefinitionSnapshot,
    MaterialVoxelSnapshot, NavigationSnapshot, PickupSnapshot, PlayerControllerSnapshot,
    PlayerInputBindingsSnapshot, SnapshotEncounterState, SnapshotEnemyState,
    SnapshotExtractionBeaconState, SnapshotItemKind, SnapshotNavigationState,
    SnapshotPickupCollectionCause, SnapshotPickupState, SnapshotWeaponAttackMode,
    VoxelCollisionSnapshot, WeaponCooldownSnapshot, WeaponSnapshot, GAME_SNAPSHOT_SCHEMA_VERSION,
};
pub use stored_project::{
    decode_stored_project, diagnostic_code, ProjectDiagnostic, StoredAsset,
    StoredAssetCatalogMetadata, StoredAssetImport, StoredBounds, StoredCollision, StoredDoor,
    StoredEncounter, StoredEntityDefinition, StoredExtractionBeacon,
    StoredGeneratedVoxelEnvironment, StoredHealth, StoredImportSource, StoredInventory,
    StoredInventoryStack, StoredItemDefinition, StoredItemKind, StoredKinematic, StoredLight,
    StoredMaterialVoxel, StoredMaterialVoxelEnvironment, StoredNavigation, StoredPickup,
    StoredPlayerController, StoredPlayerInputBindings, StoredProject, StoredProjectError,
    StoredRenderable, StoredScene, StoredSolidVoxelEnvironment, StoredSwitch,
    StoredVoxelEnvironment, StoredVoxelInstance, StoredWeapon, StoredWeaponAttackMode,
    STORED_PROJECT_SCHEMA_VERSION,
};
pub use studio_adapter::{
    AdapterDescription, AdapterRejection, CanonicalOwnerContent, EntityTranslationReceipt,
    LoadingBayDomainReadout, OwnerInspections, PathSafetyError, ProjectLocation,
    ProjectionDiagnosticReadout, ProjectionReadout, StudioAdapterRequest, StudioAdapterResponse,
    StudioAdapterService, StudioProjectIdentity, StudioProjectReadout, MAX_PROJECT_PATH_BYTES,
    MAX_REQUEST_ID_BYTES, MAX_ROOT_PATH_BYTES, MAX_STUDIO_ADAPTER_REQUEST_BYTES,
    MAX_STUDIO_ADAPTER_RESPONSE_BYTES, STUDIO_ADAPTER_PROTOCOL_VERSION,
};
