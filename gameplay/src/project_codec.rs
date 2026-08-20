//! Canonical current-schema authored-project encoding.
//!
//! Project migration is deliberately not a Loading Bay concern. The sole
//! supported authored content is the current E1M1 schema, whose immutable
//! TypeScript composition is admitted into Rust-owned runtime state.

use rusty_engine::voxel_asset::{canonicalize_voxel_asset, canonicalize_voxel_object};

use crate::stored_project::{
    decode_stored_project, diagnostic_code, validate_stored_project, StoredProject,
    StoredProjectError, StoredVoxelEnvironment, STORED_PROJECT_SCHEMA_VERSION,
};

/// A current authored project together with the schema version actually read.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedProjectDocument {
    pub project: StoredProject,
    pub source_schema_version: u32,
}

/// Decode the sole current authored-project schema. Older documents have no
/// runtime compatibility route: re-author them through the current content
/// builders instead of reconstructing fixed behavior in Rust.
pub fn decode_project_document(input: &str) -> Result<DecodedProjectDocument, StoredProjectError> {
    let source_schema_version = probe_schema_version(input)?;
    if source_schema_version != STORED_PROJECT_SCHEMA_VERSION {
        return Err(StoredProjectError::new(
            diagnostic_code::UNSUPPORTED_SCHEMA,
            "schemaVersion",
            format!(
                "Loading Bay accepts only current project schema {}; found {source_schema_version}",
                STORED_PROJECT_SCHEMA_VERSION
            ),
        ));
    }
    Ok(DecodedProjectDocument {
        project: decode_stored_project(input)?,
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

fn canonicalize(mut document: StoredProject) -> Result<StoredProject, StoredProjectError> {
    normalize_numbers(&mut document)?;
    validate_stored_project(&document)?;
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
        if let Some(voxel_object) = &mut asset.voxel_object {
            *voxel_object = canonicalize_voxel_object(voxel_object).map_err(|error| {
                StoredProjectError::new(
                    diagnostic_code::ENCODE,
                    format!("assets[{asset_index}].voxelObject"),
                    error.to_string(),
                )
            })?;
        }
    }
    document
        .assets
        .sort_by(|left, right| left.id.cmp(&right.id));
    document
        .item_definitions
        .sort_by(|left, right| left.id.cmp(&right.id));
    document
        .scenes
        .sort_by(|left, right| left.id.cmp(&right.id));
    for scene in &mut document.scenes {
        scene.entities.sort_by_key(|entity| entity.id);
        scene
            .voxel_instances
            .sort_by(|left, right| left.instance_id.cmp(&right.instance_id));
        scene
            .voxel_object_instances
            .sort_by(|left, right| left.instance_id.cmp(&right.instance_id));
        for instance in &mut scene.voxel_object_instances {
            instance
                .material_overrides
                .sort_by_key(|binding| binding.material_slot);
        }
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
                component.effects.sort();
                component.effects.dedup();
            }
            if let Some(component) = &mut entity.encounter {
                component.members.sort_unstable();
                component.members.dedup();
            }
        }
    }
    for asset in &mut document.assets {
        asset
            .voxel_annotations
            .sort_by(|left, right| left.layer_id.cmp(&right.layer_id));
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
            normalize_vec4(&mut entity.rotation, format!("{root}.rotation"))?;
            normalize_vec3(&mut entity.scale, format!("{root}.scale"))?;
            if let Some(light) = &mut entity.light {
                normalize_light(light, format!("{root}.light"))?;
            }
            if let Some(bounds) = &mut entity.bounds {
                normalize_vec3(&mut bounds.min, format!("{root}.bounds.min"))?;
                normalize_vec3(&mut bounds.max, format!("{root}.bounds.max"))?;
            }
            if let Some(component) = &mut entity.door {
                normalize_vec3(
                    &mut component.open_translation,
                    format!("{root}.door.openTranslation"),
                )?;
                if let Some(access) = &mut component.access {
                    normalize_f32(
                        &mut access.activation_radius,
                        format!("{root}.door.access.activationRadius"),
                    )?;
                }
            }
            if let Some(component) = &mut entity.floor_action {
                normalize_vec3(
                    &mut component.upper_translation,
                    format!("{root}.floorAction.upperTranslation"),
                )?;
                normalize_vec3(
                    &mut component.lowered_translation,
                    format!("{root}.floorAction.loweredTranslation"),
                )?;
            }
            if let Some(component) = &mut entity.lift {
                normalize_vec3(
                    &mut component.raised_translation,
                    format!("{root}.lift.raisedTranslation"),
                )?;
                normalize_vec3(
                    &mut component.lowered_translation,
                    format!("{root}.lift.loweredTranslation"),
                )?;
            }
            if let Some(component) = &mut entity.health {
                normalize_vec3(
                    &mut component.hitbox_half_extents,
                    format!("{root}.health.hitboxHalfExtents"),
                )?;
            }
            if let Some(component) = &mut entity.enemy_combat {
                normalize_f32(
                    &mut component.sight_range,
                    format!("{root}.enemyCombat.sightRange"),
                )?;
                normalize_f32(
                    &mut component.hearing_range,
                    format!("{root}.enemyCombat.hearingRange"),
                )?;
                normalize_f32(
                    &mut component.attack.range,
                    format!("{root}.enemyCombat.attack.range"),
                )?;
                normalize_vec3(
                    &mut component.attack.origin_offset,
                    format!("{root}.enemyCombat.attack.originOffset"),
                )?;
            }
            if let Some(component) = &mut entity.level_exit {
                normalize_f32(
                    &mut component.activation_radius,
                    format!("{root}.levelExit.activationRadius"),
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
            if let Some(component) = &mut entity.extraction_beacon {
                normalize_f32(
                    &mut component.activation_radius,
                    format!("{root}.extractionBeacon.activationRadius"),
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
        }
        for (instance_index, instance) in scene.voxel_instances.iter_mut().enumerate() {
            let root = format!("scenes[{scene_index}].voxelInstances[{instance_index}]");
            normalize_vec3(&mut instance.translation, format!("{root}.translation"))?;
            normalize_vec4(&mut instance.rotation, format!("{root}.rotation"))?;
            normalize_vec3(&mut instance.scale, format!("{root}.scale"))?;
        }
        for (instance_index, instance) in scene.voxel_object_instances.iter_mut().enumerate() {
            let root = format!("scenes[{scene_index}].voxelObjectInstances[{instance_index}]");
            normalize_vec3(&mut instance.translation, format!("{root}.translation"))?;
            normalize_vec4(&mut instance.rotation, format!("{root}.rotation"))?;
            normalize_vec3(&mut instance.scale, format!("{root}.scale"))?;
        }
    }
    Ok(())
}

fn normalize_light(light: &mut crate::StoredLight, root: String) -> Result<(), StoredProjectError> {
    use crate::StoredLight;
    match light {
        StoredLight::Ambient {
            color, intensity, ..
        }
        | StoredLight::Directional {
            color, intensity, ..
        } => {
            normalize_vec3(color, format!("{root}.color"))?;
            normalize_f32(intensity, format!("{root}.intensity"))?;
        }
        StoredLight::Point {
            color,
            intensity,
            range,
            decay,
            ..
        } => {
            normalize_vec3(color, format!("{root}.color"))?;
            normalize_f32(intensity, format!("{root}.intensity"))?;
            normalize_optional_f32(range, format!("{root}.range"))?;
            normalize_f32(decay, format!("{root}.decay"))?;
        }
        StoredLight::Spot {
            color,
            intensity,
            range,
            decay,
            outer_angle_radians,
            penumbra,
            ..
        } => {
            normalize_vec3(color, format!("{root}.color"))?;
            normalize_f32(intensity, format!("{root}.intensity"))?;
            normalize_optional_f32(range, format!("{root}.range"))?;
            normalize_f32(decay, format!("{root}.decay"))?;
            normalize_f32(outer_angle_radians, format!("{root}.outerAngleRadians"))?;
            normalize_f32(penumbra, format!("{root}.penumbra"))?;
        }
    }
    Ok(())
}

fn normalize_optional_f32(value: &mut Option<f32>, path: String) -> Result<(), StoredProjectError> {
    if let Some(value) = value {
        normalize_f32(value, path)?;
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

fn normalize_vec4(value: &mut [f32; 4], path: String) -> Result<(), StoredProjectError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    const E1M1_PROJECT: &str = include_str!("../../content/projects/doom-e1m1.project.json");

    #[test]
    fn predecessor_schema_is_rejected_without_installing_behavior() {
        let mut predecessor: serde_json::Value = serde_json::from_str(E1M1_PROJECT).unwrap();
        predecessor["schemaVersion"] = 25.into();

        let error = decode_project_document(&predecessor.to_string()).unwrap_err();
        assert_eq!(error.diagnostic().code, diagnostic_code::UNSUPPORTED_SCHEMA);
        assert_eq!(error.diagnostic().path, "schemaVersion");
    }

    #[test]
    fn current_e1m1_schema_decodes_without_migration() {
        let decoded = decode_project_document(E1M1_PROJECT).expect("current E1M1 project");
        assert_eq!(decoded.source_schema_version, STORED_PROJECT_SCHEMA_VERSION);
        assert_eq!(
            decoded.project.schema_version,
            STORED_PROJECT_SCHEMA_VERSION
        );
    }
}
