use asset_catalog::StoredMaterialDefinition;
use core_assets::{AssetId, AssetKind};
use engine_spatial::{
    VoxelEdit, VoxelEditHistoryDiffOptions, VoxelPrimitive, VoxelPrimitiveEditService,
    VoxelPrimitiveRequest, VoxelTemplate, VoxelTemplateEditService, VoxelTemplateRequest,
    VOXEL_HOUSE_TEMPLATE_BOUNDS,
};
use voxel_annotation::{
    finalize_annotation_draft, VoxelAnnotationEditService, VoxelAnnotationEditTransaction,
    VoxelAnnotationLayerDraft, VoxelAnnotationLimits,
};
use voxel_asset::{
    replace_voxel_palette, with_computed_content_hash, VoxelAsset, VoxelAssetBounds,
    VoxelAssetGrid, VoxelAssetMaterialBinding, VoxelAssetMaterialMapping, VoxelAssetProvenance,
    VoxelAssetProvenanceKind, VoxelCoordinateSystem, VoxelPaletteUpdateRequest,
    VoxelRepresentation, VoxelRepresentationKind, VoxelSparseRun, MAX_REPRESENTED_VOXELS,
    VOXEL_ASSET_SCHEMA_VERSION,
};
use voxel_convert::source_sha256;

use crate::{StoredAsset, StoredVoxelInstance};

use super::super::project::{publish_project_mutation, PublishedProject};
use super::super::protocol::{
    AdapterRejection, ProjectMutationReceipt, StudioProjectReadout, VoxelBrushMode,
};
use super::super::ProjectLocation;
use super::model::{
    find_asset, find_scene_mut, find_voxel_asset, find_voxel_asset_mut, install_scene_and_history,
    local_to_authority_address, reject, require_asset_hash, scene_and_history,
};

pub(crate) fn upsert_material(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: String,
    definition: StoredMaterialDefinition,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    let parsed = AssetId::parse(&asset_id)
        .map_err(|error| reject("material.invalidAssetId", error.to_string()))?;
    if parsed.kind() != AssetKind::Material {
        return Err(reject(
            "material.wrongAssetKind",
            format!("expected material identity, found {}", parsed.kind()),
        ));
    }
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_, project| {
            if let Some(asset) = project.assets.iter_mut().find(|asset| asset.id == asset_id) {
                if asset.voxel_volume.is_some() || asset.material.is_none() {
                    return Err(reject(
                        "material.identityConflict",
                        format!("asset `{asset_id}` already belongs to another payload kind"),
                    ));
                }
                asset.material = Some(definition);
            } else {
                project.assets.push(StoredAsset {
                    id: asset_id.clone(),
                    voxel_volume: None,
                    voxel_edit_history: None,
                    voxel_annotations: Vec::new(),
                    material: Some(definition),
                });
            }
            Ok(ProjectMutationReceipt::MaterialUpserted { asset_id })
        },
    )?)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn initialize_voxel_asset(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: String,
    cell_size: f64,
    chunk_size: u32,
    origin: [i64; 3],
    bounds: VoxelAssetBounds,
    material_palette: Vec<VoxelAssetMaterialBinding>,
    initial_material_slot: u16,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    require_voxel_identity(&asset_id)?;
    let sparse_runs = filled_volume_runs(bounds, initial_material_slot)?;
    let settings = serde_json::to_vec(&(
        &asset_id,
        cell_size,
        chunk_size,
        origin,
        bounds,
        &material_palette,
        initial_material_slot,
    ))
    .expect("closed voxel initialization settings serialize");
    let identity = source_sha256(&settings);
    let asset = with_computed_content_hash(VoxelAsset {
        schema_version: VOXEL_ASSET_SCHEMA_VERSION,
        asset_id: asset_id.clone(),
        grid: VoxelAssetGrid {
            coordinate_system: VoxelCoordinateSystem::RightHandedYUp,
            cell_size,
            chunk_size,
            origin,
        },
        bounds,
        representation: VoxelRepresentation {
            kind: VoxelRepresentationKind::SparseRuns,
            sparse_runs,
        },
        material_palette,
        material_map: vec![VoxelAssetMaterialMapping {
            source_material_slot: 0,
            source_material_name: Some("authored-initial-material".to_string()),
            voxel_material_slot: initial_material_slot,
        }],
        provenance: VoxelAssetProvenance {
            kind: VoxelAssetProvenanceKind::Authored,
            source_path: format!("studio-authored/{asset_id}"),
            source_sha256: identity.clone(),
            source_byte_count: settings.len() as u64,
            converter: "rusty-engine.studio.voxel-authoring.v1".to_string(),
            settings_sha256: identity,
            license_path: None,
        },
        voxel_data_hash: String::new(),
        content_hash: String::new(),
    })
    .map_err(|error| reject("voxel.assetRejected", error.to_string()))?;
    let content_hash = asset.content_hash.clone();
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_, project| {
            if project.assets.iter().any(|stored| stored.id == asset_id) {
                return Err(reject(
                    "voxel.assetExists",
                    format!("asset `{asset_id}` already exists"),
                ));
            }
            project.assets.push(StoredAsset {
                id: asset_id.clone(),
                voxel_volume: Some(asset),
                voxel_edit_history: None,
                voxel_annotations: Vec::new(),
                material: None,
            });
            Ok(ProjectMutationReceipt::VoxelAssetInitialized {
                asset_id,
                content_hash,
            })
        },
    )?)
}

fn filled_volume_runs(
    bounds: VoxelAssetBounds,
    material_slot: u16,
) -> Result<Vec<VoxelSparseRun>, AdapterRejection> {
    let mut lengths = [0u64; 3];
    for (axis, length) in lengths.iter_mut().enumerate() {
        let span = bounds.max[axis]
            .checked_sub(bounds.min[axis])
            .and_then(|difference| difference.checked_add(1))
            .ok_or_else(|| {
                reject(
                    "voxel.invalidBounds",
                    "initial voxel bounds must be ordered with a representable extent",
                )
            })?;
        *length = u64::try_from(span).map_err(|_| {
            reject(
                "voxel.invalidBounds",
                "initial voxel bounds must be ordered on every axis",
            )
        })?;
    }
    let represented = lengths.iter().try_fold(1u64, |total, length| {
        total.checked_mul(*length).ok_or_else(|| {
            reject(
                "voxel.resourceLimit",
                "initial voxel volume size overflowed",
            )
        })
    })?;
    if represented == 0 || represented > MAX_REPRESENTED_VOXELS as u64 {
        return Err(reject(
            "voxel.resourceLimit",
            format!(
                "initial voxel volume represents {represented} cells; limit is {MAX_REPRESENTED_VOXELS}"
            ),
        ));
    }
    let run_length = u32::try_from(lengths[0]).map_err(|_| {
        reject(
            "voxel.resourceLimit",
            "initial voxel row does not fit the sparse-run format",
        )
    })?;
    let run_count = usize::try_from(lengths[1].saturating_mul(lengths[2])).map_err(|_| {
        reject(
            "voxel.resourceLimit",
            "initial voxel row count exceeds this host",
        )
    })?;
    let mut sparse_runs = Vec::with_capacity(run_count);
    for z in bounds.min[2]..=bounds.max[2] {
        for y in bounds.min[1]..=bounds.max[1] {
            sparse_runs.push(VoxelSparseRun {
                start: [bounds.min[0], y, z],
                length: run_length,
                material_slot,
            });
        }
    }
    Ok(sparse_runs)
}

pub(crate) fn duplicate_voxel_asset(
    location: &ProjectLocation,
    expected_project_hash: &str,
    source_asset_id: String,
    expected_source_content_hash: String,
    target_asset_id: String,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    require_voxel_identity(&target_asset_id)?;
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_, project| {
            if project
                .assets
                .iter()
                .any(|asset| asset.id == target_asset_id)
            {
                return Err(reject(
                    "voxel.assetExists",
                    format!("asset `{target_asset_id}` already exists"),
                ));
            }
            let source = find_asset(project, &source_asset_id)?;
            let voxel = source
                .voxel_volume
                .as_ref()
                .ok_or_else(|| reject("voxel.wrongAssetKind", "source is not a voxel asset"))?;
            require_asset_hash(voxel, &expected_source_content_hash)?;
            let mut duplicate = voxel.clone();
            duplicate.asset_id = target_asset_id.clone();
            duplicate.content_hash.clear();
            duplicate = with_computed_content_hash(duplicate)
                .map_err(|error| reject("voxel.assetRejected", error.to_string()))?;
            let content_hash = duplicate.content_hash.clone();
            project.assets.push(StoredAsset {
                id: target_asset_id.clone(),
                voxel_volume: Some(duplicate),
                voxel_edit_history: None,
                voxel_annotations: Vec::new(),
                material: None,
            });
            Ok(ProjectMutationReceipt::VoxelAssetDuplicated {
                source_asset_id,
                target_asset_id,
                content_hash,
            })
        },
    )?)
}

pub(crate) fn attach_voxel_instance(
    location: &ProjectLocation,
    expected_project_hash: &str,
    scene_id: String,
    instance: StoredVoxelInstance,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_, project| {
            find_voxel_asset(project, &instance.voxel_asset_id)?;
            let scene = find_scene_mut(project, &scene_id)?;
            if scene
                .voxel_instances
                .iter()
                .any(|existing| existing.instance_id == instance.instance_id)
            {
                return Err(reject(
                    "voxel.instanceExists",
                    format!("instance `{}` already exists", instance.instance_id),
                ));
            }
            let instance_id = instance.instance_id.clone();
            scene.voxel_instances.push(instance);
            Ok(ProjectMutationReceipt::VoxelInstanceAttached {
                scene_id,
                instance_id,
            })
        },
    )?)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn set_voxel_instance_transform(
    location: &ProjectLocation,
    expected_project_hash: &str,
    scene_id: String,
    instance_id: String,
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_, project| {
            let scene = find_scene_mut(project, &scene_id)?;
            let instance = scene
                .voxel_instances
                .iter_mut()
                .find(|instance| instance.instance_id == instance_id)
                .ok_or_else(|| {
                    reject(
                        "voxel.instanceMissing",
                        format!("scene has no voxel instance `{instance_id}`"),
                    )
                })?;
            instance.translation = translation;
            instance.rotation = rotation;
            instance.scale = scale;
            Ok(ProjectMutationReceipt::VoxelInstanceTransformSet {
                scene_id,
                instance_id,
            })
        },
    )?)
}

pub(crate) fn remove_voxel_instance(
    location: &ProjectLocation,
    expected_project_hash: &str,
    scene_id: String,
    instance_id: String,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_, project| {
            let scene = find_scene_mut(project, &scene_id)?;
            let index = scene
                .voxel_instances
                .iter()
                .position(|instance| instance.instance_id == instance_id)
                .ok_or_else(|| {
                    reject(
                        "voxel.instanceMissing",
                        format!("scene has no voxel instance `{instance_id}`"),
                    )
                })?;
            scene.voxel_instances.remove(index);
            Ok(ProjectMutationReceipt::VoxelInstanceRemoved {
                scene_id,
                instance_id,
            })
        },
    )?)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn replace_palette(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: String,
    expected_asset_content_hash: String,
    expected_voxel_data_hash: String,
    replacement: Vec<VoxelAssetMaterialBinding>,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_, project| {
            let stored = find_voxel_asset_mut(project, &asset_id)?;
            let receipt = replace_voxel_palette(
                stored
                    .voxel_volume
                    .as_mut()
                    .expect("voxel asset helper checked payload"),
                VoxelPaletteUpdateRequest {
                    expected_content_hash: expected_asset_content_hash,
                    expected_voxel_data_hash,
                    replacement,
                },
            )
            .map_err(|error| reject("voxel.paletteRejected", error.to_string()))?;
            Ok(ProjectMutationReceipt::VoxelPaletteReplaced {
                asset_id,
                content_hash_before: receipt.content_hash_before,
                content_hash_after: receipt.content_hash_after,
                voxel_data_hash: receipt.voxel_data_hash,
                material_count_before: receipt.material_count_before,
                material_count_after: receipt.material_count_after,
            })
        },
    )?)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_brush(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: String,
    expected_asset_content_hash: String,
    center: [i64; 3],
    radius: u32,
    mode: VoxelBrushMode,
    material_slot: Option<u16>,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    let paint_slot = match mode {
        VoxelBrushMode::Paint => Some(material_slot.ok_or_else(|| {
            reject(
                "voxel.materialRequired",
                "paint brush requires materialSlot",
            )
        })?),
        VoxelBrushMode::Erase => None,
    };
    let request = VoxelPrimitiveRequest {
        primitive: VoxelPrimitive::Line {
            start: center,
            end: center,
            radius,
        },
        material: match paint_slot {
            Some(material_slot) => engine_spatial::VoxelPrimitiveMaterial::Set { material_slot },
            None => engine_spatial::VoxelPrimitiveMaterial::Clear,
        },
    };
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_, project| {
            let stored = find_voxel_asset_mut(project, &asset_id)?;
            let content_hash_before = stored
                .voxel_volume
                .as_ref()
                .expect("voxel asset helper checked payload")
                .content_hash
                .clone();
            require_asset_hash(
                stored
                    .voxel_volume
                    .as_ref()
                    .expect("voxel asset helper checked payload"),
                &expected_asset_content_hash,
            )?;
            let request = primitive_to_authority(
                stored
                    .voxel_volume
                    .as_ref()
                    .expect("voxel asset helper checked payload"),
                request,
            )?;
            let edits = VoxelPrimitiveEditService
                .generate(request)
                .map_err(|error| reject("voxel.brushRejected", error.to_string()))?;
            let applied = apply_edits(stored, &edits)?;
            let content_hash_after = stored
                .voxel_volume
                .as_ref()
                .expect("installed voxel asset")
                .content_hash
                .clone();
            Ok(ProjectMutationReceipt::VoxelBrushApplied {
                asset_id,
                content_hash_before,
                content_hash_after,
                changed_voxels: applied.changed_voxels,
                source_revision: applied.source_revision,
                history_cursor: applied.history_cursor,
                undo_depth: applied.undo_depth,
                redo_depth: applied.redo_depth,
            })
        },
    )?)
}

pub(crate) fn apply_primitive(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: String,
    expected_asset_content_hash: String,
    request: VoxelPrimitiveRequest,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    let primitive_kind = match request.primitive {
        VoxelPrimitive::Block { .. } => "block",
        VoxelPrimitive::Box { .. } => "box",
        VoxelPrimitive::Line { .. } => "line",
    };
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_, project| {
            let stored = find_voxel_asset_mut(project, &asset_id)?;
            let voxel = stored
                .voxel_volume
                .as_ref()
                .expect("voxel asset helper checked payload");
            require_asset_hash(voxel, &expected_asset_content_hash)?;
            let content_hash_before = voxel.content_hash.clone();
            let request = primitive_to_authority(voxel, request)?;
            let edits = VoxelPrimitiveEditService
                .generate(request)
                .map_err(|error| reject("voxel.primitiveRejected", error.to_string()))?;
            let applied = apply_edits(stored, &edits)?;
            let content_hash_after = stored
                .voxel_volume
                .as_ref()
                .expect("installed voxel asset")
                .content_hash
                .clone();
            Ok(ProjectMutationReceipt::VoxelPrimitiveApplied {
                asset_id,
                primitive_kind,
                content_hash_before,
                content_hash_after,
                changed_voxels: applied.changed_voxels,
                source_revision: applied.source_revision,
                history_cursor: applied.history_cursor,
                undo_depth: applied.undo_depth,
                redo_depth: applied.redo_depth,
            })
        },
    )?)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn initialize_voxel_template(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: String,
    cell_size: f64,
    chunk_size: u32,
    material_palette: Vec<VoxelAssetMaterialBinding>,
    request: VoxelTemplateRequest,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    require_voxel_identity(&asset_id)?;
    let template_kind = match request.template {
        VoxelTemplate::House => "house",
    };
    let settings =
        serde_json::to_vec(&(&asset_id, cell_size, chunk_size, &material_palette, request))
            .expect("closed template settings serialize");
    let identity = source_sha256(&settings);
    let edits = VoxelTemplateEditService
        .generate(request)
        .map_err(|error| reject("voxel.templateRejected", error.to_string()))?;
    let sparse_runs = edits
        .iter()
        .map(|edit| {
            let VoxelEdit::Set {
                address,
                material_slot,
            } = *edit
            else {
                return Err(reject(
                    "voxel.templateRejected",
                    "template generation unexpectedly produced a clear edit",
                ));
            };
            let local = [0, 1, 2].map(|axis| {
                address[axis]
                    .checked_sub(request.origin[axis])
                    .ok_or_else(|| {
                        reject(
                            "voxel.coordinateOverflow",
                            "template coordinate could not be mapped into asset-local space",
                        )
                    })
            });
            Ok(VoxelSparseRun {
                start: [local[0].clone()?, local[1].clone()?, local[2].clone()?],
                length: 1,
                material_slot,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let changed_voxels = sparse_runs.len();
    let asset = with_computed_content_hash(VoxelAsset {
        schema_version: VOXEL_ASSET_SCHEMA_VERSION,
        asset_id: asset_id.clone(),
        grid: VoxelAssetGrid {
            coordinate_system: VoxelCoordinateSystem::RightHandedYUp,
            cell_size,
            chunk_size,
            origin: request.origin,
        },
        bounds: VoxelAssetBounds {
            min: VOXEL_HOUSE_TEMPLATE_BOUNDS[0],
            max: VOXEL_HOUSE_TEMPLATE_BOUNDS[1],
        },
        representation: VoxelRepresentation {
            kind: VoxelRepresentationKind::SparseRuns,
            sparse_runs,
        },
        material_palette,
        material_map: vec![VoxelAssetMaterialMapping {
            source_material_slot: 0,
            source_material_name: Some(format!("studio-{template_kind}-template")),
            voxel_material_slot: request.material_slot,
        }],
        provenance: VoxelAssetProvenance {
            kind: VoxelAssetProvenanceKind::Authored,
            source_path: format!("studio-template/{template_kind}/{asset_id}"),
            source_sha256: identity.clone(),
            source_byte_count: settings.len() as u64,
            converter: "rusty-engine.studio.voxel-template.v1".to_string(),
            settings_sha256: identity,
            license_path: None,
        },
        voxel_data_hash: String::new(),
        content_hash: String::new(),
    })
    .map_err(|error| reject("voxel.templateRejected", error.to_string()))?;
    let content_hash = asset.content_hash.clone();
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_, project| {
            if project.assets.iter().any(|stored| stored.id == asset_id) {
                return Err(reject(
                    "voxel.assetExists",
                    format!("asset `{asset_id}` already exists"),
                ));
            }
            project.assets.push(StoredAsset {
                id: asset_id.clone(),
                voxel_volume: Some(asset),
                voxel_edit_history: None,
                voxel_annotations: Vec::new(),
                material: None,
            });
            Ok(ProjectMutationReceipt::VoxelTemplateInitialized {
                asset_id,
                template_kind,
                content_hash,
                changed_voxels,
                history_cursor: 0,
            })
        },
    )?)
}

struct AppliedVoxelEdits {
    changed_voxels: usize,
    source_revision: u64,
    history_cursor: usize,
    undo_depth: usize,
    redo_depth: usize,
}

fn apply_edits(
    stored: &mut StoredAsset,
    edits: &[VoxelEdit],
) -> Result<AppliedVoxelEdits, AdapterRejection> {
    let (mut scene, mut history) = scene_and_history(stored)?;
    let applied = history
        .apply(&mut scene, edits)
        .map_err(|error| reject("voxel.editRejected", error.to_string()))?;
    install_scene_and_history(stored, &scene, &history)?;
    let cursor = history.cursor();
    Ok(AppliedVoxelEdits {
        changed_voxels: applied.edit.fact.changed_voxels,
        source_revision: applied.edit.accepted_revision.raw(),
        history_cursor: cursor.index,
        undo_depth: cursor.undo_depth,
        redo_depth: cursor.redo_depth,
    })
}

fn primitive_to_authority(
    asset: &VoxelAsset,
    mut request: VoxelPrimitiveRequest,
) -> Result<VoxelPrimitiveRequest, AdapterRejection> {
    request.primitive = match request.primitive {
        VoxelPrimitive::Block { address } => VoxelPrimitive::Block {
            address: local_to_authority_address(asset, address)?,
        },
        VoxelPrimitive::Box { start, end, fill } => VoxelPrimitive::Box {
            start: local_to_authority_address(asset, start)?,
            end: local_to_authority_address(asset, end)?,
            fill,
        },
        VoxelPrimitive::Line { start, end, radius } => VoxelPrimitive::Line {
            start: local_to_authority_address(asset, start)?,
            end: local_to_authority_address(asset, end)?,
            radius,
        },
    };
    Ok(request)
}

pub(crate) fn undo_edit(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: String,
    expected_asset_content_hash: String,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    move_history(
        location,
        expected_project_hash,
        asset_id,
        expected_asset_content_hash,
        HistoryMove::Undo,
    )
}

pub(crate) fn redo_edit(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: String,
    expected_asset_content_hash: String,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    move_history(
        location,
        expected_project_hash,
        asset_id,
        expected_asset_content_hash,
        HistoryMove::Redo,
    )
}

pub(crate) fn revert_history(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: String,
    expected_asset_content_hash: String,
    target_cursor: usize,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    move_history(
        location,
        expected_project_hash,
        asset_id,
        expected_asset_content_hash,
        HistoryMove::Cursor(target_cursor),
    )
}

enum HistoryMove {
    Undo,
    Redo,
    Cursor(usize),
}

fn move_history(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: String,
    expected_asset_content_hash: String,
    operation: HistoryMove,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_, project| {
            let stored = find_voxel_asset_mut(project, &asset_id)?;
            let voxel = stored
                .voxel_volume
                .as_ref()
                .expect("voxel asset helper checked payload");
            require_asset_hash(voxel, &expected_asset_content_hash)?;
            let content_hash_before = voxel.content_hash.clone();
            let (mut scene, mut history) = scene_and_history(stored)?;
            let receipt = match operation {
                HistoryMove::Undo => history.undo_one(&mut scene),
                HistoryMove::Redo => history.redo_one(&mut scene),
                HistoryMove::Cursor(target) => history.apply_revert_to_cursor(
                    &mut scene,
                    target,
                    VoxelEditHistoryDiffOptions::default(),
                ),
            }
            .map_err(|error| reject("voxel.historyRejected", error.to_string()))?;
            install_scene_and_history(stored, &scene, &history)?;
            let content_hash_after = stored
                .voxel_volume
                .as_ref()
                .expect("installed voxel asset")
                .content_hash
                .clone();
            Ok(ProjectMutationReceipt::VoxelHistoryMoved {
                asset_id,
                content_hash_before,
                content_hash_after,
                cursor_before: receipt.cursor_before.index,
                cursor_after: receipt.cursor_after.index,
                undo_depth: receipt.cursor_after.undo_depth,
                redo_depth: receipt.cursor_after.redo_depth,
                changed_voxels: receipt.diff.changed_voxels,
            })
        },
    )?)
}

pub(crate) fn create_annotation_layer(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: String,
    draft: VoxelAnnotationLayerDraft,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_, project| {
            let stored = find_voxel_asset_mut(project, &asset_id)?;
            if stored
                .voxel_annotations
                .iter()
                .any(|layer| layer.layer_id == draft.layer_id)
            {
                return Err(reject(
                    "voxel.annotationExists",
                    format!("annotation layer `{}` already exists", draft.layer_id),
                ));
            }
            let layer = finalize_annotation_draft(
                draft,
                stored
                    .voxel_volume
                    .as_ref()
                    .expect("voxel asset helper checked payload"),
                VoxelAnnotationLimits::default(),
            )
            .map_err(|error| reject("voxel.annotationRejected", error.to_string()))?;
            let layer_id = layer.layer_id.clone();
            let layer_hash = layer.content_hashes.canonical_layer.clone();
            stored.voxel_annotations.push(layer);
            Ok(ProjectMutationReceipt::VoxelAnnotationCreated {
                asset_id,
                layer_id,
                layer_hash,
            })
        },
    )?)
}

pub(crate) fn edit_annotation(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: String,
    layer_id: String,
    transaction: VoxelAnnotationEditTransaction,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_, project| {
            let stored = find_voxel_asset_mut(project, &asset_id)?;
            let layer = stored
                .voxel_annotations
                .iter_mut()
                .find(|layer| layer.layer_id == layer_id)
                .ok_or_else(|| {
                    reject(
                        "voxel.annotationMissing",
                        format!("asset has no annotation layer `{layer_id}`"),
                    )
                })?;
            let receipt = VoxelAnnotationEditService::apply(layer, transaction)
                .map_err(|error| reject("voxel.annotationEditRejected", error.to_string()))?;
            Ok(ProjectMutationReceipt::VoxelAnnotationEdited {
                asset_id,
                layer_id,
                layer_hash_before: receipt.layer_hash_before,
                layer_hash_after: receipt.layer_hash_after,
                affected_region_ids: receipt.affected_region_ids,
            })
        },
    )?)
}

pub(crate) fn require_voxel_identity(asset_id: &str) -> Result<(), AdapterRejection> {
    let parsed = AssetId::parse(asset_id)
        .map_err(|error| reject("voxel.invalidAssetId", error.to_string()))?;
    if parsed.kind() == AssetKind::VoxelVolume {
        Ok(())
    } else {
        Err(reject(
            "voxel.wrongAssetKind",
            format!("expected voxel-volume identity, found {}", parsed.kind()),
        ))
    }
}

fn mutation_result(
    published: PublishedProject<ProjectMutationReceipt>,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    Ok((published.value, published.readout))
}
