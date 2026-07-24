//! Canonical authored-project encoding and one explicit predecessor migration.
//!
//! This is intentionally not a runtime snapshot codec. It accepts only the
//! static [`StoredProject`] shape and never observes a [`crate::GameRuntime`].

use std::collections::BTreeSet;

use serde::Deserialize;
use voxel_asset::canonicalize_voxel_asset;

use crate::content::PROJECT_CONTENT_SCHEMA_VERSION;
use crate::stored_project::{
    decode_stored_project, diagnostic_code, validate_stored_project, StoredAsset,
    StoredEntityDefinition, StoredGeneratedVoxelEnvironment, StoredProject, StoredProjectError,
    StoredScene, StoredSolidVoxelEnvironment, StoredVoxelEnvironment,
    STORED_PROJECT_SCHEMA_VERSION,
};

pub const MIGRATED_V6_PROJECT_ID: &str = "migrated-v6-project";
pub const MIGRATED_V6_SCENE_ID: &str = "scene/migrated-v6-entry";

/// A current authored project together with the schema version actually read.
/// A lower source version means Rust performed the documented migration before
/// returning this value.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedProjectDocument {
    pub project: StoredProject,
    pub source_schema_version: u32,
}

impl DecodedProjectDocument {
    pub fn was_migrated(&self) -> bool {
        self.source_schema_version != STORED_PROJECT_SCHEMA_VERSION
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LegacyProjectV6 {
    schema_version: u32,
    entities: Vec<StoredEntityDefinition>,
    voxel_collision: Option<StoredSolidVoxelEnvironment>,
    generated_voxel_environment: Option<StoredGeneratedVoxelEnvironment>,
}

/// Decode a current authored project or migrate the one supported predecessor
/// shape. Unknown, older, and future versions fail closed.
pub fn decode_project_document(input: &str) -> Result<DecodedProjectDocument, StoredProjectError> {
    let source_schema_version = probe_schema_version(input)?;
    let project = match source_schema_version {
        STORED_PROJECT_SCHEMA_VERSION => decode_stored_project(input)?,
        PROJECT_CONTENT_SCHEMA_VERSION => migrate_v6(decode_v6(input)?)?,
        actual => {
            return Err(StoredProjectError::new(
                diagnostic_code::UNSUPPORTED_SCHEMA,
                "schemaVersion",
                format!(
                    "supported project schemas are {} and {}; found {actual}",
                    PROJECT_CONTENT_SCHEMA_VERSION, STORED_PROJECT_SCHEMA_VERSION
                ),
            ));
        }
    };
    Ok(DecodedProjectDocument {
        project,
        source_schema_version,
    })
}

/// Emit canonical pretty JSON with LF line endings and one trailing newline.
/// Struct declaration fixes object-field order; catalog, scene, entity,
/// relationship, and solid-voxel sets are sorted; finite floats use
/// `serde_json`'s shortest round-trip representation; negative zero is
/// normalized to positive zero.
pub fn encode_project_document(document: &StoredProject) -> Result<String, StoredProjectError> {
    let canonical = canonicalize(document.clone())?;
    let mut encoded = serde_json::to_string_pretty(&canonical).map_err(|error| {
        StoredProjectError::new(diagnostic_code::ENCODE, "$", error.to_string())
    })?;
    encoded.push('\n');
    Ok(encoded)
}

fn probe_schema_version(input: &str) -> Result<u32, StoredProjectError> {
    let value: serde_json::Value = serde_json::from_str(input).map_err(|error| {
        StoredProjectError::new(
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
    let Some(version) = value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
    else {
        return Err(StoredProjectError::new(
            diagnostic_code::DECODE,
            "schemaVersion",
            "schemaVersion must be an unsigned integer",
        ));
    };
    u32::try_from(version).map_err(|_| {
        StoredProjectError::new(
            diagnostic_code::UNSUPPORTED_SCHEMA,
            "schemaVersion",
            format!("schema version {version} is outside the supported integer range"),
        )
    })
}

fn decode_v6(input: &str) -> Result<LegacyProjectV6, StoredProjectError> {
    let mut deserializer = serde_json::Deserializer::from_str(input);
    let document = serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
        StoredProjectError::new(
            diagnostic_code::DECODE,
            json_path(&error.path().to_string()),
            error.inner().to_string(),
        )
    })?;
    deserializer.end().map_err(|error| {
        StoredProjectError::new(
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
    Ok(document)
}

fn migrate_v6(mut legacy: LegacyProjectV6) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(legacy.schema_version, PROJECT_CONTENT_SCHEMA_VERSION);
    let voxel_environment = match (
        legacy.voxel_collision.take(),
        legacy.generated_voxel_environment.take(),
    ) {
        (Some(_), Some(_)) => {
            return Err(StoredProjectError::new(
                diagnostic_code::MIGRATION,
                "$",
                "schema 6 declares both voxelCollision and generatedVoxelEnvironment",
            ));
        }
        (Some(environment), None) => Some(StoredVoxelEnvironment::Solid(environment)),
        (None, Some(environment)) => Some(StoredVoxelEnvironment::GeneratedRoom(environment)),
        (None, None) => None,
    };

    let mut asset_ids = BTreeSet::new();
    for entity in &mut legacy.entities {
        if let Some(renderable) = &mut entity.renderable {
            renderable.asset = migrate_v6_asset_id(&renderable.asset);
            asset_ids.insert(renderable.asset.clone());
        }
    }

    canonicalize(StoredProject {
        schema_version: STORED_PROJECT_SCHEMA_VERSION,
        project_id: MIGRATED_V6_PROJECT_ID.to_string(),
        name: "Migrated Schema 6 Project".to_string(),
        entry_scene: MIGRATED_V6_SCENE_ID.to_string(),
        assets: asset_ids
            .into_iter()
            .map(|id| StoredAsset {
                id,
                voxel_volume: None,
            })
            .collect(),
        scenes: vec![StoredScene {
            id: MIGRATED_V6_SCENE_ID.to_string(),
            name: "Migrated Schema 6 Entry".to_string(),
            voxel_environment,
            entities: legacy.entities,
        }],
    })
}

fn migrate_v6_asset_id(asset: &str) -> String {
    asset
        .strip_prefix("primitive/")
        .map_or_else(|| asset.to_string(), |name| format!("mesh/{name}"))
}

fn canonicalize(mut document: StoredProject) -> Result<StoredProject, StoredProjectError> {
    validate_stored_project(&document)?;
    normalize_numbers(&mut document)?;
    for (asset_index, asset) in document.assets.iter_mut().enumerate() {
        if let Some(voxel_volume) = &mut asset.voxel_volume {
            *voxel_volume = canonicalize_voxel_asset(voxel_volume).map_err(|error| {
                let diagnostic = error
                    .diagnostics()
                    .first()
                    .expect("voxel asset error has diagnostic");
                StoredProjectError::new(
                    diagnostic_code::ENCODE,
                    format!("assets[{asset_index}].voxelVolume.{}", diagnostic.path),
                    format!("{}: {}", diagnostic.code, diagnostic.message),
                )
            })?;
        }
    }
    document
        .assets
        .sort_by(|left, right| left.id.cmp(&right.id));
    document
        .scenes
        .sort_by(|left, right| left.id.cmp(&right.id));
    for scene in &mut document.scenes {
        scene.entities.sort_by_key(|entity| entity.id);
        if let Some(StoredVoxelEnvironment::Solid(environment)) = &mut scene.voxel_environment {
            environment.solid_voxels.sort_unstable();
            environment.solid_voxels.dedup();
        }
        if let Some(StoredVoxelEnvironment::Material(environment)) = &mut scene.voxel_environment {
            environment.material_voxels.sort_unstable();
            environment.material_voxels.dedup();
            environment.voxel_assets.sort();
            environment.voxel_assets.dedup();
        }
        for entity in &mut scene.entities {
            if let Some(component) = &mut entity.switch {
                component.controls.sort_unstable();
                component.controls.dedup();
            }
            if let Some(component) = &mut entity.encounter {
                component.members.sort_unstable();
                component.members.dedup();
            }
        }
    }
    Ok(document)
}

fn normalize_numbers(document: &mut StoredProject) -> Result<(), StoredProjectError> {
    for (scene_index, scene) in document.scenes.iter_mut().enumerate() {
        if let Some(environment) = &mut scene.voxel_environment {
            match environment {
                StoredVoxelEnvironment::Solid(environment) => normalize_f64(
                    &mut environment.voxel_size,
                    format!("scenes[{scene_index}].voxelEnvironment.voxelSize"),
                )?,
                StoredVoxelEnvironment::Material(environment) => normalize_f64(
                    &mut environment.voxel_size,
                    format!("scenes[{scene_index}].voxelEnvironment.voxelSize"),
                )?,
                StoredVoxelEnvironment::GeneratedRoom(environment) => normalize_f64(
                    &mut environment.voxel_size,
                    format!("scenes[{scene_index}].voxelEnvironment.voxelSize"),
                )?,
            }
        }
        for (entity_index, entity) in scene.entities.iter_mut().enumerate() {
            let root = format!("scenes[{scene_index}].entities[{entity_index}]");
            normalize_optional_vec3(&mut entity.translation, format!("{root}.translation"))?;
            if let Some(component) = &mut entity.door {
                normalize_vec3(
                    &mut component.open_translation,
                    format!("{root}.door.openTranslation"),
                )?;
            }
            if let Some(component) = &mut entity.health {
                normalize_vec3(
                    &mut component.hitbox_half_extents,
                    format!("{root}.health.hitboxHalfExtents"),
                )?;
            }
            if let Some(component) = &mut entity.kinematic {
                normalize_vec3(
                    &mut component.half_extents,
                    format!("{root}.kinematic.halfExtents"),
                )?;
                normalize_vec3(
                    &mut component.velocity,
                    format!("{root}.kinematic.velocity"),
                )?;
            }
            if let Some(component) = &mut entity.navigation {
                normalize_vec3(&mut component.goal, format!("{root}.navigation.goal"))?;
                normalize_f32(
                    &mut component.speed_units_per_second,
                    format!("{root}.navigation.speedUnitsPerSecond"),
                )?;
            }
            if let Some(component) = &mut entity.player_controller {
                normalize_f32(
                    &mut component.move_speed_units_per_second,
                    format!("{root}.playerController.moveSpeedUnitsPerSecond"),
                )?;
                normalize_f32(
                    &mut component.move_step_seconds,
                    format!("{root}.playerController.moveStepSeconds"),
                )?;
                normalize_f32(
                    &mut component.look_degrees_per_unit,
                    format!("{root}.playerController.lookDegreesPerUnit"),
                )?;
                normalize_f32(
                    &mut component.initial_yaw_degrees,
                    format!("{root}.playerController.initialYawDegrees"),
                )?;
                normalize_f32(
                    &mut component.initial_pitch_degrees,
                    format!("{root}.playerController.initialPitchDegrees"),
                )?;
            }
            if let Some(component) = &mut entity.weapon {
                normalize_f32(
                    &mut component.max_distance,
                    format!("{root}.weapon.maxDistance"),
                )?;
                normalize_vec3(
                    &mut component.muzzle_offset,
                    format!("{root}.weapon.muzzleOffset"),
                )?;
            }
        }
    }
    Ok(())
}

fn normalize_optional_vec3(
    value: &mut Option<[f32; 3]>,
    path: String,
) -> Result<(), StoredProjectError> {
    if let Some(value) = value {
        normalize_vec3(value, path)?;
    }
    Ok(())
}

fn normalize_vec3(value: &mut [f32; 3], path: String) -> Result<(), StoredProjectError> {
    for (index, number) in value.iter_mut().enumerate() {
        normalize_f32(number, format!("{path}[{index}]"))?;
    }
    Ok(())
}

fn normalize_f32(value: &mut f32, path: String) -> Result<(), StoredProjectError> {
    if !value.is_finite() {
        return Err(StoredProjectError::new(
            diagnostic_code::ENCODE,
            path,
            "authored project numbers must be finite",
        ));
    }
    if *value == 0.0 {
        *value = 0.0;
    }
    Ok(())
}

fn normalize_f64(value: &mut f64, path: String) -> Result<(), StoredProjectError> {
    if !value.is_finite() {
        return Err(StoredProjectError::new(
            diagnostic_code::ENCODE,
            path,
            "authored project numbers must be finite",
        ));
    }
    if *value == 0.0 {
        *value = 0.0;
    }
    Ok(())
}

fn json_path(path: &str) -> String {
    if path.is_empty() || path == "." {
        "$".to_string()
    } else {
        path.trim_start_matches('.').to_string()
    }
}
