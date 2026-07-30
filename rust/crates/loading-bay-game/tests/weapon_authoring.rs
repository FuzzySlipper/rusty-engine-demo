use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use loading_bay_game::{
    decode_loading_bay_weapon_authoring_request, encode_loading_bay_weapon_authoring_response,
    LoadingBayWeaponAuthoringCandidate, LoadingBayWeaponAuthoringRejectionCode,
    LoadingBayWeaponAuthoringRequest, LoadingBayWeaponAuthoringResponse,
    LoadingBayWeaponAuthoringService, ProjectLocation, ProjectStore,
    LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID, LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION,
    LOADING_BAY_WEAPON_COMPONENT_TYPE_ID, MAX_ITEM_DEFINITION_ID_BYTES,
    MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES,
};
use serde_json::json;

const CURRENT_PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const PROJECT_FILE: &str = "content/projects/loading-bay.project.json";
const ARC_PISTOL_ENTITY: u64 = 113;
const READ_FIXTURE: &str =
    include_str!("../../../../contracts/loading-bay-weapon-authoring-v1/read-request.json");
const REPLACE_FIXTURE: &str =
    include_str!("../../../../contracts/loading-bay-weapon-authoring-v1/replace-request.json");
const READ_RESPONSE_FIXTURE: &str =
    include_str!("../../../../contracts/loading-bay-weapon-authoring-v1/read-response.json");

#[test]
fn closed_codec_and_readout_freeze_the_real_weapon_identity() {
    let root = TestProjectRoot::new();
    let location = root.location();
    let hash = project_hash(&location);
    let input = json!({
        "type": "readLoadingBayWeapon",
        "contractVersion": 1,
        "requestId": "fixture-read-arc-pistol",
        "expectedProjectHash": hash,
        "ownerEntityId": ARC_PISTOL_ENTITY,
    })
    .to_string();
    let request = decode_loading_bay_weapon_authoring_request(&input).unwrap();
    let response = LoadingBayWeaponAuthoringService::new().handle(&location, request);
    let encoded_response = encode_loading_bay_weapon_authoring_response(&response).unwrap();
    let LoadingBayWeaponAuthoringResponse::LoadingBayWeaponRead {
        contract_version,
        request_id,
        weapon,
    } = response
    else {
        panic!("expected canonical weapon readout");
    };

    assert_eq!(
        contract_version,
        LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION
    );
    assert_eq!(request_id, "fixture-read-arc-pistol");
    assert_eq!(
        weapon.component_type_id,
        LOADING_BAY_WEAPON_COMPONENT_TYPE_ID
    );
    assert_eq!(weapon.contract_id, LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID);
    assert_eq!(weapon.item_definition_id, "weapon/arc-pistol");
    assert_eq!(weapon.binding.inventory_owner_entity_id, 1);
    assert_eq!(weapon.binding.slot_index, 0);
    assert_eq!(weapon.binding.starting_quantity, 1);
    assert!(weapon.binding.initially_equipped);
    assert_eq!(weapon.definition.damage, 60);
    let mut actual_fixture: serde_json::Value = serde_json::from_str(&encoded_response).unwrap();
    actual_fixture["weapon"]["componentRevision"] =
        json!("0000000000000000000000000000000000000000000000000000000000000000");
    assert_eq!(
        actual_fixture,
        serde_json::from_str::<serde_json::Value>(READ_RESPONSE_FIXTURE).unwrap()
    );

    let unknown = json!({
        "type": "readLoadingBayWeapon",
        "contractVersion": 1,
        "requestId": "unknown",
        "expectedProjectHash": hash,
        "ownerEntityId": ARC_PISTOL_ENTITY,
        "operation": "genericGet",
    })
    .to_string();
    assert!(decode_loading_bay_weapon_authoring_request(&unknown).is_err());
    assert!(decode_loading_bay_weapon_authoring_request("{").is_err());
    assert!(decode_loading_bay_weapon_authoring_request(
        &" ".repeat(MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES + 1)
    )
    .is_err());
    assert!(matches!(
        decode_loading_bay_weapon_authoring_request(READ_FIXTURE).unwrap(),
        LoadingBayWeaponAuthoringRequest::ReadLoadingBayWeapon { .. }
    ));
    assert!(matches!(
        decode_loading_bay_weapon_authoring_request(REPLACE_FIXTURE).unwrap(),
        LoadingBayWeaponAuthoringRequest::ReplaceLoadingBayWeapon { .. }
    ));

    let mut exact_limit: serde_json::Value = serde_json::from_str(REPLACE_FIXTURE).unwrap();
    let empty_length = exact_limit.to_string().len()
        - exact_limit["candidate"]["presentation"]
            .as_str()
            .unwrap()
            .len();
    exact_limit["candidate"]["presentation"] =
        json!("x".repeat(MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES - empty_length));
    let exact_limit = exact_limit.to_string();
    assert_eq!(
        exact_limit.len(),
        MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES
    );
    assert!(decode_loading_bay_weapon_authoring_request(&exact_limit).is_ok());
    let one_over = exact_limit.replacen(
        &format!(
            "\"{}\"",
            "x".repeat(MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES - empty_length)
        ),
        &format!(
            "\"{}\"",
            "x".repeat(MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES - empty_length + 1)
        ),
        1,
    );
    assert_eq!(
        one_over.len(),
        MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES + 1
    );
    assert!(decode_loading_bay_weapon_authoring_request(&one_over).is_err());
}

#[test]
fn complete_replace_accepts_exact_limit_and_rejects_one_over_without_mutation() {
    let root = TestProjectRoot::new();
    let location = root.location();
    let service = LoadingBayWeaponAuthoringService::new();
    let (hash, revision, mut candidate) = read_weapon(&service, &location);
    candidate.presentation = "p".repeat(MAX_ITEM_DEFINITION_ID_BYTES);

    let replaced = service.handle(
        &location,
        replace_request(&hash, &revision, candidate.clone(), "exact-limit"),
    );
    let LoadingBayWeaponAuthoringResponse::LoadingBayWeaponReplaced {
        receipt, weapon, ..
    } = replaced
    else {
        panic!("exact-limit candidate should be admitted");
    };
    assert_ne!(receipt.project_hash_before, receipt.project_hash_after);
    assert_ne!(
        receipt.component_revision_before,
        receipt.component_revision_after
    );
    assert_eq!(
        weapon.definition.presentation.len(),
        MAX_ITEM_DEFINITION_ID_BYTES
    );

    let bytes_before_rejection = fs::read(location.project_file()).unwrap();
    let mut one_over = weapon.definition;
    one_over.presentation = "p".repeat(MAX_ITEM_DEFINITION_ID_BYTES + 1);
    let rejected = service.handle(
        &location,
        replace_request(
            &receipt.project_hash_after,
            &receipt.component_revision_after,
            one_over,
            "one-over",
        ),
    );
    assert_rejection(
        rejected,
        LoadingBayWeaponAuthoringRejectionCode::CandidateRejected,
    );
    assert_eq!(
        fs::read(location.project_file()).unwrap(),
        bytes_before_rejection
    );
}

#[test]
fn stale_project_and_component_guards_are_independent_and_typed() {
    let root = TestProjectRoot::new();
    let location = root.location();
    let service = LoadingBayWeaponAuthoringService::new();
    let (initial_hash, initial_revision, mut candidate) = read_weapon(&service, &location);
    candidate.damage += 1;
    let response = service.handle(
        &location,
        replace_request(&initial_hash, &initial_revision, candidate.clone(), "first"),
    );
    let LoadingBayWeaponAuthoringResponse::LoadingBayWeaponReplaced {
        receipt, weapon, ..
    } = response
    else {
        panic!("first replacement should succeed");
    };

    let stale_project = service.handle(
        &location,
        replace_request(
            &initial_hash,
            &initial_revision,
            candidate.clone(),
            "stale-project",
        ),
    );
    assert_rejection(
        stale_project,
        LoadingBayWeaponAuthoringRejectionCode::StaleProject,
    );

    candidate.damage += 1;
    let stale_component = service.handle(
        &location,
        replace_request(
            &receipt.project_hash_after,
            &initial_revision,
            candidate,
            "stale-component",
        ),
    );
    assert_rejection(
        stale_component,
        LoadingBayWeaponAuthoringRejectionCode::StaleComponent,
    );
    assert_eq!(
        project_hash(&location),
        receipt.project_hash_after,
        "both rejected guards preserve the published candidate"
    );
    assert_eq!(weapon.definition.damage, 61);
}

#[test]
fn semantic_rejection_is_atomic_and_fresh_service_reconstructs_published_state() {
    let root = TestProjectRoot::new();
    let location = root.location();
    let first_process = LoadingBayWeaponAuthoringService::new();
    let (hash, revision, mut candidate) = read_weapon(&first_process, &location);
    let original_bytes = fs::read(location.project_file()).unwrap();
    candidate.ammunition_item_id = "ammo/not-authored".to_string();

    let rejected = first_process.handle(
        &location,
        replace_request(&hash, &revision, candidate, "bad-ammo"),
    );
    assert_rejection(
        rejected,
        LoadingBayWeaponAuthoringRejectionCode::CandidateRejected,
    );
    assert_eq!(fs::read(location.project_file()).unwrap(), original_bytes);

    let (_, _, mut accepted) = read_weapon(&first_process, &location);
    accepted.cooldown_ticks += 1;
    let replaced = first_process.handle(
        &location,
        replace_request(&hash, &revision, accepted, "publish"),
    );
    let LoadingBayWeaponAuthoringResponse::LoadingBayWeaponReplaced {
        receipt, weapon, ..
    } = replaced
    else {
        panic!("valid replacement should publish");
    };
    assert_eq!(weapon.definition.cooldown_ticks, 3);

    let fresh_process = LoadingBayWeaponAuthoringService::new();
    let (fresh_hash, fresh_revision, fresh_candidate) = read_weapon(&fresh_process, &location);
    assert_eq!(fresh_hash, receipt.project_hash_after);
    assert_eq!(fresh_revision, receipt.component_revision_after);
    assert_eq!(fresh_candidate.cooldown_ticks, 3);
}

fn read_weapon(
    service: &LoadingBayWeaponAuthoringService,
    location: &ProjectLocation,
) -> (String, String, LoadingBayWeaponAuthoringCandidate) {
    let hash = project_hash(location);
    let response = service.handle(
        location,
        LoadingBayWeaponAuthoringRequest::ReadLoadingBayWeapon {
            contract_version: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION,
            request_id: "read".to_string(),
            expected_project_hash: hash.clone(),
            owner_entity_id: ARC_PISTOL_ENTITY,
        },
    );
    let LoadingBayWeaponAuthoringResponse::LoadingBayWeaponRead { weapon, .. } = response else {
        panic!("expected weapon readout");
    };
    (hash, weapon.component_revision, weapon.definition)
}

fn replace_request(
    expected_project_hash: &str,
    expected_component_revision: &str,
    candidate: LoadingBayWeaponAuthoringCandidate,
    request_id: &str,
) -> LoadingBayWeaponAuthoringRequest {
    LoadingBayWeaponAuthoringRequest::ReplaceLoadingBayWeapon {
        contract_version: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION,
        request_id: request_id.to_string(),
        expected_project_hash: expected_project_hash.to_string(),
        owner_entity_id: ARC_PISTOL_ENTITY,
        expected_component_revision: expected_component_revision.to_string(),
        candidate,
    }
}

fn assert_rejection(
    response: LoadingBayWeaponAuthoringResponse,
    expected: LoadingBayWeaponAuthoringRejectionCode,
) {
    let LoadingBayWeaponAuthoringResponse::LoadingBayWeaponRejected { rejection, .. } = response
    else {
        panic!("expected typed rejection");
    };
    assert_eq!(rejection.code, expected);
}

fn project_hash(location: &ProjectLocation) -> String {
    ProjectStore::default()
        .load_source(location.project_file())
        .unwrap()
        .source_hash()
        .to_hex()
}

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestProjectRoot(PathBuf);

impl TestProjectRoot {
    fn new() -> Self {
        let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rusty-engine-weapon-authoring-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(path.join("content/projects")).unwrap();
        fs::write(path.join(PROJECT_FILE), CURRENT_PROJECT).unwrap();
        Self(path)
    }

    fn location(&self) -> ProjectLocation {
        ProjectLocation::resolve(self.path().to_str().unwrap(), PROJECT_FILE).unwrap()
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestProjectRoot {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
