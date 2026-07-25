use loading_bay_game::{
    decode_project_document, diagnostic_code, encode_project_document, GameRuntime, RuntimeError,
    VoxelEdit, VoxelEditTransaction, VoxelProjectionRevisions,
};
use voxel_asset::{decode_voxel_asset, VoxelAsset};

const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const ASSET: &str = include_str!("../../../../content/assets/kenney-wall-a.voxel.json");

#[test]
fn converted_artifact_uses_normal_project_admission_and_matches_authored_voxels() {
    let asset = decode_voxel_asset(ASSET).unwrap();
    let converted_project = project_with_asset(&asset);
    let mut converted = GameRuntime::from_stored_project(&converted_project).unwrap();
    let authored = GameRuntime::from_stored_project(&project_with_authored_voxels(&asset)).unwrap();

    let converted_scene = converted.collision_scene().unwrap();
    let authored_scene = authored.collision_scene().unwrap();
    assert_eq!(
        converted_scene.material_voxels(),
        authored_scene.material_voxels()
    );
    assert_eq!(
        converted_scene.authority_hash(),
        authored_scene.authority_hash()
    );
    assert_eq!(
        converted_scene.navigation_hash(),
        authored_scene.navigation_hash()
    );
    assert_eq!(converted_scene.mesh_chunks(), authored_scene.mesh_chunks());
    assert!(converted_scene.contains_point([4.5, 0.5, 6.5]));

    let edit = [VoxelEdit::Clear { address: [4, 0, 6] }];
    let receipt = converted
        .apply_voxel_edits(VoxelEditTransaction {
            expected_revision: converted_scene.source_revision(),
            edits: &edit,
        })
        .unwrap();
    assert_eq!(
        receipt.projections,
        VoxelProjectionRevisions::coherent(receipt.accepted_revision)
    );
    assert!(!converted
        .collision_scene()
        .unwrap()
        .contains_point([4.5, 0.5, 6.5]));

    let decoded = decode_project_document(&converted_project).unwrap();
    let first = encode_project_document(&decoded.project).unwrap();
    let second =
        encode_project_document(&decode_project_document(&first).unwrap().project).unwrap();
    assert_eq!(first, second);
}

#[test]
fn malformed_or_missing_voxel_assets_report_project_source_paths() {
    let asset = decode_voxel_asset(ASSET).unwrap();
    let mut malformed: serde_json::Value =
        serde_json::from_str(&project_with_asset(&asset)).unwrap();
    let asset_index = malformed["assets"]
        .as_array()
        .unwrap()
        .iter()
        .position(|candidate| candidate["id"] == asset.asset_id)
        .expect("converted voxel asset");
    malformed["assets"][asset_index]["voxelVolume"]["contentHash"] =
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into();
    let diagnostic = stored_diagnostic(&serde_json::to_string(&malformed).unwrap());
    assert_eq!(diagnostic.code, diagnostic_code::INVALID_VOXEL_ASSET);
    assert_eq!(
        diagnostic.path,
        format!("assets[{asset_index}].voxelVolume.contentHash")
    );

    let mut missing: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    missing["scenes"][0]["voxelEnvironment"] = serde_json::json!({
        "kind": "material",
        "voxelSize": 1.0,
        "chunkSize": 16,
        "materialVoxels": [],
        "voxelAssets": ["voxel-volume/not-declared"]
    });
    let diagnostic = stored_diagnostic(&serde_json::to_string(&missing).unwrap());
    assert_eq!(diagnostic.code, diagnostic_code::MISSING_ASSET);
    assert_eq!(diagnostic.path, "scenes[0].voxelEnvironment.voxelAssets[0]");
}

fn project_with_asset(asset: &VoxelAsset) -> String {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    for binding in &asset.material_palette {
        project["assets"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id": binding.material_asset_id,
                "material": test_material_definition()
            }));
    }
    project["assets"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": asset.asset_id,
            "voxelVolume": asset
        }));
    project["scenes"][0]["voxelEnvironment"] = serde_json::json!({
        "kind": "material",
        "voxelSize": asset.grid.cell_size,
        "chunkSize": asset.grid.chunk_size,
        "materialVoxels": [],
        "voxelAssets": [asset.asset_id]
    });
    serde_json::to_string(&project).unwrap()
}

fn test_material_definition() -> serde_json::Value {
    serde_json::json!({
        "authority": {
            "solid": true,
            "collidable": true,
            "occludes": true,
            "structuralClass": "structural"
        },
        "style": {
            "color": [0.5, 0.5, 0.55, 1.0],
            "texture": null,
            "textureTint": [1.0, 1.0, 1.0, 1.0],
            "emissionColor": [0.5, 0.5, 0.55, 1.0],
            "roughness": 1.0,
            "emissive": 0.0,
            "uvStrategy": "flat"
        }
    })
}

fn project_with_authored_voxels(asset: &VoxelAsset) -> String {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    let material_voxels = asset
        .representation
        .sparse_runs
        .iter()
        .flat_map(|run| {
            (0..run.length).map(|offset| {
                serde_json::json!({
                    "address": [
                        asset.grid.origin[0] + run.start[0] + i64::from(offset),
                        asset.grid.origin[1] + run.start[1],
                        asset.grid.origin[2] + run.start[2]
                    ],
                    "materialSlot": run.material_slot
                })
            })
        })
        .collect::<Vec<_>>();
    project["scenes"][0]["voxelEnvironment"] = serde_json::json!({
        "kind": "material",
        "voxelSize": asset.grid.cell_size,
        "chunkSize": asset.grid.chunk_size,
        "materialVoxels": material_voxels
    });
    serde_json::to_string(&project).unwrap()
}

fn stored_diagnostic(input: &str) -> loading_bay_game::ProjectDiagnostic {
    match GameRuntime::from_stored_project(input).unwrap_err() {
        RuntimeError::StoredProject(error) => error.diagnostic().clone(),
        error => panic!("unexpected runtime error: {error:?}"),
    }
}
