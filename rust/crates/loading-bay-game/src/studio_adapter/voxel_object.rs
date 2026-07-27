use std::collections::BTreeSet;

use core_assets::{AssetId, AssetKind};
use voxel_asset::MAX_CONVERSION_SOURCE_BYTES;
use voxel_convert::{
    apply_voxel_object_conversion, import_animated_mesh_source, import_mesh_source,
    plan_animated_voxel_object_conversion, plan_static_voxel_object_conversion,
    preview_voxel_object_conversion, source_sha256, AnimationProperty, MeshSourceFormat,
    MeshSourceImportRequest, PreparedVoxelObjectConversion, VoxelObjectClipConversionRequest,
    VoxelObjectConversionApplyRequest, VoxelObjectConversionPlan, VoxelObjectConversionPlanRequest,
    VoxelObjectConversionPreview, VoxelObjectConversionPreviewRequest,
    VoxelObjectConversionSettings, VoxelObjectFrameSelection,
};

use crate::{StoredAsset, StoredVoxelObjectFrameSelection, StoredVoxelObjectInstance};

use super::path::ProjectLocation;
use super::project::{publish_project_mutation, OpenedOwnerProject};
use super::protocol::{
    AdapterRejection, ProjectMutationReceipt, ProjectionReadout, StudioFileSelection,
    StudioProjectReadout, StudioVoxelObjectSourceKind, VoxelObjectAnimationProperty,
    VoxelObjectSourceClipReadout, VoxelObjectSourceDiagnostic, VoxelObjectSourceInspection,
};
use super::voxel::{conversion_rejection, load_expected, read_selection};

const MAX_LICENSE_BYTES: usize = 4 * 1024 * 1024;

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
        render_model::RenderFrameDiff,
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
        render_model::RenderFrameDiff,
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
        render_model::RenderFrameDiff,
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
    instance: StoredVoxelObjectInstance,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    let instance_id = instance.instance_id.clone();
    let asset_id = instance.voxel_object_asset_id.clone();
    let frame_kind = match instance.frame {
        StoredVoxelObjectFrameSelection::Default => "default",
        StoredVoxelObjectFrameSelection::Clip { .. } => "clip",
    };
    let published =
        publish_project_mutation(location, expected_project_hash, move |_, project| {
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
            scene.voxel_object_instances.push(instance);
            Ok(ProjectMutationReceipt::VoxelObjectInstanceAttached {
                scene_id,
                instance_id,
                asset_id,
                frame_kind,
            })
        })?;
    Ok((published.value, published.readout))
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
