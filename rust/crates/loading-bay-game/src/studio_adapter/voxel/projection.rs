use std::collections::{BTreeMap, BTreeSet};

use asset_catalog::{AssetCatalog, UvStrategy, VoxelMaterialTable};
use core_assets::AssetId;
use core_voxel::VoxelMaterialId;
use engine_inspector::inspect_voxel_asset;
use render_model::{MaterialUvStrategy, RenderFrameDiff, RenderMaterialDescriptor};
use render_projection::{voxel_material_id, VoxelProjectionInstance, VoxelRenderProjector};

use crate::StoredProject;

use super::super::protocol::{
    MaterialAssetReadout, VoxelAnnotationSummaryReadout, VoxelAssetAuthoringReadout,
    VoxelAuthoringReadout, VoxelInstanceReadout,
};
use super::super::AdapterRejection;
use super::model::{history_readout, reject, render_transform, scene_and_history};

pub(crate) struct ProjectVoxelProjection {
    pub frame: RenderFrameDiff,
    pub instance_count: usize,
    pub chunk_count: usize,
}

pub(crate) fn project_voxel_authoring(
    project: &StoredProject,
    catalog: &AssetCatalog,
) -> Result<ProjectVoxelProjection, AdapterRejection> {
    let mut owned = Vec::new();
    for scene in &project.scenes {
        for instance in &scene.voxel_instances {
            let asset = project
                .assets
                .iter()
                .find(|asset| asset.id == instance.voxel_asset_id)
                .ok_or_else(|| {
                    reject(
                        "voxel.instanceAssetMissing",
                        format!(
                            "instance `{}` references missing asset `{}`",
                            instance.instance_id, instance.voxel_asset_id
                        ),
                    )
                })?;
            let (voxel_scene, _) = scene_and_history(asset)?;
            owned.push((scene.id.clone(), instance.clone(), voxel_scene));
        }
    }

    let mut slot_assets = BTreeMap::<u16, AssetId>::new();
    let mut used_slots = BTreeSet::new();
    for (_, instance, scene) in &owned {
        used_slots.extend(
            scene
                .material_voxels()
                .iter()
                .map(|voxel| voxel.material_slot),
        );
        let asset = project
            .assets
            .iter()
            .find(|asset| asset.id == instance.voxel_asset_id)
            .and_then(|asset| asset.voxel_volume.as_ref())
            .expect("stored-project validation retained voxel instance target");
        for binding in &asset.material_palette {
            let id = AssetId::parse(&binding.material_asset_id)
                .map_err(|error| reject("voxel.invalidMaterial", error.to_string()))?;
            if slot_assets
                .insert(binding.material_slot, id.clone())
                .is_some_and(|previous| previous != id)
            {
                return Err(reject(
                    "voxel.materialSlotConflict",
                    format!(
                        "project voxel slot {} is mapped to more than one material asset",
                        binding.material_slot
                    ),
                ));
            }
        }
    }
    let table = VoxelMaterialTable::from_pairs(
        slot_assets
            .iter()
            .map(|(slot, asset)| (VoxelMaterialId::new(*slot), asset.clone())),
    );
    let report = table.validate_used(
        catalog,
        used_slots.iter().copied().map(VoxelMaterialId::new),
    );
    if !report.is_collision_safe() {
        return Err(reject(
            "voxel.materialResolutionRejected",
            format!("unresolved voxel materials: {:?}", report.unresolved),
        ));
    }

    let materials = used_slots
        .iter()
        .copied()
        .map(|slot| {
            let resolved = table.render_material(catalog, VoxelMaterialId::new(slot));
            (slot, render_material(slot, resolved.material))
        })
        .collect::<BTreeMap<_, _>>();
    let instances = owned
        .iter()
        .map(|(_, instance, scene)| VoxelProjectionInstance {
            instance_id: instance.instance_id.clone(),
            asset_id: instance.voxel_asset_id.clone(),
            transform: render_transform(instance),
            scene,
        })
        .collect::<Vec<_>>();
    let projected = VoxelRenderProjector::new()
        .project(&instances, &materials)
        .map_err(|error| reject("voxel.projectionRejected", format!("{error:?}")))?;
    Ok(ProjectVoxelProjection {
        frame: projected.frame,
        instance_count: projected.readout.instance_count,
        chunk_count: projected.readout.chunk_count,
    })
}

pub(crate) fn voxel_authoring_readout(
    project: &StoredProject,
) -> Result<VoxelAuthoringReadout, AdapterRejection> {
    let mut assets = Vec::new();
    let mut materials = Vec::new();
    for stored in &project.assets {
        if let Some(asset) = &stored.voxel_volume {
            let (_, history) = scene_and_history(stored)?;
            assets.push(VoxelAssetAuthoringReadout {
                inspection: inspect_voxel_asset(asset),
                palette: asset.material_palette.clone(),
                history: history_readout(stored.voxel_edit_history.is_some(), &history),
                annotations: stored
                    .voxel_annotations
                    .iter()
                    .map(|layer| VoxelAnnotationSummaryReadout {
                        layer_id: layer.layer_id.clone(),
                        canonical_layer_hash: layer.content_hashes.canonical_layer.clone(),
                        membership_data_hash: layer.content_hashes.membership_data.clone(),
                        region_count: layer.regions.len(),
                        assigned_cell_count: layer
                            .regions
                            .iter()
                            .flat_map(|region| &region.selection.sparse_runs)
                            .map(|run| u64::from(run.length))
                            .sum(),
                    })
                    .collect(),
            });
        }
        if let Some(definition) = &stored.material {
            materials.push(MaterialAssetReadout {
                asset_id: stored.id.clone(),
                definition: definition.clone(),
            });
        }
    }
    let mut instances = project
        .scenes
        .iter()
        .flat_map(|scene| {
            scene
                .voxel_instances
                .iter()
                .cloned()
                .map(|instance| VoxelInstanceReadout {
                    scene_id: scene.id.clone(),
                    instance,
                })
        })
        .collect::<Vec<_>>();
    instances.sort_by(|left, right| {
        (&left.scene_id, &left.instance.instance_id)
            .cmp(&(&right.scene_id, &right.instance.instance_id))
    });
    Ok(VoxelAuthoringReadout {
        assets,
        instances,
        materials,
    })
}

fn render_material(slot: u16, material: asset_catalog::RenderMaterial) -> RenderMaterialDescriptor {
    RenderMaterialDescriptor {
        schema_version: 1,
        id: voxel_material_id(slot),
        color: [
            material.color.r,
            material.color.g,
            material.color.b,
            material.color.a,
        ],
        texture: material
            .texture
            .as_ref()
            .map(|reference| reference.id().as_str().to_string()),
        roughness: material.roughness,
        texture_tint: [
            material.texture_tint.r,
            material.texture_tint.g,
            material.texture_tint.b,
            material.texture_tint.a,
        ],
        emission_color: [
            material.emission_color.r,
            material.emission_color.g,
            material.emission_color.b,
        ],
        emission_intensity: material.emissive,
        uv_strategy: match material.uv_strategy {
            UvStrategy::Flat => MaterialUvStrategy::Flat,
            UvStrategy::Planar => MaterialUvStrategy::Planar,
            UvStrategy::Atlas => MaterialUvStrategy::Atlas,
        },
    }
}
