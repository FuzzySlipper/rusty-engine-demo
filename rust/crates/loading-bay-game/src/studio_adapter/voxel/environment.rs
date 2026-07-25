use authored_scene::SceneTransform;
use core_ids::SceneNodeId;
use core_math::Vec3;
use environment_authoring::{
    materialize_environment, EnvironmentLimits, EnvironmentMarkerTarget,
    EnvironmentMaterializationRequest, EnvironmentTarget, TunnelGeneratorConfig, TunnelPreset,
    TUNNEL_GENERATOR_ID,
};
use voxel_asset::VoxelAssetMaterialBinding;

use crate::{StoredAsset, StoredVoxelInstance};

use super::super::project::{publish_project_mutation, OpenedOwnerProject};
use super::super::protocol::{
    AdapterRejection, ProjectMutationReceipt, StudioEnvironmentPreset, StudioProjectReadout,
};
use super::super::ProjectLocation;
use super::model::{find_asset_mut, find_scene_mut, reject};
use super::query::load_expected;

const ENVIRONMENT_VOXEL_NODE_ID: u64 = u64::MAX - 16;
const ENVIRONMENT_PLAYER_MARKER_NODE_ID: u64 = u64::MAX - 17;
const ENVIRONMENT_EXIT_MARKER_NODE_ID: u64 = u64::MAX - 18;

#[allow(clippy::too_many_arguments)]
pub(crate) fn materialize_project_environment(
    location: &ProjectLocation,
    expected_project_hash: &str,
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
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    if player_entity_id == exit_entity_id {
        return Err(reject(
            "environment.invalidTarget",
            "player and exit targets must be different entities",
        ));
    }
    let current = load_expected(location, expected_project_hash)?;
    if current.scene_revision() != expected_scene_revision {
        return Err(reject(
            "scene.staleRevision",
            format!(
                "expected scene revision {expected_scene_revision}, found {}",
                current.scene_revision()
            ),
        ));
    }
    require_scene_and_entities(&current, &scene_id, player_entity_id, exit_entity_id)?;
    let config = match preset {
        StudioEnvironmentPreset::TinyEnclosed => TunnelGeneratorConfig {
            seed,
            preset: TunnelPreset::TinyEnclosed,
            wall_material,
            floor_material,
            accent_material,
            ..TunnelGeneratorConfig::tiny_enclosed(seed)
        },
    };
    let materialized = materialize_environment(
        current.scene(),
        &EnvironmentMaterializationRequest {
            expected_scene_revision,
            config,
            target: EnvironmentTarget {
                voxel_asset_id: voxel_asset_id.clone(),
                voxel_node_id: SceneNodeId::new(ENVIRONMENT_VOXEL_NODE_ID),
                voxel_parent_id: None,
                voxel_child_order: u32::MAX - 2,
                voxel_label: Some(format!("Generated environment {voxel_instance_id}")),
                voxel_transform: SceneTransform::at(Vec3::new(
                    voxel_translation[0],
                    voxel_translation[1],
                    voxel_translation[2],
                )),
                marker_targets: vec![
                    EnvironmentMarkerTarget {
                        source_marker_id: "player_start".to_string(),
                        node_id: SceneNodeId::new(ENVIRONMENT_PLAYER_MARKER_NODE_ID),
                        marker_id: format!("{voxel_instance_id}/player-start"),
                        child_order: u32::MAX - 1,
                    },
                    EnvironmentMarkerTarget {
                        source_marker_id: "exit_hint".to_string(),
                        node_id: SceneNodeId::new(ENVIRONMENT_EXIT_MARKER_NODE_ID),
                        marker_id: format!("{voxel_instance_id}/exit-hint"),
                        child_order: u32::MAX,
                    },
                ],
            },
            material_palette,
            limits: EnvironmentLimits::default(),
        },
    )
    .map_err(|error| reject(error.code(), error.to_string()))?;
    let player = marker_translation(&materialized, "player_start")?;
    let exit = marker_translation(&materialized, "exit_hint")?;
    let voxel_transform = materialized.voxel_world_transform;
    let content_hash = materialized.asset.content_hash.clone();
    let voxel_count = materialized.generation.voxels.len();
    let provenance = materialized.generation.provenance.clone();
    let candidate_asset = materialized.asset;

    let published =
        publish_project_mutation(location, expected_project_hash, move |_, project| {
            install_environment_asset(project, voxel_asset_id.clone(), candidate_asset)?;
            let scene = find_scene_mut(project, &scene_id)?;
            install_environment_instance(
                scene,
                voxel_instance_id.clone(),
                voxel_asset_id.clone(),
                voxel_transform,
            )?;
            set_entity_translation(scene, player_entity_id, player)?;
            set_entity_translation(scene, exit_entity_id, exit)?;
            Ok(ProjectMutationReceipt::EnvironmentMaterialized {
                scene_id,
                preset: provenance.preset,
                seed,
                asset_id: voxel_asset_id,
                instance_id: voxel_instance_id,
                content_hash,
                voxel_count,
                player_entity_id,
                player_translation: player,
                exit_entity_id,
                exit_translation: exit,
                generator_id: provenance.generator_id,
                generator_version: provenance.generator_version,
                settings_sha256: provenance.settings_sha256,
                voxel_data_sha256: provenance.voxel_data_sha256,
            })
        })?;
    Ok((published.value, published.readout))
}

fn require_scene_and_entities(
    project: &OpenedOwnerProject,
    scene_id: &str,
    player_entity_id: u64,
    exit_entity_id: u64,
) -> Result<(), AdapterRejection> {
    let scene = project
        .document()
        .scenes
        .iter()
        .find(|scene| scene.id == scene_id)
        .ok_or_else(|| {
            reject(
                "environment.sceneMissing",
                format!("scene `{scene_id}` is absent"),
            )
        })?;
    for (label, id) in [("player", player_entity_id), ("exit", exit_entity_id)] {
        if !scene.entities.iter().any(|entity| entity.id == id) {
            return Err(reject(
                "environment.entityMissing",
                format!("{label} target entity {id} is absent from scene `{scene_id}`"),
            ));
        }
    }
    Ok(())
}

fn marker_translation(
    materialized: &environment_authoring::MaterializedEnvironment,
    source_id: &str,
) -> Result<[f32; 3], AdapterRejection> {
    let marker = materialized
        .markers
        .iter()
        .find(|marker| marker.source_marker_id == source_id)
        .ok_or_else(|| {
            reject(
                "environment.markerMissing",
                format!("missing `{source_id}` marker"),
            )
        })?;
    Ok([
        marker.world_transform.translation.x,
        marker.world_transform.translation.y,
        marker.world_transform.translation.z,
    ])
}

fn install_environment_asset(
    project: &mut crate::StoredProject,
    asset_id: String,
    candidate: voxel_asset::VoxelAsset,
) -> Result<(), AdapterRejection> {
    match project.assets.iter().position(|asset| asset.id == asset_id) {
        Some(index) => {
            let stored = find_asset_mut(project, &asset_id)?;
            let existing = stored.voxel_volume.as_ref().ok_or_else(|| {
                reject(
                    "environment.targetConflict",
                    format!("asset `{asset_id}` belongs to another payload kind"),
                )
            })?;
            if existing.provenance.converter != TUNNEL_GENERATOR_ID
                || !stored.voxel_annotations.is_empty()
            {
                return Err(reject(
                    "environment.targetConflict",
                    format!("asset `{asset_id}` is not an unannotated managed environment"),
                ));
            }
            project.assets[index] = StoredAsset {
                id: asset_id,
                voxel_volume: Some(candidate),
                voxel_edit_history: None,
                voxel_annotations: Vec::new(),
                material: None,
            };
        }
        None => project.assets.push(StoredAsset {
            id: asset_id,
            voxel_volume: Some(candidate),
            voxel_edit_history: None,
            voxel_annotations: Vec::new(),
            material: None,
        }),
    }
    Ok(())
}

fn install_environment_instance(
    scene: &mut crate::StoredScene,
    instance_id: String,
    voxel_asset_id: String,
    transform: SceneTransform,
) -> Result<(), AdapterRejection> {
    let candidate = StoredVoxelInstance {
        instance_id: instance_id.clone(),
        voxel_asset_id,
        translation: [
            transform.translation.x,
            transform.translation.y,
            transform.translation.z,
        ],
        rotation: [
            transform.rotation.x,
            transform.rotation.y,
            transform.rotation.z,
            transform.rotation.w,
        ],
        scale: [transform.scale.x, transform.scale.y, transform.scale.z],
    };
    if let Some(existing) = scene
        .voxel_instances
        .iter_mut()
        .find(|instance| instance.instance_id == instance_id)
    {
        if existing.voxel_asset_id != candidate.voxel_asset_id {
            return Err(reject(
                "environment.targetConflict",
                format!("instance `{instance_id}` targets another voxel asset"),
            ));
        }
        *existing = candidate;
    } else {
        scene.voxel_instances.push(candidate);
    }
    Ok(())
}

fn set_entity_translation(
    scene: &mut crate::StoredScene,
    entity_id: u64,
    translation: [f32; 3],
) -> Result<(), AdapterRejection> {
    let entity = scene
        .entities
        .iter_mut()
        .find(|entity| entity.id == entity_id)
        .ok_or_else(|| {
            reject(
                "environment.entityMissing",
                format!("target entity {entity_id} disappeared during publication"),
            )
        })?;
    entity.translation = Some(translation);
    Ok(())
}
