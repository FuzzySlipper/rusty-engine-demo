use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use loading_bay_game::{
    decode_project_document, encode_project_document, StudioAdapterService,
    MAX_STUDIO_ADAPTER_REQUEST_BYTES, STORED_PROJECT_SCHEMA_VERSION,
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
            "protocolVersion": 9,
            "requestId": "open",
            "root": root.path(),
            "projectFile": PROJECT_FILE,
        }),
    );

    assert_eq!(response["type"], "projectOpened");
    assert_eq!(response["project"]["identity"]["projectId"], "loading-bay");
    assert_eq!(
        response["project"]["inspections"]["catalog"]["entryCount"],
        15
    );
    assert_eq!(response["project"]["inspections"]["scene"]["nodeCount"], 47);
    assert_eq!(
        response["project"]["inspections"]["entityState"]["entityCount"],
        50
    );
    assert_eq!(
        response["project"]["inspections"]["persistence"]["artifactCount"],
        1
    );
    assert_eq!(response["project"]["voxel"]["solidVoxelCount"], 3_931);
    assert_eq!(response["project"]["loadingBay"]["doorCount"], 5);
    assert_eq!(response["project"]["loadingBay"]["enemyCount"], 8);
    assert_eq!(response["project"]["sceneHierarchy"]["sceneId"], 1);
    assert_eq!(
        response["project"]["sceneHierarchy"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        47
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
        72
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
    assert_eq!(
        response["project"]["animatedMeshResources"][0]["asset"],
        "mesh-animation/kenney-retro-character-medium"
    );
    assert_eq!(
        response["project"]["animatedMeshResources"][0]["clipIds"],
        json!(["idle", "run", "jump"])
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
fn project_creation_and_save_as_publish_admitted_canonical_projects() {
    let root = TestProjectRoot::new(CURRENT_PROJECT);
    let mut service = StudioAdapterService::new();
    let created = send(
        &mut service,
        json!({
            "type": "createProject",
            "protocolVersion": 9,
            "requestId": "create-project",
            "root": root.path(),
            "projectFile": "content/projects/new-project.project.json",
            "projectId": "new-project",
            "name": "New Project",
            "entryScene": "scene/new-entry",
            "entrySceneName": "New Entry",
        }),
    );
    assert_eq!(created["type"], "projectCreated", "{created:#}");
    assert_eq!(created["project"]["identity"]["projectId"], "new-project");
    assert_eq!(
        created["project"]["identity"]["currentSchemaVersion"],
        STORED_PROJECT_SCHEMA_VERSION
    );
    let created_path = root
        .path()
        .join("content/projects/new-project.project.json");
    let created_bytes = fs::read(&created_path).unwrap();
    let created_document =
        decode_project_document(std::str::from_utf8(&created_bytes).unwrap()).unwrap();
    assert_eq!(
        created_document.source_schema_version,
        STORED_PROJECT_SCHEMA_VERSION
    );
    assert_eq!(created_document.project.scenes.len(), 1);

    let duplicate = send(
        &mut service,
        json!({
            "type": "createProject",
            "protocolVersion": 9,
            "requestId": "duplicate-project",
            "root": root.path(),
            "projectFile": "content/projects/new-project.project.json",
            "projectId": "other-project",
            "name": "Other Project",
            "entryScene": "scene/other-entry",
            "entrySceneName": "Other Entry",
        }),
    );
    assert_eq!(duplicate["type"], "rejected");
    assert_eq!(fs::read(&created_path).unwrap(), created_bytes);

    let saved = send(
        &mut service,
        json!({
            "type": "saveProjectAs",
            "protocolVersion": 9,
            "requestId": "save-as",
            "expectedProjectHash": created["project"]["identity"]["projectHash"],
            "root": root.path(),
            "projectFile": "content/projects/copied-project.project.json",
            "projectId": "copied-project",
            "name": "Copied Project",
        }),
    );
    assert_eq!(saved["type"], "projectSavedAs", "{saved:#}");
    assert_eq!(saved["project"]["identity"]["projectId"], "copied-project");
    assert_eq!(
        saved["project"]["identity"]["relativeProjectFile"],
        "content/projects/copied-project.project.json"
    );
    assert_eq!(fs::read(&created_path).unwrap(), created_bytes);
    let copied = decode_project_document(
        &fs::read_to_string(
            root.path()
                .join("content/projects/copied-project.project.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(copied.project.project_id, "copied-project");
}

#[test]
fn scene_object_hierarchy_lights_full_transforms_and_capabilities_are_owner_admitted() {
    let root = TestProjectRoot::new(CURRENT_PROJECT);
    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);
    let (mut hash, mut revision) = owner_version(&opened);

    let created = send(
        &mut service,
        json!({
            "type": "createSceneObject",
            "protocolVersion": 9,
            "requestId": "create-object",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "object": {
                "entityId": 200,
                "name": "authoring-root",
                "parentEntityId": null,
                "childOrder": 200,
                "transform": {
                    "translation": [1.0, 2.0, 3.0],
                    "rotation": [0.0, 0.0, 0.0, 1.0],
                    "scale": [1.0, 1.0, 1.0]
                },
                "appearance": { "kind": "empty" },
                "collision": null,
                "kinematic": null
            }
        }),
    );
    assert_eq!(created["type"], "projectMutationApplied", "{created:#}");
    (hash, revision) = owner_version(&created);

    let lit = send(
        &mut service,
        json!({
            "type": "createSceneObject",
            "protocolVersion": 9,
            "requestId": "create-light",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "object": {
                "entityId": 201,
                "name": "work-light",
                "parentEntityId": 200,
                "childOrder": 0,
                "transform": {
                    "translation": [0.0, 4.0, 0.0],
                    "rotation": [0.0, 0.0, 0.0, 1.0],
                    "scale": [1.0, 1.0, 1.0]
                },
                "appearance": {
                    "kind": "light",
                    "light": {
                        "kind": "point",
                        "color": [1.0, 0.8, 0.6],
                        "intensity": 3.0,
                        "enabled": true,
                        "range": 12.0,
                        "decay": 2.0,
                        "shadows": true
                    }
                },
                "collision": null,
                "kinematic": null
            }
        }),
    );
    assert_eq!(lit["type"], "projectMutationApplied", "{lit:#}");
    assert_eq!(lit["project"]["projectionReadout"]["retainedLights"], 9);
    assert!(lit["project"]["projection"]["ops"]
        .as_array()
        .unwrap()
        .iter()
        .any(|operation| operation["op"] == "createLight"));
    (hash, revision) = owner_version(&lit);

    let transformed = send(
        &mut service,
        json!({
            "type": "setSceneObjectTransform",
            "protocolVersion": 9,
            "requestId": "full-transform",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "entityId": 200,
            "transform": {
                "translation": [3.0, 2.0, 1.0],
                "rotation": [0.0, 0.70710677, 0.0, 0.70710677],
                "scale": [2.0, 1.5, 0.5]
            }
        }),
    );
    assert_eq!(transformed["type"], "projectMutationApplied");
    (hash, revision) = owner_version(&transformed);

    let appeared = send(
        &mut service,
        json!({
            "type": "setSceneObjectAppearance",
            "protocolVersion": 9,
            "requestId": "appearance",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "entityId": 200,
            "appearance": {
                "kind": "staticMesh",
                "asset": "mesh/player-marker",
                "visible": true
            }
        }),
    );
    assert_eq!(appeared["type"], "projectMutationApplied", "{appeared:#}");
    let mesh_instance = appeared["project"]["projection"]["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|operation| {
            operation["op"] == "createStaticMeshInstance"
                && operation["instance"]["metadata"]["sourceEntity"] == 200
        })
        .unwrap();
    assert_eq!(
        mesh_instance["instance"]["transform"]["rotation"],
        json!([0.0, 0.70710677, 0.0, 0.70710677])
    );
    assert_eq!(
        mesh_instance["instance"]["transform"]["scale"],
        json!([2.0, 1.5, 0.5])
    );
    (hash, revision) = owner_version(&appeared);

    let animated = send(
        &mut service,
        json!({
            "type": "setSceneObjectAppearance",
            "protocolVersion": 9,
            "requestId": "animated-appearance",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "entityId": 200,
            "appearance": {
                "kind": "animatedMesh",
                "asset": "mesh-animation/kenney-retro-character-medium",
                "visible": true,
                "clip": "run"
            }
        }),
    );
    assert_eq!(animated["type"], "projectMutationApplied", "{animated:#}");
    assert_eq!(
        animated["project"]["animatedMeshResources"][0]["clipIds"],
        json!(["idle", "run", "jump"])
    );
    assert!(animated["project"]["projection"]["ops"]
        .as_array()
        .unwrap()
        .iter()
        .any(|operation| operation["op"] == "defineAnimatedMesh"));
    let animated_instance = animated["project"]["projection"]["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|operation| {
            operation["op"] == "createAnimatedMeshInstance"
                && operation["instance"]["metadata"]["sourceEntity"] == 200
        })
        .unwrap();
    assert_eq!(animated_instance["instance"]["playback"]["clip"], "run");
    assert_eq!(animated_instance["instance"]["playback"]["loop"], "repeat");
    (hash, revision) = owner_version(&animated);
    let before_invalid_clip = fs::read(root.project_file()).unwrap();
    let invalid_clip = send(
        &mut service,
        json!({
            "type": "setSceneObjectAppearance",
            "protocolVersion": 9,
            "requestId": "invalid-animation-clip",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "entityId": 200,
            "appearance": {
                "kind": "animatedMesh",
                "asset": "mesh-animation/kenney-retro-character-medium",
                "visible": true,
                "clip": "missing"
            }
        }),
    );
    assert_eq!(invalid_clip["type"], "rejected");
    assert_eq!(fs::read(root.project_file()).unwrap(), before_invalid_clip);

    let collision = send(
        &mut service,
        json!({
            "type": "setEntityCollision",
            "protocolVersion": 9,
            "requestId": "collision",
            "expectedProjectHash": hash,
            "entityId": 200,
            "collision": { "enabled": true, "staticCollider": false }
        }),
    );
    assert_eq!(collision["type"], "projectMutationApplied");
    hash = owner_version(&collision).0;

    let kinematic = send(
        &mut service,
        json!({
            "type": "setEntityKinematic",
            "protocolVersion": 9,
            "requestId": "kinematic",
            "expectedProjectHash": hash,
            "entityId": 200,
            "kinematic": {
                "halfExtents": [0.5, 0.75, 0.25],
                "velocity": [0.0, 0.0, 0.0]
            }
        }),
    );
    assert_eq!(kinematic["type"], "projectMutationApplied", "{kinematic:#}");
    (hash, revision) = owner_version(&kinematic);

    let renamed = send(
        &mut service,
        json!({
            "type": "renameSceneObject",
            "protocolVersion": 9,
            "requestId": "rename",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "entityId": 200,
            "name": "authored-display"
        }),
    );
    assert_eq!(renamed["type"], "projectMutationApplied");
    (hash, revision) = owner_version(&renamed);

    let before_cycle = fs::read(root.project_file()).unwrap();
    let cycle = send(
        &mut service,
        json!({
            "type": "reparentSceneObject",
            "protocolVersion": 9,
            "requestId": "cycle",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "entityId": 200,
            "parentEntityId": 201,
            "childOrder": 0
        }),
    );
    assert_eq!(cycle["type"], "rejected");
    assert_eq!(cycle["error"]["code"], "invalid-scene-after-edit");
    assert_eq!(fs::read(root.project_file()).unwrap(), before_cycle);

    let deleted = send(
        &mut service,
        json!({
            "type": "deleteSceneObject",
            "protocolVersion": 9,
            "requestId": "delete-subtree",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "entityId": 200
        }),
    );
    assert_eq!(deleted["type"], "projectMutationApplied", "{deleted:#}");
    assert_eq!(deleted["receipt"]["removedObjects"], 2);
    assert_eq!(deleted["project"]["projectionReadout"]["retainedLights"], 8);

    let persisted = decode_project_document(&fs::read_to_string(root.project_file()).unwrap())
        .unwrap()
        .project;
    assert!(persisted.scenes[0]
        .entities
        .iter()
        .all(|entity| !matches!(entity.id, 200 | 201)));
}

#[test]
fn asset_import_reimport_catalog_lock_and_render_payload_are_rust_owned() {
    let root = TestProjectRoot::new(CURRENT_PROJECT);
    let source_path = "content/assets/studio-triangle.mesh.json";
    fs::create_dir_all(root.path().join("content/assets")).unwrap();
    fs::write(
        root.path().join(source_path),
        imported_triangle([0.2, 0.4, 0.8, 1.0]),
    )
    .unwrap();

    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);
    let (project_hash, _) = owner_version(&opened);
    let prepared = send(
        &mut service,
        json!({
            "type": "prepareAssetImport",
            "protocolVersion": 9,
            "requestId": "prepare-import",
            "expectedProjectHash": project_hash,
            "source": { "scope": "project", "path": source_path },
            "settings": {
                "scale": 2.0,
                "generateCollision": true,
                "materialNamespace": "studio"
            }
        }),
    );
    assert_eq!(prepared["type"], "assetImportPrepared", "{prepared:#}");
    assert_eq!(prepared["plan"]["hasErrors"], false);
    assert_eq!(prepared["plan"]["meshAssetId"], "mesh/studio-triangle");
    assert_eq!(prepared["plan"]["reimportKind"], "structuralReload");
    assert_eq!(
        prepared["plan"]["generatedAssetIds"],
        json!(["material/studio/paint", "mesh/studio-triangle"])
    );

    let applied = send(
        &mut service,
        json!({
            "type": "applyAssetImport",
            "protocolVersion": 9,
            "requestId": "apply-import",
            "expectedProjectHash": project_hash,
            "planId": prepared["plan"]["planId"],
            "expectedPlanHash": prepared["plan"]["planHash"]
        }),
    );
    assert_eq!(applied["type"], "projectMutationApplied", "{applied:#}");
    assert_eq!(applied["receipt"]["kind"], "assetImportApplied");
    let imported_entry = applied["project"]["assetBrowser"]["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["assetId"] == "mesh/studio-triangle")
        .unwrap();
    assert_eq!(
        imported_entry["dependencies"],
        json!(["material/studio/paint"])
    );
    assert_eq!(imported_entry["importedMesh"], true);
    assert_eq!(imported_entry["import"]["status"], "unchanged");
    assert!(applied["project"]["assetBrowser"]["lockEntries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["assetId"] == "mesh/studio-triangle"));
    let mesh_definition = applied["project"]["projection"]["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|operation| {
            operation["op"] == "defineStaticMesh"
                && operation["asset"]["asset"] == "mesh/studio-triangle"
        })
        .unwrap();
    assert_eq!(
        mesh_definition["asset"]["payload"]["source"]["positions"],
        json!([0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, 0.0])
    );
    assert_eq!(
        mesh_definition["asset"]["collision"],
        json!({ "kind": "aabbFallback" })
    );

    let (project_hash, _) = owner_version(&applied);
    fs::write(
        root.path().join(source_path),
        imported_triangle([0.8, 0.2, 0.1, 1.0]),
    )
    .unwrap();
    let drifted = send(
        &mut service,
        json!({
            "type": "readProject",
            "protocolVersion": 9,
            "requestId": "read-drift"
        }),
    );
    let drifted_entry = drifted["project"]["assetBrowser"]["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["assetId"] == "mesh/studio-triangle")
        .unwrap();
    assert_eq!(drifted_entry["import"]["status"], "contentChanged");

    let reimport = send(
        &mut service,
        json!({
            "type": "prepareAssetReimport",
            "protocolVersion": 9,
            "requestId": "prepare-reimport",
            "expectedProjectHash": project_hash,
            "assetId": "mesh/studio-triangle"
        }),
    );
    assert_eq!(reimport["type"], "assetImportPrepared", "{reimport:#}");
    assert_eq!(reimport["plan"]["reimportKind"], "visualUpdate");
    let reapplied = send(
        &mut service,
        json!({
            "type": "applyAssetImport",
            "protocolVersion": 9,
            "requestId": "apply-reimport",
            "expectedProjectHash": project_hash,
            "planId": reimport["plan"]["planId"],
            "expectedPlanHash": reimport["plan"]["planHash"]
        }),
    );
    assert_eq!(reapplied["type"], "projectMutationApplied", "{reapplied:#}");
    assert_eq!(reapplied["receipt"]["reimportKind"], "visualUpdate");
    let material = reapplied["project"]["projection"]["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|operation| {
            operation["op"] == "defineMaterial"
                && operation["material"]["id"] == "material/studio/paint"
        })
        .unwrap();
    assert_eq!(material["material"]["color"], json!([0.8, 0.2, 0.1, 1.0]));

    let rejected_source = "content/assets/rejected.mesh.json";
    fs::write(root.path().join(rejected_source), "{\"schemaVersion\":1}").unwrap();
    let project_hash = owner_version(&reapplied).0;
    let before = fs::read(root.project_file()).unwrap();
    let failed_plan = send(
        &mut service,
        json!({
            "type": "prepareAssetImport",
            "protocolVersion": 9,
            "requestId": "prepare-invalid",
            "expectedProjectHash": project_hash,
            "source": { "scope": "project", "path": rejected_source },
            "settings": {
                "scale": 1.0,
                "generateCollision": false,
                "materialNamespace": null
            }
        }),
    );
    assert_eq!(
        failed_plan["type"], "assetImportPrepared",
        "{failed_plan:#}"
    );
    assert_eq!(failed_plan["plan"]["hasErrors"], true);
    let rejected = send(
        &mut service,
        json!({
            "type": "applyAssetImport",
            "protocolVersion": 9,
            "requestId": "apply-invalid",
            "expectedProjectHash": project_hash,
            "planId": failed_plan["plan"]["planId"],
            "expectedPlanHash": failed_plan["plan"]["planHash"]
        }),
    );
    assert_eq!(rejected["type"], "rejected");
    assert_eq!(rejected["error"]["code"], "assetImport.planHasErrors");
    assert_eq!(fs::read(root.project_file()).unwrap(), before);
}

fn imported_triangle(color: [f32; 4]) -> String {
    serde_json::to_string_pretty(&json!({
        "schemaVersion": 1,
        "name": "studio-triangle",
        "positions": [0, 0, 0, 1, 0, 0, 0, 1, 0],
        "normals": [0, 0, 1, 0, 0, 1, 0, 0, 1],
        "indices": [0, 1, 2],
        "materials": [
            { "slot": 0, "name": "paint", "color": color, "texture": null }
        ],
        "groups": [{ "materialSlot": 0, "start": 0, "count": 3 }],
        "collision": "visualOnly"
    }))
    .unwrap()
}

#[test]
fn malformed_unbounded_and_unsafe_paths_fail_closed() {
    let root = TestProjectRoot::new(CURRENT_PROJECT);
    let mut service = StudioAdapterService::new();

    let malformed: Value =
        serde_json::from_str(&service.handle_json(
            r#"{"type":"readProject","protocolVersion":7,"requestId":"x","extra":true}"#,
        ))
        .unwrap();
    assert_eq!(malformed["error"]["code"], "protocol.malformedRequest");

    let protocol_seven: Value =
        serde_json::from_str(&service.handle_json(
            r#"{"type":"readProject","protocolVersion":7,"requestId":"protocol-seven"}"#,
        ))
        .unwrap();
    assert_eq!(
        protocol_seven["error"]["code"],
        "protocol.unsupportedVersion"
    );

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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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

fn owner_version(response: &Value) -> (String, u64) {
    (
        response["project"]["identity"]["projectHash"]
            .as_str()
            .unwrap()
            .to_string(),
        response["project"]["identity"]["sceneRevision"]
            .as_u64()
            .unwrap(),
    )
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
