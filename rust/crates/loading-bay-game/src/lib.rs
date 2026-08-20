//! Loading Bay's explicit Rust service-owned game runtime over [`entity_state`].
//!
//! Game components remain mostly data; named services own live behavior;
//! TypeScript-authored content is admitted before the session starts; and the
//! runtime owns event order, scheduling, projection, persistence, and lifecycle.

#![forbid(unsafe_code)]

// Projection modules retain the product crate name in their renderer-neutral
// imports whether compiled by a host binary or this library adapter.
extern crate self as loading_bay_game;

pub use loading_bay_gameplay::*;

mod application_content_projection;
pub mod browser_adapter;
mod game_loop;
mod product_service;
mod project_store;
mod save_game;
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
pub use product_service::{
    LoadingBayProductService, LoadingBayProjectReadout, LoadingBayServiceCommand,
    LoadingBayServiceError, LoadingBayServiceOutcome, LoadingBayServiceReceipt, LOADING_BAY_PLAYER,
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
pub use voxel_object_projection::{project_stored_voxel_objects, StoredVoxelObjectProjectionError};
pub use voxel_volume_projection::{project_stored_voxel_volume, StoredVoxelVolumeProjectionError};
