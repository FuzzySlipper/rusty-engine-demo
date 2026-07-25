use core_assets::{AssetId, AssetKind};
use voxel_asset::MAX_CONVERSION_SOURCE_BYTES;
use voxel_convert::{
    apply_conversion, import_mesh_source, plan_conversion, preview_conversion,
    ConversionApplyRequest, ConversionPlanRequest, ConversionPlanSettings,
    ConversionPreviewRequest, MeshSourceFormat, MeshSourceImportRequest, PreparedVoxelConversion,
    VoxelConversionPlan, VoxelConversionPreview,
};

use crate::StoredAsset;

use super::super::project::publish_project_mutation;
use super::super::protocol::{AdapterRejection, ProjectMutationReceipt, StudioProjectReadout};
use super::super::ProjectLocation;
use super::model::{find_voxel_asset_mut, reject, retarget_annotations};
use super::query::{conversion_rejection, load_expected};

const MAX_LICENSE_BYTES: u64 = 4 * 1024 * 1024;

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_conversion(
    location: &ProjectLocation,
    expected_project_hash: &str,
    source_asset_id: String,
    source_path: String,
    target_asset_id: String,
    license_path: Option<String>,
    settings: ConversionPlanSettings,
    max_preview_samples: u32,
) -> Result<
    (
        PreparedVoxelConversion,
        VoxelConversionPlan,
        VoxelConversionPreview,
    ),
    AdapterRejection,
> {
    let project = load_expected(location, expected_project_hash)?;
    let source_id = AssetId::parse(&source_asset_id)
        .map_err(|error| reject("conversion.invalidSourceIdentity", error.to_string()))?;
    if source_id.kind() != AssetKind::StaticMesh {
        return Err(reject(
            "conversion.invalidSourceIdentity",
            format!("expected static mesh identity, found {}", source_id.kind()),
        ));
    }
    let source_entry = project.catalog().get(&source_id).ok_or_else(|| {
        reject(
            "conversion.sourceMissing",
            format!("catalog has no static mesh `{source_asset_id}`"),
        )
    })?;
    let source_bytes = location
        .read_relative_file(&source_path, MAX_CONVERSION_SOURCE_BYTES)
        .map_err(|error| reject("conversion.sourcePathRejected", error.to_string()))?;
    if let Some(path) = &license_path {
        location
            .read_relative_file(path, MAX_LICENSE_BYTES)
            .map_err(|error| reject("conversion.licensePathRejected", error.to_string()))?;
    }
    let imported = import_mesh_source(&MeshSourceImportRequest {
        source_asset_id,
        asset_version: u64::from(source_entry.version),
        source_path,
        format: MeshSourceFormat::Glb,
        source_bytes,
        expected_source_sha256: None,
        mesh_primitive: None,
    })
    .map_err(conversion_rejection)?;
    let prepared = plan_conversion(
        &ConversionPlanRequest {
            source: imported.receipt.source.clone(),
            target_asset_id,
            license_path,
            settings,
        },
        &imported,
    )
    .map_err(conversion_rejection)?;
    let plan = prepared.plan().clone();
    let preview = preview_conversion(
        &ConversionPreviewRequest {
            plan_id: plan.plan_id.clone(),
            expected_plan_hash: plan.plan_hash.clone(),
            max_samples: max_preview_samples,
        },
        &prepared,
    )
    .map_err(conversion_rejection)?;
    Ok((prepared, plan, preview))
}

pub(crate) fn apply_prepared_conversion(
    location: &ProjectLocation,
    expected_project_hash: &str,
    prepared: &PreparedVoxelConversion,
    plan_id: String,
    expected_plan_hash: String,
    expected_output_hash: String,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    let applied = apply_conversion(
        &ConversionApplyRequest {
            plan_id: plan_id.clone(),
            expected_plan_hash: expected_plan_hash.clone(),
            expected_output_hash: Some(expected_output_hash),
        },
        prepared,
    )
    .map_err(conversion_rejection)?;
    let target_asset_id = applied.conversion.asset.asset_id.clone();
    let output_hash = applied.output_hash.clone();
    let output_voxels = applied.conversion.output_voxels;
    let candidate = applied.conversion.asset;
    let published =
        publish_project_mutation(location, expected_project_hash, move |_, project| {
            if project
                .assets
                .iter()
                .any(|asset| asset.id == target_asset_id)
            {
                let stored = find_voxel_asset_mut(project, &target_asset_id)?;
                stored.voxel_volume = Some(candidate);
                stored.voxel_edit_history = None;
                retarget_annotations(stored)?;
            } else {
                project.assets.push(StoredAsset {
                    id: target_asset_id.clone(),
                    voxel_volume: Some(candidate),
                    voxel_edit_history: None,
                    voxel_annotations: Vec::new(),
                    material: None,
                });
            }
            Ok(ProjectMutationReceipt::VoxelConversionApplied {
                plan_id,
                plan_hash: expected_plan_hash,
                asset_id: target_asset_id,
                output_hash,
                output_voxels,
            })
        })?;
    Ok((published.value, published.readout))
}
