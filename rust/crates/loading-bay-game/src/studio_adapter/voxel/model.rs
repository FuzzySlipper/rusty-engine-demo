use rusty_engine::engine_spatial::{
    decode_voxel_edit_history, encode_voxel_edit_history, VoxelCollisionScene, VoxelEditHistory,
    VoxelEditHistoryLimits,
};
use rusty_engine::entity_state::{EntityTransform, Quat};
use rusty_engine::render_model::Transform;
use rusty_engine::voxel_annotation::{
    finalize_annotation_draft, VoxelAnnotationLayerDraft, VoxelAnnotationLimits,
};
use rusty_engine::voxel_asset::{
    with_computed_content_hash, VoxelAsset, VoxelRepresentation, VoxelRepresentationKind,
    VoxelSparseRun,
};

use crate::{StoredAsset, StoredProject, StoredScene, StoredVoxelInstance};
use loading_bay_gameplay::stored_project::expand_voxel_asset;

use super::super::protocol::{AdapterRejection, VoxelHistoryReadout};

pub(crate) fn find_asset<'a>(
    project: &'a StoredProject,
    asset_id: &str,
) -> Result<&'a StoredAsset, AdapterRejection> {
    project
        .assets
        .iter()
        .find(|asset| asset.id == asset_id)
        .ok_or_else(|| {
            reject(
                "voxel.assetMissing",
                format!("project has no asset `{asset_id}`"),
            )
        })
}

pub(crate) fn find_asset_mut<'a>(
    project: &'a mut StoredProject,
    asset_id: &str,
) -> Result<&'a mut StoredAsset, AdapterRejection> {
    project
        .assets
        .iter_mut()
        .find(|asset| asset.id == asset_id)
        .ok_or_else(|| {
            reject(
                "voxel.assetMissing",
                format!("project has no asset `{asset_id}`"),
            )
        })
}

pub(crate) fn find_voxel_asset<'a>(
    project: &'a StoredProject,
    asset_id: &str,
) -> Result<&'a VoxelAsset, AdapterRejection> {
    find_asset(project, asset_id)?
        .voxel_volume
        .as_ref()
        .ok_or_else(|| {
            reject(
                "voxel.wrongAssetKind",
                format!("`{asset_id}` is not a voxel asset"),
            )
        })
}

pub(crate) fn find_voxel_asset_mut<'a>(
    project: &'a mut StoredProject,
    asset_id: &str,
) -> Result<&'a mut StoredAsset, AdapterRejection> {
    let asset = find_asset_mut(project, asset_id)?;
    if asset.voxel_volume.is_none() {
        return Err(reject(
            "voxel.wrongAssetKind",
            format!("`{asset_id}` is not a voxel asset"),
        ));
    }
    Ok(asset)
}

pub(crate) fn find_scene<'a>(
    project: &'a StoredProject,
    scene_id: &str,
) -> Result<&'a StoredScene, AdapterRejection> {
    project
        .scenes
        .iter()
        .find(|scene| scene.id == scene_id)
        .ok_or_else(|| {
            reject(
                "voxel.sceneMissing",
                format!("project has no scene `{scene_id}`"),
            )
        })
}

pub(crate) fn find_scene_mut<'a>(
    project: &'a mut StoredProject,
    scene_id: &str,
) -> Result<&'a mut StoredScene, AdapterRejection> {
    project
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .ok_or_else(|| {
            reject(
                "voxel.sceneMissing",
                format!("project has no scene `{scene_id}`"),
            )
        })
}

pub(crate) fn require_asset_hash(
    asset: &VoxelAsset,
    expected: &str,
) -> Result<(), AdapterRejection> {
    if asset.content_hash == expected {
        Ok(())
    } else {
        Err(reject(
            "voxel.staleAsset",
            format!(
                "expected voxel asset hash {expected}, found {}",
                asset.content_hash
            ),
        ))
    }
}

pub(crate) fn scene_and_history(
    asset: &StoredAsset,
) -> Result<(VoxelCollisionScene, VoxelEditHistory), AdapterRejection> {
    let voxel = asset
        .voxel_volume
        .as_ref()
        .ok_or_else(|| reject("voxel.wrongAssetKind", "asset has no voxel payload"))?;
    if let Some(encoded) = &asset.voxel_edit_history {
        let restored = decode_voxel_edit_history(encoded, VoxelEditHistoryLimits::default())
            .map_err(|error| reject("voxel.historyRejected", error.to_string()))?;
        return Ok((restored.scene, restored.history));
    }
    let scene = VoxelCollisionScene::from_material_voxels(
        voxel.grid.cell_size,
        voxel.grid.chunk_size,
        expand_voxel_asset(voxel).map_err(stored_error)?,
    )
    .map_err(|error| reject("voxel.projectionRejected", error.to_string()))?;
    let history = VoxelEditHistory::new(&scene);
    Ok((scene, history))
}

pub(crate) fn install_scene_and_history(
    asset: &mut StoredAsset,
    scene: &VoxelCollisionScene,
    history: &VoxelEditHistory,
) -> Result<(), AdapterRejection> {
    let current = asset
        .voxel_volume
        .as_ref()
        .ok_or_else(|| reject("voxel.wrongAssetKind", "asset has no voxel payload"))?;
    let updated = asset_from_scene(current, scene)?;
    asset.voxel_volume = Some(updated);
    retarget_annotations(asset)?;
    asset.voxel_edit_history = Some(
        encode_voxel_edit_history(history)
            .map_err(|error| reject("voxel.historyEncodeRejected", error.to_string()))?,
    );
    Ok(())
}

fn asset_from_scene(
    asset: &VoxelAsset,
    scene: &VoxelCollisionScene,
) -> Result<VoxelAsset, AdapterRejection> {
    let mut sparse_runs = Vec::with_capacity(scene.material_voxels().len());
    for voxel in scene.material_voxels() {
        let mut local = voxel.address;
        for (axis, coordinate) in local.iter_mut().enumerate() {
            *coordinate = coordinate
                .checked_sub(asset.grid.origin[axis])
                .ok_or_else(|| {
                    reject(
                        "voxel.coordinateOverflow",
                        "voxel origin subtraction overflowed",
                    )
                })?;
            if *coordinate < asset.bounds.min[axis] || *coordinate > asset.bounds.max[axis] {
                return Err(reject(
                    "voxel.outOfBounds",
                    format!(
                        "edited voxel {:?} falls outside the authored asset bounds",
                        local
                    ),
                ));
            }
        }
        sparse_runs.push(VoxelSparseRun {
            start: local,
            length: 1,
            material_slot: voxel.material_slot,
        });
    }
    let mut candidate = asset.clone();
    candidate.representation = VoxelRepresentation {
        kind: VoxelRepresentationKind::SparseRuns,
        sparse_runs,
    };
    candidate.voxel_data_hash.clear();
    candidate.content_hash.clear();
    with_computed_content_hash(candidate)
        .map_err(|error| reject("voxel.assetRejected", error.to_string()))
}

pub(crate) fn retarget_annotations(asset: &mut StoredAsset) -> Result<(), AdapterRejection> {
    let target = asset
        .voxel_volume
        .as_ref()
        .ok_or_else(|| reject("voxel.wrongAssetKind", "asset has no voxel payload"))?;
    let mut next = Vec::with_capacity(asset.voxel_annotations.len());
    for layer in &asset.voxel_annotations {
        next.push(
            finalize_annotation_draft(
                VoxelAnnotationLayerDraft {
                    layer_id: layer.layer_id.clone(),
                    target_voxel_asset_id: target.asset_id.clone(),
                    target_voxel_data_hash: target.voxel_data_hash.clone(),
                    target_bounds: rusty_engine::voxel_annotation::VoxelAnnotationBounds {
                        min: target.bounds.min,
                        max: target.bounds.max,
                    },
                    regions: layer.regions.clone(),
                    provenance: layer.provenance.clone(),
                },
                target,
                VoxelAnnotationLimits::default(),
            )
            .map_err(|error| reject("voxel.annotationRetargetRejected", error.to_string()))?,
        );
    }
    asset.voxel_annotations = next;
    Ok(())
}

pub(crate) fn local_to_authority_address(
    asset: &VoxelAsset,
    local: [i64; 3],
) -> Result<[i64; 3], AdapterRejection> {
    if (0..3)
        .any(|axis| local[axis] < asset.bounds.min[axis] || local[axis] > asset.bounds.max[axis])
    {
        return Err(reject(
            "voxel.outOfBounds",
            format!("local voxel {local:?} is outside the authored bounds"),
        ));
    }
    let mut address = local;
    for axis in 0..3 {
        address[axis] = asset.grid.origin[axis]
            .checked_add(local[axis])
            .ok_or_else(|| {
                reject(
                    "voxel.coordinateOverflow",
                    "voxel origin mapping overflowed",
                )
            })?;
    }
    Ok(address)
}

pub(crate) fn history_readout(persisted: bool, history: &VoxelEditHistory) -> VoxelHistoryReadout {
    let cursor = history.cursor();
    VoxelHistoryReadout {
        persisted,
        entry_count: history.entries().len(),
        cursor: cursor.index,
        undo_depth: cursor.undo_depth,
        redo_depth: cursor.redo_depth,
        authority_hash: format!("u64:{:016x}", cursor.authority_hash),
        history_hash: format!("u64:{:016x}", cursor.history_hash),
    }
}

pub(crate) fn entity_transform(instance: &StoredVoxelInstance) -> EntityTransform {
    EntityTransform {
        translation: rusty_engine::core_math::Vec3::new(
            instance.translation[0],
            instance.translation[1],
            instance.translation[2],
        ),
        rotation: Quat::new(
            instance.rotation[0],
            instance.rotation[1],
            instance.rotation[2],
            instance.rotation[3],
        ),
        scale: rusty_engine::core_math::Vec3::new(
            instance.scale[0],
            instance.scale[1],
            instance.scale[2],
        ),
    }
}

pub(crate) fn render_transform(instance: &StoredVoxelInstance) -> Transform {
    Transform {
        translation: instance.translation,
        rotation: instance.rotation,
        scale: instance.scale,
    }
}

pub(crate) fn reject(code: impl Into<String>, message: impl Into<String>) -> AdapterRejection {
    AdapterRejection::new(code, message)
}

pub(crate) fn stored_error(error: crate::StoredProjectError) -> AdapterRejection {
    AdapterRejection::new(error.diagnostic().code, error.diagnostic().message.clone())
        .at_path(error.diagnostic().path.clone())
}
