use asset_catalog::StoredMaterialDefinition;
use engine_inspector::{
    CatalogInspection, DiagnosticSet, EntityStateInspection, NamedCount, PersistenceInspection,
    SceneInspection, VoxelAssetInspection, VoxelStateInspection,
};
use engine_spatial::{VoxelEditDelta, VoxelPrimitiveRequest, VoxelTemplateRequest};
use render_model::{RenderFrameDiff, Transform};
use serde::{Deserialize, Serialize};
use voxel_annotation::{
    VoxelAnnotationEditTransaction, VoxelAnnotationLayerDraft, VoxelAnnotationQuery,
    VoxelAnnotationRegionReadout,
};
use voxel_asset::{
    VoxelAssetBounds, VoxelAssetMaterialBinding, VoxelAssetMaterialMapping,
    VoxelObjectProvenanceKind, VoxelObjectSourceClipProvenance,
};
use voxel_convert::{
    ConversionPlanSettings, VoxelConversionPlan, VoxelConversionPreview, VoxelModelInfoReadout,
    VoxelModelWindowReadout, VoxelModelWindowRequest, VoxelObjectClipConversionRequest,
    VoxelObjectConversionPlan, VoxelObjectConversionPreview, VoxelObjectConversionSettings,
    VoxelObjectFrameSelection,
};
use voxel_object_runtime::{VoxelObjectLoopMode, VoxelObjectPlaybackRate};

use crate::{
    StoredCollision, StoredImportSource, StoredKinematic, StoredLight, StoredVoxelInstance,
    StoredVoxelObjectFrameSelection, StoredVoxelObjectMaterialOverride,
};

pub const STUDIO_ADAPTER_PROTOCOL_VERSION: u32 = 11;
pub const MAX_STUDIO_ADAPTER_REQUEST_BYTES: usize = 256 * 1024;
pub const MAX_STUDIO_ADAPTER_RESPONSE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_REQUEST_ID_BYTES: usize = 256;
pub const MAX_STUDIO_ENTITY_INSPECTOR_CONTRACTS: usize = 64;
pub const MAX_STUDIO_ENTITY_COMPONENT_REFERENCES: usize = 4_096;
pub const MAX_STUDIO_ENTITY_COMPONENTS_PER_OWNER: usize = 32;

pub const VOXEL_OBJECT_COMPONENT_TYPE_ID: &str = "rusty.voxel-object.instance";
pub const VOXEL_OBJECT_INSPECTOR_CONTRACT_ID: &str = "rusty.studio.voxel-object-authoring";
pub const VOXEL_OBJECT_INSPECTOR_CONTRACT_VERSION: u32 = 1;

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
    CreateProject {
        protocol_version: u32,
        request_id: String,
        root: String,
        project_file: String,
        project_id: String,
        name: String,
        entry_scene: String,
        entry_scene_name: String,
    },
    SaveProjectAs {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        root: String,
        project_file: String,
        project_id: String,
        name: String,
    },
    ReadProject {
        protocol_version: u32,
        request_id: String,
    },
    CreateScene {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        scene_id: String,
        name: String,
        make_entry: bool,
    },
    RenameScene {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        scene_id: String,
        name: String,
    },
    DeleteScene {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        scene_id: String,
    },
    SetEntryScene {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        scene_id: String,
    },
    CreateSceneObject {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        expected_scene_revision: u64,
        object: StudioSceneObjectDraft,
    },
    DeleteSceneObject {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        expected_scene_revision: u64,
        entity_id: u64,
    },
    RenameSceneObject {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        expected_scene_revision: u64,
        entity_id: u64,
        name: String,
    },
    ReparentSceneObject {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        expected_scene_revision: u64,
        entity_id: u64,
        parent_entity_id: Option<u64>,
        child_order: u32,
    },
    SetSceneObjectTransform {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        expected_scene_revision: u64,
        entity_id: u64,
        transform: TransformReadout,
    },
    SetSceneObjectAppearance {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        expected_scene_revision: u64,
        entity_id: u64,
        appearance: StudioSceneAppearance,
    },
    SetEntityCollision {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        entity_id: u64,
        collision: Option<StoredCollision>,
    },
    SetEntityKinematic {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        entity_id: u64,
        kinematic: Option<StoredKinematic>,
    },
    SetEntityTranslation {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        expected_scene_revision: u64,
        entity_id: u64,
        translation: [f32; 3],
    },
    UpsertMaterial {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        definition: StoredMaterialDefinition,
    },
    PrepareAssetImport {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        source: StudioFileSelection,
        settings: StudioAssetImportSettings,
    },
    PrepareAssetReimport {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
    },
    ApplyAssetImport {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        plan_id: String,
        expected_plan_hash: String,
    },
    DiscardAssetImport {
        protocol_version: u32,
        request_id: String,
        plan_id: String,
    },
    InitializeVoxelAsset {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        cell_size: f64,
        chunk_size: u32,
        origin: [i64; 3],
        bounds: VoxelAssetBounds,
        material_palette: Vec<VoxelAssetMaterialBinding>,
        initial_material_slot: u16,
    },
    DuplicateVoxelAsset {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        source_asset_id: String,
        expected_source_content_hash: String,
        target_asset_id: String,
    },
    AttachVoxelInstance {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        scene_id: String,
        instance: StoredVoxelInstance,
    },
    SetVoxelInstanceTransform {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        scene_id: String,
        instance_id: String,
        translation: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    },
    RemoveVoxelInstance {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        scene_id: String,
        instance_id: String,
    },
    ReplaceVoxelPalette {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        expected_asset_content_hash: String,
        expected_voxel_data_hash: String,
        replacement: Vec<VoxelAssetMaterialBinding>,
    },
    ValidateVoxelPick {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        scene_id: String,
        instance_id: String,
        origin: [f64; 3],
        direction: [f64; 3],
        max_distance: f64,
        claimed_voxel: [i64; 3],
        claimed_face: VoxelPickFace,
    },
    ApplyVoxelBrush {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        expected_asset_content_hash: String,
        center: [i64; 3],
        radius: u32,
        mode: VoxelBrushMode,
        material_slot: Option<u16>,
    },
    ApplyVoxelPrimitive {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        expected_asset_content_hash: String,
        request: VoxelPrimitiveRequest,
    },
    InitializeVoxelTemplate {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        cell_size: f64,
        chunk_size: u32,
        material_palette: Vec<VoxelAssetMaterialBinding>,
        request: VoxelTemplateRequest,
    },
    ImportVoxelAssetFile {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        source_path: String,
        target_asset_id: String,
    },
    ExportVoxelAssetFile {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        expected_asset_content_hash: String,
        target_path: String,
        expected_target_sha256: Option<String>,
    },
    MaterializeEnvironment {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        expected_scene_revision: u64,
        scene_id: String,
        preset: StudioEnvironmentPreset,
        seed: u64,
        voxel_asset_id: String,
        voxel_instance_id: String,
        voxel_translation: [f32; 3],
        player_entity_id: u64,
        exit_entity_id: u64,
        wall_material: u16,
        floor_material: u16,
        accent_material: u16,
        material_palette: Vec<VoxelAssetMaterialBinding>,
    },
    UndoVoxelEdit {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        expected_asset_content_hash: String,
    },
    RedoVoxelEdit {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        expected_asset_content_hash: String,
    },
    RevertVoxelHistory {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        expected_asset_content_hash: String,
        target_cursor: usize,
    },
    QueryVoxelHistory {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        expected_asset_content_hash: String,
        max_entries: usize,
        max_deltas_per_entry: usize,
    },
    PrepareVoxelHistoryRevert {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        expected_asset_content_hash: String,
        target_cursor: usize,
        max_samples: usize,
    },
    ApplyVoxelHistoryRevert {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        preview_id: String,
    },
    DiscardVoxelHistoryRevert {
        protocol_version: u32,
        request_id: String,
        preview_id: String,
    },
    CreateVoxelAnnotationLayer {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        draft: VoxelAnnotationLayerDraft,
    },
    EditVoxelAnnotation {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        layer_id: String,
        transaction: VoxelAnnotationEditTransaction,
    },
    QueryVoxelAnnotation {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        layer_id: String,
        query: VoxelAnnotationQuery,
    },
    ExportVoxelAnnotation {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        layer_id: String,
        expected_layer_hash: String,
    },
    QueryVoxelModel {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        expected_asset_content_hash: String,
        window: Option<VoxelModelWindowRequest>,
    },
    PrepareVoxelConversion {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        source_asset_id: String,
        source: StudioFileSelection,
        target_asset_id: String,
        license: Option<StudioFileSelection>,
        mesh_primitive: Option<String>,
        settings: Box<ConversionPlanSettings>,
        max_preview_samples: u32,
    },
    ApplyVoxelConversion {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        plan_id: String,
        expected_plan_hash: String,
        expected_output_hash: String,
    },
    DiscardVoxelConversion {
        protocol_version: u32,
        request_id: String,
        plan_id: String,
    },
    InspectVoxelObjectSource {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        source_kind: StudioVoxelObjectSourceKind,
        source_asset_id: String,
        source: StudioFileSelection,
        mesh_primitive: Option<String>,
    },
    PrepareVoxelObjectConversion {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        source_kind: StudioVoxelObjectSourceKind,
        source_asset_id: String,
        source: StudioFileSelection,
        target_asset_id: String,
        license: Option<StudioFileSelection>,
        mesh_primitive: Option<String>,
        settings: Box<VoxelObjectConversionSettings>,
        clips: Vec<VoxelObjectClipConversionRequest>,
        default_clip: Option<String>,
        frame: VoxelObjectFrameSelection,
        max_preview_samples: u32,
    },
    PreviewVoxelObjectConversion {
        protocol_version: u32,
        request_id: String,
        plan_id: String,
        expected_plan_hash: String,
        frame: VoxelObjectFrameSelection,
        max_preview_samples: u32,
    },
    ApplyVoxelObjectConversion {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        plan_id: String,
        expected_plan_hash: String,
        expected_output_hash: String,
    },
    DiscardVoxelObjectConversion {
        protocol_version: u32,
        request_id: String,
        plan_id: String,
    },
    PrepareVoxelObjectPlacement {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        asset_id: String,
        expected_object_content_hash: String,
    },
    AttachVoxelObjectInstance {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        scene_id: String,
        instance: StudioVoxelObjectInstance,
    },
    PreviewVoxelObjectInstance {
        protocol_version: u32,
        request_id: String,
        expected_project_hash: String,
        scene_id: String,
        instance_id: String,
        now_microseconds: u64,
        command: VoxelObjectPlaybackCommand,
    },
    CloseProject {
        protocol_version: u32,
        request_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StudioVoxelObjectInstance {
    pub instance_id: String,
    pub voxel_object_asset_id: String,
    pub frame: StoredVoxelObjectFrameSelection,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    #[serde(default)]
    pub material_overrides: Vec<StoredVoxelObjectMaterialOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum VoxelObjectPlaybackCommand {
    Scrub {
        clip_id: String,
        clip_frame: u32,
        loop_mode: VoxelObjectLoopMode,
    },
    Play,
    Pause,
    Sample,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "scope",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StudioFileSelection {
    Project { path: String },
    Host { path: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StudioVoxelObjectSourceKind {
    Static,
    Animated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StudioEnvironmentPreset {
    TinyEnclosed,
}

impl StudioFileSelection {
    pub fn path(&self) -> &str {
        match self {
            Self::Project { path } | Self::Host { path } => path,
        }
    }
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
            | Self::CreateProject {
                protocol_version, ..
            }
            | Self::SaveProjectAs {
                protocol_version, ..
            }
            | Self::ReadProject {
                protocol_version, ..
            }
            | Self::CreateScene {
                protocol_version, ..
            }
            | Self::RenameScene {
                protocol_version, ..
            }
            | Self::DeleteScene {
                protocol_version, ..
            }
            | Self::SetEntryScene {
                protocol_version, ..
            }
            | Self::CreateSceneObject {
                protocol_version, ..
            }
            | Self::DeleteSceneObject {
                protocol_version, ..
            }
            | Self::RenameSceneObject {
                protocol_version, ..
            }
            | Self::ReparentSceneObject {
                protocol_version, ..
            }
            | Self::SetSceneObjectTransform {
                protocol_version, ..
            }
            | Self::SetSceneObjectAppearance {
                protocol_version, ..
            }
            | Self::SetEntityCollision {
                protocol_version, ..
            }
            | Self::SetEntityKinematic {
                protocol_version, ..
            }
            | Self::SetEntityTranslation {
                protocol_version, ..
            }
            | Self::UpsertMaterial {
                protocol_version, ..
            }
            | Self::PrepareAssetImport {
                protocol_version, ..
            }
            | Self::PrepareAssetReimport {
                protocol_version, ..
            }
            | Self::ApplyAssetImport {
                protocol_version, ..
            }
            | Self::DiscardAssetImport {
                protocol_version, ..
            }
            | Self::InitializeVoxelAsset {
                protocol_version, ..
            }
            | Self::DuplicateVoxelAsset {
                protocol_version, ..
            }
            | Self::AttachVoxelInstance {
                protocol_version, ..
            }
            | Self::SetVoxelInstanceTransform {
                protocol_version, ..
            }
            | Self::RemoveVoxelInstance {
                protocol_version, ..
            }
            | Self::ReplaceVoxelPalette {
                protocol_version, ..
            }
            | Self::ValidateVoxelPick {
                protocol_version, ..
            }
            | Self::ApplyVoxelBrush {
                protocol_version, ..
            }
            | Self::ApplyVoxelPrimitive {
                protocol_version, ..
            }
            | Self::InitializeVoxelTemplate {
                protocol_version, ..
            }
            | Self::ImportVoxelAssetFile {
                protocol_version, ..
            }
            | Self::ExportVoxelAssetFile {
                protocol_version, ..
            }
            | Self::MaterializeEnvironment {
                protocol_version, ..
            }
            | Self::UndoVoxelEdit {
                protocol_version, ..
            }
            | Self::RedoVoxelEdit {
                protocol_version, ..
            }
            | Self::RevertVoxelHistory {
                protocol_version, ..
            }
            | Self::QueryVoxelHistory {
                protocol_version, ..
            }
            | Self::PrepareVoxelHistoryRevert {
                protocol_version, ..
            }
            | Self::ApplyVoxelHistoryRevert {
                protocol_version, ..
            }
            | Self::DiscardVoxelHistoryRevert {
                protocol_version, ..
            }
            | Self::CreateVoxelAnnotationLayer {
                protocol_version, ..
            }
            | Self::EditVoxelAnnotation {
                protocol_version, ..
            }
            | Self::QueryVoxelAnnotation {
                protocol_version, ..
            }
            | Self::ExportVoxelAnnotation {
                protocol_version, ..
            }
            | Self::QueryVoxelModel {
                protocol_version, ..
            }
            | Self::PrepareVoxelConversion {
                protocol_version, ..
            }
            | Self::ApplyVoxelConversion {
                protocol_version, ..
            }
            | Self::DiscardVoxelConversion {
                protocol_version, ..
            }
            | Self::InspectVoxelObjectSource {
                protocol_version, ..
            }
            | Self::PrepareVoxelObjectConversion {
                protocol_version, ..
            }
            | Self::PreviewVoxelObjectConversion {
                protocol_version, ..
            }
            | Self::ApplyVoxelObjectConversion {
                protocol_version, ..
            }
            | Self::DiscardVoxelObjectConversion {
                protocol_version, ..
            }
            | Self::PrepareVoxelObjectPlacement {
                protocol_version, ..
            }
            | Self::AttachVoxelObjectInstance {
                protocol_version, ..
            }
            | Self::PreviewVoxelObjectInstance {
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
            | Self::CreateProject { request_id, .. }
            | Self::SaveProjectAs { request_id, .. }
            | Self::ReadProject { request_id, .. }
            | Self::CreateScene { request_id, .. }
            | Self::RenameScene { request_id, .. }
            | Self::DeleteScene { request_id, .. }
            | Self::SetEntryScene { request_id, .. }
            | Self::CreateSceneObject { request_id, .. }
            | Self::DeleteSceneObject { request_id, .. }
            | Self::RenameSceneObject { request_id, .. }
            | Self::ReparentSceneObject { request_id, .. }
            | Self::SetSceneObjectTransform { request_id, .. }
            | Self::SetSceneObjectAppearance { request_id, .. }
            | Self::SetEntityCollision { request_id, .. }
            | Self::SetEntityKinematic { request_id, .. }
            | Self::SetEntityTranslation { request_id, .. }
            | Self::UpsertMaterial { request_id, .. }
            | Self::PrepareAssetImport { request_id, .. }
            | Self::PrepareAssetReimport { request_id, .. }
            | Self::ApplyAssetImport { request_id, .. }
            | Self::DiscardAssetImport { request_id, .. }
            | Self::InitializeVoxelAsset { request_id, .. }
            | Self::DuplicateVoxelAsset { request_id, .. }
            | Self::AttachVoxelInstance { request_id, .. }
            | Self::SetVoxelInstanceTransform { request_id, .. }
            | Self::RemoveVoxelInstance { request_id, .. }
            | Self::ReplaceVoxelPalette { request_id, .. }
            | Self::ValidateVoxelPick { request_id, .. }
            | Self::ApplyVoxelBrush { request_id, .. }
            | Self::ApplyVoxelPrimitive { request_id, .. }
            | Self::InitializeVoxelTemplate { request_id, .. }
            | Self::ImportVoxelAssetFile { request_id, .. }
            | Self::ExportVoxelAssetFile { request_id, .. }
            | Self::MaterializeEnvironment { request_id, .. }
            | Self::UndoVoxelEdit { request_id, .. }
            | Self::RedoVoxelEdit { request_id, .. }
            | Self::RevertVoxelHistory { request_id, .. }
            | Self::QueryVoxelHistory { request_id, .. }
            | Self::PrepareVoxelHistoryRevert { request_id, .. }
            | Self::ApplyVoxelHistoryRevert { request_id, .. }
            | Self::DiscardVoxelHistoryRevert { request_id, .. }
            | Self::CreateVoxelAnnotationLayer { request_id, .. }
            | Self::EditVoxelAnnotation { request_id, .. }
            | Self::QueryVoxelAnnotation { request_id, .. }
            | Self::ExportVoxelAnnotation { request_id, .. }
            | Self::QueryVoxelModel { request_id, .. }
            | Self::PrepareVoxelConversion { request_id, .. }
            | Self::ApplyVoxelConversion { request_id, .. }
            | Self::DiscardVoxelConversion { request_id, .. }
            | Self::InspectVoxelObjectSource { request_id, .. }
            | Self::PrepareVoxelObjectConversion { request_id, .. }
            | Self::PreviewVoxelObjectConversion { request_id, .. }
            | Self::ApplyVoxelObjectConversion { request_id, .. }
            | Self::DiscardVoxelObjectConversion { request_id, .. }
            | Self::PrepareVoxelObjectPlacement { request_id, .. }
            | Self::AttachVoxelObjectInstance { request_id, .. }
            | Self::PreviewVoxelObjectInstance { request_id, .. }
            | Self::CloseProject { request_id, .. } => request_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StudioSceneObjectDraft {
    pub entity_id: u64,
    pub name: String,
    pub parent_entity_id: Option<u64>,
    pub child_order: u32,
    pub transform: TransformReadout,
    pub appearance: StudioSceneAppearance,
    pub collision: Option<StoredCollision>,
    pub kinematic: Option<StoredKinematic>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StudioSceneAppearance {
    Empty,
    StaticMesh {
        asset: String,
        visible: bool,
    },
    AnimatedMesh {
        asset: String,
        visible: bool,
        clip: String,
    },
    Light {
        light: StoredLight,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StudioAssetImportSettings {
    pub scale: f32,
    pub generate_collision: bool,
    pub material_namespace: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VoxelBrushMode {
    Paint,
    Erase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VoxelPickFace {
    NegativeX,
    PositiveX,
    NegativeY,
    PositiveY,
    NegativeZ,
    PositiveZ,
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
    ProjectCreated {
        protocol_version: u32,
        request_id: String,
        project: StudioProjectReadout,
    },
    ProjectSavedAs {
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
    ProjectMutationApplied {
        protocol_version: u32,
        request_id: String,
        receipt: ProjectMutationReceipt,
        project: StudioProjectReadout,
    },
    VoxelPickValidated {
        protocol_version: u32,
        request_id: String,
        anchor: VoxelPickReadout,
    },
    VoxelRead {
        protocol_version: u32,
        request_id: String,
        readout: VoxelReadout,
    },
    VoxelConversionPrepared {
        protocol_version: u32,
        request_id: String,
        plan: VoxelConversionPlan,
        preview: VoxelConversionPreview,
    },
    VoxelConversionDiscarded {
        protocol_version: u32,
        request_id: String,
        plan_id: String,
    },
    VoxelObjectSourceInspected {
        protocol_version: u32,
        request_id: String,
        inspection: VoxelObjectSourceInspection,
    },
    VoxelObjectConversionPrepared {
        protocol_version: u32,
        request_id: String,
        plan: VoxelObjectConversionPlan,
        preview: VoxelObjectConversionPreview,
        projection: RenderFrameDiff,
        projection_readout: ProjectionReadout,
    },
    VoxelObjectConversionPreviewed {
        protocol_version: u32,
        request_id: String,
        preview: VoxelObjectConversionPreview,
        projection: RenderFrameDiff,
        projection_readout: ProjectionReadout,
    },
    VoxelObjectConversionDiscarded {
        protocol_version: u32,
        request_id: String,
        plan_id: String,
        projection: RenderFrameDiff,
        projection_readout: ProjectionReadout,
    },
    VoxelObjectPlacementPrepared {
        protocol_version: u32,
        request_id: String,
        asset_id: String,
        object_content_hash: String,
        resource_frame: RenderFrameDiff,
    },
    VoxelObjectInstancePreviewed {
        protocol_version: u32,
        request_id: String,
        playback: VoxelObjectInstancePlaybackReadout,
        projection: RenderFrameDiff,
        projection_readout: ProjectionReadout,
    },
    AssetImportPrepared {
        protocol_version: u32,
        request_id: String,
        plan: AssetImportPlanReadout,
    },
    AssetImportDiscarded {
        protocol_version: u32,
        request_id: String,
        plan_id: String,
    },
    VoxelHistoryRevertPrepared {
        protocol_version: u32,
        request_id: String,
        preview: VoxelHistoryRevertPreview,
    },
    VoxelHistoryRevertDiscarded {
        protocol_version: u32,
        request_id: String,
        preview_id: String,
    },
    VoxelAssetFileExported {
        protocol_version: u32,
        request_id: String,
        asset_id: String,
        target_path: String,
        byte_count: usize,
        sha256: String,
        replaced_existing: bool,
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
    pub operations: Vec<&'static str>,
    pub entity_inspector_contracts: Vec<StudioEntityInspectorContractIdentity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioEntityInspectorContractIdentity {
    pub contract_id: &'static str,
    pub contract_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioEntityComponentReference {
    pub owner_entity_id: u64,
    pub component_type_id: &'static str,
    pub inspector_contract: Option<StudioEntityInspectorContractIdentity>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioProjectReadout {
    pub identity: StudioProjectIdentity,
    pub canonical: CanonicalOwnerContent,
    pub inspections: OwnerInspections,
    pub scene_hierarchy: SceneHierarchyReadout,
    pub asset_browser: AssetBrowserReadout,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voxel: Option<VoxelStateInspection>,
    pub voxel_authoring: VoxelAuthoringReadout,
    pub voxel_object_authoring: VoxelObjectAuthoringReadout,
    pub animated_mesh_resources: Vec<AnimatedMeshResourceReadout>,
    pub entity_components: Vec<StudioEntityComponentReference>,
    pub projection: RenderFrameDiff,
    pub projection_readout: ProjectionReadout,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelObjectSourceInspection {
    pub source_kind: StudioVoxelObjectSourceKind,
    pub source: voxel_convert::MeshSourceRef,
    pub source_path: String,
    pub source_byte_count: u64,
    pub metadata: voxel_convert::MeshSourceMetadata,
    pub clips: Vec<VoxelObjectSourceClipReadout>,
    pub diagnostics: Vec<VoxelObjectSourceDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelObjectSourceClipReadout {
    pub source_animation_index: u32,
    pub name: String,
    pub duration_microseconds: u64,
    pub channel_count: usize,
    pub target_node_indices: Vec<u32>,
    pub properties: Vec<VoxelObjectAnimationProperty>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VoxelObjectAnimationProperty {
    Translation,
    Rotation,
    Scale,
    MorphWeights,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelObjectSourceDiagnostic {
    pub severity: &'static str,
    pub code: &'static str,
    pub path: &'static str,
    pub message: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelObjectAuthoringReadout {
    pub assets: Vec<VoxelObjectAssetAuthoringReadout>,
    pub instances: Vec<VoxelObjectInstanceReadout>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelObjectAssetAuthoringReadout {
    pub asset_id: String,
    pub content_hash: String,
    pub grid: VoxelObjectGridReadout,
    pub bounds: VoxelAssetBounds,
    pub default_frame: VoxelObjectFrameAuthoringReadout,
    pub clips: Vec<VoxelObjectClipAuthoringReadout>,
    pub default_clip: Option<String>,
    pub material_palette: Vec<VoxelAssetMaterialBinding>,
    pub material_map: Vec<VoxelAssetMaterialMapping>,
    pub provenance: VoxelObjectProvenanceReadout,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelObjectProvenanceReadout {
    pub kind: VoxelObjectProvenanceKind,
    pub source_path: String,
    pub source_sha256: String,
    pub source_byte_count: u64,
    pub converter: String,
    pub settings_sha256: String,
    pub license_path: Option<String>,
    pub source_clips: Vec<VoxelObjectSourceClipProvenance>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelObjectGridReadout {
    pub coordinate_system: &'static str,
    pub cell_size: f64,
    pub chunk_size: u32,
    pub pivot: [f64; 3],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelObjectFrameAuthoringReadout {
    pub bounds: VoxelAssetBounds,
    pub voxel_data_hash: String,
    pub voxel_count: usize,
    pub sparse_run_count: usize,
    pub duration_microseconds: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelObjectClipAuthoringReadout {
    pub clip_id: String,
    pub name: Option<String>,
    pub frames_per_second: f64,
    pub frames: Vec<VoxelObjectFrameAuthoringReadout>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelObjectInstanceReadout {
    pub scene_id: String,
    pub owner_entity_id: u64,
    pub instance: StudioVoxelObjectInstance,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelObjectInstancePlaybackReadout {
    pub scene_id: String,
    pub instance_id: String,
    pub voxel_object_asset_id: String,
    pub project_hash: String,
    pub object_content_hash: String,
    pub durable_frame: StoredVoxelObjectFrameSelection,
    pub status: &'static str,
    pub clip_id: Option<String>,
    pub loop_mode: VoxelObjectLoopMode,
    pub rate: VoxelObjectPlaybackRate,
    pub elapsed_microseconds: u64,
    pub runtime_frame: u32,
    pub clip_frame: Option<u32>,
    pub ended: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimatedMeshResourceReadout {
    pub asset: String,
    pub content_hash: String,
    pub clip_ids: Vec<String>,
    pub source_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetBrowserReadout {
    pub assets: Vec<AssetEntryReadout>,
    pub lock_entries: Vec<AssetLockEntryReadout>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetEntryReadout {
    pub asset_id: String,
    pub kind: String,
    pub version: u32,
    pub hash: Option<String>,
    pub source_path: Option<String>,
    pub label: Option<String>,
    pub dependencies: Vec<String>,
    pub dependents: Vec<String>,
    pub material: bool,
    pub imported_mesh: bool,
    pub import: Option<AssetImportReadout>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetImportReadout {
    pub source: StoredImportSource,
    pub source_hash: String,
    pub source_byte_count: u64,
    pub importer_version: u32,
    pub generated_asset_ids: Vec<String>,
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetLockEntryReadout {
    pub asset_id: String,
    pub kind: String,
    pub version: u32,
    pub hash: Option<String>,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetImportPlanReadout {
    pub plan_id: String,
    pub plan_hash: String,
    pub expected_project_hash: String,
    pub source: StudioFileSelection,
    pub source_hash: String,
    pub source_byte_count: u64,
    pub mesh_asset_id: Option<String>,
    pub reimport_kind: Option<String>,
    pub has_errors: bool,
    pub diagnostics: Vec<AssetImportDiagnosticReadout>,
    pub generated_artifacts: Vec<AssetImportArtifactReadout>,
    pub generated_asset_ids: Vec<String>,
    pub settings: StudioAssetImportSettings,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetImportDiagnosticReadout {
    pub severity: String,
    pub code: String,
    pub locus: String,
    pub message: String,
    pub remedy: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetImportArtifactReadout {
    pub relative_path: String,
    pub byte_count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelAuthoringReadout {
    pub assets: Vec<VoxelAssetAuthoringReadout>,
    pub instances: Vec<VoxelInstanceReadout>,
    pub materials: Vec<MaterialAssetReadout>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelAssetAuthoringReadout {
    pub inspection: VoxelAssetInspection,
    pub palette: Vec<VoxelAssetMaterialBinding>,
    pub history: VoxelHistoryReadout,
    pub annotations: Vec<VoxelAnnotationSummaryReadout>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelHistoryReadout {
    pub persisted: bool,
    pub entry_count: usize,
    pub cursor: usize,
    pub undo_depth: usize,
    pub redo_depth: usize,
    pub authority_hash: String,
    pub history_hash: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelAnnotationSummaryReadout {
    pub layer_id: String,
    pub canonical_layer_hash: String,
    pub membership_data_hash: String,
    pub region_count: usize,
    pub assigned_cell_count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelInstanceReadout {
    pub scene_id: String,
    pub instance: StoredVoxelInstance,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialAssetReadout {
    pub asset_id: String,
    pub definition: StoredMaterialDefinition,
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
    pub entity_state: StudioEntityStateInspection,
    pub persistence: PersistenceInspection,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioEntityStateInspection {
    pub schema_version: u32,
    pub revision: u64,
    pub entity_count: usize,
    pub lifecycle: Vec<NamedCount>,
    pub sources: Vec<NamedCount>,
    pub capabilities: Vec<NamedCount>,
    pub relationships: Vec<NamedCount>,
    pub entity_ids: Vec<u64>,
    pub diagnostics: DiagnosticSet,
}

impl From<EntityStateInspection> for StudioEntityStateInspection {
    fn from(value: EntityStateInspection) -> Self {
        Self {
            schema_version: value.schema_version,
            revision: value.revision,
            entity_count: value.entity_count,
            lifecycle: value.lifecycle,
            sources: value.sources,
            capabilities: value.components,
            relationships: value.relationships,
            entity_ids: value.entity_ids,
            diagnostics: value.diagnostics,
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransformReadout {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionReadout {
    pub frame_kind: &'static str,
    pub source_revision: u64,
    pub retained_entities: usize,
    pub retained_lights: usize,
    pub retained_voxel_instances: usize,
    pub retained_voxel_chunks: usize,
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

#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ProjectMutationReceipt {
    SceneCreated {
        scene_id: String,
        made_entry: bool,
    },
    SceneRenamed {
        scene_id: String,
    },
    SceneDeleted {
        scene_id: String,
    },
    EntrySceneSet {
        scene_id: String,
    },
    SceneObjectCreated {
        entity_id: u64,
    },
    SceneObjectDeleted {
        entity_id: u64,
        removed_objects: usize,
    },
    SceneObjectRenamed {
        entity_id: u64,
    },
    SceneObjectReparented {
        entity_id: u64,
    },
    SceneObjectTransformSet {
        entity_id: u64,
    },
    SceneObjectAppearanceSet {
        entity_id: u64,
    },
    EntityCollisionSet {
        entity_id: u64,
        attached: bool,
    },
    EntityKinematicSet {
        entity_id: u64,
        attached: bool,
    },
    MaterialUpserted {
        asset_id: String,
    },
    AssetImportApplied {
        plan_id: String,
        plan_hash: String,
        asset_id: String,
        source_path: String,
        reimport_kind: String,
        generated_asset_ids: Vec<String>,
    },
    VoxelAssetInitialized {
        asset_id: String,
        content_hash: String,
    },
    VoxelAssetDuplicated {
        source_asset_id: String,
        target_asset_id: String,
        content_hash: String,
    },
    VoxelInstanceAttached {
        scene_id: String,
        instance_id: String,
    },
    VoxelInstanceTransformSet {
        scene_id: String,
        instance_id: String,
    },
    VoxelInstanceRemoved {
        scene_id: String,
        instance_id: String,
    },
    VoxelPaletteReplaced {
        asset_id: String,
        content_hash_before: String,
        content_hash_after: String,
        voxel_data_hash: String,
        material_count_before: usize,
        material_count_after: usize,
    },
    VoxelBrushApplied {
        asset_id: String,
        content_hash_before: String,
        content_hash_after: String,
        changed_voxels: usize,
        source_revision: u64,
        history_cursor: usize,
        undo_depth: usize,
        redo_depth: usize,
    },
    VoxelPrimitiveApplied {
        asset_id: String,
        primitive_kind: &'static str,
        content_hash_before: String,
        content_hash_after: String,
        changed_voxels: usize,
        source_revision: u64,
        history_cursor: usize,
        undo_depth: usize,
        redo_depth: usize,
    },
    VoxelTemplateInitialized {
        asset_id: String,
        template_kind: &'static str,
        content_hash: String,
        changed_voxels: usize,
        history_cursor: usize,
    },
    VoxelAssetFileImported {
        source_path: String,
        source_sha256: String,
        source_byte_count: usize,
        source_asset_id: String,
        target_asset_id: String,
        content_hash: String,
    },
    EnvironmentMaterialized {
        scene_id: String,
        preset: &'static str,
        seed: u64,
        asset_id: String,
        instance_id: String,
        content_hash: String,
        voxel_count: usize,
        player_entity_id: u64,
        player_translation: [f32; 3],
        exit_entity_id: u64,
        exit_translation: [f32; 3],
        generator_id: &'static str,
        generator_version: u32,
        settings_sha256: String,
        voxel_data_sha256: String,
    },
    VoxelHistoryMoved {
        asset_id: String,
        content_hash_before: String,
        content_hash_after: String,
        cursor_before: usize,
        cursor_after: usize,
        undo_depth: usize,
        redo_depth: usize,
        changed_voxels: usize,
    },
    VoxelAnnotationCreated {
        asset_id: String,
        layer_id: String,
        layer_hash: String,
    },
    VoxelAnnotationEdited {
        asset_id: String,
        layer_id: String,
        layer_hash_before: String,
        layer_hash_after: String,
        affected_region_ids: Vec<String>,
    },
    VoxelConversionApplied {
        plan_id: String,
        plan_hash: String,
        asset_id: String,
        output_hash: String,
        output_voxels: usize,
    },
    VoxelObjectConversionApplied {
        plan_id: String,
        plan_hash: String,
        asset_id: String,
        output_hash: String,
        stored_frames: usize,
        aggregate_voxels: usize,
    },
    VoxelObjectInstanceAttached {
        scene_id: String,
        instance_id: String,
        asset_id: String,
        frame_kind: &'static str,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelPickReadout {
    pub scene_id: String,
    pub instance_id: String,
    pub asset_id: String,
    /// Asset-local authored voxel coordinate used by authoring operations.
    pub hit_voxel: [i64; 3],
    pub hit_face: VoxelPickFace,
    pub place_voxel: [i64; 3],
    /// Engine-spatial authority coordinates before the asset grid origin is removed.
    pub authority_hit_voxel: [i64; 3],
    pub authority_place_voxel: [i64; 3],
    pub instance_local_point: [f64; 3],
    pub world_point: [f64; 3],
    pub world_distance: f64,
    /// Exact world-space unit-cell transforms for the two authority-approved
    /// mutation targets. Presentation may enlarge these for a brush radius,
    /// but must preserve their center, rotation, and anisotropic scale.
    pub hit_preview_transform: Transform,
    pub place_preview_transform: Transform,
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum VoxelReadout {
    Model {
        info: VoxelModelInfoReadout,
        #[serde(skip_serializing_if = "Option::is_none")]
        window: Box<Option<VoxelModelWindowReadout>>,
    },
    AnnotationQuery {
        layer_hash: String,
        total_layer_regions: usize,
        truncated: bool,
        matched_regions: Vec<VoxelAnnotationRegionReadout>,
    },
    AnnotationExport {
        layer_id: String,
        canonical_json: String,
        canonical_layer_hash: String,
        membership_data_hash: String,
    },
    History {
        asset_id: String,
        cursor: usize,
        undo_depth: usize,
        redo_depth: usize,
        entry_count: usize,
        entries_truncated: bool,
        entries: Vec<VoxelHistoryEntryReadout>,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelHistoryEntryReadout {
    pub transaction_id: u64,
    pub parent_transaction_id: Option<u64>,
    pub before_hash: String,
    pub after_hash: String,
    pub changed_voxels: usize,
    pub deltas_truncated: bool,
    pub deltas: Vec<VoxelEditDelta>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelHistoryRevertPreview {
    pub preview_id: String,
    pub asset_id: String,
    pub expected_project_hash: String,
    pub expected_asset_content_hash: String,
    pub cursor_before: usize,
    pub cursor_after: usize,
    pub undo_depth_after: usize,
    pub redo_depth_after: usize,
    pub revision_before: u64,
    pub revision_after: u64,
    pub changed_voxels: usize,
    pub bounds: Option<VoxelHistoryBoundsReadout>,
    pub material_deltas: Vec<VoxelHistoryMaterialDeltaReadout>,
    pub samples: Vec<VoxelEditDelta>,
    pub samples_truncated: bool,
    pub included_transaction_ids: Vec<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelHistoryBoundsReadout {
    pub min: [i64; 3],
    pub max: [i64; 3],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxelHistoryMaterialDeltaReadout {
    pub before_material: Option<u16>,
    pub after_material: Option<u16>,
    pub changed_voxels: usize,
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
