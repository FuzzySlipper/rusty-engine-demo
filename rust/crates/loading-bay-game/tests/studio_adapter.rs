use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use asset_import::MAX_SOURCE_BYTES;
use loading_bay_game::{
    decode_project_document, encode_project_document, StudioAdapterService,
    MAX_STUDIO_ADAPTER_REQUEST_BYTES, STORED_PROJECT_SCHEMA_VERSION,
};
use serde_json::{json, Value};

const CURRENT_PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const ARC_WARDEN_GLB: &[u8] = include_bytes!("../../../../content/assets/actor-kit/arc-warden.glb");
const BAY_RUSHER_GLB: &[u8] = include_bytes!("../../../../content/assets/actor-kit/bay-rusher.glb");
const PROJECT_FILE: &str = "content/projects/loading-bay.project.json";
const TEST_SCENE_ROOT_ID: u64 = 20_000;
const TEST_SCENE_LIGHT_ID: u64 = 20_001;

#[test]
fn open_uses_engine_owners_and_returns_canonical_projection_and_voxel_readouts() {
    let root = TestProjectRoot::new(CURRENT_PROJECT);
    let mut service = StudioAdapterService::new();
    let described = send(
        &mut service,
        json!({
            "type": "describe",
            "protocolVersion": 12,
            "requestId": "describe",
        }),
    );
    assert_eq!(described["type"], "described");
    assert_eq!(described["adapter"]["adapterVersion"], 12);
    assert!(described["adapter"]["operations"]
        .as_array()
        .is_some_and(|operations| operations
            .iter()
            .any(|operation| operation == "prepareVoxelObjectPlacement")));
    assert_eq!(
        described["adapter"]["entityInspectorContracts"],
        json!([
            {
                "contractId": "rusty.studio.voxel-object-authoring",
                "contractVersion": 1,
            },
            {
                "contractId": "rusty-engine-demo.loading-bay.weapon-authoring",
                "contractVersion": 1,
            },
        ])
    );

    let response = send(
        &mut service,
        json!({
            "type": "openProject",
            "protocolVersion": 12,
            "requestId": "open",
            "root": root.path(),
            "projectFile": PROJECT_FILE,
        }),
    );

    assert_eq!(response["type"], "projectOpened");
    assert_eq!(response["project"]["identity"]["projectId"], "loading-bay");
    assert_eq!(
        response["project"]["inspections"]["catalog"]["entryCount"],
        89
    );
    assert_eq!(
        response["project"]["inspections"]["scene"]["nodeCount"],
        response["project"]["sceneHierarchy"]["nodes"]
            .as_array()
            .unwrap()
            .len()
    );
    assert_eq!(
        response["project"]["inspections"]["entityState"]["entityCount"],
        response["project"]["inspections"]["scene"]["nodeCount"]
    );
    let entity_inspection = response["project"]["inspections"]["entityState"]
        .as_object()
        .expect("entity-state inspection is an object");
    assert!(entity_inspection["capabilities"]
        .as_array()
        .is_some_and(|capabilities| !capabilities.is_empty()));
    assert!(!entity_inspection.contains_key("components"));
    assert_eq!(
        response["project"]["inspections"]["persistence"]["artifactCount"],
        1
    );
    assert_eq!(response["project"]["voxel"]["solidVoxelCount"], 3_931);
    assert!(response["project"].get("loadingBay").is_none());
    let component_references = response["project"]["entityComponents"]
        .as_array()
        .expect("entity component references are an array");
    assert_eq!(
        component_references.len(),
        response["project"]["voxelObjectAuthoring"]["instances"]
            .as_array()
            .unwrap()
            .len()
            + 3
    );
    let voxel_references = component_references
        .iter()
        .filter(|reference| reference["componentTypeId"] == "rusty.voxel-object.instance")
        .collect::<Vec<_>>();
    assert_eq!(
        voxel_references.len(),
        response["project"]["voxelObjectAuthoring"]["instances"]
            .as_array()
            .unwrap()
            .len()
    );
    for proof_owner in 88..=112 {
        assert!(voxel_references
            .iter()
            .any(|reference| reference["ownerEntityId"] == proof_owner));
    }
    let weapon_references = component_references
        .iter()
        .filter(|reference| reference["componentTypeId"] == "rusty-engine-demo.loading-bay.weapon")
        .collect::<Vec<_>>();
    assert_eq!(weapon_references.len(), 3);
    assert_eq!(response["project"]["sceneHierarchy"]["sceneId"], 1);
    assert_eq!(
        response["project"]["sceneHierarchy"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        response["project"]["inspections"]["scene"]["nodeCount"]
    );
    assert_eq!(
        response["project"]["sceneHierarchy"]["nodes"][0]["label"],
        "player"
    );
    assert_eq!(
        response["project"]["sceneHierarchy"]["nodes"][0]["entityId"],
        1
    );
    assert_eq!(
        response["project"]["sceneHierarchy"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|node| node["tags"] == json!(["runtime-derived"]))
            .count(),
        3
    );
    assert_eq!(response["project"]["projection"]["schemaVersion"], 1);
    assert_eq!(
        response["project"]["projection"]["ops"][0]["op"],
        "defineMaterial"
    );
    let projection_ops = response["project"]["projection"]["ops"].as_array().unwrap();
    assert_eq!(
        projection_ops
            .iter()
            .filter(|operation| operation["op"] == "defineMaterial")
            .count(),
        51
    );
    assert_eq!(
        projection_ops
            .iter()
            .filter(|operation| operation["op"] == "defineStaticMesh")
            .count(),
        38
    );
    assert_eq!(
        projection_ops
            .iter()
            .filter(|operation| operation["op"] == "defineAnimatedMesh")
            .count(),
        3
    );
    assert_eq!(
        projection_ops
            .iter()
            .filter(|operation| operation["op"] == "defineVoxelObject")
            .count(),
        9
    );
    assert_eq!(
        projection_ops
            .iter()
            .filter(|operation| operation["op"] == "createVoxelObjectInstance")
            .count(),
        response["project"]["voxelObjectAuthoring"]["instances"]
            .as_array()
            .unwrap()
            .len()
    );
    assert_eq!(
        response["project"]["projectionReadout"]["frameKind"],
        "complete"
    );
    assert_eq!(
        response["project"]["projectionReadout"]["diagnostics"],
        json!([])
    );
    let proof_resource = response["project"]["animatedMeshResources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|resource| resource["asset"] == "mesh-animation/kenney-retro-character-medium")
        .unwrap();
    assert_eq!(proof_resource["clipIds"], json!(["idle", "run", "jump"]));

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
            "protocolVersion": 12,
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
            "protocolVersion": 12,
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

    let mut fresh_service = StudioAdapterService::new();
    let reopened = open(&mut fresh_service, &root);
    assert_eq!(
        reopened["project"]["identity"]["projectHash"],
        response["receipt"]["projectHashAfter"]
    );
    let reopened_project: Value = serde_json::from_str(
        reopened["project"]["canonical"]["projectJson"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let reopened_player = reopened_project["scenes"][0]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["id"] == 1)
        .unwrap();
    assert_eq!(reopened_player["translation"], json!([2.5, 1.5, 3.5]));
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
            "protocolVersion": 12,
            "requestId": "missing",
            "expectedProjectHash": identity["projectHash"],
            "expectedSceneRevision": identity["sceneRevision"],
            "entityId": 99_999,
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
            "protocolVersion": 12,
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
            "protocolVersion": 12,
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
            "protocolVersion": 12,
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
            "protocolVersion": 12,
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
            "protocolVersion": 12,
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
            "protocolVersion": 12,
            "requestId": "create-object",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "object": {
                "entityId": TEST_SCENE_ROOT_ID,
                "name": "authoring-root",
                "parentEntityId": null,
                "childOrder": TEST_SCENE_ROOT_ID,
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
            "protocolVersion": 12,
            "requestId": "create-light",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "object": {
                "entityId": TEST_SCENE_LIGHT_ID,
                "name": "work-light",
                "parentEntityId": TEST_SCENE_ROOT_ID,
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
            "protocolVersion": 12,
            "requestId": "full-transform",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "entityId": TEST_SCENE_ROOT_ID,
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
            "protocolVersion": 12,
            "requestId": "appearance",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "entityId": TEST_SCENE_ROOT_ID,
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
                && operation["instance"]["metadata"]["sourceEntity"] == TEST_SCENE_ROOT_ID
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
            "protocolVersion": 12,
            "requestId": "animated-appearance",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "entityId": TEST_SCENE_ROOT_ID,
            "appearance": {
                "kind": "animatedMesh",
                "asset": "mesh-animation/kenney-retro-character-medium",
                "visible": true,
                "clip": "run"
            }
        }),
    );
    assert_eq!(animated["type"], "projectMutationApplied", "{animated:#}");
    let proof_resource = animated["project"]["animatedMeshResources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|resource| resource["asset"] == "mesh-animation/kenney-retro-character-medium")
        .unwrap();
    assert_eq!(proof_resource["clipIds"], json!(["idle", "run", "jump"]));
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
                && operation["instance"]["metadata"]["sourceEntity"] == TEST_SCENE_ROOT_ID
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
            "protocolVersion": 12,
            "requestId": "invalid-animation-clip",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "entityId": TEST_SCENE_ROOT_ID,
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
            "protocolVersion": 12,
            "requestId": "collision",
            "expectedProjectHash": hash,
            "entityId": TEST_SCENE_ROOT_ID,
            "collision": { "enabled": true, "staticCollider": false }
        }),
    );
    assert_eq!(collision["type"], "projectMutationApplied");
    hash = owner_version(&collision).0;

    let kinematic = send(
        &mut service,
        json!({
            "type": "setEntityKinematic",
            "protocolVersion": 12,
            "requestId": "kinematic",
            "expectedProjectHash": hash,
            "entityId": TEST_SCENE_ROOT_ID,
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
            "protocolVersion": 12,
            "requestId": "rename",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "entityId": TEST_SCENE_ROOT_ID,
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
            "protocolVersion": 12,
            "requestId": "cycle",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "entityId": TEST_SCENE_ROOT_ID,
            "parentEntityId": TEST_SCENE_LIGHT_ID,
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
            "protocolVersion": 12,
            "requestId": "delete-subtree",
            "expectedProjectHash": hash,
            "expectedSceneRevision": revision,
            "entityId": TEST_SCENE_ROOT_ID
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
        .all(|entity| !matches!(entity.id, TEST_SCENE_ROOT_ID | TEST_SCENE_LIGHT_ID)));
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
            "protocolVersion": 12,
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
            "protocolVersion": 12,
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
            "protocolVersion": 12,
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
            "protocolVersion": 12,
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
            "protocolVersion": 12,
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
            "protocolVersion": 12,
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
            "protocolVersion": 12,
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

#[test]
fn animated_glb_import_reimport_and_failures_are_atomic() {
    let mut project: Value = serde_json::from_str(CURRENT_PROJECT).unwrap();
    for entity in project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .filter(|entity| {
            matches!(
                entity["renderable"]["asset"].as_str(),
                Some("mesh-animation/arc-warden" | "mesh-animation/bay-rusher")
            )
        })
    {
        entity["renderable"]["asset"] = "mesh/player-marker".into();
        entity["renderable"]
            .as_object_mut()
            .unwrap()
            .remove("initialClip");
        entity["renderable"]
            .as_object_mut()
            .unwrap()
            .remove("visualBinding");
    }
    project["assets"].as_array_mut().unwrap().retain(|asset| {
        !matches!(
            asset["id"].as_str(),
            Some("mesh-animation/arc-warden" | "mesh-animation/bay-rusher")
        )
    });
    let project = serde_json::to_string(&project).unwrap();
    let root = TestProjectRoot::new(&project);
    let source_path = "content/assets/actor-kit/arc-warden.glb";
    let alternate_path = "content/assets/alternate/arc-warden.glb";
    fs::create_dir_all(root.path().join("content/assets/actor-kit")).unwrap();
    fs::create_dir_all(root.path().join("content/assets/alternate")).unwrap();
    fs::write(root.path().join(source_path), ARC_WARDEN_GLB).unwrap();
    fs::write(root.path().join(alternate_path), ARC_WARDEN_GLB).unwrap();

    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);
    let (project_hash, _) = owner_version(&opened);
    let prepared =
        prepare_asset_import(&mut service, "prepare-animated", &project_hash, source_path);
    assert_eq!(prepared["type"], "assetImportPrepared", "{prepared:#}");
    assert_eq!(prepared["plan"]["hasErrors"], false);
    assert_eq!(prepared["plan"]["meshAssetId"], "mesh-animation/arc-warden");
    assert_eq!(
        prepared["plan"]["generatedAssetIds"],
        json!(["mesh-animation/arc-warden"])
    );
    assert_eq!(
        prepared["plan"]["generatedArtifacts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|artifact| artifact["relativePath"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "arc-warden.animated-mesh.json",
            "arc-warden.catalog.json",
            "arc-warden.glb",
            "arc-warden.import.json",
        ]
    );

    let applied = apply_asset_import(&mut service, "apply-animated", &project_hash, &prepared);
    assert_eq!(applied["type"], "projectMutationApplied", "{applied:#}");
    assert_eq!(applied["receipt"]["assetId"], "mesh-animation/arc-warden");
    assert_eq!(
        applied["project"]["animatedMeshResources"][0]["asset"],
        "mesh-animation/arc-warden"
    );
    assert_eq!(
        applied["project"]["animatedMeshResources"][0]["clipIds"],
        json!(["idle", "run", "jump", "attack", "hit", "death"])
    );
    let canonical: Value = serde_json::from_str(
        applied["project"]["canonical"]["projectJson"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let actor = canonical["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|asset| asset["id"] == "mesh-animation/arc-warden")
        .unwrap();
    assert_eq!(
        actor["catalog"]["sourcePath"],
        "content/assets/actor-kit/arc-warden.glb"
    );
    assert_eq!(
        actor["import"]["sourceHash"],
        "b60d65a65e5077d0153b42e9f4ef02bb189efd7f950ecbd547d3c5f08acdae2d"
    );

    let (project_hash, _) = owner_version(&applied);
    let before_collision = fs::read(root.project_file()).unwrap();
    let collision_plan = prepare_asset_import(
        &mut service,
        "prepare-animated-collision",
        &project_hash,
        alternate_path,
    );
    let collision = apply_asset_import(
        &mut service,
        "apply-animated-collision",
        &project_hash,
        &collision_plan,
    );
    assert_eq!(collision["type"], "rejected", "{collision:#}");
    assert_eq!(collision["error"]["code"], "assetImport.assetCollision");
    assert_eq!(fs::read(root.project_file()).unwrap(), before_collision);

    fs::write(root.path().join(source_path), BAY_RUSHER_GLB).unwrap();
    let drifted = send(
        &mut service,
        json!({
            "type": "readProject",
            "protocolVersion": 12,
            "requestId": "read-animated-drift"
        }),
    );
    let drifted_actor = drifted["project"]["assetBrowser"]["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|asset| asset["assetId"] == "mesh-animation/arc-warden")
        .unwrap();
    assert_eq!(drifted_actor["import"]["status"], "contentChanged");
    let reimport = send(
        &mut service,
        json!({
            "type": "prepareAssetReimport",
            "protocolVersion": 12,
            "requestId": "prepare-animated-reimport",
            "expectedProjectHash": project_hash,
            "assetId": "mesh-animation/arc-warden"
        }),
    );
    assert_eq!(reimport["type"], "assetImportPrepared", "{reimport:#}");
    assert_eq!(reimport["plan"]["hasErrors"], false);
    assert_ne!(reimport["plan"]["reimportKind"], "noop");
    let reapplied = apply_asset_import(
        &mut service,
        "apply-animated-reimport",
        &project_hash,
        &reimport,
    );
    assert_eq!(reapplied["type"], "projectMutationApplied", "{reapplied:#}");
    assert_eq!(
        reapplied["project"]["animatedMeshResources"][0]["clipIds"],
        json!(["idle", "run", "jump", "attack", "hit", "death"])
    );

    let malformed_hash = owner_version(&reapplied).0;
    fs::write(root.path().join(source_path), b"not a GLB").unwrap();
    let before_malformed = fs::read(root.project_file()).unwrap();
    let malformed = send(
        &mut service,
        json!({
            "type": "prepareAssetReimport",
            "protocolVersion": 12,
            "requestId": "prepare-malformed-animated",
            "expectedProjectHash": malformed_hash,
            "assetId": "mesh-animation/arc-warden"
        }),
    );
    assert_eq!(malformed["type"], "assetImportPrepared", "{malformed:#}");
    assert_eq!(malformed["plan"]["hasErrors"], true);
    assert!(malformed["plan"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"] == "invalidContainer"));
    let rejected = apply_asset_import(
        &mut service,
        "apply-malformed-animated",
        &malformed_hash,
        &malformed,
    );
    assert_eq!(rejected["type"], "rejected");
    assert_eq!(rejected["error"]["code"], "assetImport.planHasErrors");
    assert_eq!(fs::read(root.project_file()).unwrap(), before_malformed);

    let external_path = "content/assets/actor-kit/external-image.glb";
    fs::write(
        root.path().join(external_path),
        mutate_glb_json(ARC_WARDEN_GLB, |document| {
            document["images"][0] = json!({ "uri": "missing.png" });
        }),
    )
    .unwrap();
    let external = prepare_asset_import(
        &mut service,
        "prepare-external-animated",
        &malformed_hash,
        external_path,
    );
    assert_eq!(external["plan"]["hasErrors"], true);
    assert!(external["plan"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"] == "externalResource"));

    let duplicate_clip_path = "content/assets/actor-kit/duplicate-clip.glb";
    fs::write(
        root.path().join(duplicate_clip_path),
        mutate_glb_json(ARC_WARDEN_GLB, |document| {
            let first = document["animations"][0]["name"].clone();
            document["animations"][1]["name"] = first;
        }),
    )
    .unwrap();
    let duplicate_clip = prepare_asset_import(
        &mut service,
        "prepare-duplicate-clip",
        &malformed_hash,
        duplicate_clip_path,
    );
    assert_eq!(duplicate_clip["plan"]["hasErrors"], true);
    assert!(duplicate_clip["plan"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"] == "invalidAnimation"));

    let oversized_path = "content/assets/actor-kit/oversized.glb";
    fs::write(
        root.path().join(oversized_path),
        vec![0_u8; MAX_SOURCE_BYTES + 1],
    )
    .unwrap();
    let oversized = send(
        &mut service,
        json!({
            "type": "prepareAssetImport",
            "protocolVersion": 12,
            "requestId": "prepare-oversized-animated",
            "expectedProjectHash": malformed_hash,
            "source": { "scope": "project", "path": oversized_path },
            "settings": {
                "scale": 1.0,
                "generateCollision": false,
                "materialNamespace": null
            }
        }),
    );
    assert_eq!(oversized["type"], "rejected", "{oversized:#}");
    assert_eq!(
        oversized["error"]["code"],
        "assetImport.projectFileRejected"
    );
    assert_eq!(fs::read(root.project_file()).unwrap(), before_malformed);
}

fn prepare_asset_import(
    service: &mut StudioAdapterService,
    request_id: &str,
    expected_project_hash: &str,
    source_path: &str,
) -> Value {
    send(
        service,
        json!({
            "type": "prepareAssetImport",
            "protocolVersion": 12,
            "requestId": request_id,
            "expectedProjectHash": expected_project_hash,
            "source": { "scope": "project", "path": source_path },
            "settings": {
                "scale": 1.0,
                "generateCollision": false,
                "materialNamespace": null
            }
        }),
    )
}

fn apply_asset_import(
    service: &mut StudioAdapterService,
    request_id: &str,
    expected_project_hash: &str,
    prepared: &Value,
) -> Value {
    send(
        service,
        json!({
            "type": "applyAssetImport",
            "protocolVersion": 12,
            "requestId": request_id,
            "expectedProjectHash": expected_project_hash,
            "planId": prepared["plan"]["planId"],
            "expectedPlanHash": prepared["plan"]["planHash"]
        }),
    )
}

fn mutate_glb_json(source: &[u8], mutate: impl FnOnce(&mut Value)) -> Vec<u8> {
    assert_eq!(&source[..4], b"glTF");
    let json_length = u32::from_le_bytes(source[12..16].try_into().unwrap()) as usize;
    assert_eq!(&source[16..20], b"JSON");
    let mut document: Value = serde_json::from_slice(&source[20..20 + json_length]).unwrap();
    mutate(&mut document);
    let mut json_bytes = serde_json::to_vec(&document).unwrap();
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(b' ');
    }

    let mut result = Vec::with_capacity(source.len() + json_bytes.len());
    result.extend_from_slice(b"glTF");
    result.extend_from_slice(&source[4..8]);
    result.extend_from_slice(&[0; 4]);
    result.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    result.extend_from_slice(b"JSON");
    result.extend_from_slice(&json_bytes);
    result.extend_from_slice(&source[20 + json_length..]);
    let total_length = result.len() as u32;
    result[8..12].copy_from_slice(&total_length.to_le_bytes());
    result
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
            "protocolVersion": 12,
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
            "protocolVersion": 12,
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
            "protocolVersion": 12,
            "requestId": "symlink",
            "root": root.path(),
            "projectFile": "content/projects/linked.project.json",
        }),
    );
    assert_eq!(response["type"], "rejected");
    assert_eq!(response["error"]["code"], "path.rejected");
}

#[test]
fn closed_weapon_outlet_replaces_then_core_rereads_across_fresh_process() {
    let root = TestProjectRoot::new(CURRENT_PROJECT);
    let mut service = StudioAdapterService::new();
    let not_open = send(
        &mut service,
        json!({
            "type": "readLoadingBayWeapon",
            "contractVersion": 1,
            "requestId": "not-open",
            "expectedProjectHash": "0000000000000000000000000000000000000000000000000000000000000000",
            "ownerEntityId": 113,
        }),
    );
    assert_eq!(not_open["type"], "loadingBayWeaponRejected");
    assert_eq!(not_open["rejection"]["code"], "projectStoreFailure");

    let opened = open(&mut service, &root);
    let project_hash = opened["project"]["identity"]["projectHash"]
        .as_str()
        .unwrap();
    let weapon_owner = opened["project"]["entityComponents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|reference| reference["componentTypeId"] == "rusty-engine-demo.loading-bay.weapon")
        .and_then(|reference| reference["ownerEntityId"].as_u64())
        .expect("provider-derived weapon owner");
    let read = send(
        &mut service,
        json!({
            "type": "readLoadingBayWeapon",
            "contractVersion": 1,
            "requestId": "read-weapon",
            "expectedProjectHash": project_hash,
            "ownerEntityId": weapon_owner,
        }),
    );
    assert_eq!(read["type"], "loadingBayWeaponRead", "{read:#}");
    assert_eq!(read["weapon"]["itemDefinitionId"], "weapon/arc-pistol");
    assert_eq!(read["weapon"]["definition"]["damage"], 60);

    let mut candidate = read["weapon"]["definition"].clone();
    candidate["damage"] = json!(61);
    let replaced = send(
        &mut service,
        json!({
            "type": "replaceLoadingBayWeapon",
            "contractVersion": 1,
            "requestId": "replace-weapon",
            "expectedProjectHash": project_hash,
            "ownerEntityId": weapon_owner,
            "expectedComponentRevision": read["weapon"]["componentRevision"],
            "candidate": candidate,
        }),
    );
    assert_eq!(replaced["type"], "loadingBayWeaponReplaced", "{replaced:#}");
    assert_eq!(replaced["weapon"]["definition"]["damage"], 61);

    let canonical = send(
        &mut service,
        json!({
            "type": "readProject",
            "protocolVersion": 12,
            "requestId": "canonical-reread",
        }),
    );
    assert_eq!(canonical["type"], "projectRead", "{canonical:#}");
    assert_eq!(
        canonical["project"]["identity"]["projectHash"],
        replaced["receipt"]["projectHashAfter"]
    );
    let canonical_project: Value = serde_json::from_str(
        canonical["project"]["canonical"]["projectJson"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        canonical_project["itemDefinitions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|definition| definition["id"] == "weapon/arc-pistol")
            .unwrap()["kind"]["damage"],
        61
    );

    drop(service);
    let mut fresh_service = StudioAdapterService::new();
    let reopened = open(&mut fresh_service, &root);
    let reopened_hash = reopened["project"]["identity"]["projectHash"]
        .as_str()
        .unwrap();
    let reconstructed = send(
        &mut fresh_service,
        json!({
            "type": "readLoadingBayWeapon",
            "contractVersion": 1,
            "requestId": "reconstructed",
            "expectedProjectHash": reopened_hash,
            "ownerEntityId": weapon_owner,
        }),
    );
    assert_eq!(reconstructed["type"], "loadingBayWeaponRead");
    assert_eq!(reconstructed["weapon"]["definition"]["damage"], 61);

    let malformed = send(
        &mut fresh_service,
        json!({
            "type": "readLoadingBayWeapon",
            "contractVersion": 1,
            "requestId": "malformed",
            "expectedProjectHash": reopened_hash,
            "ownerEntityId": weapon_owner,
            "operation": "genericGet",
        }),
    );
    assert_eq!(malformed["type"], "rejected");
    assert_eq!(malformed["error"]["code"], "protocol.malformedRequest");
}

fn open(service: &mut StudioAdapterService, root: &TestProjectRoot) -> Value {
    let response = send(
        service,
        json!({
            "type": "openProject",
            "protocolVersion": 12,
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
