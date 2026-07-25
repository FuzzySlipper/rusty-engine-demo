//! Loading Bay's closed Studio adapter.
//!
//! The adapter owns this project's file layout and domain schema. Generic
//! admission, inspection, optimistic publication, and render projection remain
//! with their Rusty Engine owners. The protocol is deliberately finite and has
//! no method-name dispatch or runtime facade.

mod host_file;
mod path;
mod project;
mod protocol;
mod service;
mod voxel;

pub use path::{PathSafetyError, ProjectLocation, MAX_PROJECT_PATH_BYTES, MAX_ROOT_PATH_BYTES};
pub use protocol::*;
pub use service::StudioAdapterService;
