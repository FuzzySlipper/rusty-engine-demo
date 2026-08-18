use rusty_engine::content_store::ContentHash;
use rusty_engine::core_space::{Direction6, Face};
use rusty_engine::engine_spatial::{VoxelPickHint, VoxelPickService};
use rusty_engine::render_model::Transform;
use rusty_engine::voxel_annotation::{
    export_annotation_layer, query_annotation_layer, VoxelAnnotationQuery,
};
use rusty_engine::voxel_convert::{
    query_model_info, query_model_window, VoxelModelInfoRequest, VoxelModelWindowRequest,
};

use super::super::project::OpenedOwnerProject;
use super::super::protocol::{AdapterRejection, VoxelPickFace, VoxelPickReadout, VoxelReadout};
use super::super::ProjectLocation;
use super::model::{
    entity_transform, find_asset, find_scene, find_voxel_asset, local_to_authority_address, reject,
    require_asset_hash, scene_and_history,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_pick(
    location: &ProjectLocation,
    expected_project_hash: &str,
    scene_id: &str,
    instance_id: &str,
    origin: [f64; 3],
    direction: [f64; 3],
    max_distance: f64,
    claimed_voxel: [i64; 3],
    claimed_face: VoxelPickFace,
) -> Result<VoxelPickReadout, AdapterRejection> {
    let project = load_expected(location, expected_project_hash)?;
    let scene = find_scene(project.document(), scene_id)?;
    let instance = scene
        .voxel_instances
        .iter()
        .find(|instance| instance.instance_id == instance_id)
        .ok_or_else(|| {
            reject(
                "voxel.instanceMissing",
                format!("scene has no voxel instance `{instance_id}`"),
            )
        })?;
    let stored = find_asset(project.document(), &instance.voxel_asset_id)?;
    let asset = stored.voxel_volume.as_ref().ok_or_else(|| {
        reject(
            "voxel.wrongAssetKind",
            "instance target is not a voxel asset",
        )
    })?;
    let (voxel_scene, _) = scene_and_history(stored)?;
    let claimed_authority = local_to_authority_address(asset, claimed_voxel)?;
    let anchor = VoxelPickService::validate_instance(
        &voxel_scene,
        entity_transform(instance),
        VoxelPickHint {
            origin,
            direction,
            max_distance,
            claimed_voxel: claimed_authority,
            claimed_face: pick_face(claimed_face),
        },
    )
    .map_err(|error| reject("voxel.pickRejected", error.to_string()))?;
    let hit_voxel = authority_to_local(asset.grid.origin, anchor.local.hit_voxel)?;
    let place_voxel = authority_to_local(asset.grid.origin, anchor.local.place_voxel)?;
    let transform = entity_transform(instance);
    Ok(VoxelPickReadout {
        scene_id: scene_id.to_string(),
        instance_id: instance_id.to_string(),
        asset_id: instance.voxel_asset_id.clone(),
        hit_voxel,
        hit_face: readout_face(anchor.local.hit_face),
        place_voxel,
        authority_hit_voxel: anchor.local.hit_voxel,
        authority_place_voxel: anchor.local.place_voxel,
        instance_local_point: anchor.local.point,
        world_point: anchor.world_point,
        world_distance: anchor.world_distance,
        hit_preview_transform: preview_transform(
            transform,
            anchor.local.hit_voxel,
            asset.grid.cell_size,
        ),
        place_preview_transform: preview_transform(
            transform,
            anchor.local.place_voxel,
            asset.grid.cell_size,
        ),
    })
}

fn preview_transform(
    instance: rusty_engine::entity_state::EntityTransform,
    authority_voxel: [i64; 3],
    cell_size: f64,
) -> Transform {
    let local_center = rusty_engine::core_math::Vec3::new(
        ((authority_voxel[0] as f64 + 0.5) * cell_size) as f32,
        ((authority_voxel[1] as f64 + 0.5) * cell_size) as f32,
        ((authority_voxel[2] as f64 + 0.5) * cell_size) as f32,
    );
    let center = instance.transform_point(local_center);
    let cell_size = cell_size as f32;
    Transform {
        translation: [center.x, center.y, center.z],
        rotation: [
            instance.rotation.x,
            instance.rotation.y,
            instance.rotation.z,
            instance.rotation.w,
        ],
        scale: [
            instance.scale.x * cell_size,
            instance.scale.y * cell_size,
            instance.scale.z * cell_size,
        ],
    }
}

fn authority_to_local(origin: [i64; 3], authority: [i64; 3]) -> Result<[i64; 3], AdapterRejection> {
    let mut local = authority;
    for axis in 0..3 {
        local[axis] = authority[axis].checked_sub(origin[axis]).ok_or_else(|| {
            reject(
                "voxel.coordinateOverflow",
                "pick coordinate could not be mapped into asset-local space",
            )
        })?;
    }
    Ok(local)
}

pub(crate) fn query_model(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: &str,
    expected_asset_content_hash: &str,
    window: Option<VoxelModelWindowRequest>,
) -> Result<VoxelReadout, AdapterRejection> {
    let project = load_expected(location, expected_project_hash)?;
    let asset = find_voxel_asset(project.document(), asset_id)?;
    require_asset_hash(asset, expected_asset_content_hash)?;
    let info = query_model_info(
        asset,
        &VoxelModelInfoRequest {
            expected_content_hash: expected_asset_content_hash.to_string(),
            include_material_counts: true,
        },
    )
    .map_err(conversion_rejection)?;
    let window = window
        .map(|request| query_model_window(asset, &request).map_err(conversion_rejection))
        .transpose()?;
    Ok(VoxelReadout::Model {
        info,
        window: Box::new(window),
    })
}

pub(crate) fn query_annotation(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: &str,
    layer_id: &str,
    query: VoxelAnnotationQuery,
) -> Result<VoxelReadout, AdapterRejection> {
    let project = load_expected(location, expected_project_hash)?;
    let stored = find_asset(project.document(), asset_id)?;
    let layer = stored
        .voxel_annotations
        .iter()
        .find(|layer| layer.layer_id == layer_id)
        .ok_or_else(|| {
            reject(
                "voxel.annotationMissing",
                format!("asset has no annotation layer `{layer_id}`"),
            )
        })?;
    let readout = query_annotation_layer(layer, &query)
        .map_err(|error| reject("voxel.annotationQueryRejected", error.to_string()))?;
    Ok(VoxelReadout::AnnotationQuery {
        layer_hash: readout.layer_hash,
        total_layer_regions: readout.total_layer_regions,
        truncated: readout.truncated,
        matched_regions: readout.matched_regions,
    })
}

pub(crate) fn export_annotation(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: &str,
    layer_id: &str,
    expected_layer_hash: &str,
) -> Result<VoxelReadout, AdapterRejection> {
    let project = load_expected(location, expected_project_hash)?;
    let stored = find_asset(project.document(), asset_id)?;
    let layer = stored
        .voxel_annotations
        .iter()
        .find(|layer| layer.layer_id == layer_id)
        .ok_or_else(|| {
            reject(
                "voxel.annotationMissing",
                format!("asset has no annotation layer `{layer_id}`"),
            )
        })?;
    let exported = export_annotation_layer(layer, expected_layer_hash)
        .map_err(|error| reject("voxel.annotationExportRejected", error.to_string()))?;
    Ok(VoxelReadout::AnnotationExport {
        layer_id: layer_id.to_string(),
        canonical_json: exported.canonical_json,
        canonical_layer_hash: exported.canonical_layer_hash,
        membership_data_hash: exported.membership_data_hash,
    })
}

pub(crate) fn load_expected(
    location: &ProjectLocation,
    expected_project_hash: &str,
) -> Result<OpenedOwnerProject, AdapterRejection> {
    let expected = ContentHash::parse(expected_project_hash)
        .map_err(|error| reject("project.invalidHash", error.to_string()))?;
    let project = OpenedOwnerProject::load(location)?;
    if project.source_hash() != expected {
        return Err(reject(
            "project.staleHash",
            format!(
                "expected project hash {expected}, found {}",
                project.source_hash()
            ),
        ));
    }
    Ok(project)
}

pub(crate) fn conversion_rejection(
    error: rusty_engine::voxel_convert::ConversionError,
) -> AdapterRejection {
    let diagnostic = error
        .diagnostics()
        .first()
        .expect("conversion errors contain a diagnostic");
    AdapterRejection::new(diagnostic.code, diagnostic.message.clone())
        .at_path(diagnostic.path.clone())
}

pub(crate) const fn pick_face(face: VoxelPickFace) -> Face {
    match face {
        VoxelPickFace::NegativeX => Direction6::NegX,
        VoxelPickFace::PositiveX => Direction6::PosX,
        VoxelPickFace::NegativeY => Direction6::NegY,
        VoxelPickFace::PositiveY => Direction6::PosY,
        VoxelPickFace::NegativeZ => Direction6::NegZ,
        VoxelPickFace::PositiveZ => Direction6::PosZ,
    }
}

pub(crate) const fn readout_face(face: Face) -> VoxelPickFace {
    match face {
        Direction6::NegX => VoxelPickFace::NegativeX,
        Direction6::PosX => VoxelPickFace::PositiveX,
        Direction6::NegY => VoxelPickFace::NegativeY,
        Direction6::PosY => VoxelPickFace::PositiveY,
        Direction6::NegZ => VoxelPickFace::NegativeZ,
        Direction6::PosZ => VoxelPickFace::PositiveZ,
    }
}
