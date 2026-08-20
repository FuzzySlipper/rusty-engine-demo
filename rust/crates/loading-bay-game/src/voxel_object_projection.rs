//! Canonical downstream projection of admitted authored voxel-object instances.
//!
//! The game host calls this seam. Rust resolves stored object
//! identities, frames, transforms, and materials; Rusty Engine remains the
//! only owner of voxel meshing, retained handles, and render operations.

use std::collections::BTreeMap;

use rusty_engine::render_model::{
    MaterialUvStrategy, MeshMaterialSlot, RenderFrameDiff, RenderMaterialDescriptor,
    RenderMetadata, Transform,
};
use rusty_engine::render_projection::{VoxelObjectProjectionInstance, VoxelObjectRenderProjector};
use rusty_engine::voxel_object_runtime::{
    admit_voxel_object, admit_voxel_object_with_options, AdmittedVoxelObject,
    VoxelObjectAdmissionOptions, VoxelObjectRuntimeLimits,
};

use crate::{StoredProject, StoredVoxelObjectFrameSelection, StoredVoxelObjectMaterialOverride};
use loading_bay_gameplay::stored_project::validate_voxel_object_aggregate_budget;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredVoxelObjectProjectionError(String);

impl std::fmt::Display for StoredVoxelObjectProjectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for StoredVoxelObjectProjectionError {}

pub fn project_stored_voxel_objects(
    project: &StoredProject,
) -> Result<RenderFrameDiff, StoredVoxelObjectProjectionError> {
    validate_voxel_object_aggregate_budget(project, None)
        .map_err(|error| projection_error(error.to_string()))?;
    let admitted = project
        .assets
        .iter()
        .filter_map(|asset| asset.voxel_object.as_ref())
        .map(|object| {
            admit_voxel_object(object, VoxelObjectRuntimeLimits::default())
                .map(|admitted| (object.asset_id.clone(), admitted))
                .map_err(|error| {
                    projection_error(format!("{} admission failed: {error}", object.asset_id))
                })
        })
        .collect::<Result<BTreeMap<String, AdmittedVoxelObject>, _>>()?;
    let entry_scene = project
        .scenes
        .iter()
        .find(|scene| scene.id == project.entry_scene)
        .ok_or_else(|| projection_error("entry scene is missing"))?;
    let requested_variants = entry_scene
        .voxel_object_instances
        .iter()
        .filter(|instance| !instance.surface_mode.is_default())
        .map(|instance| {
            (
                instance.voxel_object_asset_id.clone(),
                instance.surface_mode,
            )
        })
        .collect::<std::collections::BTreeSet<_>>();
    let mut variants = BTreeMap::new();
    for (asset_id, surface_mode) in requested_variants {
        let canonical = admitted.get(&asset_id).ok_or_else(|| {
            projection_error(format!(
                "surface mode references missing object `{asset_id}`"
            ))
        })?;
        let variant = admit_voxel_object_with_options(
            canonical.source(),
            VoxelObjectAdmissionOptions {
                surface_mode: surface_mode.as_engine(),
                ..VoxelObjectAdmissionOptions::default()
            },
        )
        .map_err(|error| {
            projection_error(format!(
                "{asset_id} {} admission failed: {error}",
                surface_mode.as_str()
            ))
        })?;
        variants.insert((asset_id, surface_mode), variant);
    }

    let mut instances = Vec::with_capacity(entry_scene.voxel_object_instances.len());
    for instance in &entry_scene.voxel_object_instances {
        let object = if instance.surface_mode.is_default() {
            admitted.get(&instance.voxel_object_asset_id)
        } else {
            variants.get(&(
                instance.voxel_object_asset_id.clone(),
                instance.surface_mode,
            ))
        }
        .ok_or_else(|| {
            projection_error(format!(
                "instance `{}` references missing object `{}`",
                instance.instance_id, instance.voxel_object_asset_id
            ))
        })?;
        instances.push(VoxelObjectProjectionInstance {
            instance_id: instance.instance_id.clone(),
            object,
            frame: stored_object_frame(object, &instance.frame)?,
            transform: Transform {
                translation: instance.translation,
                rotation: instance.rotation,
                scale: instance.scale,
            },
            visible: true,
            material_overrides: material_overrides(&instance.material_overrides),
            metadata: RenderMetadata {
                source_entity: Some(instance.owner_entity_id),
                source_scene_node: Some(instance.owner_entity_id),
                tags: vec!["voxel-object".to_string()],
                label: Some(instance.instance_id.clone()),
            },
        });
    }
    VoxelObjectRenderProjector::new()
        .project(&instances, &stored_project_material_descriptors(project))
        .map(|projected| projected.frame)
        .map_err(|error| projection_error(format!("Engine projection rejected: {error:?}")))
}

pub(crate) fn stored_project_material_descriptors(
    project: &StoredProject,
) -> BTreeMap<String, RenderMaterialDescriptor> {
    project
        .assets
        .iter()
        .filter_map(|asset| {
            let material = asset.material.as_ref()?;
            let style = &material.style;
            Some((
                asset.id.clone(),
                RenderMaterialDescriptor {
                    schema_version: 1,
                    id: asset.id.clone(),
                    color: style.color,
                    texture: style.texture.as_ref().map(|reference| reference.id.clone()),
                    roughness: style.roughness,
                    texture_tint: style.texture_tint,
                    emission_color: [
                        style.emission_color[0],
                        style.emission_color[1],
                        style.emission_color[2],
                    ],
                    emission_intensity: style.emissive,
                    alpha_mode: Default::default(),
                    double_sided: false,
                    uv_strategy: match style.uv_strategy.as_str() {
                        "flat" => MaterialUvStrategy::Flat,
                        "planar" => MaterialUvStrategy::Planar,
                        "atlas" => MaterialUvStrategy::Atlas,
                        _ => MaterialUvStrategy::Flat,
                    },
                    voxel_surface: None,
                },
            ))
        })
        .collect()
}

fn material_overrides(overrides: &[StoredVoxelObjectMaterialOverride]) -> Vec<MeshMaterialSlot> {
    overrides
        .iter()
        .map(|entry| MeshMaterialSlot {
            slot: entry.material_slot,
            material: entry.material_asset_id.clone(),
        })
        .collect()
}

fn stored_object_frame(
    object: &AdmittedVoxelObject,
    selection: &StoredVoxelObjectFrameSelection,
) -> Result<u32, StoredVoxelObjectProjectionError> {
    match selection {
        StoredVoxelObjectFrameSelection::Default => Ok(0),
        StoredVoxelObjectFrameSelection::Clip {
            clip_id,
            frame_index,
        } => object
            .clip(clip_id)
            .and_then(|clip| clip.frame_indices.get(*frame_index as usize))
            .copied()
            .ok_or_else(|| {
                projection_error(format!("clip `{clip_id}` has no frame {frame_index}"))
            }),
    }
}

fn projection_error(message: impl Into<String>) -> StoredVoxelObjectProjectionError {
    StoredVoxelObjectProjectionError(message.into())
}

#[cfg(test)]
mod tests {
    use rusty_engine::render_model::RenderDiff;

    use super::project_stored_voxel_objects;
    use crate::decode_project_document;

    const E1M1_PROJECT: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");

    #[test]
    fn e1m1_projection_has_no_legacy_voxel_object_instances() {
        let project = decode_project_document(E1M1_PROJECT).unwrap().project;
        let frame = project_stored_voxel_objects(&project).unwrap();
        let definitions = frame
            .ops
            .iter()
            .filter(|operation| matches!(operation, RenderDiff::DefineVoxelObject { .. }))
            .count();
        let instances = frame
            .ops
            .iter()
            .filter_map(|operation| match operation {
                RenderDiff::CreateVoxelObjectInstance { instance, .. } => Some(instance),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(definitions, 0);
        assert!(instances.is_empty());
        let definition_sizes = frame
            .ops
            .iter()
            .filter_map(|operation| match operation {
                RenderDiff::DefineVoxelObject { asset } => Some((
                    asset.asset.as_str(),
                    serde_json::to_vec(operation).unwrap().len(),
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        let encoded_bytes = serde_json::to_vec(&frame).unwrap().len();
        assert!(
            encoded_bytes < 2 * 1024 * 1024,
            "voxel-object frame expanded to {encoded_bytes} bytes: {definition_sizes:?}"
        );
        assert!(instances.iter().all(|instance| {
            instance.metadata.source_entity.is_some()
                && instance.metadata.source_entity == instance.metadata.source_scene_node
        }));
    }
}
