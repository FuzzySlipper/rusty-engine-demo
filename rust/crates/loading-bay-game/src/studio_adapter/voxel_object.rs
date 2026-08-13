use std::collections::BTreeSet;

use rusty_engine::core_assets::{AssetId, AssetKind};
use rusty_engine::voxel_asset::MAX_CONVERSION_SOURCE_BYTES;
use rusty_engine::voxel_convert::{
    apply_voxel_object_conversion, import_animated_mesh_source, import_mesh_source,
    plan_animated_voxel_object_conversion, plan_static_voxel_object_conversion,
    preview_voxel_object_conversion, source_sha256, AnimationProperty, MeshSourceFormat,
    MeshSourceImportRequest, PreparedVoxelObjectConversion, VoxelObjectClipConversionRequest,
    VoxelObjectConversionApplyRequest, VoxelObjectConversionPlan, VoxelObjectConversionPlanRequest,
    VoxelObjectConversionPreview, VoxelObjectConversionPreviewRequest,
    VoxelObjectConversionSettings, VoxelObjectFrameSelection,
};

use crate::{
    StoredAsset, StoredEntityDefinition, StoredVoxelObjectFrameSelection,
    StoredVoxelObjectInstance, StoredVoxelObjectMaterialOverride, StoredVoxelObjectSurfaceMode,
};

use super::path::ProjectLocation;
use super::project::{
    publish_project_mutation, publish_project_mutation_with_validation, OpenedOwnerProject,
};
use super::protocol::{
    AdapterRejection, ProjectMutationReceipt, ProjectionReadout, StudioFileSelection,
    StudioProjectReadout, StudioVoxelObjectInstance, StudioVoxelObjectPlacement,
    StudioVoxelObjectSourceKind, VoxelObjectAnimationProperty,
    VoxelObjectInstanceAttachmentReceipt, VoxelObjectSourceClipReadout,
    VoxelObjectSourceDiagnostic, VoxelObjectSourceInspection, MAX_STUDIO_ADAPTER_RESPONSE_BYTES,
    MAX_VOXEL_OBJECT_INSTANCE_BATCH, STUDIO_ADAPTER_PROTOCOL_VERSION,
};
use super::voxel::{conversion_rejection, load_expected, read_selection};

const MAX_LICENSE_BYTES: usize = 4 * 1024 * 1024;
const JSON_SAFE_U64_MASK: u64 = (1_u64 << 53) - 1;

pub(crate) struct PreparedProjectVoxelObjectConversion {
    expected_project_hash: String,
    source: StudioFileSelection,
    source_sha256: String,
    prepared: PreparedVoxelObjectConversion,
}

impl PreparedProjectVoxelObjectConversion {
    pub(crate) fn plan_id(&self) -> &str {
        &self.prepared.plan().plan_id
    }
}

pub(crate) fn inspect_voxel_object_source(
    location: &ProjectLocation,
    expected_project_hash: &str,
    source_kind: StudioVoxelObjectSourceKind,
    source_asset_id: String,
    source: StudioFileSelection,
    mesh_primitive: Option<String>,
) -> Result<VoxelObjectSourceInspection, AdapterRejection> {
    let project = load_expected(location, expected_project_hash)?;
    let request = source_import_request(
        &project,
        location,
        source_kind,
        source_asset_id,
        &source,
        mesh_primitive,
    )?;
    match source_kind {
        StudioVoxelObjectSourceKind::Static => {
            let imported = import_mesh_source(&request).map_err(conversion_rejection)?;
            Ok(VoxelObjectSourceInspection {
                source_kind,
                source: imported.receipt.source,
                source_path: imported.receipt.source_path,
                source_byte_count: imported.receipt.source_byte_count,
                metadata: imported.receipt.metadata,
                clips: Vec::new(),
                diagnostics: vec![VoxelObjectSourceDiagnostic {
                    severity: "info",
                    code: "voxelObject.staticSource",
                    path: "sourceKind",
                    message: "static source will produce one reusable default frame",
                }],
            })
        }
        StudioVoxelObjectSourceKind::Animated => {
            let imported = import_animated_mesh_source(&request).map_err(conversion_rejection)?;
            let clips = imported
                .model
                .clips
                .iter()
                .map(|clip| {
                    let mut targets = clip
                        .channels
                        .iter()
                        .map(|channel| channel.target_node_index)
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>();
                    targets.sort_unstable();
                    let mut properties = Vec::new();
                    for channel in &clip.channels {
                        let property = match channel.property {
                            AnimationProperty::Translation => {
                                VoxelObjectAnimationProperty::Translation
                            }
                            AnimationProperty::Rotation => VoxelObjectAnimationProperty::Rotation,
                            AnimationProperty::Scale => VoxelObjectAnimationProperty::Scale,
                            AnimationProperty::MorphWeights => {
                                VoxelObjectAnimationProperty::MorphWeights
                            }
                        };
                        if !properties.iter().any(|existing| {
                            std::mem::discriminant(existing) == std::mem::discriminant(&property)
                        }) {
                            properties.push(property);
                        }
                    }
                    VoxelObjectSourceClipReadout {
                        source_animation_index: clip.source_animation_index,
                        name: clip.name.clone(),
                        duration_microseconds: clip.duration_microseconds,
                        channel_count: clip.channels.len(),
                        target_node_indices: targets,
                        properties,
                    }
                })
                .collect();
            Ok(VoxelObjectSourceInspection {
                source_kind,
                source: imported.source.receipt.source,
                source_path: imported.source.receipt.source_path,
                source_byte_count: imported.source.receipt.source_byte_count,
                metadata: imported.source.receipt.metadata,
                clips,
                diagnostics: vec![VoxelObjectSourceDiagnostic {
                    severity: "info",
                    code: "voxelObject.animatedSource",
                    path: "sourceKind",
                    message: "animated source exposes Rust-owned clips and channel targets",
                }],
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_voxel_object_conversion(
    location: &ProjectLocation,
    expected_project_hash: &str,
    source_kind: StudioVoxelObjectSourceKind,
    source_asset_id: String,
    source: StudioFileSelection,
    target_asset_id: String,
    license: Option<StudioFileSelection>,
    mesh_primitive: Option<String>,
    settings: VoxelObjectConversionSettings,
    clips: Vec<VoxelObjectClipConversionRequest>,
    default_clip: Option<String>,
    frame: VoxelObjectFrameSelection,
    max_preview_samples: u32,
) -> Result<
    (
        PreparedProjectVoxelObjectConversion,
        VoxelObjectConversionPlan,
        VoxelObjectConversionPreview,
        rusty_engine::render_model::RenderFrameDiff,
        ProjectionReadout,
    ),
    AdapterRejection,
> {
    let project = load_expected(location, expected_project_hash)?;
    let request = source_import_request(
        &project,
        location,
        source_kind,
        source_asset_id,
        &source,
        mesh_primitive,
    )?;
    let license_path = license
        .as_ref()
        .map(|selection| selection.path().to_string());
    if let Some(selection) = &license {
        read_selection(location, selection, MAX_LICENSE_BYTES)
            .map_err(|error| error.at_path(selection.path().to_string()))?;
    }
    let (prepared, source_sha256) = match source_kind {
        StudioVoxelObjectSourceKind::Static => {
            let imported = import_mesh_source(&request).map_err(conversion_rejection)?;
            let source_ref = imported.receipt.source.clone();
            let prepared = plan_static_voxel_object_conversion(
                &VoxelObjectConversionPlanRequest {
                    source: source_ref.clone(),
                    source_path: request.source_path.clone(),
                    target_asset_id,
                    license_path,
                    settings,
                    clips,
                    default_clip,
                },
                &imported,
            )
            .map_err(conversion_rejection)?;
            (prepared, source_ref.source_sha256)
        }
        StudioVoxelObjectSourceKind::Animated => {
            let imported = import_animated_mesh_source(&request).map_err(conversion_rejection)?;
            let source_ref = imported.source.receipt.source.clone();
            let prepared = plan_animated_voxel_object_conversion(
                &VoxelObjectConversionPlanRequest {
                    source: source_ref.clone(),
                    source_path: request.source_path.clone(),
                    target_asset_id,
                    license_path,
                    settings,
                    clips,
                    default_clip,
                },
                &imported,
            )
            .map_err(conversion_rejection)?;
            (prepared, source_ref.source_sha256)
        }
    };
    finish_prepare(
        project,
        expected_project_hash,
        source,
        source_sha256,
        prepared,
        frame,
        max_preview_samples,
    )
}

fn finish_prepare(
    project: OpenedOwnerProject,
    expected_project_hash: &str,
    source: StudioFileSelection,
    source_sha256: String,
    prepared: PreparedVoxelObjectConversion,
    frame: VoxelObjectFrameSelection,
    max_preview_samples: u32,
) -> Result<
    (
        PreparedProjectVoxelObjectConversion,
        VoxelObjectConversionPlan,
        VoxelObjectConversionPreview,
        rusty_engine::render_model::RenderFrameDiff,
        ProjectionReadout,
    ),
    AdapterRejection,
> {
    project.validate_voxel_object_candidate(&prepared.candidate().asset)?;
    let plan = prepared.plan().clone();
    let preview = preview_voxel_object_conversion(
        &VoxelObjectConversionPreviewRequest {
            plan_id: plan.plan_id.clone(),
            expected_plan_hash: plan.plan_hash.clone(),
            frame: frame.clone(),
            max_samples: max_preview_samples,
        },
        &prepared,
    )
    .map_err(conversion_rejection)?;
    let (projection, projection_readout) =
        project.voxel_object_candidate_projection(&prepared.candidate().asset, &frame)?;
    Ok((
        PreparedProjectVoxelObjectConversion {
            expected_project_hash: expected_project_hash.to_string(),
            source,
            source_sha256,
            prepared,
        },
        plan,
        preview,
        projection,
        projection_readout,
    ))
}

pub(crate) fn preview_prepared_voxel_object_conversion(
    location: &ProjectLocation,
    candidate: &PreparedProjectVoxelObjectConversion,
    plan_id: String,
    expected_plan_hash: String,
    frame: VoxelObjectFrameSelection,
    max_preview_samples: u32,
) -> Result<
    (
        VoxelObjectConversionPreview,
        rusty_engine::render_model::RenderFrameDiff,
        ProjectionReadout,
    ),
    AdapterRejection,
> {
    let project = load_expected(location, &candidate.expected_project_hash)?;
    project.validate_voxel_object_candidate(&candidate.prepared.candidate().asset)?;
    let preview = preview_voxel_object_conversion(
        &VoxelObjectConversionPreviewRequest {
            plan_id,
            expected_plan_hash,
            frame: frame.clone(),
            max_samples: max_preview_samples,
        },
        &candidate.prepared,
    )
    .map_err(conversion_rejection)?;
    let (projection, projection_readout) =
        project.voxel_object_candidate_projection(&candidate.prepared.candidate().asset, &frame)?;
    Ok((preview, projection, projection_readout))
}

pub(crate) fn apply_prepared_voxel_object_conversion(
    location: &ProjectLocation,
    expected_project_hash: &str,
    candidate: &PreparedProjectVoxelObjectConversion,
    plan_id: String,
    expected_plan_hash: String,
    expected_output_hash: String,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    if expected_project_hash != candidate.expected_project_hash {
        return Err(reject(
            "project.staleHash",
            "object candidate belongs to a different project revision",
        ));
    }
    let project = load_expected(location, expected_project_hash)?;
    project.validate_voxel_object_candidate(&candidate.prepared.candidate().asset)?;
    let source_bytes = read_selection(
        location,
        &candidate.source,
        MAX_CONVERSION_SOURCE_BYTES as usize,
    )?;
    if source_sha256(&source_bytes) != candidate.source_sha256 {
        return Err(reject(
            "conversion.staleSource",
            "object source changed after prepare",
        ));
    }
    let applied = apply_voxel_object_conversion(
        &VoxelObjectConversionApplyRequest {
            plan_id: plan_id.clone(),
            expected_plan_hash: expected_plan_hash.clone(),
            expected_output_hash: Some(expected_output_hash),
        },
        &candidate.prepared,
    )
    .map_err(conversion_rejection)?;
    let asset_id = applied.conversion.asset.asset_id.clone();
    let output_hash = applied.output_hash.clone();
    let stored_frames = applied.conversion.stored_frames;
    let aggregate_voxels = applied.conversion.aggregate_voxels;
    let object = applied.conversion.asset;
    let published =
        publish_project_mutation(location, expected_project_hash, move |_, project| {
            if let Some(existing) = project.assets.iter_mut().find(|asset| asset.id == asset_id) {
                if existing.voxel_object.is_none() {
                    return Err(reject(
                        "voxelObject.assetConflict",
                        format!("asset `{asset_id}` belongs to another payload kind"),
                    ));
                }
                existing.voxel_object = Some(object);
            } else {
                project.assets.push(StoredAsset {
                    id: asset_id.clone(),
                    catalog: None,
                    static_mesh: None,
                    animated_mesh: None,
                    sprite_atlas: None,
                    import: None,
                    voxel_volume: None,
                    voxel_object: Some(object),
                    voxel_edit_history: None,
                    voxel_annotations: Vec::new(),
                    material: None,
                });
            }
            Ok(ProjectMutationReceipt::VoxelObjectConversionApplied {
                plan_id,
                plan_hash: expected_plan_hash,
                asset_id,
                output_hash,
                stored_frames,
                aggregate_voxels,
            })
        })?;
    Ok((published.value, published.readout))
}

pub(crate) fn attach_voxel_object_instance(
    location: &ProjectLocation,
    expected_project_hash: &str,
    scene_id: String,
    instance: StudioVoxelObjectInstance,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    let instance_id = instance.instance_id.clone();
    let asset_id = instance.voxel_object_asset_id.clone();
    let frame_kind = match instance.frame {
        StoredVoxelObjectFrameSelection::Default => "default",
        StoredVoxelObjectFrameSelection::Clip { .. } => "clip",
    };
    let published =
        publish_project_mutation(location, expected_project_hash, move |_, project| {
            ensure_surface_mode_supported(
                project,
                &instance.voxel_object_asset_id,
                &instance.material_overrides,
                instance.surface_mode,
            )?;
            let scene = project
                .scenes
                .iter_mut()
                .find(|scene| scene.id == scene_id)
                .ok_or_else(|| {
                    reject(
                        "project.missingScene",
                        format!("project has no scene `{scene_id}`"),
                    )
                })?;
            if let Some(existing_index) = scene
                .voxel_object_instances
                .iter()
                .position(|existing| existing.instance_id == instance.instance_id)
            {
                let owner_entity_id = scene.voxel_object_instances[existing_index].owner_entity_id;
                let owner = scene
                    .entities
                    .iter_mut()
                    .find(|entity| entity.id == owner_entity_id)
                    .ok_or_else(|| {
                        reject(
                            "voxelObject.ownerMissing",
                            format!(
                                "voxel-object instance `{}` has no entity owner {}",
                                instance.instance_id, owner_entity_id
                            ),
                        )
                    })?;
                owner.translation = Some(instance.translation);
                owner.rotation = instance.rotation;
                owner.scale = instance.scale;
                scene.voxel_object_instances[existing_index] =
                    stored_voxel_object_instance(owner_entity_id, instance);
            } else {
                let owner_entity_id = scene
                    .entities
                    .iter()
                    .map(|entity| entity.id)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .filter(|entity_id| *entity_id <= JSON_SAFE_U64_MASK)
                    .ok_or_else(|| {
                        reject(
                            "voxelObject.ownerIdentityExhausted",
                            "cannot allocate another JSON-safe entity owner",
                        )
                    })?;
                let child_order = scene
                    .entities
                    .iter()
                    .filter(|entity| entity.parent.is_none())
                    .map(|entity| entity.child_order)
                    .max()
                    .map_or(Some(0), |order| order.checked_add(1))
                    .ok_or_else(|| {
                        reject(
                            "voxelObject.ownerOrderExhausted",
                            "cannot allocate another root hierarchy order",
                        )
                    })?;
                scene
                    .entities
                    .push(voxel_object_owner(owner_entity_id, child_order, &instance));
                scene
                    .voxel_object_instances
                    .push(stored_voxel_object_instance(owner_entity_id, instance));
            }
            Ok(ProjectMutationReceipt::VoxelObjectInstanceAttached {
                scene_id,
                instance_id,
                asset_id,
                frame_kind,
            })
        })?;
    Ok((published.value, published.readout))
}

pub(crate) fn attach_voxel_object_instances(
    location: &ProjectLocation,
    request_id: &str,
    expected_project_hash: &str,
    placements: Vec<StudioVoxelObjectPlacement>,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    if placements.is_empty() || placements.len() > MAX_VOXEL_OBJECT_INSTANCE_BATCH {
        return Err(reject(
            "voxelObject.invalidPlacementBatch",
            format!(
                "voxel-object placement batch must contain 1..={} entries",
                MAX_VOXEL_OBJECT_INSTANCE_BATCH
            ),
        ));
    }
    let mut request_instance_ids = BTreeSet::new();
    for (index, placement) in placements.iter().enumerate() {
        if !request_instance_ids.insert(placement.instance.instance_id.as_str()) {
            return Err(reject(
                "voxelObject.duplicatePlacementIdentity",
                format!(
                    "placements[{index}] repeats instance identity `{}`",
                    placement.instance.instance_id
                ),
            ));
        }
    }

    let request_id = request_id.to_string();
    let published = publish_project_mutation_with_validation(
        location,
        expected_project_hash,
        move |current, project| {
            let existing_instance_ids = current
                .document()
                .scenes
                .iter()
                .flat_map(|scene| scene.voxel_object_instances.iter())
                .map(|instance| instance.instance_id.as_str())
                .collect::<BTreeSet<_>>();
            if let Some((index, placement)) =
                placements.iter().enumerate().find(|(_, placement)| {
                    existing_instance_ids.contains(placement.instance.instance_id.as_str())
                })
            {
                return Err(reject(
                    "voxelObject.instanceIdentityCollision",
                    format!(
                        "placements[{index}] collides with existing instance `{}`",
                        placement.instance.instance_id
                    ),
                ));
            }

            let mut next_owner_entity_id = project
                .scenes
                .iter()
                .flat_map(|scene| scene.entities.iter())
                .map(|entity| entity.id)
                .max()
                .unwrap_or(0);
            let mut receipt = Vec::with_capacity(placements.len());
            for (index, placement) in placements.into_iter().enumerate() {
                ensure_surface_mode_supported(
                    project,
                    &placement.instance.voxel_object_asset_id,
                    &placement.instance.material_overrides,
                    placement.instance.surface_mode,
                )?;
                next_owner_entity_id = next_owner_entity_id
                    .checked_add(1)
                    .filter(|entity_id| *entity_id <= JSON_SAFE_U64_MASK)
                    .ok_or_else(|| {
                        reject(
                            "voxelObject.ownerIdentityExhausted",
                            format!(
                                "placements[{index}] cannot allocate another JSON-safe entity owner"
                            ),
                        )
                    })?;
                let scene = project
                    .scenes
                    .iter_mut()
                    .find(|scene| scene.id == placement.scene_id)
                    .ok_or_else(|| {
                        reject(
                            "project.missingScene",
                            format!(
                                "placements[{index}] references missing scene `{}`",
                                placement.scene_id
                            ),
                        )
                    })?;
                let child_order = scene
                    .entities
                    .iter()
                    .filter(|entity| entity.parent.is_none())
                    .map(|entity| entity.child_order)
                    .max()
                    .map_or(Some(0), |order| order.checked_add(1))
                    .ok_or_else(|| {
                        reject(
                            "voxelObject.ownerOrderExhausted",
                            format!(
                                "placements[{index}] cannot allocate another root hierarchy order"
                            ),
                        )
                    })?;
                let frame_kind = match &placement.instance.frame {
                    StoredVoxelObjectFrameSelection::Default => "default",
                    StoredVoxelObjectFrameSelection::Clip { .. } => "clip",
                };
                let instance_id = placement.instance.instance_id.clone();
                let asset_id = placement.instance.voxel_object_asset_id.clone();
                scene.entities.push(voxel_object_owner(
                    next_owner_entity_id,
                    child_order,
                    &placement.instance,
                ));
                scene
                    .voxel_object_instances
                    .push(stored_voxel_object_instance(
                        next_owner_entity_id,
                        placement.instance,
                    ));
                receipt.push(VoxelObjectInstanceAttachmentReceipt {
                    scene_id: placement.scene_id,
                    instance_id,
                    asset_id,
                    frame_kind,
                    owner_entity_id: next_owner_entity_id,
                });
            }
            Ok(ProjectMutationReceipt::VoxelObjectInstancesAttached {
                placements: receipt,
            })
        },
        |receipt, readout| {
            #[derive(serde::Serialize)]
            #[serde(rename_all = "camelCase")]
            struct MutationResponseProbe<'a> {
                #[serde(rename = "type")]
                response_type: &'static str,
                protocol_version: u32,
                request_id: &'a str,
                receipt: &'a ProjectMutationReceipt,
                project: &'a StudioProjectReadout,
            }

            let encoded = serde_json::to_vec(&MutationResponseProbe {
                response_type: "projectMutationApplied",
                protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
                request_id: &request_id,
                receipt,
                project: readout,
            })
            .map_err(|error| reject("protocol.responseEncode", error.to_string()))?;
            if encoded.len() > MAX_STUDIO_ADAPTER_RESPONSE_BYTES {
                return Err(reject(
                    "protocol.responseTooLarge",
                    format!(
                        "batch mutation response is {} bytes, exceeding the {}-byte bound",
                        encoded.len(),
                        MAX_STUDIO_ADAPTER_RESPONSE_BYTES
                    ),
                ));
            }
            Ok(())
        },
    )?;
    Ok((published.value, published.readout))
}

pub(crate) fn set_voxel_object_instance_surface_mode(
    location: &ProjectLocation,
    expected_project_hash: &str,
    scene_id: String,
    instance_id: String,
    surface_mode: StoredVoxelObjectSurfaceMode,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    let published =
        publish_project_mutation(location, expected_project_hash, move |_, project| {
            let scene_index = project
                .scenes
                .iter()
                .position(|scene| scene.id == scene_id)
                .ok_or_else(|| {
                    reject(
                        "project.missingScene",
                        format!("project has no scene `{scene_id}`"),
                    )
                })?;
            let instance_index = project.scenes[scene_index]
                .voxel_object_instances
                .iter()
                .position(|instance| instance.instance_id == instance_id)
                .ok_or_else(|| {
                    reject(
                        "voxelObject.instanceMissing",
                        format!("scene `{scene_id}` has no voxel-object instance `{instance_id}`"),
                    )
                })?;
            let instance = &project.scenes[scene_index].voxel_object_instances[instance_index];
            ensure_surface_mode_supported(
                project,
                &instance.voxel_object_asset_id,
                &instance.material_overrides,
                surface_mode,
            )?;
            let instance = &mut project.scenes[scene_index].voxel_object_instances[instance_index];
            let before = instance.surface_mode;
            instance.surface_mode = surface_mode;
            Ok(ProjectMutationReceipt::VoxelObjectSurfaceModeSet {
                scene_id,
                instance_id,
                before,
                after: surface_mode,
            })
        })?;
    Ok((published.value, published.readout))
}

pub(crate) fn prepare_voxel_object_placement(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: &str,
    expected_object_content_hash: &str,
) -> Result<rusty_engine::render_model::RenderFrameDiff, AdapterRejection> {
    const MAX_PRESENTATION_IDENTITY_BYTES: usize = 128;

    if asset_id.len() > MAX_PRESENTATION_IDENTITY_BYTES {
        return Err(reject(
            "voxelObject.invalidAssetIdentity",
            format!(
                "asset identity is {} bytes, exceeding the {}-byte placement bound",
                asset_id.len(),
                MAX_PRESENTATION_IDENTITY_BYTES
            ),
        ));
    }
    let parsed = AssetId::parse(asset_id)
        .map_err(|error| reject("voxelObject.invalidAssetIdentity", error.to_string()))?;
    if parsed.kind() != AssetKind::VoxelObject {
        return Err(reject(
            "voxelObject.invalidAssetIdentity",
            format!("expected voxel-object identity, found {}", parsed.kind()),
        ));
    }
    let project = load_expected(location, expected_project_hash)?;
    project.voxel_object_placement_resource(asset_id, expected_object_content_hash)
}

fn stored_voxel_object_instance(
    owner_entity_id: u64,
    instance: StudioVoxelObjectInstance,
) -> StoredVoxelObjectInstance {
    StoredVoxelObjectInstance {
        owner_entity_id,
        instance_id: instance.instance_id,
        voxel_object_asset_id: instance.voxel_object_asset_id,
        surface_mode: instance.surface_mode,
        frame: instance.frame,
        translation: instance.translation,
        rotation: instance.rotation,
        scale: instance.scale,
        material_overrides: instance.material_overrides,
    }
}

fn ensure_surface_mode_supported(
    project: &crate::StoredProject,
    asset_id: &str,
    material_overrides: &[StoredVoxelObjectMaterialOverride],
    surface_mode: StoredVoxelObjectSurfaceMode,
) -> Result<(), AdapterRejection> {
    if surface_mode.is_default() {
        return Ok(());
    }
    let object = project
        .assets
        .iter()
        .find(|asset| asset.id == asset_id)
        .and_then(|asset| asset.voxel_object.as_ref())
        .ok_or_else(|| {
            reject(
                "voxelObject.assetMissing",
                format!("project has no voxel-object asset `{asset_id}`"),
            )
        })?;
    let textured = object.material_palette.iter().any(|binding| {
        let material_id = material_overrides
            .iter()
            .find(|entry| entry.material_slot == binding.material_slot)
            .map_or(binding.material_asset_id.as_str(), |entry| {
                entry.material_asset_id.as_str()
            });
        project.assets.iter().any(|asset| {
            asset.id == material_id
                && asset.material.as_ref().is_some_and(|material| {
                    material.style.texture.is_some() || material.style.voxel_surface.is_some()
                })
        })
    });
    if textured {
        return Err(reject(
            "voxelObject.surfaceTextureUnsupported",
            format!(
                "{} has no stable UV contract for a textured voxel-object material",
                surface_mode.as_str()
            ),
        ));
    }
    Ok(())
}

fn voxel_object_owner(
    entity_id: u64,
    child_order: u32,
    instance: &StudioVoxelObjectInstance,
) -> StoredEntityDefinition {
    StoredEntityDefinition {
        id: entity_id,
        name: instance.instance_id.clone(),
        parent: None,
        child_order,
        translation: Some(instance.translation),
        rotation: instance.rotation,
        scale: instance.scale,
        light: None,
        bounds: None,
        collision: None,
        renderable: None,
        door: None,
        switch: None,
        floor_action: None,
        lift: None,
        enemy: false,
        enemy_combat: None,
        defeat_drop: None,
        health: None,
        explosive_prop: None,
        hazard: None,
        encounter: None,
        extraction_beacon: None,
        kinematic: None,
        navigation: None,
        player_controller: None,
        inventory: None,
        pickup: None,
        weapon: None,
        secret_region: None,
        level_exit: None,
        doom_sprite_inspection: None,
    }
}

fn source_import_request(
    project: &OpenedOwnerProject,
    location: &ProjectLocation,
    source_kind: StudioVoxelObjectSourceKind,
    source_asset_id: String,
    source: &StudioFileSelection,
    mesh_primitive: Option<String>,
) -> Result<MeshSourceImportRequest, AdapterRejection> {
    let source_id = AssetId::parse(&source_asset_id)
        .map_err(|error| reject("conversion.invalidSourceIdentity", error.to_string()))?;
    let expected_kind = match source_kind {
        StudioVoxelObjectSourceKind::Static => AssetKind::StaticMesh,
        StudioVoxelObjectSourceKind::Animated => AssetKind::AnimatedMesh,
    };
    if source_id.kind() != expected_kind {
        return Err(reject(
            "conversion.invalidSourceIdentity",
            format!(
                "expected {expected_kind} identity, found {}",
                source_id.kind()
            ),
        ));
    }
    let entry = project.catalog().get(&source_id).ok_or_else(|| {
        reject(
            "conversion.sourceMissing",
            format!("catalog has no source `{source_asset_id}`"),
        )
    })?;
    let source_bytes = read_selection(location, source, MAX_CONVERSION_SOURCE_BYTES as usize)
        .map_err(|error| error.at_path(source.path().to_string()))?;
    Ok(MeshSourceImportRequest {
        source_asset_id,
        asset_version: u64::from(entry.version),
        source_path: source.path().to_string(),
        format: MeshSourceFormat::Glb,
        source_bytes,
        expected_source_sha256: None,
        mesh_primitive,
    })
}

fn reject(code: impl Into<String>, message: impl Into<String>) -> AdapterRejection {
    AdapterRejection::new(code, message)
}

#[cfg(test)]
mod tests {
    use rusty_engine::asset_catalog::{StoredAssetReference, StoredAssetVersionRequirement};

    use super::ensure_surface_mode_supported;
    use crate::{decode_project_document, StoredVoxelObjectSurfaceMode};

    const LOADING_BAY_PROJECT: &str =
        include_str!("../../../../../content/projects/loading-bay.project.json");

    #[test]
    fn reconstructed_surface_rejects_an_effective_textured_material() {
        let mut project = decode_project_document(LOADING_BAY_PROJECT)
            .unwrap()
            .project;
        let object = project
            .assets
            .iter()
            .find_map(|asset| asset.voxel_object.as_ref())
            .unwrap();
        let asset_id = object.asset_id.clone();
        let material_id = object.material_palette[0].material_asset_id.clone();
        project
            .assets
            .iter_mut()
            .find(|asset| asset.id == material_id)
            .unwrap()
            .material
            .as_mut()
            .unwrap()
            .style
            .texture = Some(StoredAssetReference {
            id: "texture/test-tile".to_string(),
            version: StoredAssetVersionRequirement::Exact { value: 1 },
            hash: None,
        });

        let rejected = ensure_surface_mode_supported(
            &project,
            &asset_id,
            &[],
            StoredVoxelObjectSurfaceMode::MarchingCubes,
        )
        .unwrap_err();
        assert_eq!(rejected.code, "voxelObject.surfaceTextureUnsupported");
    }
}
