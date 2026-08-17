//! Loading Bay's explicit Rust service-owned game runtime over [`entity_state`].
//!
//! Game components remain mostly data; named services own live behavior;
//! TypeScript-authored content is admitted before the session starts; and the
//! runtime owns event order, scheduling, projection, persistence, and lifecycle.

#![forbid(unsafe_code)]

pub use loading_bay_gameplay::*;

mod weapon_authoring;
pub use weapon_authoring::*;

mod application_content_projection;
mod game_loop;
mod project_store;
mod save_game;
mod studio_adapter;
mod voxel_object_projection;
mod voxel_volume_projection;

pub use application_content_projection::{
    doom_sky_projection, doom_texture_projection, externalize_frame_meshes,
    project_doom_e1m1_application_content, GameplayApplicationProjector,
    ProjectedApplicationContent,
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
pub use project_store::{
    LoadedProjectSource, ProjectSaveMode, ProjectStore, ProjectStoreError,
    DEFAULT_MAX_PROJECT_FILE_BYTES,
};
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
pub(crate) use voxel_object_projection::project_stored_voxel_objects_with;
pub use voxel_object_projection::{project_stored_voxel_objects, StoredVoxelObjectProjectionError};
pub use voxel_volume_projection::{project_stored_voxel_volume, StoredVoxelVolumeProjectionError};
