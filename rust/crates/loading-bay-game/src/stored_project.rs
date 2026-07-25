//! Static authored-project document shapes and format-level validation.
//!
//! This module owns the inspectable candidate format. It deliberately stops
//! before runtime admission: `content` remains the sole place that can turn a
//! document into a live [`crate::GameSession`].

use std::collections::BTreeMap;

use asset_catalog::StoredMaterialDefinition;
use core_assets::{AssetId, AssetKind};
use engine_spatial::{decode_voxel_edit_history, MaterialVoxel, VoxelEditHistoryLimits};
use serde::{Deserialize, Serialize};
use voxel_annotation::{validate_annotation_layer, VoxelAnnotationLayer, VoxelAnnotationLimits};
use voxel_asset::VoxelAsset;

pub const STORED_PROJECT_SCHEMA_VERSION: u32 = 9;

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
    pub const INVALID_MATERIAL: &str = "project.invalidMaterial";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredProject {
    pub schema_version: u32,
    pub project_id: String,
    pub name: String,
    pub entry_scene: String,
    pub assets: Vec<StoredAsset>,
    pub scenes: Vec<StoredScene>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredAsset {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voxel_volume: Option<VoxelAsset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voxel_edit_history: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub voxel_annotations: Vec<VoxelAnnotationLayer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<StoredMaterialDefinition>,
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
    pub translation: Option<[f32; 3]>,
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
    pub health: Option<StoredHealth>,
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
    pub weapon: Option<StoredWeapon>,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredDoor {
    pub open_translation: [f32; 3],
    pub auto_close_after_ticks: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredSwitch {
    pub controls: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredEncounter {
    pub members: Vec<u64>,
    pub exit: u64,
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
            AssetKind::Material => {
                if asset.material.is_none()
                    || asset.voxel_volume.is_some()
                    || asset.voxel_edit_history.is_some()
                    || !asset.voxel_annotations.is_empty()
                {
                    return Err(failure(
                        diagnostic_code::INVALID_MATERIAL,
                        format!("assets[{index}]"),
                        "material assets require exactly one asset-catalog material definition",
                    ));
                }
            }
            _ => {
                if asset.voxel_volume.is_some()
                    || asset.voxel_edit_history.is_some()
                    || !asset.voxel_annotations.is_empty()
                    || asset.material.is_some()
                {
                    return Err(failure(
                        diagnostic_code::WRONG_ASSET_KIND,
                        format!("assets[{index}]"),
                        "voxel and material payloads must match their typed asset identity",
                    ));
                }
            }
        }
    }

    for (asset_index, asset) in document.assets.iter().enumerate() {
        let Some(voxel) = &asset.voxel_volume else {
            continue;
        };
        for (binding_index, binding) in voxel.material_palette.iter().enumerate() {
            let Some(material_index) = assets.get(&binding.material_asset_id).copied() else {
                return Err(failure(
                    diagnostic_code::MISSING_ASSET,
                    format!(
                        "assets[{asset_index}].voxelVolume.materialPalette[{binding_index}].materialAssetId"
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
                        "assets[{asset_index}].voxelVolume.materialPalette[{binding_index}].materialAssetId"
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

fn validate_stored_voxel_asset(
    asset: &StoredAsset,
    index: usize,
) -> Result<(), StoredProjectError> {
    if asset.material.is_some() {
        return Err(failure(
            diagnostic_code::WRONG_ASSET_KIND,
            format!("assets[{index}].material"),
            "voxel-volume assets cannot carry a material definition",
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
