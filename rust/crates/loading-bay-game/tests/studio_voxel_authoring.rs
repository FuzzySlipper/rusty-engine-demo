use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use loading_bay_game::{decode_project_document, StudioAdapterService};
use serde_json::{json, Value};

const PROJECT: &str = include_str!("../../../../content/projects/converted-wall.project.json");
const PROJECT_FILE: &str = "content/projects/converted-wall.project.json";
const SOURCE: &[u8] = include_bytes!("../../../../fixtures/voxel-conversion/kenney-wall-a.glb");
const LICENSE: &str =
    include_str!("../../../../fixtures/voxel-conversion/KENNEY-RETRO-URBAN-KIT-LICENSE.txt");
const ASSET_ID: &str = "voxel-volume/kenney-wall-a";

#[test]
fn deterministic_environment_materialization_updates_asset_instance_and_named_entities_atomically()
{
    let root = TestProjectRoot::new();
    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);
    let before = fs::read(root.project_file()).unwrap();
    let invalid = send(&mut service, environment_request(&opened, 42, [7, 7, 9]));
    assert_eq!(invalid["type"], "rejected");
    assert_eq!(invalid["error"]["code"], "environment-generation-rejected");
    assert_eq!(fs::read(root.project_file()).unwrap(), before);

    let materialized = send(&mut service, environment_request(&opened, 42, [7, 8, 9]));
    assert_eq!(
        materialized["receipt"]["kind"], "environmentMaterialized",
        "{materialized:#}"
    );
    assert_eq!(materialized["receipt"]["preset"], "tiny-enclosed");
    assert_eq!(materialized["receipt"]["seed"], 42);
    assert!(materialized["receipt"]["voxelCount"].as_u64().unwrap() > 100);
    assert_ne!(
        materialized["receipt"]["playerTranslation"],
        materialized["receipt"]["exitTranslation"]
    );
    assert_eq!(
        materialized["project"]["projectionReadout"]["retainedVoxelInstances"],
        3
    );

    let stored = decode_project_document(&fs::read_to_string(root.project_file()).unwrap())
        .unwrap()
        .project;
    let scene = &stored.scenes[0];
    assert!(stored
        .assets
        .iter()
        .any(|asset| asset.id == "voxel-volume/generated-tunnel"));
    assert!(scene
        .voxel_instances
        .iter()
        .any(|instance| instance.instance_id == "generated-tunnel"));
    assert_eq!(
        scene
            .entities
            .iter()
            .find(|entity| entity.id == 1)
            .unwrap()
            .translation,
        Some([
            materialized["receipt"]["playerTranslation"][0]
                .as_f64()
                .unwrap() as f32,
            materialized["receipt"]["playerTranslation"][1]
                .as_f64()
                .unwrap() as f32,
            materialized["receipt"]["playerTranslation"][2]
                .as_f64()
                .unwrap() as f32,
        ])
    );

    let mut restarted = StudioAdapterService::new();
    let reopened = open(&mut restarted, &root);
    assert_eq!(
        reopened["project"]["projectionReadout"]["retainedVoxelInstances"],
        3
    );
}

#[test]
fn voxel_host_file_open_export_save_as_and_stale_guards_are_atomic() {
    let root = TestProjectRoot::new();
    fs::create_dir(root.path().join("exports")).unwrap();
    let target = root.path().join("exports/wall.avxl.json");
    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);

    let exported = send(
        &mut service,
        json!({
            "type": "exportVoxelAssetFile",
            "protocolVersion": 10,
            "requestId": "export",
            "expectedProjectHash": project_hash(&opened),
            "assetId": ASSET_ID,
            "expectedAssetContentHash": asset_hash(&opened),
            "targetPath": target,
            "expectedTargetSha256": null
        }),
    );
    assert_eq!(exported["type"], "voxelAssetFileExported", "{exported:#}");
    assert_eq!(exported["replacedExisting"], false);
    let exported_bytes = fs::read(&target).unwrap();
    assert_eq!(exported["byteCount"], exported_bytes.len());

    let imported = send(
        &mut service,
        json!({
            "type": "importVoxelAssetFile",
            "protocolVersion": 10,
            "requestId": "import",
            "expectedProjectHash": project_hash(&opened),
            "sourcePath": target,
            "targetAssetId": "voxel-volume/imported-wall"
        }),
    );
    assert_eq!(imported["receipt"]["kind"], "voxelAssetFileImported");
    assert_eq!(imported["receipt"]["sourceAssetId"], ASSET_ID);
    assert_eq!(
        imported["receipt"]["targetAssetId"],
        "voxel-volume/imported-wall"
    );

    let stale = send(
        &mut service,
        json!({
            "type": "exportVoxelAssetFile",
            "protocolVersion": 10,
            "requestId": "stale-export",
            "expectedProjectHash": project_hash(&imported),
            "assetId": ASSET_ID,
            "expectedAssetContentHash": asset_hash(&imported),
            "targetPath": target,
            "expectedTargetSha256": "sha256:stale"
        }),
    );
    assert_eq!(stale["error"]["code"], "hostFile.staleTarget");
    assert_eq!(fs::read(&target).unwrap(), exported_bytes);

    let replaced = send(
        &mut service,
        json!({
            "type": "exportVoxelAssetFile",
            "protocolVersion": 10,
            "requestId": "replace-export",
            "expectedProjectHash": project_hash(&imported),
            "assetId": ASSET_ID,
            "expectedAssetContentHash": asset_hash(&imported),
            "targetPath": target,
            "expectedTargetSha256": exported["sha256"]
        }),
    );
    assert_eq!(replaced["type"], "voxelAssetFileExported", "{replaced:#}");
    assert_eq!(replaced["replacedExisting"], true);
    assert_eq!(fs::read(&target).unwrap(), exported_bytes);

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = root.path().join("exports/wall-link.avxl.json");
        symlink(&target, &link).unwrap();
        let rejected = send(
            &mut service,
            json!({
                "type": "importVoxelAssetFile",
                "protocolVersion": 10,
                "requestId": "symlink-import",
                "expectedProjectHash": project_hash(&imported),
                "sourcePath": link,
                "targetAssetId": "voxel-volume/symlink-wall"
            }),
        );
        assert_eq!(rejected["error"]["code"], "hostFile.symlinkRejected");
    }
}

#[test]
fn typed_primitives_templates_and_history_previews_share_durable_authority() {
    let root = TestProjectRoot::new();
    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);

    let primitive = send(
        &mut service,
        json!({
            "type": "applyVoxelPrimitive",
            "protocolVersion": 10,
            "requestId": "line",
            "expectedProjectHash": project_hash(&opened),
            "assetId": ASSET_ID,
            "expectedAssetContentHash": asset_hash(&opened),
            "request": {
                "primitive": {
                    "kind": "line",
                    "start": [0, 0, 0],
                    "end": [1, 1, 1],
                    "radius": 0
                },
                "material": { "kind": "clear" }
            }
        }),
    );
    assert_eq!(primitive["receipt"]["kind"], "voxelPrimitiveApplied");
    assert_eq!(primitive["receipt"]["primitiveKind"], "line");
    assert_eq!(primitive["receipt"]["changedVoxels"], 2);

    let history = send(
        &mut service,
        json!({
            "type": "queryVoxelHistory",
            "protocolVersion": 10,
            "requestId": "history",
            "expectedProjectHash": project_hash(&primitive),
            "assetId": ASSET_ID,
            "expectedAssetContentHash": asset_hash(&primitive),
            "maxEntries": 16,
            "maxDeltasPerEntry": 16
        }),
    );
    assert_eq!(history["readout"]["kind"], "history");
    assert_eq!(history["readout"]["entryCount"], 1);
    assert_eq!(history["readout"]["entries"][0]["changedVoxels"], 2);
    assert_eq!(
        history["readout"]["entries"][0]["deltas"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let bytes_before_preview = fs::read(root.project_file()).unwrap();
    let preview = send(
        &mut service,
        json!({
            "type": "prepareVoxelHistoryRevert",
            "protocolVersion": 10,
            "requestId": "preview",
            "expectedProjectHash": project_hash(&primitive),
            "assetId": ASSET_ID,
            "expectedAssetContentHash": asset_hash(&primitive),
            "targetCursor": 0,
            "maxSamples": 8
        }),
    );
    assert_eq!(preview["type"], "voxelHistoryRevertPrepared");
    assert_eq!(preview["preview"]["changedVoxels"], 2);
    assert_eq!(preview["preview"]["cursorBefore"], 1);
    assert_eq!(preview["preview"]["cursorAfter"], 0);
    assert_eq!(fs::read(root.project_file()).unwrap(), bytes_before_preview);

    let replacement_preview = send(
        &mut service,
        json!({
            "type": "prepareVoxelHistoryRevert",
            "protocolVersion": 10,
            "requestId": "replacement-preview",
            "expectedProjectHash": project_hash(&primitive),
            "assetId": ASSET_ID,
            "expectedAssetContentHash": asset_hash(&primitive),
            "targetCursor": 0,
            "maxSamples": 8
        }),
    );
    assert_ne!(
        preview["preview"]["previewId"],
        replacement_preview["preview"]["previewId"]
    );
    let replaced_preview = send(
        &mut service,
        json!({
            "type": "discardVoxelHistoryRevert",
            "protocolVersion": 10,
            "requestId": "discard-replaced-preview",
            "previewId": preview["preview"]["previewId"]
        }),
    );
    assert_eq!(replaced_preview["type"], "rejected");
    assert_eq!(
        replaced_preview["error"]["code"],
        "voxel.historyPreviewMissing"
    );
    assert_eq!(fs::read(root.project_file()).unwrap(), bytes_before_preview);

    let reverted = send(
        &mut service,
        json!({
            "type": "applyVoxelHistoryRevert",
            "protocolVersion": 10,
            "requestId": "apply-preview",
            "expectedProjectHash": project_hash(&primitive),
            "previewId": replacement_preview["preview"]["previewId"]
        }),
    );
    assert_eq!(reverted["receipt"]["kind"], "voxelHistoryMoved");
    assert_eq!(voxel_asset(&reverted)["history"]["cursor"], 0);
    assert_eq!(
        voxel_asset(&reverted)["inspection"]["state"]["solidVoxelCount"],
        8
    );

    let template = send(
        &mut service,
        json!({
            "type": "initializeVoxelTemplate",
            "protocolVersion": 10,
            "requestId": "house-template",
            "expectedProjectHash": project_hash(&reverted),
            "assetId": "voxel-volume/studio-house",
            "cellSize": 1.0,
            "chunkSize": 16,
            "materialPalette": [{
                "materialSlot": 7,
                "materialAssetId": "material/wall-lines",
                "displayName": "Wall lines"
            }],
            "request": {
                "template": "house",
                "origin": [20, 0, 0],
                "materialSlot": 7
            }
        }),
    );
    assert_eq!(
        template["receipt"]["kind"], "voxelTemplateInitialized",
        "{template:#}"
    );
    assert_eq!(template["receipt"]["changedVoxels"], 329);
    let house = template["project"]["voxelAuthoring"]["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|asset| asset["inspection"]["assetId"] == "voxel-volume/studio-house")
        .unwrap();
    assert_eq!(house["inspection"]["state"]["solidVoxelCount"], 329);
    assert_eq!(house["history"]["cursor"], 0);

    let mut restarted = StudioAdapterService::new();
    let reopened = open(&mut restarted, &root);
    let house = reopened["project"]["voxelAuthoring"]["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|asset| asset["inspection"]["assetId"] == "voxel-volume/studio-house")
        .unwrap();
    assert_eq!(house["inspection"]["state"]["solidVoxelCount"], 329);
    assert_eq!(house["history"]["persisted"], false);
}

#[test]
fn shared_projection_pick_model_and_durable_history_use_engine_owners() {
    let root = TestProjectRoot::new();
    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);

    assert_eq!(
        opened["project"]["projectionReadout"]["retainedVoxelInstances"],
        2
    );
    assert_eq!(
        opened["project"]["projectionReadout"]["retainedVoxelChunks"],
        2
    );
    assert_eq!(
        opened["project"]["voxelAuthoring"]["materials"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let projection = opened["project"]["projection"].to_string();
    assert!(projection.contains("voxel-instance:wall-primary"));
    assert!(projection.contains("voxel-instance:wall-offset"));
    assert!(projection.contains("voxelChunk"));

    let project_hash = project_hash(&opened).to_string();
    let asset_hash = asset_hash(&opened).to_string();
    let model = send(
        &mut service,
        json!({
            "type": "queryVoxelModel",
            "protocolVersion": 10,
            "requestId": "model",
            "expectedProjectHash": project_hash,
            "assetId": ASSET_ID,
            "expectedAssetContentHash": asset_hash,
            "window": {
                "expectedContentHash": asset_hash,
                "bounds": { "min": [0, 0, 0], "max": [1, 1, 1] },
                "includeEmpty": false,
                "materialFilter": [],
                "maxSamples": 16
            }
        }),
    );
    assert_eq!(model["type"], "voxelRead");
    assert_eq!(model["readout"]["kind"], "model");
    assert_eq!(model["readout"]["info"]["voxelCount"], 8);
    assert_eq!(
        model["readout"]["window"]["samples"]
            .as_array()
            .unwrap()
            .len(),
        8
    );

    let pick = send(
        &mut service,
        json!({
            "type": "validateVoxelPick",
            "protocolVersion": 10,
            "requestId": "pick",
            "expectedProjectHash": project_hash,
            "sceneId": "scene/converted-wall",
            "instanceId": "wall-primary",
            "origin": [4.5, 0.5, 0.0],
            "direction": [0.0, 0.0, 1.0],
            "maxDistance": 20.0,
            "claimedVoxel": [0, 0, 0],
            "claimedFace": "negativeZ"
        }),
    );
    assert_eq!(pick["type"], "voxelPickValidated", "{pick:#}");
    assert_eq!(pick["anchor"]["hitVoxel"], json!([0, 0, 0]));
    assert_eq!(pick["anchor"]["placeVoxel"], json!([0, 0, -1]));
    assert_eq!(pick["anchor"]["authorityHitVoxel"], json!([4, 0, 6]));
    assert_eq!(
        pick["anchor"]["hitPreviewTransform"],
        json!({
            "translation": [4.5, 0.5, 6.5],
            "rotation": [0.0, 0.0, 0.0, 1.0],
            "scale": [1.0, 1.0, 1.0]
        })
    );
    assert_eq!(
        pick["anchor"]["placePreviewTransform"],
        json!({
            "translation": [4.5, 0.5, 5.5],
            "rotation": [0.0, 0.0, 0.0, 1.0],
            "scale": [1.0, 1.0, 1.0]
        })
    );

    let erased = send(
        &mut service,
        json!({
            "type": "applyVoxelBrush",
            "protocolVersion": 10,
            "requestId": "erase",
            "expectedProjectHash": project_hash,
            "assetId": ASSET_ID,
            "expectedAssetContentHash": asset_hash,
            "center": [0, 0, 0],
            "radius": 0,
            "mode": "erase",
            "materialSlot": null
        }),
    );
    assert_eq!(erased["type"], "projectMutationApplied", "{erased:#}");
    assert_eq!(erased["receipt"]["kind"], "voxelBrushApplied");
    assert_eq!(erased["receipt"]["changedVoxels"], 1);
    assert_eq!(
        voxel_asset(&erased)["inspection"]["state"]["solidVoxelCount"],
        7
    );
    assert_eq!(voxel_asset(&erased)["history"]["cursor"], 1);
    assert_eq!(voxel_asset(&erased)["history"]["undoDepth"], 1);
    assert_eq!(voxel_asset(&erased)["history"]["persisted"], true);

    let installed = fs::read(root.project_file()).unwrap();
    let stale = send(
        &mut service,
        json!({
            "type": "applyVoxelBrush",
            "protocolVersion": 10,
            "requestId": "stale",
            "expectedProjectHash": project_hash,
            "assetId": ASSET_ID,
            "expectedAssetContentHash": asset_hash,
            "center": [1, 1, 1],
            "radius": 0,
            "mode": "erase",
            "materialSlot": null
        }),
    );
    assert_eq!(stale["type"], "rejected");
    assert_eq!(stale["error"]["code"], "project.staleHash");
    assert_eq!(fs::read(root.project_file()).unwrap(), installed);

    let mut restarted = StudioAdapterService::new();
    let reopened = open(&mut restarted, &root);
    assert_eq!(
        voxel_asset(&reopened)["inspection"]["state"]["solidVoxelCount"],
        7
    );
    assert_eq!(voxel_asset(&reopened)["history"]["cursor"], 1);
    assert_eq!(voxel_asset(&reopened)["history"]["persisted"], true);

    let undone = history_request(&mut restarted, &reopened, "undoVoxelEdit", "undo", None);
    assert_eq!(
        voxel_asset(&undone)["inspection"]["state"]["solidVoxelCount"],
        8
    );
    assert_eq!(voxel_asset(&undone)["history"]["cursor"], 0);
    assert_eq!(voxel_asset(&undone)["history"]["redoDepth"], 1);

    let redone = history_request(&mut restarted, &undone, "redoVoxelEdit", "redo", None);
    assert_eq!(
        voxel_asset(&redone)["inspection"]["state"]["solidVoxelCount"],
        7
    );
    assert_eq!(voxel_asset(&redone)["history"]["cursor"], 1);

    let reverted = history_request(
        &mut restarted,
        &redone,
        "revertVoxelHistory",
        "revert",
        Some(0),
    );
    assert_eq!(
        voxel_asset(&reverted)["inspection"]["state"]["solidVoxelCount"],
        8
    );
    assert_eq!(voxel_asset(&reverted)["history"]["cursor"], 0);
}

#[test]
fn annotations_are_typed_queryable_editable_exportable_and_hash_guarded() {
    let root = TestProjectRoot::new();
    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);
    let opened_project_hash = project_hash(&opened);
    let data_hash = voxel_asset(&opened)["inspection"]["voxelDataHash"]
        .as_str()
        .unwrap();

    let created = send(
        &mut service,
        json!({
            "type": "createVoxelAnnotationLayer",
            "protocolVersion": 10,
            "requestId": "create-annotation",
            "expectedProjectHash": opened_project_hash,
            "assetId": ASSET_ID,
            "draft": {
                "layerId": "voxel-annotation/wall-semantics",
                "targetVoxelAssetId": ASSET_ID,
                "targetVoxelDataHash": data_hash,
                "targetBounds": { "min": [0, 0, 0], "max": [1, 1, 1] },
                "regions": [{
                    "regionId": "region/entry-cover",
                    "label": "Entry cover",
                    "kind": "cover",
                    "tags": ["cover", "entry"],
                    "bounds": { "min": [0, 0, 0], "max": [1, 0, 0] },
                    "selection": {
                        "sparseRuns": [{ "start": [0, 0, 0], "length": 2 }]
                    }
                }],
                "provenance": []
            }
        }),
    );
    assert_eq!(created["type"], "projectMutationApplied", "{created:#}");
    let initial_layer_hash = created["receipt"]["layerHash"].as_str().unwrap();
    assert_eq!(voxel_asset(&created)["annotations"][0]["regionCount"], 1);
    assert_eq!(
        voxel_asset(&created)["annotations"][0]["assignedCellCount"],
        2
    );

    let queried = send(
        &mut service,
        json!({
            "type": "queryVoxelAnnotation",
            "protocolVersion": 10,
            "requestId": "query-annotation",
            "expectedProjectHash": project_hash(&created),
            "assetId": ASSET_ID,
            "layerId": "voxel-annotation/wall-semantics",
            "query": {
                "expectedLayerHash": initial_layer_hash,
                "mode": { "kind": "cell", "coordinate": [1, 0, 0] },
                "maxResults": 8
            }
        }),
    );
    assert_eq!(queried["type"], "voxelRead");
    assert_eq!(
        queried["readout"]["matchedRegions"][0]["regionId"],
        "region/entry-cover"
    );

    let edited = send(
        &mut service,
        json!({
            "type": "editVoxelAnnotation",
            "protocolVersion": 10,
            "requestId": "edit-annotation",
            "expectedProjectHash": project_hash(&created),
            "assetId": ASSET_ID,
            "layerId": "voxel-annotation/wall-semantics",
            "transaction": {
                "expectedLayerHash": initial_layer_hash,
                "commands": [{
                    "kind": "setLabel",
                    "regionId": "region/entry-cover",
                    "label": "Primary entry cover"
                }]
            }
        }),
    );
    assert_eq!(edited["type"], "projectMutationApplied", "{edited:#}");
    let edited_layer_hash = edited["receipt"]["layerHashAfter"].as_str().unwrap();
    assert_ne!(edited_layer_hash, initial_layer_hash);

    let exported = send(
        &mut service,
        json!({
            "type": "exportVoxelAnnotation",
            "protocolVersion": 10,
            "requestId": "export-annotation",
            "expectedProjectHash": project_hash(&edited),
            "assetId": ASSET_ID,
            "layerId": "voxel-annotation/wall-semantics",
            "expectedLayerHash": edited_layer_hash
        }),
    );
    assert_eq!(exported["type"], "voxelRead");
    assert_eq!(exported["readout"]["kind"], "annotationExport");
    assert_eq!(exported["readout"]["canonicalLayerHash"], edited_layer_hash);
    assert!(exported["readout"]["canonicalJson"]
        .as_str()
        .unwrap()
        .contains("Primary entry cover"));

    let before = fs::read(root.project_file()).unwrap();
    let stale = send(
        &mut service,
        json!({
            "type": "editVoxelAnnotation",
            "protocolVersion": 10,
            "requestId": "stale-annotation",
            "expectedProjectHash": project_hash(&edited),
            "assetId": ASSET_ID,
            "layerId": "voxel-annotation/wall-semantics",
            "transaction": {
                "expectedLayerHash": initial_layer_hash,
                "commands": [{
                    "kind": "setLabel",
                    "regionId": "region/entry-cover",
                    "label": "Stale write"
                }]
            }
        }),
    );
    assert_eq!(stale["type"], "rejected");
    assert_eq!(stale["error"]["code"], "voxel.annotationEditRejected");
    assert_eq!(fs::read(root.project_file()).unwrap(), before);
}

#[test]
fn conversion_plans_stay_private_and_apply_atomically_with_provenance() {
    let root = TestProjectRoot::new();
    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);
    let prepared = send(
        &mut service,
        json!({
            "type": "prepareVoxelConversion",
            "protocolVersion": 10,
            "requestId": "prepare",
            "expectedProjectHash": project_hash(&opened),
            "sourceAssetId": "mesh/kenney-wall-a",
            "source": {
                "scope": "host",
                "path": root.path().join("fixtures/voxel-conversion/kenney-wall-a.glb")
            },
            "targetAssetId": "voxel-volume/kenney-wall-b",
            "license": {
                "scope": "host",
                "path": root.path().join("fixtures/voxel-conversion/KENNEY-RETRO-URBAN-KIT-LICENSE.txt")
            },
            "meshPrimitive": "group/0",
            "settings": conversion_settings_for_group(),
            "maxPreviewSamples": 32
        }),
    );
    assert_eq!(prepared["type"], "voxelConversionPrepared", "{prepared:#}");
    assert_eq!(
        prepared["plan"]["targetAssetId"],
        "voxel-volume/kenney-wall-b"
    );
    assert_eq!(prepared["plan"]["source"]["meshPrimitive"], "group/0");
    assert!(prepared["preview"]["sampleVoxels"].as_array().is_some());
    let plan_id = prepared["plan"]["planId"].as_str().unwrap();
    let plan_hash = prepared["plan"]["planHash"].as_str().unwrap();
    let output_hash = prepared["plan"]["expectedOutputContentHash"]
        .as_str()
        .unwrap();

    let before = fs::read(root.project_file()).unwrap();
    let forged = send(
        &mut service,
        json!({
            "type": "applyVoxelConversion",
            "protocolVersion": 10,
            "requestId": "forged-apply",
            "expectedProjectHash": project_hash(&opened),
            "planId": plan_id,
            "expectedPlanHash": plan_hash,
            "expectedOutputHash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }),
    );
    assert_eq!(forged["type"], "rejected");
    assert_eq!(forged["error"]["code"], "conversion.staleOutput");
    assert_eq!(fs::read(root.project_file()).unwrap(), before);

    let applied = send(
        &mut service,
        json!({
            "type": "applyVoxelConversion",
            "protocolVersion": 10,
            "requestId": "apply",
            "expectedProjectHash": project_hash(&opened),
            "planId": plan_id,
            "expectedPlanHash": plan_hash,
            "expectedOutputHash": output_hash
        }),
    );
    assert_eq!(applied["type"], "projectMutationApplied", "{applied:#}");
    assert_eq!(applied["receipt"]["kind"], "voxelConversionApplied");
    assert_eq!(applied["receipt"]["assetId"], "voxel-volume/kenney-wall-b");

    let persisted = decode_project_document(&fs::read_to_string(root.project_file()).unwrap())
        .unwrap()
        .project;
    let converted = persisted
        .assets
        .iter()
        .find(|asset| asset.id == "voxel-volume/kenney-wall-b")
        .unwrap()
        .voxel_volume
        .as_ref()
        .unwrap();
    assert_eq!(
        converted.provenance.source_path,
        root.path()
            .join("fixtures/voxel-conversion/kenney-wall-a.glb")
            .display()
            .to_string()
    );
    assert_eq!(
        converted.provenance.converter,
        "rusty-engine.mesh-to-voxel.v2"
    );
    assert_eq!(converted.content_hash, output_hash);

    let missing_private_plan = send(
        &mut StudioAdapterService::new(),
        json!({
            "type": "applyVoxelConversion",
            "protocolVersion": 10,
            "requestId": "missing-private-plan",
            "expectedProjectHash": project_hash(&applied),
            "planId": plan_id,
            "expectedPlanHash": plan_hash,
            "expectedOutputHash": output_hash
        }),
    );
    assert_eq!(missing_private_plan["type"], "rejected");
    assert_eq!(missing_private_plan["error"]["code"], "project.notOpen");
}

#[test]
fn conversion_preparation_replaces_the_single_retained_private_plan() {
    let root = TestProjectRoot::new();
    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);

    let prepare = |request_id: &str, target_asset_id: &str| {
        json!({
            "type": "prepareVoxelConversion",
            "protocolVersion": 10,
            "requestId": request_id,
            "expectedProjectHash": project_hash(&opened),
            "sourceAssetId": "mesh/kenney-wall-a",
            "source": {
                "scope": "host",
                "path": root.path().join("fixtures/voxel-conversion/kenney-wall-a.glb")
            },
            "targetAssetId": target_asset_id,
            "license": {
                "scope": "host",
                "path": root.path().join("fixtures/voxel-conversion/KENNEY-RETRO-URBAN-KIT-LICENSE.txt")
            },
            "meshPrimitive": "group/0",
            "settings": conversion_settings_for_group(),
            "maxPreviewSamples": 4
        })
    };

    let first = send(&mut service, prepare("prepare-first", "voxel-volume/first"));
    let second = send(
        &mut service,
        prepare("prepare-second", "voxel-volume/second"),
    );
    assert_eq!(first["type"], "voxelConversionPrepared", "{first:#}");
    assert_eq!(second["type"], "voxelConversionPrepared", "{second:#}");
    assert_ne!(first["plan"]["planId"], second["plan"]["planId"]);

    let bytes_before_rejected_apply = fs::read(root.project_file()).unwrap();
    let replaced = send(
        &mut service,
        json!({
            "type": "applyVoxelConversion",
            "protocolVersion": 10,
            "requestId": "apply-replaced",
            "expectedProjectHash": project_hash(&opened),
            "planId": first["plan"]["planId"],
            "expectedPlanHash": first["plan"]["planHash"],
            "expectedOutputHash": first["preview"]["outputHash"]
        }),
    );
    assert_eq!(replaced["type"], "rejected", "{replaced:#}");
    assert_eq!(replaced["error"]["code"], "conversion.planMissing");
    assert_eq!(
        fs::read(root.project_file()).unwrap(),
        bytes_before_rejected_apply
    );

    let discarded = send(
        &mut service,
        json!({
            "type": "discardVoxelConversion",
            "protocolVersion": 10,
            "requestId": "discard-current",
            "planId": second["plan"]["planId"]
        }),
    );
    assert_eq!(discarded["type"], "voxelConversionDiscarded");
}

#[test]
fn material_asset_and_transformed_instance_lifecycle_is_atomic_and_projected() {
    let root = TestProjectRoot::new();
    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);

    let material = send(
        &mut service,
        json!({
            "type": "upsertMaterial",
            "protocolVersion": 10,
            "requestId": "material",
            "expectedProjectHash": project_hash(&opened),
            "assetId": "material/studio-accent",
            "definition": material_definition([0.95, 0.35, 0.12, 1.0])
        }),
    );
    assert_eq!(material["receipt"]["kind"], "materialUpserted");

    let initialized = send(
        &mut service,
        json!({
            "type": "initializeVoxelAsset",
            "protocolVersion": 10,
            "requestId": "initialize",
            "expectedProjectHash": project_hash(&material),
            "assetId": "voxel-volume/studio-block",
            "cellSize": 0.5,
            "chunkSize": 16,
            "origin": [0, 0, 0],
            "bounds": { "min": [0, 0, 0], "max": [2, 2, 2] },
            "materialPalette": [{
                "materialSlot": 11,
                "materialAssetId": "material/studio-accent",
                "displayName": "Studio accent"
            }],
            "initialMaterialSlot": 11
        }),
    );
    assert_eq!(
        initialized["receipt"]["kind"], "voxelAssetInitialized",
        "{initialized:#}"
    );
    let initialized_hash = initialized["receipt"]["contentHash"].as_str().unwrap();

    let attached = send(
        &mut service,
        json!({
            "type": "attachVoxelInstance",
            "protocolVersion": 10,
            "requestId": "attach",
            "expectedProjectHash": project_hash(&initialized),
            "sceneId": "scene/converted-wall",
            "instance": {
                "instanceId": "studio-block",
                "voxelAssetId": "voxel-volume/studio-block",
                "translation": [1.0, 2.0, 3.0],
                "rotation": [0.0, 0.0, 0.0, 1.0],
                "scale": [1.0, 1.0, 1.0]
            }
        }),
    );
    assert_eq!(
        attached["project"]["projectionReadout"]["retainedVoxelInstances"],
        3
    );
    assert!(attached["project"]["projection"]
        .to_string()
        .contains("voxel-instance:studio-block"));

    let transformed = send(
        &mut service,
        json!({
            "type": "setVoxelInstanceTransform",
            "protocolVersion": 10,
            "requestId": "transform",
            "expectedProjectHash": project_hash(&attached),
            "sceneId": "scene/converted-wall",
            "instanceId": "studio-block",
            "translation": [3.0, 2.0, 1.0],
            "rotation": [0.0, 0.70710677, 0.0, 0.70710677],
            "scale": [1.5, 0.75, 1.25]
        }),
    );
    let instance = transformed["project"]["voxelAuthoring"]["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|readout| readout["instance"]["instanceId"] == "studio-block")
        .unwrap();
    assert_eq!(instance["instance"]["translation"], json!([3.0, 2.0, 1.0]));

    let duplicated = send(
        &mut service,
        json!({
            "type": "duplicateVoxelAsset",
            "protocolVersion": 10,
            "requestId": "duplicate",
            "expectedProjectHash": project_hash(&transformed),
            "sourceAssetId": "voxel-volume/studio-block",
            "expectedSourceContentHash": initialized_hash,
            "targetAssetId": "voxel-volume/studio-block-copy"
        }),
    );
    assert_eq!(duplicated["receipt"]["kind"], "voxelAssetDuplicated");

    let source = duplicated["project"]["voxelAuthoring"]["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|asset| asset["inspection"]["assetId"] == "voxel-volume/studio-block")
        .unwrap();
    let palette = send(
        &mut service,
        json!({
            "type": "replaceVoxelPalette",
            "protocolVersion": 10,
            "requestId": "palette",
            "expectedProjectHash": project_hash(&duplicated),
            "assetId": "voxel-volume/studio-block",
            "expectedAssetContentHash": source["inspection"]["contentHash"],
            "expectedVoxelDataHash": source["inspection"]["voxelDataHash"],
            "replacement": [{
                "materialSlot": 11,
                "materialAssetId": "material/concrete",
                "displayName": "Concrete"
            }]
        }),
    );
    assert_eq!(palette["receipt"]["kind"], "voxelPaletteReplaced");
    assert_eq!(
        palette["receipt"]["voxelDataHash"],
        source["inspection"]["voxelDataHash"]
    );

    let removed = send(
        &mut service,
        json!({
            "type": "removeVoxelInstance",
            "protocolVersion": 10,
            "requestId": "remove",
            "expectedProjectHash": project_hash(&palette),
            "sceneId": "scene/converted-wall",
            "instanceId": "studio-block"
        }),
    );
    assert_eq!(removed["receipt"]["kind"], "voxelInstanceRemoved");
    assert_eq!(
        removed["project"]["projectionReadout"]["retainedVoxelInstances"],
        2
    );

    let persisted = decode_project_document(&fs::read_to_string(root.project_file()).unwrap())
        .unwrap()
        .project;
    assert!(persisted
        .assets
        .iter()
        .any(|asset| asset.id == "voxel-volume/studio-block-copy"));
    assert!(persisted.scenes[0]
        .voxel_instances
        .iter()
        .all(|instance| instance.instance_id != "studio-block"));
}

fn history_request(
    service: &mut StudioAdapterService,
    current: &Value,
    request_type: &str,
    request_id: &str,
    target_cursor: Option<usize>,
) -> Value {
    let mut request = json!({
        "type": request_type,
        "protocolVersion": 10,
        "requestId": request_id,
        "expectedProjectHash": project_hash(current),
        "assetId": ASSET_ID,
        "expectedAssetContentHash": asset_hash(current)
    });
    if let Some(cursor) = target_cursor {
        request["targetCursor"] = cursor.into();
    }
    let response = send(service, request);
    assert_eq!(response["type"], "projectMutationApplied", "{response:#}");
    response
}

fn conversion_settings() -> Value {
    json!({
        "conversion": {
            "resolution": [4, 3, 2],
            "cellSize": 1.0,
            "chunkSize": 16,
            "origin": [4, 0, 6],
            "fitPolicy": "contain",
            "originPolicy": "targetMin",
            "mode": "surface",
            "materialPalette": [
                {
                    "materialSlot": 7,
                    "materialAssetId": "material/wall-lines",
                    "displayName": "Wall lines"
                },
                {
                    "materialSlot": 8,
                    "materialAssetId": "material/concrete",
                    "displayName": "Concrete"
                }
            ],
            "materialMap": [
                {
                    "sourceMaterialSlot": 0,
                    "sourceMaterialName": "wall_lines",
                    "voxelMaterialSlot": 7
                },
                {
                    "sourceMaterialSlot": 1,
                    "sourceMaterialName": "concrete",
                    "voxelMaterialSlot": 8
                }
            ],
            "maxOutputVoxels": 64
        },
        "transform": [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0
        ],
        "materialPolicy": {
            "textureAssets": [],
            "textureBindings": []
        }
    })
}

fn conversion_settings_for_group() -> Value {
    let mut settings = conversion_settings();
    settings["conversion"]["materialPalette"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    settings["conversion"]["materialMap"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    settings
}

fn environment_request(current: &Value, seed: u64, materials: [u16; 3]) -> Value {
    json!({
        "type": "materializeEnvironment",
        "protocolVersion": 10,
        "requestId": format!("environment-{seed}-{}-{}-{}", materials[0], materials[1], materials[2]),
        "expectedProjectHash": project_hash(current),
        "expectedSceneRevision": current["project"]["identity"]["sceneRevision"],
        "sceneId": "scene/converted-wall",
        "preset": "tinyEnclosed",
        "seed": seed,
        "voxelAssetId": "voxel-volume/generated-tunnel",
        "voxelInstanceId": "generated-tunnel",
        "voxelTranslation": [0.0, 0.0, 12.0],
        "playerEntityId": 1,
        "exitEntityId": 3,
        "wallMaterial": materials[0],
        "floorMaterial": materials[1],
        "accentMaterial": materials[2],
        "materialPalette": [
            {
                "materialSlot": 7,
                "materialAssetId": "material/wall-lines",
                "displayName": "Wall"
            },
            {
                "materialSlot": 8,
                "materialAssetId": "material/concrete",
                "displayName": "Floor"
            },
            {
                "materialSlot": 9,
                "materialAssetId": "material/wall-lines",
                "displayName": "Accent"
            }
        ]
    })
}

fn material_definition(color: [f32; 4]) -> Value {
    json!({
        "authority": {
            "solid": true,
            "collidable": true,
            "occludes": true,
            "structuralClass": "structural"
        },
        "style": {
            "color": color,
            "texture": null,
            "textureTint": [1.0, 1.0, 1.0, 1.0],
            "emissionColor": color,
            "roughness": 0.8,
            "emissive": 0.05,
            "uvStrategy": "flat"
        }
    })
}

fn open(service: &mut StudioAdapterService, root: &TestProjectRoot) -> Value {
    let response = send(
        service,
        json!({
            "type": "openProject",
            "protocolVersion": 10,
            "requestId": "open",
            "root": root.path(),
            "projectFile": PROJECT_FILE
        }),
    );
    assert_eq!(response["type"], "projectOpened", "{response:#}");
    response
}

fn send(service: &mut StudioAdapterService, request: Value) -> Value {
    serde_json::from_str(&service.handle_json(&request.to_string())).unwrap()
}

fn project_hash(response: &Value) -> &str {
    response["project"]["identity"]["projectHash"]
        .as_str()
        .unwrap()
}

fn asset_hash(response: &Value) -> &str {
    voxel_asset(response)["inspection"]["contentHash"]
        .as_str()
        .unwrap()
}

fn voxel_asset(response: &Value) -> &Value {
    response["project"]["voxelAuthoring"]["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|asset| asset["inspection"]["assetId"] == ASSET_ID)
        .unwrap()
}

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestProjectRoot(PathBuf);

impl TestProjectRoot {
    fn new() -> Self {
        let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rusty-engine-studio-voxel-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(path.join("content/projects")).unwrap();
        fs::create_dir_all(path.join("fixtures/voxel-conversion")).unwrap();
        fs::write(path.join(PROJECT_FILE), PROJECT).unwrap();
        fs::write(
            path.join("fixtures/voxel-conversion/kenney-wall-a.glb"),
            SOURCE,
        )
        .unwrap();
        fs::write(
            path.join("fixtures/voxel-conversion/KENNEY-RETRO-URBAN-KIT-LICENSE.txt"),
            LICENSE,
        )
        .unwrap();
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
