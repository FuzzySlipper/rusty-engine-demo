//! Canonical authored-project encoding and explicit predecessor migrations.
//!
//! This is intentionally not a runtime snapshot codec. It accepts only the
//! static [`StoredProject`] shape and never observes a [`crate::GameRuntime`].

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use voxel_asset::{canonicalize_voxel_asset, canonicalize_voxel_object};

use crate::content::PROJECT_CONTENT_SCHEMA_VERSION;
use crate::stored_project::{
    decode_stored_project, diagnostic_code, validate_stored_project, StoredAsset,
    StoredEntityDefinition, StoredGeneratedVoxelEnvironment, StoredInventory, StoredInventoryStack,
    StoredItemDefinition, StoredItemKind, StoredProject, StoredProjectError, StoredScene,
    StoredSolidVoxelEnvironment, StoredVoxelEnvironment, StoredWeaponAttackMode,
    STORED_PROJECT_SCHEMA_VERSION,
};

pub const MIGRATED_V6_PROJECT_ID: &str = "migrated-v6-project";
pub const MIGRATED_V6_SCENE_ID: &str = "scene/migrated-v6-entry";
const PREVIOUS_STORED_PROJECT_SCHEMA_VERSION: u32 = 23;
const LEGACY_V22_STORED_PROJECT_SCHEMA_VERSION: u32 = 22;
const LEGACY_V21_STORED_PROJECT_SCHEMA_VERSION: u32 = 21;
const LEGACY_V20_STORED_PROJECT_SCHEMA_VERSION: u32 = 20;
const LEGACY_V19_STORED_PROJECT_SCHEMA_VERSION: u32 = 19;
const LEGACY_V18_STORED_PROJECT_SCHEMA_VERSION: u32 = 18;
const LEGACY_V17_STORED_PROJECT_SCHEMA_VERSION: u32 = 17;
const LEGACY_V16_STORED_PROJECT_SCHEMA_VERSION: u32 = 16;
const LEGACY_V15_STORED_PROJECT_SCHEMA_VERSION: u32 = 15;
const LEGACY_V14_STORED_PROJECT_SCHEMA_VERSION: u32 = 14;
const LEGACY_V13_STORED_PROJECT_SCHEMA_VERSION: u32 = 13;
const LEGACY_V12_STORED_PROJECT_SCHEMA_VERSION: u32 = 12;
const LEGACY_V11_STORED_PROJECT_SCHEMA_VERSION: u32 = 11;
const LEGACY_V10_STORED_PROJECT_SCHEMA_VERSION: u32 = 10;
const LEGACY_V9_STORED_PROJECT_SCHEMA_VERSION: u32 = 9;
const LEGACY_V8_STORED_PROJECT_SCHEMA_VERSION: u32 = 8;
const LEGACY_V7_STORED_PROJECT_SCHEMA_VERSION: u32 = 7;

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
        PREVIOUS_STORED_PROJECT_SCHEMA_VERSION => {
            reject_schema_twenty_three_renderable_transform_field(input)?;
            migrate_v23(decode_legacy_project(input)?)?
        }
        LEGACY_V22_STORED_PROJECT_SCHEMA_VERSION => {
            reject_schema_twenty_two_visual_binding_field(input)?;
            migrate_v22(decode_legacy_project(input)?)?
        }
        LEGACY_V21_STORED_PROJECT_SCHEMA_VERSION => {
            reject_schema_twenty_one_gameplay_proxy_field(input)?;
            migrate_v21(decode_legacy_project(input)?)?
        }
        LEGACY_V20_STORED_PROJECT_SCHEMA_VERSION => {
            reject_schema_twenty_instances_without_owners(input)?;
            migrate_v20(decode_legacy_project(input)?)?
        }
        LEGACY_V19_STORED_PROJECT_SCHEMA_VERSION => migrate_v19(decode_legacy_project(input)?)?,
        LEGACY_V18_STORED_PROJECT_SCHEMA_VERSION => migrate_v18(decode_legacy_project(input)?)?,
        LEGACY_V17_STORED_PROJECT_SCHEMA_VERSION => migrate_v17(decode_legacy_project(input)?)?,
        LEGACY_V16_STORED_PROJECT_SCHEMA_VERSION => migrate_v16(decode_legacy_project(input)?)?,
        LEGACY_V15_STORED_PROJECT_SCHEMA_VERSION => migrate_v15(decode_legacy_project(input)?)?,
        LEGACY_V14_STORED_PROJECT_SCHEMA_VERSION => migrate_v14(decode_legacy_project(input)?)?,
        LEGACY_V13_STORED_PROJECT_SCHEMA_VERSION => migrate_v13(decode_legacy_project(input)?)?,
        LEGACY_V12_STORED_PROJECT_SCHEMA_VERSION => migrate_v12(decode_legacy_project(input)?)?,
        LEGACY_V11_STORED_PROJECT_SCHEMA_VERSION => migrate_v11(decode_legacy_project(input)?)?,
        LEGACY_V10_STORED_PROJECT_SCHEMA_VERSION => migrate_v10(decode_legacy_project(input)?)?,
        LEGACY_V9_STORED_PROJECT_SCHEMA_VERSION => migrate_v9(decode_legacy_project(input)?)?,
        LEGACY_V8_STORED_PROJECT_SCHEMA_VERSION => migrate_v8(decode_legacy_project(input)?)?,
        LEGACY_V7_STORED_PROJECT_SCHEMA_VERSION => migrate_v7(decode_legacy_project(input)?)?,
        PROJECT_CONTENT_SCHEMA_VERSION => migrate_v6(decode_v6(input)?)?,
        actual => {
            return Err(StoredProjectError::new(
                diagnostic_code::UNSUPPORTED_SCHEMA,
                "schemaVersion",
                format!(
                    "supported project schemas are {} through {}; found {actual}",
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
    let document: LegacyProjectV6 =
        serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
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

fn decode_legacy_project(input: &str) -> Result<StoredProject, StoredProjectError> {
    let mut deserializer = serde_json::Deserializer::from_str(input);
    let document: StoredProject =
        serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
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
    if document.schema_version <= LEGACY_V16_STORED_PROJECT_SCHEMA_VERSION {
        reject_future_progression_fields(&document)?;
    }
    if document.schema_version <= LEGACY_V17_STORED_PROJECT_SCHEMA_VERSION {
        reject_future_enemy_combat_fields(&document)?;
    }
    if document.schema_version <= LEGACY_V18_STORED_PROJECT_SCHEMA_VERSION {
        reject_future_enemy_archetype_fields(&document)?;
    }
    if document.schema_version <= LEGACY_V19_STORED_PROJECT_SCHEMA_VERSION {
        reject_future_voxel_object_fields(&document)?;
    }
    Ok(document)
}

fn migrate_v20(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        LEGACY_V20_STORED_PROJECT_SCHEMA_VERSION
    );
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn migrate_v21(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        LEGACY_V21_STORED_PROJECT_SCHEMA_VERSION
    );
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn migrate_v22(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        LEGACY_V22_STORED_PROJECT_SCHEMA_VERSION
    );
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn migrate_v23(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        PREVIOUS_STORED_PROJECT_SCHEMA_VERSION
    );
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn reject_schema_twenty_three_renderable_transform_field(
    input: &str,
) -> Result<(), StoredProjectError> {
    let value: serde_json::Value = serde_json::from_str(input).map_err(|error| {
        StoredProjectError::new(diagnostic_code::DECODE, "$", error.to_string())
    })?;
    if value
        .get("scenes")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|scenes| {
            scenes.iter().any(|scene| {
                scene
                    .get("entities")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|entities| {
                        entities.iter().any(|entity| {
                            entity.get("renderable").is_some_and(|renderable| {
                                renderable.get("localTransform").is_some()
                            })
                        })
                    })
            })
        })
    {
        return Err(StoredProjectError::new(
            diagnostic_code::DECODE,
            "scenes[].entities[].renderable.localTransform",
            "schema 23 cannot declare renderable local transforms",
        ));
    }
    Ok(())
}

fn reject_schema_twenty_two_visual_binding_field(input: &str) -> Result<(), StoredProjectError> {
    let value: serde_json::Value = serde_json::from_str(input).map_err(|error| {
        StoredProjectError::new(diagnostic_code::DECODE, "$", error.to_string())
    })?;
    if value
        .get("scenes")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|scenes| {
            scenes.iter().any(|scene| {
                scene
                    .get("entities")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|entities| {
                        entities.iter().any(|entity| {
                            entity
                                .get("renderable")
                                .and_then(serde_json::Value::as_object)
                                .is_some_and(|renderable| renderable.contains_key("visualBinding"))
                        })
                    })
            })
        })
    {
        return Err(StoredProjectError::new(
            diagnostic_code::MIGRATION,
            "scenes[].entities[].renderable.visualBinding",
            "schema 22 cannot declare visualBinding",
        ));
    }
    Ok(())
}

fn reject_schema_twenty_one_gameplay_proxy_field(input: &str) -> Result<(), StoredProjectError> {
    let value: serde_json::Value = serde_json::from_str(input).map_err(|error| {
        StoredProjectError::new(diagnostic_code::DECODE, "$", error.to_string())
    })?;
    if value
        .get("scenes")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|scenes| {
            scenes.iter().any(|scene| {
                scene
                    .get("voxelEnvironment")
                    .and_then(serde_json::Value::as_object)
                    .is_some_and(|environment| environment.contains_key("gameplayProxy"))
            })
        })
    {
        return Err(StoredProjectError::new(
            diagnostic_code::UNSUPPORTED_SCHEMA,
            "scenes[].voxelEnvironment.gameplayProxy",
            "schema 21 cannot declare the schema-22 gameplay proxy role",
        ));
    }
    Ok(())
}

fn reject_schema_twenty_instances_without_owners(input: &str) -> Result<(), StoredProjectError> {
    let value: serde_json::Value = serde_json::from_str(input).map_err(|error| {
        StoredProjectError::new(diagnostic_code::DECODE, "$", error.to_string())
    })?;
    if value
        .get("scenes")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|scenes| {
            scenes.iter().any(|scene| {
                scene
                    .get("voxelObjectInstances")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|instances| !instances.is_empty())
            })
        })
    {
        return Err(StoredProjectError::new(
            diagnostic_code::MIGRATION,
            "scenes",
            "schema 20 voxel-object instances have no explicit entity owner and cannot migrate",
        ));
    }
    Ok(())
}

fn migrate_v19(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        LEGACY_V19_STORED_PROJECT_SCHEMA_VERSION
    );
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn migrate_v18(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        LEGACY_V18_STORED_PROJECT_SCHEMA_VERSION
    );
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn reject_future_voxel_object_fields(legacy: &StoredProject) -> Result<(), StoredProjectError> {
    if legacy
        .assets
        .iter()
        .any(|asset| asset.voxel_object.is_some())
        || legacy
            .scenes
            .iter()
            .any(|scene| !scene.voxel_object_instances.is_empty())
    {
        return Err(StoredProjectError::new(
            diagnostic_code::MIGRATION,
            "$",
            "schema 19 projects cannot carry schema 20 voxel-object fields",
        ));
    }
    Ok(())
}

fn migrate_v17(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        LEGACY_V17_STORED_PROJECT_SCHEMA_VERSION
    );
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn migrate_v16(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        LEGACY_V16_STORED_PROJECT_SCHEMA_VERSION
    );
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn reject_future_enemy_combat_fields(legacy: &StoredProject) -> Result<(), StoredProjectError> {
    if legacy
        .scenes
        .iter()
        .flat_map(|scene| &scene.entities)
        .any(|entity| entity.enemy_combat.is_some())
    {
        return Err(StoredProjectError::new(
            diagnostic_code::MIGRATION,
            "scenes",
            format!(
                "schema {} cannot declare enemyCombat",
                legacy.schema_version
            ),
        ));
    }
    Ok(())
}

fn reject_future_enemy_archetype_fields(legacy: &StoredProject) -> Result<(), StoredProjectError> {
    if legacy
        .scenes
        .iter()
        .flat_map(|scene| &scene.entities)
        .any(|entity| {
            entity.defeat_drop.is_some()
                || entity
                    .encounter
                    .as_ref()
                    .is_some_and(|encounter| encounter.activation_radius.is_some())
        })
    {
        return Err(StoredProjectError::new(
            diagnostic_code::MIGRATION,
            "scenes",
            format!(
                "schema {} cannot declare defeatDrop or encounter activationRadius",
                legacy.schema_version
            ),
        ));
    }
    Ok(())
}

fn migrate_v15(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        LEGACY_V15_STORED_PROJECT_SCHEMA_VERSION
    );
    reject_future_weapon_behavior_fields(&legacy)?;
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn reject_future_progression_fields(legacy: &StoredProject) -> Result<(), StoredProjectError> {
    if legacy
        .scenes
        .iter()
        .flat_map(|scene| &scene.entities)
        .any(|entity| {
            entity
                .door
                .as_ref()
                .is_some_and(|door| door.access.is_some())
                || entity
                    .switch
                    .as_ref()
                    .is_some_and(|switch| switch.loading_bay_interlock.is_some())
                || entity.secret_region.is_some()
                || entity.level_exit.is_some()
        })
    {
        return Err(StoredProjectError::new(
            diagnostic_code::MIGRATION,
            "scenes",
            format!(
                "schema {} cannot declare key access, loading-bay interlocks, secrets, or level exits",
                legacy.schema_version
            ),
        ));
    }
    Ok(())
}

fn migrate_v14(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        LEGACY_V14_STORED_PROJECT_SCHEMA_VERSION
    );
    reject_future_weapon_behavior_fields(&legacy)?;
    reject_future_vitality_fields(&legacy)?;
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn migrate_v13(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        LEGACY_V13_STORED_PROJECT_SCHEMA_VERSION
    );
    reject_future_vitality_fields(&legacy)?;
    reject_future_weapon_fields(&legacy)?;
    migrate_legacy_weapon_authority(&mut legacy)?;
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn migrate_v12(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        LEGACY_V12_STORED_PROJECT_SCHEMA_VERSION
    );
    reject_future_vitality_fields(&legacy)?;
    reject_future_pickup_fields(&legacy)?;
    reject_future_weapon_fields(&legacy)?;
    migrate_legacy_weapon_authority(&mut legacy)?;
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn migrate_v11(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        LEGACY_V11_STORED_PROJECT_SCHEMA_VERSION
    );
    reject_future_vitality_fields(&legacy)?;
    reject_future_inventory_fields(&legacy)?;
    reject_future_pickup_fields(&legacy)?;
    reject_future_weapon_fields(&legacy)?;
    migrate_legacy_weapon_authority(&mut legacy)?;
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn migrate_v10(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        LEGACY_V10_STORED_PROJECT_SCHEMA_VERSION
    );
    reject_future_vitality_fields(&legacy)?;
    reject_future_inventory_fields(&legacy)?;
    reject_future_pickup_fields(&legacy)?;
    reject_future_weapon_fields(&legacy)?;
    migrate_legacy_weapon_authority(&mut legacy)?;
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn migrate_v9(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        LEGACY_V9_STORED_PROJECT_SCHEMA_VERSION
    );
    reject_future_vitality_fields(&legacy)?;
    reject_future_inventory_fields(&legacy)?;
    reject_future_pickup_fields(&legacy)?;
    reject_future_weapon_fields(&legacy)?;
    migrate_legacy_weapon_authority(&mut legacy)?;
    assign_legacy_child_order(&mut legacy);
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn migrate_v8(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        LEGACY_V8_STORED_PROJECT_SCHEMA_VERSION
    );
    reject_future_vitality_fields(&legacy)?;
    reject_future_inventory_fields(&legacy)?;
    reject_future_pickup_fields(&legacy)?;
    reject_future_weapon_fields(&legacy)?;
    migrate_legacy_weapon_authority(&mut legacy)?;
    assign_legacy_child_order(&mut legacy);
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn migrate_v7(mut legacy: StoredProject) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(
        legacy.schema_version,
        LEGACY_V7_STORED_PROJECT_SCHEMA_VERSION
    );
    reject_future_vitality_fields(&legacy)?;
    reject_future_inventory_fields(&legacy)?;
    reject_future_pickup_fields(&legacy)?;
    reject_future_weapon_fields(&legacy)?;
    migrate_legacy_weapon_authority(&mut legacy)?;
    if legacy.scenes.iter().any(|scene| {
        scene
            .entities
            .iter()
            .any(|entity| entity.extraction_beacon.is_some())
    }) {
        return Err(StoredProjectError::new(
            diagnostic_code::MIGRATION,
            "scenes",
            "schema 7 cannot declare extractionBeacon",
        ));
    }
    assign_legacy_child_order(&mut legacy);
    legacy.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    canonicalize(legacy)
}

fn migrate_v6(mut legacy: LegacyProjectV6) -> Result<StoredProject, StoredProjectError> {
    debug_assert_eq!(legacy.schema_version, PROJECT_CONTENT_SCHEMA_VERSION);
    if legacy.entities.iter().any(|entity| {
        entity.inventory.is_some()
            || entity.pickup.is_some()
            || entity.bounds.is_some()
            || entity.hazard.is_some()
            || entity
                .door
                .as_ref()
                .is_some_and(|door| door.access.is_some())
            || entity
                .switch
                .as_ref()
                .is_some_and(|switch| switch.loading_bay_interlock.is_some())
            || entity.secret_region.is_some()
            || entity.level_exit.is_some()
            || entity
                .health
                .is_some_and(|health| health.max_armor != 0 || health.armor_absorption_percent != 0)
    }) {
        return Err(StoredProjectError::new(
            diagnostic_code::MIGRATION,
            "entities",
            "schema 6 cannot declare inventory, pickup, or entity bounds",
        ));
    }
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
    for (index, entity) in legacy.entities.iter_mut().enumerate() {
        entity.child_order = index as u32;
    }

    let mut migrated = StoredProject {
        schema_version: STORED_PROJECT_SCHEMA_VERSION,
        project_id: MIGRATED_V6_PROJECT_ID.to_string(),
        name: "Migrated Schema 6 Project".to_string(),
        entry_scene: MIGRATED_V6_SCENE_ID.to_string(),
        assets: asset_ids
            .into_iter()
            .map(|id| StoredAsset {
                id,
                catalog: None,
                static_mesh: None,
                animated_mesh: None,
                import: None,
                voxel_volume: None,
                voxel_object: None,
                voxel_edit_history: None,
                voxel_annotations: Vec::new(),
                material: None,
            })
            .collect(),
        item_definitions: Vec::new(),
        scenes: vec![StoredScene {
            id: MIGRATED_V6_SCENE_ID.to_string(),
            name: "Migrated Schema 6 Entry".to_string(),
            voxel_environment,
            voxel_instances: Vec::new(),
            voxel_object_instances: Vec::new(),
            entities: legacy.entities,
        }],
    };
    migrate_legacy_weapon_authority(&mut migrated)?;
    canonicalize(migrated)
}

fn reject_future_weapon_behavior_fields(legacy: &StoredProject) -> Result<(), StoredProjectError> {
    if legacy.item_definitions.iter().any(|definition| {
        matches!(
            &definition.kind,
            StoredItemKind::Weapon {
                attack_mode: Some(
                    StoredWeaponAttackMode::Spread | StoredWeaponAttackMode::Automatic
                ),
                ..
            } | StoredItemKind::Weapon {
                pellet_count: Some(_),
                ..
            } | StoredItemKind::Weapon {
                spread_degrees: Some(_),
                ..
            }
        )
    }) {
        return Err(StoredProjectError::new(
            diagnostic_code::MIGRATION,
            "itemDefinitions",
            format!(
                "schema {} cannot declare spread or automatic weapon behavior",
                legacy.schema_version
            ),
        ));
    }
    Ok(())
}

fn reject_future_vitality_fields(legacy: &StoredProject) -> Result<(), StoredProjectError> {
    if legacy
        .scenes
        .iter()
        .flat_map(|scene| &scene.entities)
        .any(|entity| {
            entity.hazard.is_some()
                || entity.health.is_some_and(|health| {
                    health.max_armor != 0 || health.armor_absorption_percent != 0
                })
        })
    {
        return Err(StoredProjectError::new(
            diagnostic_code::MIGRATION,
            "scenes",
            format!(
                "schema {} cannot declare armor vitality or hazard fields",
                legacy.schema_version
            ),
        ));
    }
    Ok(())
}

fn reject_future_weapon_fields(legacy: &StoredProject) -> Result<(), StoredProjectError> {
    let item_has_future = legacy.item_definitions.iter().any(|definition| {
        matches!(
            &definition.kind,
            StoredItemKind::Weapon {
                attack_mode: Some(_),
                ..
            } | StoredItemKind::Weapon {
                damage: Some(_),
                ..
            } | StoredItemKind::Weapon {
                max_distance: Some(_),
                ..
            } | StoredItemKind::Weapon {
                cooldown_ticks: Some(_),
                ..
            } | StoredItemKind::Weapon {
                ammunition_cost: Some(_),
                ..
            } | StoredItemKind::Weapon {
                muzzle_offset: Some(_),
                ..
            } | StoredItemKind::Weapon {
                presentation: Some(_),
                ..
            }
        )
    });
    let entity_has_future = legacy
        .scenes
        .iter()
        .flat_map(|scene| &scene.entities)
        .any(|entity| {
            entity
                .inventory
                .as_ref()
                .is_some_and(|inventory| !inventory.weapon_slots.is_empty())
                || entity
                    .player_controller
                    .as_ref()
                    .is_some_and(|controller| !controller.bindings.select_weapon.is_empty())
                || entity
                    .pickup
                    .as_ref()
                    .is_some_and(|pickup| pickup.starter_ammunition.is_some())
        });
    if item_has_future || entity_has_future {
        return Err(StoredProjectError::new(
            diagnostic_code::MIGRATION,
            "scenes",
            format!(
                "schema {} cannot declare inventory-backed weapon fields",
                legacy.schema_version
            ),
        ));
    }
    Ok(())
}

fn migrate_legacy_weapon_authority(project: &mut StoredProject) -> Result<(), StoredProjectError> {
    let mut migrated_items = BTreeMap::<String, (StoredItemKind, String)>::new();
    for (scene_index, scene) in project.scenes.iter_mut().enumerate() {
        for (entity_index, entity) in scene.entities.iter_mut().enumerate() {
            let Some(legacy_weapon) = entity.weapon.take() else {
                continue;
            };
            let weapon_path = format!("scenes[{scene_index}].entities[{entity_index}].weapon");
            let Some(controller) = entity.player_controller.as_mut() else {
                return Err(StoredProjectError::new(
                    diagnostic_code::MIGRATION,
                    weapon_path,
                    format!(
                        "entity {} has a legacy weapon without a player controller",
                        entity.id
                    ),
                ));
            };
            if entity.inventory.is_none() {
                let weapon_id = format!("weapon/migrated-player-{}", entity.id);
                let ammunition_id = format!("ammo/migrated-player-{}", entity.id);
                project.item_definitions.push(StoredItemDefinition {
                    id: ammunition_id.clone(),
                    max_quantity: legacy_weapon.ammo_capacity,
                    kind: StoredItemKind::Ammunition,
                });
                project.item_definitions.push(StoredItemDefinition {
                    id: weapon_id.clone(),
                    max_quantity: 1,
                    kind: legacy_weapon_item_kind(&weapon_id, ammunition_id.clone(), legacy_weapon),
                });
                entity.inventory = Some(StoredInventory {
                    capacity_slots: 2,
                    starting_stacks: vec![
                        StoredInventoryStack {
                            item: weapon_id.clone(),
                            quantity: 1,
                        },
                        StoredInventoryStack {
                            item: ammunition_id,
                            quantity: legacy_weapon.ammo_capacity,
                        },
                    ],
                    initially_equipped_weapon: Some(weapon_id.clone()),
                    weapon_slots: vec![weapon_id],
                });
                controller.bindings.select_weapon = vec!["Digit1".to_string()];
            } else if let Some(equipped) = entity
                .inventory
                .as_ref()
                .and_then(|inventory| inventory.initially_equipped_weapon.clone())
            {
                let Some(definition) = project
                    .item_definitions
                    .iter()
                    .find(|definition| definition.id == equipped)
                else {
                    return Err(StoredProjectError::new(
                        diagnostic_code::MIGRATION,
                        "itemDefinitions",
                        format!("legacy equipped weapon `{equipped}` has no item definition"),
                    ));
                };
                let ammunition = match &definition.kind {
                    StoredItemKind::Weapon { ammunition, .. } => ammunition.clone(),
                    _ => {
                        return Err(StoredProjectError::new(
                            diagnostic_code::MIGRATION,
                            "itemDefinitions",
                            format!("legacy equipped item `{equipped}` is not a weapon"),
                        ));
                    }
                };
                let migrated = legacy_weapon_item_kind(&equipped, ammunition, legacy_weapon);
                if let Some((previous, previous_path)) = migrated_items.get(&equipped) {
                    if previous != &migrated {
                        return Err(StoredProjectError::new(
                            diagnostic_code::MIGRATION,
                            weapon_path,
                            format!(
                                "legacy weapon `{equipped}` conflicts with its mapping at {previous_path}"
                            ),
                        ));
                    }
                } else {
                    migrated_items.insert(equipped.clone(), (migrated.clone(), weapon_path));
                }
                project
                    .item_definitions
                    .iter_mut()
                    .find(|definition| definition.id == equipped)
                    .expect("legacy definition was resolved above")
                    .kind = migrated;
            }
        }
    }

    for definition in &mut project.item_definitions {
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
        } = &mut definition.kind
        else {
            continue;
        };
        *attack_mode = Some(StoredWeaponAttackMode::Hitscan);
        *pellet_count = None;
        *spread_degrees = None;
        *damage = Some(damage.unwrap_or(40));
        *max_distance = Some(max_distance.unwrap_or(20.0));
        *cooldown_ticks = Some(cooldown_ticks.unwrap_or(6));
        *ammunition_cost = Some(ammunition_cost.unwrap_or(1));
        *muzzle_offset = Some(muzzle_offset.unwrap_or([0.0, 0.0, 0.0]));
        *presentation = Some(
            presentation
                .clone()
                .unwrap_or_else(|| definition.id.clone()),
        );
        let _ = ammunition;
    }
    let weapon_ids = project
        .item_definitions
        .iter()
        .filter(|definition| matches!(definition.kind, StoredItemKind::Weapon { .. }))
        .map(|definition| definition.id.clone())
        .collect::<Vec<_>>();
    for entity in project
        .scenes
        .iter_mut()
        .flat_map(|scene| &mut scene.entities)
    {
        let Some(inventory) = entity.inventory.as_mut() else {
            continue;
        };
        if inventory.weapon_slots.is_empty() {
            inventory.weapon_slots = weapon_ids.clone();
        }
        if let Some(controller) = entity.player_controller.as_mut() {
            controller.bindings.select_weapon = inventory
                .weapon_slots
                .iter()
                .enumerate()
                .map(|(index, _)| format!("Digit{}", index + 1))
                .collect();
        }
    }
    Ok(())
}

fn legacy_weapon_item_kind(
    presentation: &str,
    ammunition: String,
    weapon: crate::StoredWeapon,
) -> StoredItemKind {
    StoredItemKind::Weapon {
        ammunition,
        attack_mode: Some(StoredWeaponAttackMode::Hitscan),
        pellet_count: None,
        spread_degrees: None,
        damage: Some(weapon.damage),
        max_distance: Some(weapon.max_distance),
        cooldown_ticks: Some(weapon.cooldown_ticks),
        ammunition_cost: Some(1),
        muzzle_offset: Some(weapon.muzzle_offset),
        presentation: Some(presentation.to_string()),
    }
}

fn reject_future_inventory_fields(legacy: &StoredProject) -> Result<(), StoredProjectError> {
    if !legacy.item_definitions.is_empty() {
        return Err(StoredProjectError::new(
            diagnostic_code::MIGRATION,
            "itemDefinitions",
            format!(
                "schema {} cannot declare item definitions",
                legacy.schema_version
            ),
        ));
    }
    if legacy
        .scenes
        .iter()
        .flat_map(|scene| &scene.entities)
        .any(|entity| entity.inventory.is_some())
    {
        return Err(StoredProjectError::new(
            diagnostic_code::MIGRATION,
            "scenes",
            format!("schema {} cannot declare inventory", legacy.schema_version),
        ));
    }
    Ok(())
}

fn reject_future_pickup_fields(legacy: &StoredProject) -> Result<(), StoredProjectError> {
    if legacy
        .scenes
        .iter()
        .flat_map(|scene| &scene.entities)
        .any(|entity| entity.pickup.is_some() || entity.bounds.is_some())
    {
        return Err(StoredProjectError::new(
            diagnostic_code::MIGRATION,
            "scenes",
            format!(
                "schema {} cannot declare pickup or entity bounds",
                legacy.schema_version
            ),
        ));
    }
    Ok(())
}

fn assign_legacy_child_order(project: &mut StoredProject) {
    for scene in &mut project.scenes {
        for (index, entity) in scene.entities.iter_mut().enumerate() {
            entity.parent = None;
            entity.child_order = index as u32;
        }
    }
}

fn migrate_v6_asset_id(asset: &str) -> String {
    asset
        .strip_prefix("primitive/")
        .map_or_else(|| asset.to_string(), |name| format!("mesh/{name}"))
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

fn json_path(path: &str) -> String {
    if path.is_empty() || path == "." {
        "$".to_string()
    } else {
        path.trim_start_matches('.').to_string()
    }
}
