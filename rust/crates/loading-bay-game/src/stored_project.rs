//! Static authored-project document shapes and format-level validation.
//!
//! This module owns the inspectable candidate format. It deliberately stops
//! before runtime admission: `content` remains the sole place that can turn a
//! document into a live [`crate::GameSession`].

use std::collections::{BTreeMap, BTreeSet};

use asset_catalog::{StoredAssetReference, StoredMaterialDefinition};
use content_store::is_safe_relative_path;
use core_assets::{AssetId, AssetKind};
use engine_spatial::{decode_voxel_edit_history, MaterialVoxel, VoxelEditHistoryLimits};
use render_model::{AnimatedMeshAsset, StaticMeshAsset};
use serde::{Deserialize, Serialize};
use voxel_annotation::{validate_annotation_layer, VoxelAnnotationLayer, VoxelAnnotationLimits};
use voxel_asset::{VoxelAsset, VoxelObjectAsset};
use voxel_object_runtime::{admit_voxel_object, VoxelObjectRuntimeLimits};

use crate::combat::{
    MAX_WEAPON_COOLDOWN_TICKS, MAX_WEAPON_DAMAGE, MAX_WEAPON_MUZZLE_OFFSET, MAX_WEAPON_PELLETS,
    MAX_WEAPON_RANGE, MAX_WEAPON_SPREAD_DEGREES,
};
use crate::inventory::{ItemDefinitionId, MAX_INVENTORY_SLOTS, MAX_ITEM_QUANTITY};

pub const STORED_PROJECT_SCHEMA_VERSION: u32 = 20;
pub const MAX_PROJECT_VOXEL_OBJECT_RESOLVED_CELLS: u64 = 65_536;

pub mod diagnostic_code {
    pub const DECODE: &str = "project.decode";
    pub const ENCODE: &str = "project.encode";
    pub const MIGRATION: &str = "project.migration";
    pub const UNSUPPORTED_SCHEMA: &str = "project.unsupportedSchema";
    pub const INVALID_PROJECT_ID: &str = "project.invalidProjectId";
    pub const INVALID_VALUE: &str = "project.invalidValue";
    pub const INVALID_ASSET_ID: &str = "project.invalidAssetId";
    pub const WRONG_ASSET_KIND: &str = "project.wrongAssetKind";
    pub const DUPLICATE_ASSET: &str = "project.duplicateAsset";
    pub const DUPLICATE_ITEM_DEFINITION: &str = "project.duplicateItemDefinition";
    pub const DUPLICATE_INVENTORY_STACK: &str = "project.duplicateInventoryStack";
    pub const MISSING_ITEM_DEFINITION: &str = "project.missingItemDefinition";
    pub const DUPLICATE_SCENE: &str = "project.duplicateScene";
    pub const MISSING_ENTRY_SCENE: &str = "project.missingEntryScene";
    pub const MISSING_ASSET: &str = "project.missingAsset";
    pub const DUPLICATE_ENTITY: &str = "project.duplicateEntity";
    pub const INVALID_COMPONENT: &str = "project.invalidComponent";
    pub const INVALID_RELATIONSHIP: &str = "project.invalidRelationship";
    pub const INVALID_SPATIAL: &str = "project.invalidSpatial";
    pub const INVALID_VOXEL_ASSET: &str = "project.invalidVoxelAsset";
    pub const INVALID_VOXEL_HISTORY: &str = "project.invalidVoxelHistory";
    pub const INVALID_VOXEL_ANNOTATION: &str = "project.invalidVoxelAnnotation";
    pub const INVALID_VOXEL_INSTANCE: &str = "project.invalidVoxelInstance";
    pub const INVALID_VOXEL_OBJECT: &str = "project.invalidVoxelObject";
    pub const VOXEL_OBJECT_AGGREGATE_LIMIT: &str = "project.voxelObjectAggregateLimit";
    pub const INVALID_VOXEL_OBJECT_INSTANCE: &str = "project.invalidVoxelObjectInstance";
    pub const INVALID_MATERIAL: &str = "project.invalidMaterial";
    pub const INVALID_IMPORT: &str = "project.invalidImport";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredProject {
    pub schema_version: u32,
    pub project_id: String,
    pub name: String,
    pub entry_scene: String,
    pub assets: Vec<StoredAsset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub item_definitions: Vec<StoredItemDefinition>,
    pub scenes: Vec<StoredScene>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredItemDefinition {
    pub id: String,
    pub max_quantity: u32,
    pub kind: StoredItemKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredItemKind {
    Weapon {
        ammunition: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attack_mode: Option<StoredWeaponAttackMode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pellet_count: Option<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spread_degrees: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        damage: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_distance: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cooldown_ticks: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ammunition_cost: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        muzzle_offset: Option<[f32; 3]>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        presentation: Option<String>,
    },
    Ammunition,
    AccessKey,
    HealthSupply {
        restore_health: u32,
    },
    Armor {
        protection: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredWeaponAttackMode {
    Hitscan,
    Spread,
    Automatic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredAsset {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog: Option<StoredAssetCatalogMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub static_mesh: Option<StaticMeshAsset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animated_mesh: Option<AnimatedMeshAsset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import: Option<StoredAssetImport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voxel_volume: Option<VoxelAsset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voxel_object: Option<VoxelObjectAsset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voxel_edit_history: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub voxel_annotations: Vec<VoxelAnnotationLayer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<StoredMaterialDefinition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredAssetCatalogMetadata {
    pub version: u32,
    pub hash: Option<String>,
    pub source_path: Option<String>,
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<StoredAssetReference>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredAssetImport {
    pub source: StoredImportSource,
    pub source_hash: String,
    pub source_byte_count: u64,
    pub importer_version: u32,
    pub manifest_json: String,
    pub sidecar_json: String,
    pub generated_asset_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "scope",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredImportSource {
    Project { path: String },
    Host { path: String },
}

impl StoredImportSource {
    pub fn path(&self) -> &str {
        match self {
            Self::Project { path } | Self::Host { path } => path,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredScene {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voxel_environment: Option<StoredVoxelEnvironment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub voxel_instances: Vec<StoredVoxelInstance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub voxel_object_instances: Vec<StoredVoxelObjectInstance>,
    pub entities: Vec<StoredEntityDefinition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredVoxelInstance {
    pub instance_id: String,
    pub voxel_asset_id: String,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredVoxelObjectInstance {
    pub instance_id: String,
    pub voxel_object_asset_id: String,
    pub frame: StoredVoxelObjectFrameSelection,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    #[serde(default)]
    pub material_overrides: Vec<StoredVoxelObjectMaterialOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredVoxelObjectFrameSelection {
    Default,
    Clip { clip_id: String, frame_index: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredVoxelObjectMaterialOverride {
    pub material_slot: u16,
    pub material_asset_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StoredVoxelEnvironment {
    Solid(StoredSolidVoxelEnvironment),
    Material(StoredMaterialVoxelEnvironment),
    GeneratedRoom(StoredGeneratedVoxelEnvironment),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredSolidVoxelEnvironment {
    pub voxel_size: f64,
    pub chunk_size: u32,
    pub solid_voxels: Vec<[i64; 3]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredMaterialVoxelEnvironment {
    pub voxel_size: f64,
    pub chunk_size: u32,
    pub material_voxels: Vec<StoredMaterialVoxel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub voxel_assets: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredMaterialVoxel {
    pub address: [i64; 3],
    pub material_slot: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredGeneratedVoxelEnvironment {
    pub seed: u64,
    pub voxel_size: f64,
    pub chunk_size: u32,
    pub width: u32,
    pub height: u32,
    pub length: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredEntityDefinition {
    pub id: u64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<u64>,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub child_order: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation: Option<[f32; 3]>,
    #[serde(
        default = "identity_rotation",
        skip_serializing_if = "is_identity_rotation"
    )]
    pub rotation: [f32; 4],
    #[serde(default = "unit_scale", skip_serializing_if = "is_unit_scale")]
    pub scale: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light: Option<StoredLight>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<StoredBounds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collision: Option<StoredCollision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renderable: Option<StoredRenderable>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub door: Option<StoredDoor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch: Option<StoredSwitch>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub enemy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enemy_combat: Option<StoredEnemyCombat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defeat_drop: Option<StoredEnemyDrop>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<StoredHealth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hazard: Option<StoredHazard>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encounter: Option<StoredEncounter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_beacon: Option<StoredExtractionBeacon>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kinematic: Option<StoredKinematic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub navigation: Option<StoredNavigation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_controller: Option<StoredPlayerController>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inventory: Option<StoredInventory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pickup: Option<StoredPickup>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weapon: Option<StoredWeapon>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_region: Option<StoredSecretRegion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level_exit: Option<StoredLevelExit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredInventory {
    pub capacity_slots: usize,
    pub starting_stacks: Vec<StoredInventoryStack>,
    pub initially_equipped_weapon: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weapon_slots: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredInventoryStack {
    pub item: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredPickup {
    pub item: String,
    pub quantity: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starter_ammunition: Option<StoredInventoryStack>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredLight {
    Ambient {
        color: [f32; 3],
        intensity: f32,
        enabled: bool,
        shadows: bool,
    },
    Directional {
        color: [f32; 3],
        intensity: f32,
        enabled: bool,
        shadows: bool,
    },
    Point {
        color: [f32; 3],
        intensity: f32,
        enabled: bool,
        range: Option<f32>,
        decay: f32,
        shadows: bool,
    },
    Spot {
        color: [f32; 3],
        intensity: f32,
        enabled: bool,
        range: Option<f32>,
        decay: f32,
        outer_angle_radians: f32,
        penumbra: f32,
        shadows: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredCollision {
    pub enabled: bool,
    pub static_collider: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredRenderable {
    pub asset: String,
    pub visible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_clip: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredDoor {
    pub open_translation: [f32; 3],
    pub auto_close_after_ticks: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access: Option<StoredDoorAccess>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredDoorAccess {
    pub required_key: String,
    pub key_policy: StoredRequiredKeyPolicy,
    pub activation_radius: f32,
    pub denied_presentation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredRequiredKeyPolicy {
    Retain,
    Consume,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredSwitch {
    pub controls: Vec<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loading_bay_interlock: Option<StoredLoadingBayInterlock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredLoadingBayInterlock {
    pub close_door: u64,
    pub open_door: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredSecretRegion {
    pub presentation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredLevelExit {
    pub activation_radius: f32,
    pub presentation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredEncounter {
    pub members: Vec<u64>,
    pub exit: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activation_radius: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredEnemyDrop {
    pub pickup: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredExtractionBeacon {
    pub activation_radius: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredHealth {
    pub max: u32,
    pub hitbox_half_extents: [f32; 3],
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub max_armor: u32,
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub armor_absorption_percent: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredEnemyCombat {
    pub sight_range: f32,
    pub hearing_range: f32,
    pub attack: StoredEnemyAttack,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredEnemyAttack {
    pub kind: StoredEnemyAttackKind,
    pub damage: u32,
    pub range: f32,
    pub cooldown_ticks: u64,
    pub origin_offset: [f32; 3],
    pub presentation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredEnemyAttackKind {
    Melee,
    RangedHitscan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredHazard {
    pub damage: u32,
    pub cooldown_ticks: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredKinematic {
    pub half_extents: [f32; 3],
    pub velocity: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredNavigation {
    pub goal: [f32; 3],
    pub speed_units_per_second: f32,
    pub max_visited: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredPlayerController {
    pub move_speed_units_per_second: f32,
    pub move_step_seconds: f32,
    pub look_degrees_per_unit: f32,
    pub initial_yaw_degrees: f32,
    pub initial_pitch_degrees: f32,
    pub bindings: StoredPlayerInputBindings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredPlayerInputBindings {
    pub move_forward: String,
    pub move_backward: String,
    pub move_left: String,
    pub move_right: String,
    pub mouse_look: String,
    pub primary_fire: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub select_weapon: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredWeapon {
    pub damage: u32,
    pub max_distance: f32,
    pub cooldown_ticks: u64,
    pub ammo_capacity: u32,
    pub muzzle_offset: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDiagnostic {
    pub code: &'static str,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredProjectError {
    diagnostic: ProjectDiagnostic,
}

impl StoredProjectError {
    pub fn diagnostic(&self) -> &ProjectDiagnostic {
        &self.diagnostic
    }

    pub(crate) fn new(
        code: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        failure(code, path, message)
    }
}

impl std::fmt::Display for StoredProjectError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} at {}: {}",
            self.diagnostic.code, self.diagnostic.path, self.diagnostic.message
        )
    }
}

impl std::error::Error for StoredProjectError {}

/// Decode and validate document-level identities without constructing runtime
/// state. Component invariants and relationships are admitted in one later,
/// all-or-nothing pass by the responsible content owner.
pub fn decode_stored_project(input: &str) -> Result<StoredProject, StoredProjectError> {
    let mut deserializer = serde_json::Deserializer::from_str(input);
    let document: StoredProject =
        serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
            failure(
                diagnostic_code::DECODE,
                json_path(&error.path().to_string()),
                error.inner().to_string(),
            )
        })?;
    deserializer.end().map_err(|error| {
        failure(
            diagnostic_code::DECODE,
            "$",
            format!(
                "{} at line {}, column {}",
                error,
                error.line(),
                error.column()
            ),
        )
    })?;
    validate_stored_project(&document)?;
    Ok(document)
}

pub(crate) fn validate_stored_project(document: &StoredProject) -> Result<(), StoredProjectError> {
    if document.schema_version != STORED_PROJECT_SCHEMA_VERSION {
        return Err(failure(
            diagnostic_code::UNSUPPORTED_SCHEMA,
            "schemaVersion",
            format!(
                "expected schema {}, found {}",
                STORED_PROJECT_SCHEMA_VERSION, document.schema_version
            ),
        ));
    }
    if !is_kebab_segment(&document.project_id) {
        return Err(failure(
            diagnostic_code::INVALID_PROJECT_ID,
            "projectId",
            "project identity must be one kebab-case segment",
        ));
    }
    if document.name.trim().is_empty() {
        return Err(failure(
            diagnostic_code::INVALID_VALUE,
            "name",
            "project name must not be empty",
        ));
    }
    validate_voxel_object_aggregate_budget(document, None)?;

    let mut item_definitions = BTreeMap::new();
    for (index, definition) in document.item_definitions.iter().enumerate() {
        let path = format!("itemDefinitions[{index}]");
        let id = ItemDefinitionId::parse(definition.id.clone()).map_err(|error| {
            failure(
                diagnostic_code::INVALID_VALUE,
                format!("{path}.id"),
                error.to_string(),
            )
        })?;
        if definition.max_quantity == 0
            || definition.max_quantity > MAX_ITEM_QUANTITY
            || matches!(
                definition.kind,
                StoredItemKind::Weapon { .. } | StoredItemKind::AccessKey
            ) && definition.max_quantity != 1
            || matches!(
                definition.kind,
                StoredItemKind::HealthSupply { restore_health: 0 }
                    | StoredItemKind::Armor { protection: 0 }
            )
        {
            return Err(failure(
                diagnostic_code::INVALID_VALUE,
                format!("{path}.maxQuantity"),
                "item quantity and effect values must be within their concrete kind limits",
            ));
        }
        if let Some(first) = item_definitions.insert(id.as_str().to_string(), index) {
            return Err(failure(
                diagnostic_code::DUPLICATE_ITEM_DEFINITION,
                format!("{path}.id"),
                format!(
                    "item definition `{id}` was already declared at itemDefinitions[{first}].id"
                ),
            ));
        }
    }
    for (index, definition) in document.item_definitions.iter().enumerate() {
        let StoredItemKind::Weapon {
            ammunition,
            attack_mode,
            pellet_count,
            spread_degrees,
            damage,
            max_distance,
            cooldown_ticks,
            ammunition_cost,
            muzzle_offset,
            presentation,
        } = &definition.kind
        else {
            continue;
        };
        let valid_attack_mode = match attack_mode {
            Some(StoredWeaponAttackMode::Hitscan | StoredWeaponAttackMode::Automatic) => {
                pellet_count.is_none() && spread_degrees.is_none()
            }
            Some(StoredWeaponAttackMode::Spread) => {
                pellet_count.is_some_and(|value| (2..=MAX_WEAPON_PELLETS).contains(&value))
                    && spread_degrees.is_some_and(|value| {
                        value.is_finite() && value > 0.0 && value <= MAX_WEAPON_SPREAD_DEGREES
                    })
            }
            None => false,
        };
        let valid_weapon = valid_attack_mode
            && damage.is_some_and(|value| (1..=MAX_WEAPON_DAMAGE).contains(&value))
            && max_distance
                .is_some_and(|value| value.is_finite() && value > 0.0 && value <= MAX_WEAPON_RANGE)
            && cooldown_ticks.is_some_and(|value| value <= MAX_WEAPON_COOLDOWN_TICKS)
            && ammunition_cost.is_some_and(|value| value > 0 && value <= MAX_ITEM_QUANTITY)
            && muzzle_offset.is_some_and(|value| {
                value
                    .into_iter()
                    .all(|axis| axis.is_finite() && axis.abs() <= MAX_WEAPON_MUZZLE_OFFSET)
            })
            && presentation
                .as_ref()
                .is_some_and(|value| !value.is_empty() && value.len() <= 96);
        if !valid_weapon {
            return Err(failure(
                diagnostic_code::INVALID_VALUE,
                format!("itemDefinitions[{index}].kind"),
                "current weapon definitions require valid attackMode, damage, maxDistance, cooldownTicks, ammunitionCost, muzzleOffset, and presentation",
            ));
        }
        ItemDefinitionId::parse(ammunition.clone()).map_err(|error| {
            failure(
                diagnostic_code::INVALID_VALUE,
                format!("itemDefinitions[{index}].kind.ammunition"),
                error.to_string(),
            )
        })?;
        let Some(ammunition_index) = item_definitions.get(ammunition).copied() else {
            return Err(failure(
                diagnostic_code::MISSING_ITEM_DEFINITION,
                format!("itemDefinitions[{index}].kind.ammunition"),
                format!("weapon references missing ammunition `{ammunition}`"),
            ));
        };
        if !matches!(
            document.item_definitions[ammunition_index].kind,
            StoredItemKind::Ammunition
        ) {
            return Err(failure(
                diagnostic_code::INVALID_VALUE,
                format!("itemDefinitions[{index}].kind.ammunition"),
                "weapon ammunition must reference an ammunition item definition",
            ));
        }
    }

    let entry_scene = parse_asset_id(&document.entry_scene, "entryScene")?;
    expect_kind(&entry_scene, AssetKind::Scene, "entryScene")?;

    let mut assets = BTreeMap::new();
    for (index, asset) in document.assets.iter().enumerate() {
        let path = format!("assets[{index}].id");
        let id = parse_asset_id(&asset.id, &path)?;
        if id.kind() == AssetKind::Scene {
            return Err(failure(
                diagnostic_code::WRONG_ASSET_KIND,
                path,
                "scene documents belong in `scenes`, not the asset catalog",
            ));
        }
        if let Some(first) = assets.insert(id.as_str().to_string(), index) {
            return Err(failure(
                diagnostic_code::DUPLICATE_ASSET,
                path,
                format!("asset `{id}` was already declared at assets[{first}].id"),
            ));
        }
        match id.kind() {
            AssetKind::VoxelVolume => validate_stored_voxel_asset(asset, index)?,
            AssetKind::VoxelObject => validate_stored_voxel_object(asset, index)?,
            AssetKind::Material => {
                if asset.material.is_none()
                    || asset.voxel_volume.is_some()
                    || asset.voxel_object.is_some()
                    || asset.voxel_edit_history.is_some()
                    || !asset.voxel_annotations.is_empty()
                    || asset.static_mesh.is_some()
                    || asset.animated_mesh.is_some()
                    || asset.import.is_some()
                {
                    return Err(failure(
                        diagnostic_code::INVALID_MATERIAL,
                        format!("assets[{index}]"),
                        "material assets require exactly one asset-catalog material definition",
                    ));
                }
            }
            AssetKind::StaticMesh => {
                if let Some(mesh) = &asset.static_mesh {
                    if mesh.asset != asset.id {
                        return Err(failure(
                            diagnostic_code::INVALID_IMPORT,
                            format!("assets[{index}].staticMesh.asset"),
                            "static mesh descriptor identity must match the stored asset identity",
                        ));
                    }
                    mesh.validate().map_err(|error| {
                        failure(
                            diagnostic_code::INVALID_IMPORT,
                            format!("assets[{index}].staticMesh"),
                            format!("static mesh descriptor is invalid: {error:?}"),
                        )
                    })?;
                } else if asset.import.is_some() {
                    return Err(failure(
                        diagnostic_code::INVALID_IMPORT,
                        format!("assets[{index}].import"),
                        "an imported static mesh requires its canonical render-model descriptor",
                    ));
                }
                if asset.voxel_volume.is_some()
                    || asset.voxel_object.is_some()
                    || asset.voxel_edit_history.is_some()
                    || !asset.voxel_annotations.is_empty()
                    || asset.material.is_some()
                    || asset.animated_mesh.is_some()
                {
                    return Err(failure(
                        diagnostic_code::WRONG_ASSET_KIND,
                        format!("assets[{index}]"),
                        "static mesh assets cannot carry voxel or material payloads",
                    ));
                }
                if let Some(import) = &asset.import {
                    validate_stored_import(import, &asset.id, index)?;
                }
            }
            AssetKind::AnimatedMesh => {
                let Some(mesh) = &asset.animated_mesh else {
                    return Err(failure(
                        diagnostic_code::INVALID_IMPORT,
                        format!("assets[{index}].animatedMesh"),
                        "an animated mesh requires its canonical render-model descriptor",
                    ));
                };
                if mesh.asset != asset.id {
                    return Err(failure(
                        diagnostic_code::INVALID_IMPORT,
                        format!("assets[{index}].animatedMesh.asset"),
                        "animated mesh descriptor identity must match the stored asset identity",
                    ));
                }
                mesh.validate().map_err(|error| {
                    failure(
                        diagnostic_code::INVALID_IMPORT,
                        format!("assets[{index}].animatedMesh"),
                        format!("animated mesh descriptor is invalid: {error:?}"),
                    )
                })?;
                if mesh.content_hash.is_none() {
                    return Err(failure(
                        diagnostic_code::INVALID_IMPORT,
                        format!("assets[{index}].animatedMesh.contentHash"),
                        "animated mesh resources require a pinned content hash",
                    ));
                }
                let source_path = asset
                    .catalog
                    .as_ref()
                    .and_then(|metadata| metadata.source_path.as_deref());
                if !source_path.is_some_and(is_safe_relative_path) {
                    return Err(failure(
                        diagnostic_code::INVALID_IMPORT,
                        format!("assets[{index}].catalog.sourcePath"),
                        "animated mesh resources require a safe project-relative source path",
                    ));
                }
                if asset.voxel_volume.is_some()
                    || asset.voxel_object.is_some()
                    || asset.voxel_edit_history.is_some()
                    || !asset.voxel_annotations.is_empty()
                    || asset.material.is_some()
                    || asset.static_mesh.is_some()
                    || asset.import.is_some()
                {
                    return Err(failure(
                        diagnostic_code::WRONG_ASSET_KIND,
                        format!("assets[{index}]"),
                        "animated mesh assets cannot carry unrelated payloads",
                    ));
                }
            }
            _ => {
                if asset.voxel_volume.is_some()
                    || asset.voxel_object.is_some()
                    || asset.voxel_edit_history.is_some()
                    || !asset.voxel_annotations.is_empty()
                    || asset.material.is_some()
                    || asset.static_mesh.is_some()
                    || asset.animated_mesh.is_some()
                    || asset.import.is_some()
                {
                    return Err(failure(
                        diagnostic_code::WRONG_ASSET_KIND,
                        format!("assets[{index}]"),
                        "voxel and material payloads must match their typed asset identity",
                    ));
                }
            }
        }
        if asset
            .catalog
            .as_ref()
            .is_some_and(|metadata| metadata.version == 0)
        {
            return Err(failure(
                diagnostic_code::INVALID_VALUE,
                format!("assets[{index}].catalog.version"),
                "catalog versions must be non-zero",
            ));
        }
    }

    for (asset_index, asset) in document.assets.iter().enumerate() {
        let palette = asset
            .voxel_volume
            .as_ref()
            .map(|voxel| (voxel.material_palette.as_slice(), "voxelVolume"))
            .or_else(|| {
                asset
                    .voxel_object
                    .as_ref()
                    .map(|object| (object.material_palette.as_slice(), "voxelObject"))
            });
        let Some((palette, payload_path)) = palette else {
            continue;
        };
        for (binding_index, binding) in palette.iter().enumerate() {
            let Some(material_index) = assets.get(&binding.material_asset_id).copied() else {
                return Err(failure(
                    diagnostic_code::MISSING_ASSET,
                    format!(
                        "assets[{asset_index}].{payload_path}.materialPalette[{binding_index}].materialAssetId"
                    ),
                    format!(
                        "voxel material binding references missing asset `{}`",
                        binding.material_asset_id
                    ),
                ));
            };
            if document.assets[material_index].material.is_none() {
                return Err(failure(
                    diagnostic_code::INVALID_MATERIAL,
                    format!(
                        "assets[{asset_index}].{payload_path}.materialPalette[{binding_index}].materialAssetId"
                    ),
                    "voxel material binding must resolve to an asset-catalog material definition",
                ));
            }
        }
    }

    let mut scenes = BTreeMap::new();
    for (index, scene) in document.scenes.iter().enumerate() {
        let path = format!("scenes[{index}].id");
        let id = parse_asset_id(&scene.id, &path)?;
        expect_kind(&id, AssetKind::Scene, &path)?;
        if scene.name.trim().is_empty() {
            return Err(failure(
                diagnostic_code::INVALID_VALUE,
                format!("scenes[{index}].name"),
                "scene name must not be empty",
            ));
        }
        if let Some(first) = scenes.insert(id.as_str().to_string(), index) {
            return Err(failure(
                diagnostic_code::DUPLICATE_SCENE,
                path,
                format!("scene `{id}` was already declared at scenes[{first}].id"),
            ));
        }
        let mut instance_ids = BTreeMap::new();
        for (instance_index, instance) in scene.voxel_instances.iter().enumerate() {
            let instance_path = format!("scenes[{index}].voxelInstances[{instance_index}]");
            if !is_kebab_segment(&instance.instance_id) {
                return Err(failure(
                    diagnostic_code::INVALID_VOXEL_INSTANCE,
                    format!("{instance_path}.instanceId"),
                    "voxel instance identity must be one kebab-case segment",
                ));
            }
            if let Some(first) = instance_ids.insert(&instance.instance_id, instance_index) {
                return Err(failure(
                    diagnostic_code::INVALID_VOXEL_INSTANCE,
                    format!("{instance_path}.instanceId"),
                    format!("instance identity was already declared at voxelInstances[{first}]"),
                ));
            }
            let Some(asset_index) = assets.get(&instance.voxel_asset_id).copied() else {
                return Err(failure(
                    diagnostic_code::MISSING_ASSET,
                    format!("{instance_path}.voxelAssetId"),
                    format!(
                        "voxel instance references missing asset `{}`",
                        instance.voxel_asset_id
                    ),
                ));
            };
            if document.assets[asset_index].voxel_volume.is_none() {
                return Err(failure(
                    diagnostic_code::INVALID_VOXEL_INSTANCE,
                    format!("{instance_path}.voxelAssetId"),
                    "voxel instance must reference a canonical voxel-volume asset",
                ));
            }
            validate_voxel_instance_transform(instance, &instance_path)?;
        }
        let mut object_instance_ids = BTreeMap::new();
        for (instance_index, instance) in scene.voxel_object_instances.iter().enumerate() {
            let instance_path = format!("scenes[{index}].voxelObjectInstances[{instance_index}]");
            if !is_kebab_segment(&instance.instance_id) {
                return Err(failure(
                    diagnostic_code::INVALID_VOXEL_OBJECT_INSTANCE,
                    format!("{instance_path}.instanceId"),
                    "voxel-object instance identity must be one kebab-case segment",
                ));
            }
            if let Some(first) = object_instance_ids.insert(&instance.instance_id, instance_index) {
                return Err(failure(
                    diagnostic_code::INVALID_VOXEL_OBJECT_INSTANCE,
                    format!("{instance_path}.instanceId"),
                    format!(
                        "instance identity was already declared at voxelObjectInstances[{first}]"
                    ),
                ));
            }
            let Some(asset_index) = assets.get(&instance.voxel_object_asset_id).copied() else {
                return Err(failure(
                    diagnostic_code::MISSING_ASSET,
                    format!("{instance_path}.voxelObjectAssetId"),
                    format!(
                        "voxel-object instance references missing asset `{}`",
                        instance.voxel_object_asset_id
                    ),
                ));
            };
            let Some(object) = document.assets[asset_index].voxel_object.as_ref() else {
                return Err(failure(
                    diagnostic_code::INVALID_VOXEL_OBJECT_INSTANCE,
                    format!("{instance_path}.voxelObjectAssetId"),
                    "voxel-object instance must reference a canonical voxel-object asset",
                ));
            };
            validate_voxel_object_instance(instance, object, &assets, document, &instance_path)?;
        }
        validate_scene_entities(scene, index, document, &assets)?;
    }
    if !scenes.contains_key(entry_scene.as_str()) {
        return Err(failure(
            diagnostic_code::MISSING_ENTRY_SCENE,
            "entryScene",
            format!(
                "entry scene `{}` is not present in `scenes`",
                entry_scene.as_str()
            ),
        ));
    }
    Ok(())
}

pub(crate) fn validate_voxel_object_aggregate_budget(
    document: &StoredProject,
    replacement: Option<&VoxelObjectAsset>,
) -> Result<(), StoredProjectError> {
    let replacement_id = replacement.map(|object| object.asset_id.as_str());
    let mut resolved_cells = 0_u64;
    for object in document
        .assets
        .iter()
        .filter_map(|asset| asset.voxel_object.as_ref())
        .filter(|object| replacement_id != Some(object.asset_id.as_str()))
    {
        resolved_cells = add_voxel_object_cells(resolved_cells, object)?;
    }
    if let Some(replacement) = replacement {
        resolved_cells = add_voxel_object_cells(resolved_cells, replacement)?;
    }
    if resolved_cells > MAX_PROJECT_VOXEL_OBJECT_RESOLVED_CELLS {
        return Err(failure(
            diagnostic_code::VOXEL_OBJECT_AGGREGATE_LIMIT,
            if replacement.is_some() {
                "voxelObjectCandidate"
            } else {
                "assets"
            },
            format!(
                "voxel-object frames resolve {resolved_cells} aggregate cells; project limit is {MAX_PROJECT_VOXEL_OBJECT_RESOLVED_CELLS}"
            ),
        ));
    }
    Ok(())
}

fn add_voxel_object_cells(
    mut total: u64,
    object: &VoxelObjectAsset,
) -> Result<u64, StoredProjectError> {
    total = add_voxel_frame_cells(total, &object.default_frame)?;
    for clip in &object.clips {
        for frame in &clip.frames {
            total = add_voxel_frame_cells(total, &frame.frame)?;
        }
    }
    Ok(total)
}

fn add_voxel_frame_cells(
    total: u64,
    frame: &voxel_asset::VoxelFrame,
) -> Result<u64, StoredProjectError> {
    frame
        .representation
        .sparse_runs
        .iter()
        .try_fold(total, |total, run| {
            total.checked_add(u64::from(run.length)).ok_or_else(|| {
                failure(
                    diagnostic_code::VOXEL_OBJECT_AGGREGATE_LIMIT,
                    "assets",
                    "voxel-object aggregate cell count overflowed",
                )
            })
        })
}

fn validate_stored_import(
    import: &StoredAssetImport,
    asset_id: &str,
    asset_index: usize,
) -> Result<(), StoredProjectError> {
    let path = format!("assets[{asset_index}].import");
    let manifest =
        asset_import::decode_import_manifest(&import.manifest_json).map_err(|error| {
            failure(
                diagnostic_code::INVALID_IMPORT,
                format!("{path}.manifestJson.{}", error.path),
                error.message,
            )
        })?;
    let sidecar = asset_import::decode_sidecar(&import.sidecar_json).map_err(|error| {
        failure(
            diagnostic_code::INVALID_IMPORT,
            format!("{path}.sidecarJson.{}", error.path),
            error.message,
        )
    })?;
    if manifest.mesh_asset_id != asset_id
        || manifest.source_hash != import.source_hash
        || sidecar.source_hash != import.source_hash
        || manifest.importer_version != import.importer_version
        || sidecar.importer_version != import.importer_version
    {
        return Err(failure(
            diagnostic_code::INVALID_IMPORT,
            path,
            "stored source, manifest, sidecar, and mesh identities must agree",
        ));
    }
    if import.generated_asset_ids.is_empty()
        || !import.generated_asset_ids.iter().any(|id| id == asset_id)
    {
        return Err(failure(
            diagnostic_code::INVALID_IMPORT,
            format!("assets[{asset_index}].import.generatedAssetIds"),
            "generated asset identities must include the imported mesh",
        ));
    }
    Ok(())
}

fn validate_scene_entities(
    scene: &StoredScene,
    scene_index: usize,
    project: &StoredProject,
    assets: &BTreeMap<String, usize>,
) -> Result<(), StoredProjectError> {
    let mut entities = BTreeMap::new();
    for (entity_index, entity) in scene.entities.iter().enumerate() {
        let root = format!("scenes[{scene_index}].entities[{entity_index}]");
        if entity.name.trim().is_empty() {
            return Err(failure(
                diagnostic_code::INVALID_VALUE,
                format!("{root}.name"),
                "entity name must not be empty",
            ));
        }
        if let Some(first) = entities.insert(entity.id, entity_index) {
            return Err(failure(
                diagnostic_code::DUPLICATE_ENTITY,
                format!("{root}.id"),
                format!(
                    "entity {} was already declared at scenes[{scene_index}].entities[{first}].id",
                    entity.id
                ),
            ));
        }
        if entity.parent == Some(entity.id) {
            return Err(failure(
                diagnostic_code::INVALID_RELATIONSHIP,
                format!("{root}.parent"),
                "entity cannot be its own parent",
            ));
        }
        validate_entity_transform(entity, &root)?;
        if let Some(bounds) = entity.bounds {
            let valid =
                bounds.min.into_iter().chain(bounds.max).all(|value| {
                    value.is_finite() && value.abs() <= entity_state::MAX_ABS_TRANSLATION
                }) && bounds
                    .min
                    .into_iter()
                    .zip(bounds.max)
                    .all(|(minimum, maximum)| minimum <= maximum);
            if !valid {
                return Err(failure(
                    diagnostic_code::INVALID_COMPONENT,
                    format!("{root}.bounds"),
                    "entity bounds must be finite, ordered, and within the spatial limit",
                ));
            }
        }
        if entity.light.is_some() && entity.renderable.is_some() {
            return Err(failure(
                diagnostic_code::INVALID_COMPONENT,
                root,
                "a scene object cannot be both a light and a renderable mesh",
            ));
        }
        if let Some(renderable) = &entity.renderable {
            let Some(asset_index) = assets.get(&renderable.asset).copied() else {
                return Err(failure(
                    diagnostic_code::MISSING_ASSET,
                    format!("{root}.renderable.asset"),
                    format!("renderable references missing asset `{}`", renderable.asset),
                ));
            };
            let asset = &project.assets[asset_index];
            if let Some(animated) = &asset.animated_mesh {
                let clip = renderable
                    .initial_clip
                    .as_ref()
                    .or(animated.default_clip.as_ref());
                let Some(clip) = clip else {
                    return Err(failure(
                        diagnostic_code::INVALID_COMPONENT,
                        format!("{root}.renderable.initialClip"),
                        "animated renderables require an initial clip or asset default clip",
                    ));
                };
                if !animated.clips.iter().any(|candidate| candidate.id == *clip) {
                    return Err(failure(
                        diagnostic_code::INVALID_COMPONENT,
                        format!("{root}.renderable.initialClip"),
                        format!("animated mesh has no clip `{clip}`"),
                    ));
                }
            } else if parse_asset_id(&asset.id, &format!("{root}.renderable.asset"))?.kind()
                == AssetKind::StaticMesh
            {
                if renderable.initial_clip.is_some() {
                    return Err(failure(
                        diagnostic_code::INVALID_COMPONENT,
                        format!("{root}.renderable.initialClip"),
                        "static meshes cannot select animation clips",
                    ));
                }
            } else {
                return Err(failure(
                    diagnostic_code::WRONG_ASSET_KIND,
                    format!("{root}.renderable.asset"),
                    "renderable must reference a static or animated mesh asset",
                ));
            }
        }
        if let Some(light) = entity.light {
            validate_stored_light(light, entity.scale, &format!("{root}.light"))?;
        }
        if let Some(inventory) = &entity.inventory {
            if inventory.capacity_slots == 0 || inventory.capacity_slots > MAX_INVENTORY_SLOTS {
                return Err(failure(
                    diagnostic_code::INVALID_COMPONENT,
                    format!("{root}.inventory.capacitySlots"),
                    "inventory capacity must be within the bounded slot limit",
                ));
            }
            if inventory.starting_stacks.len() > inventory.capacity_slots {
                return Err(failure(
                    diagnostic_code::INVALID_COMPONENT,
                    format!("{root}.inventory.startingStacks"),
                    "starting stacks exceed inventory capacity",
                ));
            }
            let mut stacks = BTreeSet::new();
            for (stack_index, stack) in inventory.starting_stacks.iter().enumerate() {
                let stack_path = format!("{root}.inventory.startingStacks[{stack_index}]");
                ItemDefinitionId::parse(stack.item.clone()).map_err(|error| {
                    failure(
                        diagnostic_code::INVALID_VALUE,
                        format!("{stack_path}.item"),
                        error.to_string(),
                    )
                })?;
                let Some(definition_index) = project
                    .item_definitions
                    .iter()
                    .position(|item| item.id == stack.item)
                else {
                    return Err(failure(
                        diagnostic_code::MISSING_ITEM_DEFINITION,
                        format!("{stack_path}.item"),
                        format!("starting stack references missing item `{}`", stack.item),
                    ));
                };
                if !stacks.insert(&stack.item) {
                    return Err(failure(
                        diagnostic_code::DUPLICATE_INVENTORY_STACK,
                        format!("{stack_path}.item"),
                        "an inventory can contain at most one stack per item definition",
                    ));
                }
                let definition = &project.item_definitions[definition_index];
                if stack.quantity == 0 || stack.quantity > definition.max_quantity {
                    return Err(failure(
                        diagnostic_code::INVALID_COMPONENT,
                        format!("{stack_path}.quantity"),
                        format!(
                            "starting quantity must be between 1 and {}",
                            definition.max_quantity
                        ),
                    ));
                }
            }
            if let Some(equipped) = &inventory.initially_equipped_weapon {
                let Some(definition) = project
                    .item_definitions
                    .iter()
                    .find(|definition| definition.id == *equipped)
                else {
                    return Err(failure(
                        diagnostic_code::MISSING_ITEM_DEFINITION,
                        format!("{root}.inventory.initiallyEquippedWeapon"),
                        format!("equipped weapon references missing item `{equipped}`"),
                    ));
                };
                if !matches!(definition.kind, StoredItemKind::Weapon { .. })
                    || !inventory
                        .starting_stacks
                        .iter()
                        .any(|stack| stack.item == *equipped)
                {
                    return Err(failure(
                        diagnostic_code::INVALID_COMPONENT,
                        format!("{root}.inventory.initiallyEquippedWeapon"),
                        "equipped weapon must be an owned weapon item",
                    ));
                }
            }
            let mut weapon_slots = BTreeSet::new();
            for (slot_index, item) in inventory.weapon_slots.iter().enumerate() {
                let path = format!("{root}.inventory.weaponSlots[{slot_index}]");
                if !weapon_slots.insert(item) {
                    return Err(failure(
                        diagnostic_code::INVALID_COMPONENT,
                        path,
                        "weapon slots must not repeat an item identity",
                    ));
                }
                let Some(definition) = project
                    .item_definitions
                    .iter()
                    .find(|definition| definition.id == *item)
                else {
                    return Err(failure(
                        diagnostic_code::MISSING_ITEM_DEFINITION,
                        path,
                        format!("weapon slot references missing item `{item}`"),
                    ));
                };
                if !matches!(definition.kind, StoredItemKind::Weapon { .. }) {
                    return Err(failure(
                        diagnostic_code::INVALID_COMPONENT,
                        path,
                        "weapon slots must reference weapon item definitions",
                    ));
                }
            }
            if inventory
                .initially_equipped_weapon
                .as_ref()
                .is_some_and(|equipped| !inventory.weapon_slots.contains(equipped))
            {
                return Err(failure(
                    diagnostic_code::INVALID_COMPONENT,
                    format!("{root}.inventory.initiallyEquippedWeapon"),
                    "equipped weapon must occupy an authored weapon slot",
                ));
            }
        }
        if let Some(pickup) = &entity.pickup {
            ItemDefinitionId::parse(pickup.item.clone()).map_err(|error| {
                failure(
                    diagnostic_code::INVALID_VALUE,
                    format!("{root}.pickup.item"),
                    error.to_string(),
                )
            })?;
            let Some(definition) = project
                .item_definitions
                .iter()
                .find(|definition| definition.id == pickup.item)
            else {
                return Err(failure(
                    diagnostic_code::MISSING_ITEM_DEFINITION,
                    format!("{root}.pickup.item"),
                    format!("pickup references missing item `{}`", pickup.item),
                ));
            };
            if pickup.quantity == 0 || pickup.quantity > definition.max_quantity {
                return Err(failure(
                    diagnostic_code::INVALID_COMPONENT,
                    format!("{root}.pickup.quantity"),
                    format!(
                        "pickup quantity must be between 1 and {}",
                        definition.max_quantity
                    ),
                ));
            }
            if let Some(starter) = &pickup.starter_ammunition {
                let StoredItemKind::Weapon { ammunition, .. } = &definition.kind else {
                    return Err(failure(
                        diagnostic_code::INVALID_COMPONENT,
                        format!("{root}.pickup.starterAmmunition"),
                        "only a weapon pickup can declare starter ammunition",
                    ));
                };
                if starter.item != *ammunition {
                    return Err(failure(
                        diagnostic_code::INVALID_COMPONENT,
                        format!("{root}.pickup.starterAmmunition.item"),
                        "starter ammunition must match the picked-up weapon definition",
                    ));
                }
                let Some(ammunition_definition) = project
                    .item_definitions
                    .iter()
                    .find(|candidate| candidate.id == starter.item)
                else {
                    return Err(failure(
                        diagnostic_code::MISSING_ITEM_DEFINITION,
                        format!("{root}.pickup.starterAmmunition.item"),
                        format!(
                            "starter ammunition references missing item `{}`",
                            starter.item
                        ),
                    ));
                };
                if starter.quantity == 0 || starter.quantity > ammunition_definition.max_quantity {
                    return Err(failure(
                        diagnostic_code::INVALID_COMPONENT,
                        format!("{root}.pickup.starterAmmunition.quantity"),
                        "starter ammunition quantity is outside its definition limit",
                    ));
                }
            }
            if entity.translation.is_none()
                || entity.bounds.is_none()
                || entity.renderable.is_none()
            {
                return Err(failure(
                    diagnostic_code::INVALID_COMPONENT,
                    format!("{root}.pickup"),
                    "pickup entities require an authored translation, bounds, and renderable",
                ));
            }
            if entity.collision.is_some() || entity.kinematic.is_some() {
                return Err(failure(
                    diagnostic_code::INVALID_COMPONENT,
                    format!("{root}.pickup"),
                    "pickup trigger bounds must remain non-solid and non-kinematic",
                ));
            }
            if entity.door.is_some()
                || entity.switch.is_some()
                || entity.enemy
                || entity.health.is_some()
                || entity.encounter.is_some()
                || entity.extraction_beacon.is_some()
                || entity.navigation.is_some()
                || entity.player_controller.is_some()
                || entity.inventory.is_some()
                || entity.weapon.is_some()
            {
                return Err(failure(
                    diagnostic_code::INVALID_COMPONENT,
                    format!("{root}.pickup"),
                    "pickup cannot also own another gameplay behavior",
                ));
            }
        }
    }

    for (entity_index, entity) in scene.entities.iter().enumerate() {
        if let Some(parent) = entity.parent {
            if !entities.contains_key(&parent) {
                return Err(failure(
                    diagnostic_code::INVALID_RELATIONSHIP,
                    format!("scenes[{scene_index}].entities[{entity_index}].parent"),
                    format!("parent entity {parent} does not exist in this scene"),
                ));
            }
        }
        let mut cursor = entity.parent;
        let mut visited = BTreeSet::from([entity.id]);
        while let Some(parent) = cursor {
            if !visited.insert(parent) {
                return Err(failure(
                    diagnostic_code::INVALID_RELATIONSHIP,
                    format!("scenes[{scene_index}].entities[{entity_index}].parent"),
                    "entity parent relationship contains a cycle",
                ));
            }
            cursor = scene.entities[entities[&parent]].parent;
        }
    }
    Ok(())
}

fn validate_entity_transform(
    entity: &StoredEntityDefinition,
    root: &str,
) -> Result<(), StoredProjectError> {
    if entity
        .translation
        .is_some_and(|value| value.iter().any(|coordinate| !coordinate.is_finite()))
        || entity.rotation.iter().any(|value| !value.is_finite())
        || entity
            .scale
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err(failure(
            diagnostic_code::INVALID_COMPONENT,
            format!("{root}.transform"),
            "entity transform must be finite with positive scale",
        ));
    }
    let rotation_norm = entity
        .rotation
        .iter()
        .map(|value| value * value)
        .sum::<f32>();
    if (rotation_norm - 1.0).abs() > 0.001 {
        return Err(failure(
            diagnostic_code::INVALID_COMPONENT,
            format!("{root}.rotation"),
            "entity rotation must be a normalized quaternion",
        ));
    }
    Ok(())
}

fn validate_stored_light(
    light: StoredLight,
    scale: [f32; 3],
    path: &str,
) -> Result<(), StoredProjectError> {
    if scale != unit_scale() {
        return Err(failure(
            diagnostic_code::INVALID_COMPONENT,
            path,
            "light scene objects require unit scale",
        ));
    }
    let (color, intensity, range, decay, spot) = match light {
        StoredLight::Ambient {
            color, intensity, ..
        }
        | StoredLight::Directional {
            color, intensity, ..
        } => (color, intensity, None, None, None),
        StoredLight::Point {
            color,
            intensity,
            range,
            decay,
            ..
        } => (color, intensity, range, Some(decay), None),
        StoredLight::Spot {
            color,
            intensity,
            range,
            decay,
            outer_angle_radians,
            penumbra,
            ..
        } => (
            color,
            intensity,
            range,
            Some(decay),
            Some((outer_angle_radians, penumbra)),
        ),
    };
    if color
        .iter()
        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
        || !intensity.is_finite()
        || intensity < 0.0
        || range.is_some_and(|value| !value.is_finite() || value <= 0.0)
        || decay.is_some_and(|value| !value.is_finite() || value < 0.0)
        || spot.is_some_and(|(angle, penumbra)| {
            !angle.is_finite()
                || angle <= 0.0
                || angle > std::f32::consts::FRAC_PI_2
                || !penumbra.is_finite()
                || !(0.0..=1.0).contains(&penumbra)
        })
    {
        return Err(failure(
            diagnostic_code::INVALID_COMPONENT,
            path,
            "light values are outside the admitted range",
        ));
    }
    Ok(())
}

fn validate_stored_voxel_asset(
    asset: &StoredAsset,
    index: usize,
) -> Result<(), StoredProjectError> {
    if asset.material.is_some()
        || asset.voxel_object.is_some()
        || asset.static_mesh.is_some()
        || asset.animated_mesh.is_some()
        || asset.import.is_some()
    {
        return Err(failure(
            diagnostic_code::WRONG_ASSET_KIND,
            format!("assets[{index}]"),
            "voxel-volume assets cannot carry unrelated payloads",
        ));
    }
    let Some(voxel) = &asset.voxel_volume else {
        return Err(failure(
            diagnostic_code::INVALID_VOXEL_ASSET,
            format!("assets[{index}].voxelVolume"),
            "voxel-volume identity requires an embedded canonical voxel asset",
        ));
    };
    if voxel.asset_id != asset.id {
        return Err(failure(
            diagnostic_code::INVALID_VOXEL_ASSET,
            format!("assets[{index}].voxelVolume.assetId"),
            format!(
                "embedded voxel asset identity {:?} does not match catalog identity {:?}",
                voxel.asset_id, asset.id
            ),
        ));
    }
    if let Err(error) = voxel_asset::validate_voxel_asset(voxel) {
        let diagnostic = error
            .diagnostics()
            .first()
            .expect("voxel asset error has a diagnostic");
        return Err(failure(
            diagnostic_code::INVALID_VOXEL_ASSET,
            format!("assets[{index}].voxelVolume.{}", diagnostic.path),
            format!("{}: {}", diagnostic.code, diagnostic.message),
        ));
    }
    if let Some(encoded) = &asset.voxel_edit_history {
        let restored = decode_voxel_edit_history(encoded, VoxelEditHistoryLimits::default())
            .map_err(|error| {
                failure(
                    diagnostic_code::INVALID_VOXEL_HISTORY,
                    format!("assets[{index}].voxelEditHistory"),
                    error.to_string(),
                )
            })?;
        if restored.scene.material_voxels() != expand_voxel_asset(voxel)?.as_slice() {
            return Err(failure(
                diagnostic_code::INVALID_VOXEL_HISTORY,
                format!("assets[{index}].voxelEditHistory"),
                "history cursor authority does not match the embedded voxel asset",
            ));
        }
    }
    for (layer_index, layer) in asset.voxel_annotations.iter().enumerate() {
        validate_annotation_layer(layer, Some(voxel), VoxelAnnotationLimits::default()).map_err(
            |error| {
                failure(
                    diagnostic_code::INVALID_VOXEL_ANNOTATION,
                    format!("assets[{index}].voxelAnnotations[{layer_index}]"),
                    error.to_string(),
                )
            },
        )?;
    }
    Ok(())
}

fn validate_stored_voxel_object(
    asset: &StoredAsset,
    index: usize,
) -> Result<(), StoredProjectError> {
    let Some(object) = &asset.voxel_object else {
        return Err(failure(
            diagnostic_code::INVALID_VOXEL_OBJECT,
            format!("assets[{index}].voxelObject"),
            "voxel-object identity requires an embedded canonical voxel object",
        ));
    };
    if object.asset_id != asset.id {
        return Err(failure(
            diagnostic_code::INVALID_VOXEL_OBJECT,
            format!("assets[{index}].voxelObject.assetId"),
            "embedded voxel-object identity must match the stored asset identity",
        ));
    }
    if asset.voxel_volume.is_some()
        || asset.voxel_edit_history.is_some()
        || !asset.voxel_annotations.is_empty()
        || asset.material.is_some()
        || asset.static_mesh.is_some()
        || asset.animated_mesh.is_some()
        || asset.import.is_some()
    {
        return Err(failure(
            diagnostic_code::WRONG_ASSET_KIND,
            format!("assets[{index}]"),
            "voxel-object assets cannot carry unrelated payloads",
        ));
    }
    admit_voxel_object(object, VoxelObjectRuntimeLimits::default()).map_err(|error| {
        failure(
            diagnostic_code::INVALID_VOXEL_OBJECT,
            format!("assets[{index}].voxelObject"),
            error.to_string(),
        )
    })?;
    Ok(())
}

pub(crate) fn expand_voxel_asset(
    asset: &VoxelAsset,
) -> Result<Vec<MaterialVoxel>, StoredProjectError> {
    let mut voxels = Vec::new();
    for run in &asset.representation.sparse_runs {
        for offset in 0..run.length {
            let local_x = run.start[0].checked_add(i64::from(offset)).ok_or_else(|| {
                failure(
                    diagnostic_code::INVALID_VOXEL_ASSET,
                    "representation",
                    "sparse run overflowed",
                )
            })?;
            let mut address = [local_x, run.start[1], run.start[2]];
            for (axis, coordinate) in address.iter_mut().enumerate() {
                *coordinate = asset.grid.origin[axis]
                    .checked_add(*coordinate)
                    .ok_or_else(|| {
                        failure(
                            diagnostic_code::INVALID_VOXEL_ASSET,
                            "grid.origin",
                            "voxel origin mapping overflowed",
                        )
                    })?;
            }
            voxels.push(MaterialVoxel {
                address,
                material_slot: run.material_slot,
            });
        }
    }
    voxels.sort_unstable_by_key(|voxel| voxel.address);
    Ok(voxels)
}

fn validate_voxel_instance_transform(
    instance: &StoredVoxelInstance,
    path: &str,
) -> Result<(), StoredProjectError> {
    if instance.translation.iter().any(|value| !value.is_finite())
        || instance.rotation.iter().any(|value| !value.is_finite())
        || instance
            .scale
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err(failure(
            diagnostic_code::INVALID_VOXEL_INSTANCE,
            path,
            "voxel instance transform must be finite with positive scale",
        ));
    }
    let rotation_norm = instance
        .rotation
        .iter()
        .map(|value| value * value)
        .sum::<f32>();
    if (rotation_norm - 1.0).abs() > 0.002 {
        return Err(failure(
            diagnostic_code::INVALID_VOXEL_INSTANCE,
            format!("{path}.rotation"),
            "voxel instance rotation must be a normalized quaternion",
        ));
    }
    Ok(())
}

fn validate_voxel_object_instance(
    instance: &StoredVoxelObjectInstance,
    object: &VoxelObjectAsset,
    assets: &BTreeMap<String, usize>,
    project: &StoredProject,
    path: &str,
) -> Result<(), StoredProjectError> {
    if instance.translation.iter().any(|value| !value.is_finite())
        || instance.rotation.iter().any(|value| !value.is_finite())
        || instance
            .scale
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err(failure(
            diagnostic_code::INVALID_VOXEL_OBJECT_INSTANCE,
            path,
            "voxel-object instance transform must be finite with positive scale",
        ));
    }
    let rotation_norm = instance
        .rotation
        .iter()
        .map(|value| value * value)
        .sum::<f32>();
    if (rotation_norm - 1.0).abs() > 0.002 {
        return Err(failure(
            diagnostic_code::INVALID_VOXEL_OBJECT_INSTANCE,
            format!("{path}.rotation"),
            "voxel-object instance rotation must be a normalized quaternion",
        ));
    }
    match &instance.frame {
        StoredVoxelObjectFrameSelection::Default => {}
        StoredVoxelObjectFrameSelection::Clip {
            clip_id,
            frame_index,
        } => {
            let Some(clip) = object.clip(clip_id) else {
                return Err(failure(
                    diagnostic_code::INVALID_VOXEL_OBJECT_INSTANCE,
                    format!("{path}.frame.clipId"),
                    format!("voxel object has no clip `{clip_id}`"),
                ));
            };
            if *frame_index as usize >= clip.frames.len() {
                return Err(failure(
                    diagnostic_code::INVALID_VOXEL_OBJECT_INSTANCE,
                    format!("{path}.frame.frameIndex"),
                    format!(
                        "frame {} is outside clip `{clip_id}` with {} frames",
                        frame_index,
                        clip.frames.len()
                    ),
                ));
            }
        }
    }
    let bound_slots = object
        .material_palette
        .iter()
        .map(|binding| binding.material_slot)
        .collect::<BTreeSet<_>>();
    let mut override_slots = BTreeSet::new();
    for (override_index, binding) in instance.material_overrides.iter().enumerate() {
        let override_path = format!("{path}.materialOverrides[{override_index}]");
        if !bound_slots.contains(&binding.material_slot)
            || !override_slots.insert(binding.material_slot)
        {
            return Err(failure(
                diagnostic_code::INVALID_VOXEL_OBJECT_INSTANCE,
                format!("{override_path}.materialSlot"),
                "material override slots must be bound by the object and unique",
            ));
        }
        let Some(material_index) = assets.get(&binding.material_asset_id).copied() else {
            return Err(failure(
                diagnostic_code::MISSING_ASSET,
                format!("{override_path}.materialAssetId"),
                format!(
                    "material override references missing asset `{}`",
                    binding.material_asset_id
                ),
            ));
        };
        if project.assets[material_index].material.is_none() {
            return Err(failure(
                diagnostic_code::INVALID_MATERIAL,
                format!("{override_path}.materialAssetId"),
                "material override must reference a material definition",
            ));
        }
    }
    Ok(())
}

fn parse_asset_id(value: &str, path: &str) -> Result<AssetId, StoredProjectError> {
    AssetId::parse(value)
        .map_err(|error| failure(diagnostic_code::INVALID_ASSET_ID, path, error.to_string()))
}

fn expect_kind(id: &AssetId, expected: AssetKind, path: &str) -> Result<(), StoredProjectError> {
    if id.kind() == expected {
        return Ok(());
    }
    Err(failure(
        diagnostic_code::WRONG_ASSET_KIND,
        path,
        format!("expected `{}` identity, found `{}`", expected, id.kind()),
    ))
}

fn failure(
    code: &'static str,
    path: impl Into<String>,
    message: impl Into<String>,
) -> StoredProjectError {
    StoredProjectError {
        diagnostic: ProjectDiagnostic {
            code,
            path: path.into(),
            message: message.into(),
        },
    }
}

fn json_path(path: &str) -> String {
    if path.is_empty() || path == "." {
        "$".to_string()
    } else {
        path.trim_start_matches('.').to_string()
    }
}

fn is_false(value: &bool) -> bool {
    !value
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

fn is_zero_u8(value: &u8) -> bool {
    *value == 0
}

const fn identity_rotation() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

fn is_identity_rotation(value: &[f32; 4]) -> bool {
    *value == identity_rotation()
}

const fn unit_scale() -> [f32; 3] {
    [1.0; 3]
}

fn is_unit_scale(value: &[f32; 3]) -> bool {
    *value == unit_scale()
}

fn is_kebab_segment(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    let mut previous_hyphen = true;
    for character in value.chars() {
        match character {
            'a'..='z' | '0'..='9' => previous_hyphen = false,
            '-' if !previous_hyphen => previous_hyphen = true,
            _ => return false,
        }
    }
    !previous_hyphen
}
