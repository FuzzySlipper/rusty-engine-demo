//! Loading Bay's closed Studio adapter.
//!
//! The adapter owns this project's file layout and domain schema. Generic
//! admission, inspection, optimistic publication, and render projection remain
//! with their Rusty Engine owners. The protocol is deliberately finite and has
//! no method-name dispatch or runtime facade.

mod path;
mod project;
mod protocol;
mod service;

pub use path::{PathSafetyError, ProjectLocation, MAX_PROJECT_PATH_BYTES, MAX_ROOT_PATH_BYTES};
pub use protocol::{
    AdapterDescription, AdapterRejection, CanonicalOwnerContent, EntityTranslationReceipt,
    LoadingBayDomainReadout, OwnerInspections, ProjectionDiagnosticReadout, ProjectionReadout,
    StudioAdapterRequest, StudioAdapterResponse, StudioProjectIdentity, StudioProjectReadout,
    MAX_REQUEST_ID_BYTES, MAX_STUDIO_ADAPTER_REQUEST_BYTES, MAX_STUDIO_ADAPTER_RESPONSE_BYTES,
    STUDIO_ADAPTER_PROTOCOL_VERSION,
};
pub use service::StudioAdapterService;
