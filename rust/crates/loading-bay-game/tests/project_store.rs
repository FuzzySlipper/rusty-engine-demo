use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use loading_bay_game::{
    admit_stored_project_with_document, decode_project_document, encode_project_document,
    AdmittedStoredProject, ProjectSaveMode, ProjectStore, ProjectStoreError,
    STORED_PROJECT_SCHEMA_VERSION,
};
use rusty_engine::content_store::ContentHash;

const CURRENT_PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const LEGACY_PROJECT: &str =
    include_str!("../../../../content/generated/encounter-gate.project.json");

#[test]
fn current_project_save_load_and_reencode_are_byte_stable() {
    let directory = TestDirectory::new();
    let first_path = directory.path().join("first.project.json");
    let second_path = directory.path().join("second.project.json");
    let store = ProjectStore::default();
    let admitted = admitted(CURRENT_PROJECT);

    store
        .save(&first_path, &admitted, ProjectSaveMode::CreateNew)
        .unwrap();
    let first_bytes = fs::read_to_string(&first_path).unwrap();
    let loaded = store.load(&first_path).unwrap();
    let (readmitted, _) = admit_stored_project_with_document(loaded.project).unwrap();
    store
        .save(&second_path, &readmitted, ProjectSaveMode::CreateNew)
        .unwrap();

    assert_eq!(fs::read_to_string(second_path).unwrap(), first_bytes);
    assert!(first_bytes.ends_with('\n'));
}

#[test]
fn repeated_real_migration_emits_identical_current_artifacts() {
    let directory = TestDirectory::new();
    let legacy_path = directory.path().join("legacy.project.json");
    let first_path = directory.path().join("first.project.json");
    let second_path = directory.path().join("second.project.json");
    fs::write(&legacy_path, LEGACY_PROJECT).unwrap();
    let store = ProjectStore::default();

    let first = store.load(&legacy_path).unwrap();
    let second = store.load(&legacy_path).unwrap();
    assert_eq!(first.source_schema_version, 6);
    assert_eq!(first, second);
    let (first, _) = admit_stored_project_with_document(first.project).unwrap();
    let (second, _) = admit_stored_project_with_document(second.project).unwrap();
    store
        .save(&first_path, &first, ProjectSaveMode::CreateNew)
        .unwrap();
    store
        .save(&second_path, &second, ProjectSaveMode::CreateNew)
        .unwrap();

    let first_bytes = fs::read_to_string(first_path).unwrap();
    assert_eq!(fs::read_to_string(second_path).unwrap(), first_bytes);
    assert_eq!(
        decode_project_document(&first_bytes)
            .unwrap()
            .source_schema_version,
        STORED_PROJECT_SCHEMA_VERSION
    );
}

#[test]
fn create_new_and_replace_existing_are_explicit() {
    let directory = TestDirectory::new();
    let target = directory.path().join("project.json");
    let store = ProjectStore::default();
    let original = admitted(CURRENT_PROJECT);
    store
        .save(&target, &original, ProjectSaveMode::CreateNew)
        .unwrap();

    let error = store
        .save(&target, &original, ProjectSaveMode::CreateNew)
        .unwrap_err();
    assert!(matches!(error, ProjectStoreError::TargetExists { .. }));

    let missing = directory.path().join("missing.project.json");
    let error = store
        .save(&missing, &original, ProjectSaveMode::ReplaceExisting)
        .unwrap_err();
    assert!(matches!(error, ProjectStoreError::TargetMissing { .. }));

    let mut changed = original.document().clone();
    changed.name = "Changed Loading Bay".to_string();
    let (changed, _) = admit_stored_project_with_document(changed).unwrap();
    store
        .save(&target, &changed, ProjectSaveMode::ReplaceExisting)
        .unwrap();
    assert_eq!(
        store.load(&target).unwrap().project.name,
        "Changed Loading Bay"
    );
}

#[test]
fn loaded_project_uses_the_existing_semantic_admission_diagnostics() {
    let directory = TestDirectory::new();
    let target = directory.path().join("invalid.project.json");
    let mut invalid = decode_project_document(CURRENT_PROJECT).unwrap().project;
    invalid.scenes[0].entities[5]
        .switch
        .as_mut()
        .unwrap()
        .controls = vec![999];
    fs::write(&target, encode_project_document(&invalid).unwrap()).unwrap();

    let loaded = ProjectStore::default().load(&target).unwrap();
    let error = admit_stored_project_with_document(loaded.project).unwrap_err();
    assert_eq!(
        error.diagnostic().code,
        loading_bay_game::diagnostic_code::INVALID_RELATIONSHIP
    );
    assert_eq!(
        error.diagnostic().path,
        "scenes[0].entities[5].switch.controls"
    );
}

#[test]
fn invalid_non_entry_scene_cannot_mint_the_project_save_token() {
    let directory = TestDirectory::new();
    let target = directory.path().join("must-not-exist.project.json");
    let mut invalid = decode_project_document(CURRENT_PROJECT).unwrap().project;
    let mut second_scene = invalid.scenes[0].clone();
    second_scene.id = "scene/storage-wing".to_string();
    second_scene.name = "Storage Wing".to_string();
    second_scene.entities[5].switch.as_mut().unwrap().controls = vec![999];
    invalid.scenes.push(second_scene);

    let error = admit_stored_project_with_document(invalid).unwrap_err();

    assert_eq!(
        error.diagnostic().code,
        loading_bay_game::diagnostic_code::INVALID_RELATIONSHIP
    );
    assert_eq!(
        error.diagnostic().path,
        "scenes[1].entities[5].switch.controls"
    );
    assert!(!target.exists(), "invalid project acquired save authority");
}

#[test]
fn bounded_and_corrupt_inputs_fail_before_admission() {
    let directory = TestDirectory::new();
    let target = directory.path().join("project.json");
    fs::write(&target, CURRENT_PROJECT).unwrap();

    let error = ProjectStore::with_max_bytes(64).load(&target).unwrap_err();
    assert!(matches!(error, ProjectStoreError::TooLarge { .. }));

    fs::write(&target, "{\"schemaVersion\":7,\"broken\":").unwrap();
    let error = ProjectStore::default().load(&target).unwrap_err();
    assert!(matches!(error, ProjectStoreError::Codec(_)));
}

#[test]
fn failed_pending_write_preserves_known_good_readback() {
    let directory = TestDirectory::new();
    let target = directory.path().join("project.json");
    let store = ProjectStore::default();
    let original = admitted(CURRENT_PROJECT);
    store
        .save(&target, &original, ProjectSaveMode::CreateNew)
        .unwrap();
    let known_good = fs::read_to_string(&target).unwrap();

    let pending = ProjectStore::pending_path(&target).unwrap();
    fs::create_dir(&pending).unwrap();
    let error = store
        .save(&target, &original, ProjectSaveMode::ReplaceExisting)
        .unwrap_err();
    assert!(matches!(error, ProjectStoreError::PendingConflict { .. }));
    assert_eq!(fs::read_to_string(&target).unwrap(), known_good);
    assert_eq!(
        encode_project_document(&store.load(&target).unwrap().project).unwrap(),
        known_good
    );
}

#[test]
fn optimistic_replace_is_hash_guarded_and_atomically_rereadable() {
    let directory = TestDirectory::new();
    let target = directory.path().join("project.json");
    let store = ProjectStore::default();
    let original = admitted(CURRENT_PROJECT);
    store
        .save(&target, &original, ProjectSaveMode::CreateNew)
        .unwrap();

    let opened = store.load_source(&target).unwrap();
    assert_eq!(
        opened.source_hash(),
        ContentHash::of(opened.source_bytes().as_bytes())
    );
    let mut changed = original.document().clone();
    changed.name = "CAS Loading Bay".to_string();
    let (changed, _) = admit_stored_project_with_document(changed).unwrap();

    let installed_hash = store
        .replace_if_unchanged(&target, &changed, opened.source_hash())
        .unwrap();
    let reread = store.load_source(&target).unwrap();
    assert_eq!(reread.source_hash(), installed_hash);
    assert_eq!(reread.decoded.project.name, "CAS Loading Bay");
    assert!(!ProjectStore::pending_path(&target).unwrap().exists());
}

#[test]
fn stale_optimistic_replace_preserves_external_bytes_and_candidate_is_not_installed() {
    let directory = TestDirectory::new();
    let target = directory.path().join("project.json");
    let store = ProjectStore::default();
    let original = admitted(CURRENT_PROJECT);
    store
        .save(&target, &original, ProjectSaveMode::CreateNew)
        .unwrap();
    let opened = store.load_source(&target).unwrap();

    let mut external = original.document().clone();
    external.name = "External Change".to_string();
    let (external, _) = admit_stored_project_with_document(external).unwrap();
    store
        .save(&target, &external, ProjectSaveMode::ReplaceExisting)
        .unwrap();
    let external_bytes = fs::read(&target).unwrap();

    let mut candidate = original.document().clone();
    candidate.name = "Stale Candidate".to_string();
    let (candidate, _) = admit_stored_project_with_document(candidate).unwrap();
    let error = store
        .replace_if_unchanged(&target, &candidate, opened.source_hash())
        .unwrap_err();

    assert!(matches!(error, ProjectStoreError::StaleSource { .. }));
    assert_eq!(fs::read(&target).unwrap(), external_bytes);
    assert!(!ProjectStore::pending_path(&target).unwrap().exists());
}

#[test]
fn complete_pending_file_recovers_when_target_is_absent() {
    let directory = TestDirectory::new();
    let target = directory.path().join("project.json");
    let store = ProjectStore::default();
    let project = admitted(CURRENT_PROJECT);
    let canonical = encode_project_document(project.document()).unwrap();
    let pending = ProjectStore::pending_path(&target).unwrap();
    fs::write(&pending, &canonical).unwrap();

    let loaded = store.load(&target).unwrap();
    assert_eq!(loaded.project, project.document().clone());
    assert_eq!(fs::read_to_string(&target).unwrap(), canonical);
    assert!(!pending.exists());
}

#[test]
fn noncanonical_pending_file_is_not_promoted() {
    let directory = TestDirectory::new();
    let target = directory.path().join("project.json");
    let pending = ProjectStore::pending_path(&target).unwrap();
    fs::write(&pending, CURRENT_PROJECT.trim_end()).unwrap();

    let error = ProjectStore::default().load(&target).unwrap_err();
    assert!(matches!(
        error,
        ProjectStoreError::NonCanonicalPending { .. }
    ));
    assert!(!target.exists());
    assert!(pending.exists());
}

fn admitted(input: &str) -> AdmittedStoredProject {
    let decoded = decode_project_document(input).unwrap();
    admit_stored_project_with_document(decoded.project)
        .unwrap()
        .0
}

#[test]
fn doom_admission_rejects_incomplete_palette_set_when_unused_binding_removed() {
    // R6678-3: the Doom closure contract requires the complete 54-material
    // manifest set at the authoritative admission boundary. Removing an unused
    // binding (FLAT23 has no sparse runs) must reject before publication even
    // though no persisted run references it.
    let doom_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../content/projects/doom-e1m1.project.json");
    let raw = fs::read_to_string(&doom_path).unwrap();
    let mut document = decode_project_document(&raw).unwrap().project;
    assert_eq!(document.project_id, "doom-e1m1");

    // Remove the unused material asset, its texture asset, its palette entry,
    // and its materialMap entry so the document is internally consistent
    // (runs never reference FLAT23) but the declared manifest set is
    // incomplete: the manifest still declares 54 materials.
    const UNUSED_MATERIAL: &str = "material/doom-flat-flat23";
    const UNUSED_TEXTURE: &str = "texture/doom-flat-flat23";
    const UNUSED_VOXEL_SLOT: u16 = 7; // materialSlot of FLAT23 in the voxel palette
    document
        .assets
        .retain(|asset| asset.id != UNUSED_MATERIAL && asset.id != UNUSED_TEXTURE);
    for asset in &mut document.assets {
        if let Some(voxel) = asset.voxel_volume.as_mut() {
            voxel
                .material_palette
                .retain(|binding| binding.material_asset_id != UNUSED_MATERIAL);
            voxel
                .material_map
                .retain(|mapping| mapping.voxel_material_slot != UNUSED_VOXEL_SLOT);
            // Keep the embedded voxel artifact internally valid (runs never
            // reference slot 7) so the only defect left is the incomplete
            // declared palette set. The voxel asset's own hashes must be
            // recomputed over the canonical mutated content, otherwise the
            // generic voxel validation rejects the stale hash first.
            let recomputed = rusty_engine::voxel_asset::with_computed_content_hash(voxel.clone())
                .expect("mutated voxel asset stays canonical");
            *voxel = recomputed;
        }
    }

    let error = admit_stored_project_with_document(document).unwrap_err();
    assert_eq!(
        error.diagnostic().code,
        loading_bay_game::diagnostic_code::MISSING_ASSET
    );
    assert!(
        error
            .diagnostic()
            .message
            .contains("missing from the voxel palette"),
        "expected the complete-manifest-set diagnostic, got {}",
        error.diagnostic().message
    );
}

#[test]
fn doom_admission_rejects_duplicate_palette_identity_under_unique_slot() {
    // R6678-4: set equality alone is insufficient because a duplicate material
    // identity disappears when palette ids are collected into a set. Preserve
    // the complete declared set, add a 55th binding under a valid unique slot,
    // and prove the authoritative admission boundary rejects it.
    let doom_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../content/projects/doom-e1m1.project.json");
    let raw = fs::read_to_string(&doom_path).unwrap();
    let mut document = decode_project_document(&raw).unwrap().project;
    assert_eq!(document.project_id, "doom-e1m1");

    for asset in &mut document.assets {
        if let Some(voxel) = asset.voxel_volume.as_mut() {
            let mut duplicate = voxel
                .material_palette
                .iter()
                .find(|binding| binding.material_asset_id == "material/doom-flat-ceil3-5")
                .expect("declared Doom material binding")
                .clone();
            duplicate.material_slot = 4095;
            voxel.material_palette.push(duplicate);
            let recomputed = rusty_engine::voxel_asset::with_computed_content_hash(voxel.clone())
                .expect("mutated voxel asset stays canonical");
            *voxel = recomputed;
        }
    }

    let error = admit_stored_project_with_document(document).unwrap_err();
    assert_eq!(
        error.diagnostic().code,
        loading_bay_game::diagnostic_code::INVALID_MATERIAL
    );
    assert!(
        error
            .diagnostic()
            .message
            .contains("55 bindings for 54 unique identities"),
        "expected the duplicate-identity diagnostic, got {}",
        error.diagnostic().message
    );
}

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rusty-engine-project-store-{}-{id}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
