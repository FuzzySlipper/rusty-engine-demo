use core_assets::{AssetId, AssetKind};
use voxel_asset::MAX_CONVERSION_SOURCE_BYTES;
use voxel_convert::{
    apply_conversion, import_mesh_source, plan_conversion, preview_conversion,
    ConversionApplyRequest, ConversionPlanRequest, ConversionPlanSettings,
    ConversionPreviewRequest, MeshSourceFormat, MeshSourceImportRequest, PreparedVoxelConversion,
    VoxelConversionPlan, VoxelConversionPreview,
};

use crate::StoredAsset;

use super::super::host_file::read_host_file;
use super::super::project::publish_project_mutation;
use super::super::protocol::{
    AdapterRejection, ProjectMutationReceipt, StudioFileSelection, StudioProjectReadout,
};
use super::super::ProjectLocation;
use super::model::{find_voxel_asset_mut, reject, retarget_annotations};
use super::query::{conversion_rejection, load_expected};

const MAX_LICENSE_BYTES: u64 = 4 * 1024 * 1024;

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_conversion(
    location: &ProjectLocation,
    expected_project_hash: &str,
    source_asset_id: String,
    source: StudioFileSelection,
    target_asset_id: String,
    license: Option<StudioFileSelection>,
    mesh_primitive: Option<String>,
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
    let source_path = source.path().to_string();
    let source_bytes = read_selection(location, &source, MAX_CONVERSION_SOURCE_BYTES as usize)
        .map_err(|error| error.at_path(source_path.clone()))?;
    let license_path = license
        .as_ref()
        .map(|selection| selection.path().to_string());
    if let Some(selection) = &license {
        read_selection(location, selection, MAX_LICENSE_BYTES as usize)
            .map_err(|error| error.at_path(selection.path().to_string()))?;
    }
    let imported = import_mesh_source(&MeshSourceImportRequest {
        source_asset_id,
        asset_version: u64::from(source_entry.version),
        source_path,
        format: MeshSourceFormat::Glb,
        source_bytes,
        expected_source_sha256: None,
        mesh_primitive,
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

fn read_selection(
    location: &ProjectLocation,
    selection: &StudioFileSelection,
    max_bytes: usize,
) -> Result<Vec<u8>, AdapterRejection> {
    match selection {
        StudioFileSelection::Project { path } => location
            .read_relative_file(path, max_bytes as u64)
            .map_err(|error| reject("conversion.projectFileRejected", error.to_string())),
        StudioFileSelection::Host { path } => read_host_file(path, max_bytes)
            .map(|source| source.bytes)
            .map_err(|error| {
                reject(
                    "conversion.hostFileRejected",
                    format!("{}: {}", error.code, error.message),
                )
            }),
    }
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
                    catalog: None,
                    static_mesh: None,
                    import: None,
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
