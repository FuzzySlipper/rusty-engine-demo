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
mod interaction;
mod navigation;
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

pub use combat::{
    CombatFact, CombatMissReason, CombatReceipt, CombatRejectionReason, EnemyComponent, EnemyState,
    EnemyView, HealthComponent, HealthConfig, HealthView, ResolvedAttackAction, WeaponComponent,
    WeaponConfig, WeaponState, WeaponView, MAX_COMBAT_HITBOX_HALF_EXTENT, MAX_HEALTH,
    MAX_WEAPON_AMMO, MAX_WEAPON_COOLDOWN_TICKS, MAX_WEAPON_DAMAGE, MAX_WEAPON_MUZZLE_OFFSET,
    MAX_WEAPON_RANGE,
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
pub use interaction::{SwitchComponent, SwitchView};
pub use navigation::{
    NavigationComponent, NavigationConfig, NavigationFact, NavigationFailure,
    NavigationPhaseReceipt, NavigationState, NavigationView, MAX_NAVIGATION_QUERY_BUDGET,
    MAX_NAVIGATION_SPEED_UNITS_PER_SECOND,
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
    ProjectSaveMode, ProjectStore, ProjectStoreError, DEFAULT_MAX_PROJECT_FILE_BYTES,
};
pub use runtime::{GameRuntime, RuntimeError, MAX_EVENT_WAVE, MAX_TICK_ADVANCE};
pub use runtime_records::{GameEvent, JournalEntry, RuntimeReadout, RuntimeReceipt};
pub use scheduler::{ScheduledIntent, ScheduledIntentKind, Scheduler};
pub use session::GameSession;
pub use snapshot::{
    decode_game_snapshot, encode_game_snapshot, EncounterSnapshot, EnemySnapshot,
    ExtractionBeaconSnapshot, GameSnapshot, GameSnapshotError, GeneratedRoomSnapshot,
    HealthSnapshot, MaterialVoxelSnapshot, NavigationSnapshot, PlayerControllerSnapshot,
    PlayerInputBindingsSnapshot, SnapshotEncounterState, SnapshotEnemyState,
    SnapshotExtractionBeaconState, SnapshotNavigationState, VoxelCollisionSnapshot, WeaponSnapshot,
    GAME_SNAPSHOT_SCHEMA_VERSION,
};
pub use stored_project::{
    decode_stored_project, diagnostic_code, ProjectDiagnostic, StoredAsset, StoredCollision,
    StoredDoor, StoredEncounter, StoredEntityDefinition, StoredExtractionBeacon,
    StoredGeneratedVoxelEnvironment, StoredHealth, StoredKinematic, StoredMaterialVoxel,
    StoredMaterialVoxelEnvironment, StoredNavigation, StoredPlayerController,
    StoredPlayerInputBindings, StoredProject, StoredProjectError, StoredRenderable, StoredScene,
    StoredSolidVoxelEnvironment, StoredSwitch, StoredVoxelEnvironment, StoredWeapon,
    STORED_PROJECT_SCHEMA_VERSION,
};
