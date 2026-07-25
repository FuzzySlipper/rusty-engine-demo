use engine_inspector::{
    CatalogInspection, EntityStateInspection, PersistenceInspection, SceneInspection,
    VoxelStateInspection,
};
use render_model::RenderFrameDiff;
use serde::{Deserialize, Serialize};

pub const STUDIO_ADAPTER_PROTOCOL_VERSION: u32 = 2;
pub const MAX_STUDIO_ADAPTER_REQUEST_BYTES: usize = 64 * 1024;
pub const MAX_STUDIO_ADAPTER_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_REQUEST_ID_BYTES: usize = 256;

#[derive(Debug, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StudioAdapterRequest {
    Describe {
        protocol_version: u32,
        request_id: String,
    },
    OpenProject {
        protocol_version: u32,
        request_id: String,
        root: String,
        project_file: String,
    },
    ReadProject {
        protocol_version: u32,
        request_id: String,
    },
    SetEntityTranslation {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        expected_scene_revision: u64,
        entity_id: u64,
        translation: [f32; 3],
    },
    CloseProject {
        protocol_version: u32,
        request_id: String,
    },
}

impl StudioAdapterRequest {
    pub const fn protocol_version(&self) -> u32 {
        match self {
            Self::Describe {
                protocol_version, ..
            }
            | Self::OpenProject {
                protocol_version, ..
            }
            | Self::ReadProject {
                protocol_version, ..
            }
            | Self::SetEntityTranslation {
                protocol_version, ..
            }
            | Self::CloseProject {
                protocol_version, ..
            } => *protocol_version,
        }
    }

    pub fn request_id(&self) -> &str {
        match self {
            Self::Describe { request_id, .. }
            | Self::OpenProject { request_id, .. }
            | Self::ReadProject { request_id, .. }
            | Self::SetEntityTranslation { request_id, .. }
            | Self::CloseProject { request_id, .. } => request_id,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StudioAdapterResponse {
    Described {
        protocol_version: u32,
        request_id: String,
        adapter: AdapterDescription,
    },
    ProjectOpened {
        protocol_version: u32,
        request_id: String,
        project: StudioProjectReadout,
    },
    ProjectRead {
        protocol_version: u32,
        request_id: String,
        project: StudioProjectReadout,
    },
    EntityTranslationApplied {
        protocol_version: u32,
        request_id: String,
        receipt: EntityTranslationReceipt,
        project: StudioProjectReadout,
    },
    ProjectClosed {
        protocol_version: u32,
        request_id: String,
    },
    Rejected {
        protocol_version: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
        error: AdapterRejection,
    },
}

impl StudioAdapterResponse {
    pub fn rejected(request_id: Option<String>, error: AdapterRejection) -> Self {
        Self::Rejected {
            protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
            request_id,
            error,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterDescription {
    pub adapter_id: &'static str,
    pub adapter_version: u32,
    pub protocol_version: u32,
    pub project_kind: &'static str,
    pub project_schema_version: u32,
    pub operations: [&'static str; 5],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioProjectReadout {
    pub identity: StudioProjectIdentity,
    pub canonical: CanonicalOwnerContent,
    pub inspections: OwnerInspections,
    pub scene_hierarchy: SceneHierarchyReadout,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voxel: Option<VoxelStateInspection>,
    pub loading_bay: LoadingBayDomainReadout,
    pub projection: RenderFrameDiff,
    pub projection_readout: ProjectionReadout,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioProjectIdentity {
    pub project_id: String,
    pub name: String,
    pub entry_scene: String,
    pub source_schema_version: u32,
    pub current_schema_version: u32,
    pub project_hash: String,
    pub scene_revision: u64,
    pub relative_project_file: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalOwnerContent {
    pub project_json: String,
    pub asset_catalog_json: String,
    pub authored_scene_json: String,
    pub entity_state_json: String,
    pub content_manifest_json: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerInspections {
    pub catalog: CatalogInspection,
    pub scene: SceneInspection,
    pub entity_state: EntityStateInspection,
    pub persistence: PersistenceInspection,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneHierarchyReadout {
    pub scene_id: u64,
    pub revision: u64,
    pub name: Option<String>,
    pub root_node_ids: Vec<u64>,
    pub nodes: Vec<SceneHierarchyNodeReadout>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneHierarchyNodeReadout {
    pub node_id: u64,
    pub parent_node_id: Option<u64>,
    pub child_order: u32,
    pub display_order: u32,
    pub depth: u32,
    pub node_kind: &'static str,
    pub label: String,
    pub tags: Vec<String>,
    pub asset: Option<String>,
    pub entity_id: Option<u64>,
    pub local_transform: TransformReadout,
    pub world_transform: TransformReadout,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransformReadout {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadingBayDomainReadout {
    pub scene_name: String,
    pub entity_count: usize,
    pub door_count: usize,
    pub switch_count: usize,
    pub enemy_count: usize,
    pub encounter_count: usize,
    pub extraction_beacon_count: usize,
    pub navigator_count: usize,
    pub player_controller_count: usize,
    pub weapon_count: usize,
    pub voxel_environment: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionReadout {
    pub frame_kind: &'static str,
    pub source_revision: u64,
    pub retained_entities: usize,
    pub diagnostics: Vec<ProjectionDiagnosticReadout>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionDiagnosticReadout {
    pub code: &'static str,
    pub entity_id: u64,
    pub asset: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_kind: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityTranslationReceipt {
    pub entity_id: u64,
    pub translation: [f32; 3],
    pub project_hash_before: String,
    pub project_hash_after: String,
    pub scene_revision_before: u64,
    pub scene_revision_after: u64,
    pub content_candidate_hash: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterRejection {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub message: String,
}

impl AdapterRejection {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            path: None,
            message: message.into(),
        }
    }

    pub fn at_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}
