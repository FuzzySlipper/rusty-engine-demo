use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use loading_bay_game::{
    admit_stored_project_with_document, decode_game_snapshot, decode_project_document,
    encode_game_snapshot, encode_project_document, materialize_stored_project_voxels, GameRuntime,
    ProjectSaveMode, ProjectStore, StoredVoxelEnvironment, VoxelEdit, VoxelEditTransaction,
    VoxelSourceRevision, GAME_SNAPSHOT_SCHEMA_VERSION,
};

const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const CONVERTED_PROJECT: &str =
    include_str!("../../../../content/projects/converted-wall.project.json");
const PILLAR: [i64; 3] = [2, 1, 6];
const CONVERTED_WALL: [[i64; 3]; 4] = [[4, 1, 6], [5, 1, 6], [4, 1, 7], [5, 1, 7]];

#[test]
fn edited_authority_reopens_from_snapshot_and_explicit_static_project_save() {
    let decoded = decode_project_document(PROJECT).unwrap();
    let (authored, admitted) =
        admit_stored_project_with_document(decoded.project).expect("admit source project");
    let mut runtime = GameRuntime::from_admitted_project(admitted);
    let before_count = runtime.collision_scene().unwrap().solid_voxel_count();
    runtime
        .apply_voxel_edits(VoxelEditTransaction {
            expected_revision: VoxelSourceRevision::INITIAL,
            edits: &[VoxelEdit::Clear { address: PILLAR }],
        })
        .expect("clear generated pillar voxel");

    let edited = runtime.collision_scene().unwrap();
    assert_eq!(edited.source_revision().raw(), 1);
    assert_eq!(edited.solid_voxel_count(), before_count - 1);
    assert!(edited.generated_room().is_none());
    let edited_hash = edited.authority_hash();
    let edited_navigation = edited.navigation_hash();
    let edited_mesh = edited.mesh_chunks().to_vec();
    let edited_voxels = edited.material_voxels().to_vec();

    let snapshot = encode_game_snapshot(&runtime).expect("encode edited runtime");
    let snapshot_json: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(snapshot_json["schemaVersion"], GAME_SNAPSHOT_SCHEMA_VERSION);
    assert_eq!(snapshot_json["voxelCollision"]["sourceRevision"], 1);
    assert_eq!(
        snapshot_json["voxelCollision"]["authorityHash"],
        edited_hash
    );
    assert_eq!(
        snapshot_json["voxelCollision"]["generatedRoom"],
        serde_json::Value::Null
    );
    assert_eq!(
        snapshot_json["voxelCollision"]["materialVoxels"]
            .as_array()
            .unwrap()
            .len(),
        before_count - 1
    );
    for forbidden in ["voxelEdit", "changedVoxels", "editHistory", "events"] {
        assert!(!snapshot.contains(forbidden), "snapshot leaked {forbidden}");
    }

    let reopened = decode_game_snapshot(&snapshot).expect("reopen edited runtime snapshot");
    let reopened_scene = reopened.collision_scene().unwrap();
    assert_eq!(reopened_scene.source_revision().raw(), 1);
    assert_eq!(reopened_scene.authority_hash(), edited_hash);
    assert_eq!(reopened_scene.material_voxels(), edited_voxels);
    assert_eq!(reopened_scene.navigation_hash(), edited_navigation);
    assert_eq!(reopened_scene.mesh_chunks(), edited_mesh);
    assert!(!reopened_scene.contains_point([4.5, 1.5, 6.5]));

    let saved = materialize_stored_project_voxels(&authored, edited)
        .expect("materialize edited static authority");
    let directory = TestDirectory::new();
    let project_path = directory.path().join("edited.project.json");
    let store = ProjectStore::default();
    store
        .save(&project_path, &saved, ProjectSaveMode::CreateNew)
        .expect("save edited authored project");
    let bytes = fs::read_to_string(&project_path).unwrap();
    for forbidden in [
        "sourceRevision",
        "authorityHash",
        "voxelEdit",
        "changedVoxels",
        "editHistory",
        "events",
    ] {
        assert!(!bytes.contains(forbidden), "project leaked {forbidden}");
    }

    let loaded = store
        .load(&project_path)
        .expect("load edited authored project");
    assert!(matches!(
        loaded.project.scenes[0].voxel_environment,
        Some(StoredVoxelEnvironment::Material(_))
    ));
    let (_, admitted) = admit_stored_project_with_document(loaded.project).unwrap();
    let project_runtime = GameRuntime::from_admitted_project(admitted);
    let project_scene = project_runtime.collision_scene().unwrap();
    assert_eq!(project_scene.source_revision().raw(), 0);
    assert_eq!(project_scene.authority_hash(), edited_hash);
    assert_eq!(project_scene.material_voxels(), edited_voxels);
    assert_eq!(project_scene.navigation_hash(), edited_navigation);
    assert_eq!(project_scene.mesh_chunks(), edited_mesh);
    assert!(!project_scene.contains_point([4.5, 1.5, 6.5]));
}

#[test]
fn converted_asset_edit_reopens_as_runtime_snapshot_and_static_authored_project() {
    let decoded = decode_project_document(CONVERTED_PROJECT).unwrap();
    let (authored, admitted) =
        admit_stored_project_with_document(decoded.project).expect("admit converted project");
    let mut runtime = GameRuntime::from_admitted_project(admitted);
    let before = runtime.collision_scene().unwrap();
    assert_eq!(before.source_revision(), VoxelSourceRevision::INITIAL);
    assert_eq!(before.solid_voxel_count(), 94);
    assert!(CONVERTED_WALL
        .iter()
        .all(|address| before.contains_point(address.map(|axis| axis as f64 + 0.5))));

    let edits = CONVERTED_WALL.map(|address| VoxelEdit::Clear { address });
    let receipt = runtime
        .apply_voxel_edits(VoxelEditTransaction {
            expected_revision: VoxelSourceRevision::INITIAL,
            edits: &edits,
        })
        .expect("clear converted wall");
    assert_eq!(receipt.fact.changed_voxels, 4);

    let edited = runtime.collision_scene().unwrap();
    assert_eq!(edited.source_revision().raw(), 1);
    assert_eq!(edited.solid_voxel_count(), 90);
    assert!(CONVERTED_WALL
        .iter()
        .all(|address| !edited.contains_point(address.map(|axis| axis as f64 + 0.5))));
    let edited_hash = edited.authority_hash();
    let edited_navigation = edited.navigation_hash();
    let edited_mesh = edited.mesh_chunks().to_vec();
    let edited_voxels = edited.material_voxels().to_vec();

    let snapshot = encode_game_snapshot(&runtime).expect("encode converted edit snapshot");
    let reopened = decode_game_snapshot(&snapshot).expect("reopen converted edit snapshot");
    let reopened_scene = reopened.collision_scene().unwrap();
    assert_eq!(reopened_scene.source_revision().raw(), 1);
    assert_eq!(reopened_scene.authority_hash(), edited_hash);
    assert_eq!(reopened_scene.material_voxels(), edited_voxels);
    assert_eq!(reopened_scene.navigation_hash(), edited_navigation);
    assert_eq!(reopened_scene.mesh_chunks(), edited_mesh);
    for forbidden in [
        "voxelEdit",
        "changedVoxels",
        "editHistory",
        "events",
        "replay",
    ] {
        assert!(!snapshot.contains(forbidden), "snapshot leaked {forbidden}");
    }

    let saved = materialize_stored_project_voxels(&authored, edited)
        .expect("materialize converted edited authority");
    let environment = saved.document().scenes[0]
        .voxel_environment
        .as_ref()
        .expect("materialized environment");
    let StoredVoxelEnvironment::Material(environment) = environment else {
        panic!("converted edit did not become static material voxels");
    };
    assert!(environment.voxel_assets.is_empty());
    assert_eq!(environment.material_voxels.len(), 90);

    let directory = TestDirectory::new();
    let project_path = directory.path().join("converted-edited.project.json");
    let store = ProjectStore::default();
    store
        .save(&project_path, &saved, ProjectSaveMode::CreateNew)
        .expect("save converted edited authored project");
    let bytes = fs::read_to_string(&project_path).unwrap();
    for forbidden in [
        "voxelAssets",
        "sourceRevision",
        "authorityHash",
        "voxelEdit",
        "changedVoxels",
        "editHistory",
        "events",
        "replay",
    ] {
        assert!(!bytes.contains(forbidden), "project leaked {forbidden}");
    }

    let loaded = store
        .load(&project_path)
        .expect("load converted edited project");
    let (_, admitted) = admit_stored_project_with_document(loaded.project).unwrap();
    let project_runtime = GameRuntime::from_admitted_project(admitted);
    let project_scene = project_runtime.collision_scene().unwrap();
    assert_eq!(
        project_scene.source_revision(),
        VoxelSourceRevision::INITIAL
    );
    assert_eq!(project_scene.authority_hash(), edited_hash);
    assert_eq!(project_scene.material_voxels(), edited_voxels);
    assert_eq!(project_scene.navigation_hash(), edited_navigation);
    assert_eq!(project_scene.mesh_chunks(), edited_mesh);
}

#[test]
fn material_voxel_bounds_are_identical_for_admission_save_and_reopen() {
    let mut boundary = decode_project_document(CONVERTED_PROJECT).unwrap().project;
    let Some(StoredVoxelEnvironment::Material(environment)) =
        boundary.scenes[0].voxel_environment.as_mut()
    else {
        panic!("converted project must use material voxels");
    };
    environment.material_voxels[0].address = [
        -rusty_engine::engine_spatial::MAX_VOXEL_COORDINATE_ABS,
        rusty_engine::engine_spatial::MAX_VOXEL_COORDINATE_ABS,
        0,
    ];
    environment.material_voxels[0].material_slot =
        rusty_engine::engine_spatial::MAX_VOXEL_MATERIAL_SLOT;
    let (boundary, _) =
        admit_stored_project_with_document(boundary).expect("inclusive bounds admit");

    let directory = TestDirectory::new();
    let project_path = directory.path().join("boundary.project.json");
    let store = ProjectStore::default();
    store
        .save(&project_path, &boundary, ProjectSaveMode::CreateNew)
        .expect("save bounded project");
    let loaded = store.load(&project_path).expect("load bounded project");
    admit_stored_project_with_document(loaded.project).expect("readmit bounded project");

    for (address, material_slot, expected_field) in [
        (
            [
                rusty_engine::engine_spatial::MAX_VOXEL_COORDINATE_ABS + 1,
                0,
                0,
            ],
            1,
            "address[0]",
        ),
        ([0, 0, 0], 0, "materialSlot"),
        (
            [0, 0, 0],
            rusty_engine::engine_spatial::MAX_VOXEL_MATERIAL_SLOT + 1,
            "materialSlot",
        ),
    ] {
        let mut invalid = decode_project_document(CONVERTED_PROJECT).unwrap().project;
        let Some(StoredVoxelEnvironment::Material(environment)) =
            invalid.scenes[0].voxel_environment.as_mut()
        else {
            unreachable!();
        };
        environment.material_voxels[0].address = address;
        environment.material_voxels[0].material_slot = material_slot;

        let canonical = encode_project_document(&invalid).unwrap();
        let invalid_path = directory.path().join(format!(
            "invalid-{}-{}.project.json",
            address[0], material_slot
        ));
        fs::write(&invalid_path, canonical).unwrap();
        let loaded = store
            .load(&invalid_path)
            .expect("shape-valid project loads");
        let Some(StoredVoxelEnvironment::Material(environment)) =
            loaded.project.scenes[0].voxel_environment.as_ref()
        else {
            unreachable!();
        };
        let voxel_index = environment
            .material_voxels
            .iter()
            .position(|voxel| voxel.address == address && voxel.material_slot == material_slot)
            .expect("invalid voxel retained in canonical document");
        let expected_path =
            format!("scenes[0].voxelEnvironment.materialVoxels[{voxel_index}].{expected_field}");
        let error = admit_stored_project_with_document(loaded.project).unwrap_err();
        assert_eq!(
            error.diagnostic().code,
            loading_bay_game::diagnostic_code::INVALID_SPATIAL
        );
        assert_eq!(error.diagnostic().path, expected_path);
    }
}

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "rusty-engine-voxel-edit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create test directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
