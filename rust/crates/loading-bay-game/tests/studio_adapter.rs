use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use loading_bay_game::{
    decode_project_document, encode_project_document, StudioAdapterService,
    MAX_STUDIO_ADAPTER_REQUEST_BYTES,
};
use serde_json::{json, Value};

const CURRENT_PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const PROJECT_FILE: &str = "content/projects/loading-bay.project.json";

#[test]
fn open_uses_engine_owners_and_returns_canonical_projection_and_voxel_readouts() {
    let root = TestProjectRoot::new(CURRENT_PROJECT);
    let mut service = StudioAdapterService::new();

    let response = send(
        &mut service,
        json!({
            "type": "openProject",
            "protocolVersion": 3,
            "requestId": "open",
            "root": root.path(),
            "projectFile": PROJECT_FILE,
        }),
    );

    assert_eq!(response["type"], "projectOpened");
    assert_eq!(response["project"]["identity"]["projectId"], "loading-bay");
    assert_eq!(
        response["project"]["inspections"]["catalog"]["entryCount"],
        6
    );
    assert_eq!(response["project"]["inspections"]["scene"]["nodeCount"], 8);
    assert_eq!(
        response["project"]["inspections"]["entityState"]["entityCount"],
        8
    );
    assert_eq!(
        response["project"]["inspections"]["persistence"]["artifactCount"],
        1
    );
    assert_eq!(response["project"]["voxel"]["solidVoxelCount"], 366);
    assert_eq!(response["project"]["loadingBay"]["doorCount"], 1);
    assert_eq!(response["project"]["loadingBay"]["enemyCount"], 2);
    assert_eq!(response["project"]["sceneHierarchy"]["sceneId"], 1);
    assert_eq!(
        response["project"]["sceneHierarchy"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    assert_eq!(
        response["project"]["sceneHierarchy"]["nodes"][0]["label"],
        "player"
    );
    assert_eq!(
        response["project"]["sceneHierarchy"]["nodes"][0]["entityId"],
        1
    );
    assert_eq!(response["project"]["projection"]["schemaVersion"], 1);
    assert_eq!(
        response["project"]["projection"]["ops"]
            .as_array()
            .unwrap()
            .len(),
        19
    );
    assert_eq!(
        response["project"]["projection"]["ops"][0]["op"],
        "defineMaterial"
    );
    assert_eq!(
        response["project"]["projection"]["ops"][1]["op"],
        "defineStaticMesh"
    );
    assert_eq!(
        response["project"]["projectionReadout"]["frameKind"],
        "complete"
    );
    assert_eq!(
        response["project"]["projectionReadout"]["diagnostics"],
        json!([])
    );

    for canonical in [
        "projectJson",
        "assetCatalogJson",
        "authoredSceneJson",
        "entityStateJson",
        "contentManifestJson",
    ] {
        let value = response["project"]["canonical"][canonical]
            .as_str()
            .expect("canonical owner content is encoded JSON");
        serde_json::from_str::<Value>(value).expect("canonical owner content is valid JSON");
    }
}

#[test]
fn typed_transform_is_owner_admitted_hash_guarded_persisted_and_reread() {
    let root = TestProjectRoot::new(CURRENT_PROJECT);
    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);
    let identity = &opened["project"]["identity"];
    let project_hash = identity["projectHash"].as_str().unwrap();
    let scene_revision = identity["sceneRevision"].as_u64().unwrap();

    let response = send(
        &mut service,
        json!({
            "type": "setEntityTranslation",
            "protocolVersion": 3,
            "requestId": "move-player",
            "expectedProjectHash": project_hash,
            "expectedSceneRevision": scene_revision,
            "entityId": 1,
            "translation": [2.5, 1.5, 3.5],
        }),
    );

    assert_eq!(response["type"], "entityTranslationApplied");
    assert_eq!(response["receipt"]["projectHashBefore"], project_hash);
    assert_ne!(response["receipt"]["projectHashAfter"], project_hash);
    assert_eq!(response["receipt"]["translation"], json!([2.5, 1.5, 3.5]));
    assert_eq!(
        response["project"]["identity"]["projectHash"],
        response["receipt"]["projectHashAfter"]
    );
    assert_ne!(
        response["project"]["identity"]["sceneRevision"],
        scene_revision
    );

    let persisted = decode_project_document(&fs::read_to_string(root.project_file()).unwrap())
        .unwrap()
        .project;
    let player = persisted.scenes[0]
        .entities
        .iter()
        .find(|entity| entity.id == 1)
        .unwrap();
    assert_eq!(player.translation, Some([2.5, 1.5, 3.5]));
    assert_eq!(
        fs::read_to_string(root.project_file()).unwrap(),
        encode_project_document(&persisted).unwrap()
    );

    let installed = fs::read(root.project_file()).unwrap();
    let stale = send(
        &mut service,
        json!({
            "type": "setEntityTranslation",
            "protocolVersion": 3,
            "requestId": "stale-move",
            "expectedProjectHash": project_hash,
            "expectedSceneRevision": scene_revision,
            "entityId": 1,
            "translation": [9.0, 9.0, 9.0],
        }),
    );
    assert_eq!(stale["type"], "rejected");
    assert_eq!(stale["error"]["code"], "project.staleHash");
    assert_eq!(fs::read(root.project_file()).unwrap(), installed);
}

#[test]
fn invalid_owner_operation_and_bad_downstream_semantics_preserve_project_bytes() {
    let root = TestProjectRoot::new(CURRENT_PROJECT);
    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);
    let identity = &opened["project"]["identity"];
    let before = fs::read(root.project_file()).unwrap();

    let missing = send(
        &mut service,
        json!({
            "type": "setEntityTranslation",
            "protocolVersion": 3,
            "requestId": "missing",
            "expectedProjectHash": identity["projectHash"],
            "expectedSceneRevision": identity["sceneRevision"],
            "entityId": 999,
            "translation": [1.0, 2.0, 3.0],
        }),
    );
    assert_eq!(missing["type"], "rejected");
    assert_eq!(missing["error"]["code"], "scene.missingEntity");
    assert_eq!(fs::read(root.project_file()).unwrap(), before);

    let invalid = send(
        &mut service,
        json!({
            "type": "setEntityTranslation",
            "protocolVersion": 3,
            "requestId": "invalid",
            "expectedProjectHash": identity["projectHash"],
            "expectedSceneRevision": identity["sceneRevision"],
            "entityId": 1,
            "translation": [2000000.0, 1.5, 2.5],
        }),
    );
    assert_eq!(invalid["type"], "rejected");
    assert_eq!(invalid["error"]["code"], "invalid-scene-after-edit");
    assert_eq!(fs::read(root.project_file()).unwrap(), before);

    let mut downstream_invalid = decode_project_document(CURRENT_PROJECT).unwrap().project;
    downstream_invalid.scenes[0].entities[5]
        .switch
        .as_mut()
        .unwrap()
        .controls = vec![999];
    fs::write(
        root.project_file(),
        encode_project_document(&downstream_invalid).unwrap(),
    )
    .unwrap();
    let invalid_bytes = fs::read(root.project_file()).unwrap();
    let response = send(
        &mut StudioAdapterService::new(),
        json!({
            "type": "openProject",
            "protocolVersion": 3,
            "requestId": "bad-domain",
            "root": root.path(),
            "projectFile": PROJECT_FILE,
        }),
    );
    assert_eq!(response["type"], "rejected");
    assert_eq!(response["error"]["code"], "project.invalidRelationship");
    assert_eq!(fs::read(root.project_file()).unwrap(), invalid_bytes);
}

#[test]
fn malformed_unbounded_and_unsafe_paths_fail_closed() {
    let root = TestProjectRoot::new(CURRENT_PROJECT);
    let mut service = StudioAdapterService::new();

    let malformed: Value =
        serde_json::from_str(&service.handle_json(
            r#"{"type":"readProject","protocolVersion":1,"requestId":"x","extra":true}"#,
        ))
        .unwrap();
    assert_eq!(malformed["error"]["code"], "protocol.malformedRequest");

    let unknown: Value = serde_json::from_str(
        &service.handle_json(r#"{"type":"sendMessage","protocolVersion":1,"requestId":"x"}"#),
    )
    .unwrap();
    assert_eq!(unknown["error"]["code"], "protocol.malformedRequest");

    let too_large: Value = serde_json::from_str(
        &service.handle_json(&" ".repeat(MAX_STUDIO_ADAPTER_REQUEST_BYTES + 1)),
    )
    .unwrap();
    assert_eq!(too_large["error"]["code"], "protocol.requestTooLarge");

    let traversal = send(
        &mut service,
        json!({
            "type": "openProject",
            "protocolVersion": 3,
            "requestId": "traversal",
            "root": root.path(),
            "projectFile": "../outside.project.json",
        }),
    );
    assert_eq!(traversal["error"]["code"], "path.rejected");

    let relative_root = send(
        &mut service,
        json!({
            "type": "openProject",
            "protocolVersion": 3,
            "requestId": "relative-root",
            "root": "relative",
            "projectFile": PROJECT_FILE,
        }),
    );
    assert_eq!(relative_root["error"]["code"], "path.rejected");
}

#[cfg(unix)]
#[test]
fn symlinked_project_paths_are_rejected_even_when_the_target_stays_inside_root() {
    use std::os::unix::fs::symlink;

    let root = TestProjectRoot::new(CURRENT_PROJECT);
    let link = root.path().join("content/projects/linked.project.json");
    symlink(root.project_file(), &link).unwrap();
    let response = send(
        &mut StudioAdapterService::new(),
        json!({
            "type": "openProject",
            "protocolVersion": 3,
            "requestId": "symlink",
            "root": root.path(),
            "projectFile": "content/projects/linked.project.json",
        }),
    );
    assert_eq!(response["type"], "rejected");
    assert_eq!(response["error"]["code"], "path.rejected");
}

fn open(service: &mut StudioAdapterService, root: &TestProjectRoot) -> Value {
    let response = send(
        service,
        json!({
            "type": "openProject",
            "protocolVersion": 3,
            "requestId": "open",
            "root": root.path(),
            "projectFile": PROJECT_FILE,
        }),
    );
    assert_eq!(response["type"], "projectOpened", "{response:#}");
    response
}

fn send(service: &mut StudioAdapterService, request: Value) -> Value {
    serde_json::from_str(&service.handle_json(&request.to_string())).unwrap()
}

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestProjectRoot(PathBuf);

impl TestProjectRoot {
    fn new(project: &str) -> Self {
        let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rusty-engine-studio-adapter-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(path.join("content/projects")).unwrap();
        fs::write(path.join(PROJECT_FILE), project).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn project_file(&self) -> PathBuf {
        self.path().join(PROJECT_FILE)
    }
}

impl Drop for TestProjectRoot {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
