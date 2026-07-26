use voxel_asset::{
    decode_voxel_asset, encode_voxel_asset, with_computed_content_hash, MAX_ARTIFACT_BYTES,
};

use crate::StoredAsset;

use super::super::host_file::{read_host_file, write_host_file_atomic, HostFileWriteReceipt};
use super::super::project::publish_project_mutation;
use super::super::protocol::{AdapterRejection, ProjectMutationReceipt, StudioProjectReadout};
use super::super::ProjectLocation;
use super::model::{find_voxel_asset, reject, require_asset_hash};

pub(crate) fn import_voxel_asset_file(
    location: &ProjectLocation,
    expected_project_hash: &str,
    source_path: String,
    target_asset_id: String,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    super::mutation::require_voxel_identity(&target_asset_id)?;
    let source = read_host_file(&source_path, MAX_ARTIFACT_BYTES)?;
    let text = std::str::from_utf8(&source.bytes)
        .map_err(|error| reject("voxel.fileInvalidUtf8", error.to_string()))?;
    let mut asset = decode_voxel_asset(text)
        .map_err(|error| reject("voxel.fileRejected", error.to_string()))?;
    let source_asset_id = asset.asset_id.clone();
    asset.asset_id = target_asset_id.clone();
    asset.content_hash.clear();
    asset = with_computed_content_hash(asset)
        .map_err(|error| reject("voxel.fileRejected", error.to_string()))?;
    let content_hash = asset.content_hash.clone();
    let source_path = source.path.display().to_string();
    let source_sha256 = source.sha256;
    let source_byte_count = source.bytes.len();
    let published =
        publish_project_mutation(location, expected_project_hash, move |_, project| {
            if project
                .assets
                .iter()
                .any(|stored| stored.id == target_asset_id)
            {
                return Err(reject(
                    "voxel.assetExists",
                    format!("asset `{target_asset_id}` already exists"),
                ));
            }
            project.assets.push(StoredAsset {
                id: target_asset_id.clone(),
                catalog: None,
                static_mesh: None,
                animated_mesh: None,
                import: None,
                voxel_volume: Some(asset),
                voxel_edit_history: None,
                voxel_annotations: Vec::new(),
                material: None,
            });
            Ok(ProjectMutationReceipt::VoxelAssetFileImported {
                source_path,
                source_sha256,
                source_byte_count,
                source_asset_id,
                target_asset_id,
                content_hash,
            })
        })?;
    Ok((published.value, published.readout))
}

pub(crate) fn export_voxel_asset_file(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: &str,
    expected_asset_content_hash: &str,
    target_path: &str,
    expected_target_sha256: Option<&str>,
) -> Result<HostFileWriteReceipt, AdapterRejection> {
    let project = super::query::load_expected(location, expected_project_hash)?;
    let asset = find_voxel_asset(project.document(), asset_id)?;
    require_asset_hash(asset, expected_asset_content_hash)?;
    let encoded = encode_voxel_asset(asset)
        .map_err(|error| reject("voxel.fileEncodeRejected", error.to_string()))?;
    write_host_file_atomic(target_path, encoded.as_bytes(), expected_target_sha256)
}
