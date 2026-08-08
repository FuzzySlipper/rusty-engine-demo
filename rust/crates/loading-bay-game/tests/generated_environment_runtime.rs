use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, GameRuntime, GameSnapshotError,
};
use rusty_engine::core_ids::EntityId;
use serde_json::{json, Value};

const PROJECT: &str = include_str!("../../../../content/generated/encounter-gate.project.json");
const NAVIGATOR: EntityId = EntityId::new(4);
const DELTA_SECONDS: f32 = 1.0 / 60.0;

#[test]
fn admitted_generation_config_builds_one_material_voxel_authority_and_mesh() {
    let runtime = GameRuntime::from_project_content(PROJECT).unwrap();
    let scene = runtime.collision_scene().unwrap();
    let (config, record) = scene.generated_room().expect("generated room provenance");

    assert_eq!(config.seed, 4);
    assert_eq!(record.pillar_voxel, [4, 1, 6]);
    assert_eq!(record.solid_voxel_count, scene.solid_voxel_count());
    assert_eq!(scene.mesh_chunks().len(), scene.resident_chunk_count());
    assert!(scene.mesh_chunks()[0]
        .groups
        .iter()
        .any(|group| group.material_slot == 3));
    assert!(scene.contains_point([4.5, 1.5, 6.5]));
}

#[test]
fn seed_variation_changes_generated_geometry_without_changing_entity_content() {
    let baseline = GameRuntime::from_project_content(PROJECT).unwrap();
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    project["generatedVoxelEnvironment"]["seed"] = json!(9);
    let variation = GameRuntime::from_project_content(&project.to_string()).unwrap();

    let baseline_scene = baseline.collision_scene().unwrap();
    let variation_scene = variation.collision_scene().unwrap();
    assert_ne!(
        baseline_scene.generated_room().unwrap().1.pillar_voxel,
        variation_scene.generated_room().unwrap().1.pillar_voxel,
    );
    assert_ne!(
        baseline_scene.mesh_chunks()[0].content_hash,
        variation_scene.mesh_chunks()[0].content_hash,
    );
    assert_eq!(
        baseline.readout().projection,
        variation.readout().projection
    );
}

#[test]
fn snapshot_reopen_regenerates_voxels_and_all_derived_consumers_identically() {
    let mut uninterrupted = GameRuntime::from_project_content(PROJECT).unwrap();
    for _ in 0..30 {
        uninterrupted.run_navigation_phase(DELTA_SECONDS).unwrap();
    }
    let encoded = encode_game_snapshot(&uninterrupted).unwrap();
    let encoded_value: Value = serde_json::from_str(&encoded).unwrap();

    assert_eq!(encoded_value["voxelCollision"]["materialVoxels"], json!([]));
    assert_eq!(encoded_value["voxelCollision"]["sourceRevision"], json!(0));
    assert_eq!(
        encoded_value["voxelCollision"]["generatedRoom"]["seed"],
        json!(4),
    );
    let mut reopened = decode_game_snapshot(&encoded).unwrap();
    let left = uninterrupted.collision_scene().unwrap();
    let right = reopened.collision_scene().unwrap();
    assert_eq!(left.generated_room(), right.generated_room());
    assert_eq!(left.material_voxels(), right.material_voxels());
    assert_eq!(left.mesh_chunks(), right.mesh_chunks());
    assert_eq!(left.navigation_hash(), right.navigation_hash());

    for _ in 0..210 {
        uninterrupted.run_navigation_phase(DELTA_SECONDS).unwrap();
        reopened.run_navigation_phase(DELTA_SECONDS).unwrap();
    }
    assert_eq!(
        uninterrupted.session().navigation(NAVIGATOR),
        reopened.session().navigation(NAVIGATOR),
    );
}

#[test]
fn snapshot_rejects_a_generated_output_hash_that_cannot_be_reproduced() {
    let runtime = GameRuntime::from_project_content(PROJECT).unwrap();
    let mut snapshot: Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    snapshot["voxelCollision"]["generatedRoom"]["outputHash"] = json!(0);

    let error = decode_game_snapshot(&snapshot.to_string()).unwrap_err();

    assert!(matches!(
        error,
        GameSnapshotError::GeneratedRoomHashMismatch { .. }
    ));
}
