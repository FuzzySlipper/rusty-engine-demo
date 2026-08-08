use rusty_engine::engine_spatial::{
    PreparedVoxelHistoryRevert, VoxelEditHistoryDiffOptions, VoxelEditHistoryRevertReceipt,
};

use super::super::project::publish_project_mutation;
use super::super::protocol::{
    AdapterRejection, ProjectMutationReceipt, StudioProjectReadout, VoxelHistoryBoundsReadout,
    VoxelHistoryEntryReadout, VoxelHistoryMaterialDeltaReadout, VoxelHistoryRevertPreview,
    VoxelReadout,
};
use super::super::ProjectLocation;
use super::model::{
    find_asset, find_voxel_asset_mut, install_scene_and_history, reject, require_asset_hash,
    scene_and_history,
};
use super::query::load_expected;

const MAX_HISTORY_QUERY_ENTRIES: usize = 512;
const MAX_HISTORY_QUERY_DELTAS_PER_ENTRY: usize = 4_096;
const MAX_HISTORY_PREVIEW_SAMPLES: usize = 4_096;

pub(crate) struct PreparedProjectHistoryRevert {
    asset_id: String,
    expected_project_hash: String,
    expected_asset_content_hash: String,
    prepared: PreparedVoxelHistoryRevert,
}

pub(crate) fn query_history(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: &str,
    expected_asset_content_hash: &str,
    max_entries: usize,
    max_deltas_per_entry: usize,
) -> Result<VoxelReadout, AdapterRejection> {
    if !(1..=MAX_HISTORY_QUERY_ENTRIES).contains(&max_entries) {
        return Err(reject(
            "voxel.historyQueryLimit",
            format!("maxEntries must be between 1 and {MAX_HISTORY_QUERY_ENTRIES}"),
        ));
    }
    if !(1..=MAX_HISTORY_QUERY_DELTAS_PER_ENTRY).contains(&max_deltas_per_entry) {
        return Err(reject(
            "voxel.historyQueryLimit",
            format!("maxDeltasPerEntry must be between 1 and {MAX_HISTORY_QUERY_DELTAS_PER_ENTRY}"),
        ));
    }
    let project = load_expected(location, expected_project_hash)?;
    let stored = find_asset(project.document(), asset_id)?;
    let voxel = stored
        .voxel_volume
        .as_ref()
        .ok_or_else(|| reject("voxel.wrongAssetKind", "asset has no voxel payload"))?;
    require_asset_hash(voxel, expected_asset_content_hash)?;
    let (_, history) = scene_and_history(stored)?;
    let entry_count = history.entries().len();
    let first = entry_count.saturating_sub(max_entries);
    let entries = history.entries()[first..]
        .iter()
        .map(|entry| VoxelHistoryEntryReadout {
            transaction_id: entry.transaction_id,
            parent_transaction_id: entry.parent_transaction_id,
            before_hash: format!("{:016x}", entry.before_hash),
            after_hash: format!("{:016x}", entry.after_hash),
            changed_voxels: entry.deltas.len(),
            deltas_truncated: entry.deltas.len() > max_deltas_per_entry,
            deltas: entry
                .deltas
                .iter()
                .take(max_deltas_per_entry)
                .cloned()
                .collect(),
        })
        .collect();
    let cursor = history.cursor();
    Ok(VoxelReadout::History {
        asset_id: asset_id.to_string(),
        cursor: cursor.index,
        undo_depth: cursor.undo_depth,
        redo_depth: cursor.redo_depth,
        entry_count,
        entries_truncated: first > 0,
        entries,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_history_revert(
    location: &ProjectLocation,
    preview_id: String,
    expected_project_hash: String,
    asset_id: String,
    expected_asset_content_hash: String,
    target_cursor: usize,
    max_samples: usize,
) -> Result<(PreparedProjectHistoryRevert, VoxelHistoryRevertPreview), AdapterRejection> {
    if max_samples > MAX_HISTORY_PREVIEW_SAMPLES {
        return Err(reject(
            "voxel.historyPreviewLimit",
            format!("maxSamples exceeds {MAX_HISTORY_PREVIEW_SAMPLES}"),
        ));
    }
    let project = load_expected(location, &expected_project_hash)?;
    let stored = find_asset(project.document(), &asset_id)?;
    let voxel = stored
        .voxel_volume
        .as_ref()
        .ok_or_else(|| reject("voxel.wrongAssetKind", "asset has no voxel payload"))?;
    require_asset_hash(voxel, &expected_asset_content_hash)?;
    let (scene, history) = scene_and_history(stored)?;
    let prepared = history
        .preview_revert_to_cursor(
            &scene,
            target_cursor,
            VoxelEditHistoryDiffOptions { max_samples },
        )
        .map_err(|error| reject("voxel.historyRejected", error.to_string()))?;
    let preview = history_preview(
        preview_id,
        asset_id.clone(),
        expected_project_hash.clone(),
        expected_asset_content_hash.clone(),
        prepared.receipt(),
    );
    Ok((
        PreparedProjectHistoryRevert {
            asset_id,
            expected_project_hash,
            expected_asset_content_hash,
            prepared,
        },
        preview,
    ))
}

pub(crate) fn apply_prepared_history_revert(
    location: &ProjectLocation,
    expected_project_hash: &str,
    prepared: PreparedProjectHistoryRevert,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    if expected_project_hash != prepared.expected_project_hash {
        return Err(reject(
            "voxel.historyPreviewStale",
            "apply project hash does not match the prepared history preview",
        ));
    }
    let published =
        publish_project_mutation(location, expected_project_hash, move |_, project| {
            let stored = find_voxel_asset_mut(project, &prepared.asset_id)?;
            let voxel = stored
                .voxel_volume
                .as_ref()
                .expect("voxel asset helper checked payload");
            require_asset_hash(voxel, &prepared.expected_asset_content_hash)?;
            let content_hash_before = voxel.content_hash.clone();
            let (mut scene, mut history) = scene_and_history(stored)?;
            let receipt = history
                .commit_revert(&mut scene, prepared.prepared)
                .map_err(|error| reject("voxel.historyPreviewStale", error.to_string()))?;
            install_scene_and_history(stored, &scene, &history)?;
            let content_hash_after = stored
                .voxel_volume
                .as_ref()
                .expect("installed voxel asset")
                .content_hash
                .clone();
            Ok(ProjectMutationReceipt::VoxelHistoryMoved {
                asset_id: prepared.asset_id,
                content_hash_before,
                content_hash_after,
                cursor_before: receipt.cursor_before.index,
                cursor_after: receipt.cursor_after.index,
                undo_depth: receipt.cursor_after.undo_depth,
                redo_depth: receipt.cursor_after.redo_depth,
                changed_voxels: receipt.diff.changed_voxels,
            })
        })?;
    Ok((published.value, published.readout))
}

fn history_preview(
    preview_id: String,
    asset_id: String,
    expected_project_hash: String,
    expected_asset_content_hash: String,
    receipt: &VoxelEditHistoryRevertReceipt,
) -> VoxelHistoryRevertPreview {
    VoxelHistoryRevertPreview {
        preview_id,
        asset_id,
        expected_project_hash,
        expected_asset_content_hash,
        cursor_before: receipt.cursor_before.index,
        cursor_after: receipt.cursor_after.index,
        undo_depth_after: receipt.cursor_after.undo_depth,
        redo_depth_after: receipt.cursor_after.redo_depth,
        revision_before: receipt.revision_before.raw(),
        revision_after: receipt.revision_after.raw(),
        changed_voxels: receipt.diff.changed_voxels,
        bounds: receipt.diff.bounds.map(|bounds| VoxelHistoryBoundsReadout {
            min: bounds.min,
            max: bounds.max,
        }),
        material_deltas: receipt
            .diff
            .material_deltas
            .iter()
            .map(|delta| VoxelHistoryMaterialDeltaReadout {
                before_material: delta.before_material,
                after_material: delta.after_material,
                changed_voxels: delta.changed_voxels,
            })
            .collect(),
        samples: receipt.diff.samples.clone(),
        samples_truncated: receipt.diff.samples_truncated,
        included_transaction_ids: receipt.diff.included_transaction_ids.clone(),
    }
}
