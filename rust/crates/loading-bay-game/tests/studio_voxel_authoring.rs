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
            "protocolVersion": 3,
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
            "protocolVersion": 3,
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

    let erased = send(
        &mut service,
        json!({
            "type": "applyVoxelBrush",
            "protocolVersion": 3,
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
            "protocolVersion": 3,
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
            "protocolVersion": 3,
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
            "protocolVersion": 3,
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
            "protocolVersion": 3,
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
            "protocolVersion": 3,
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
            "protocolVersion": 3,
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
            "protocolVersion": 3,
            "requestId": "prepare",
            "expectedProjectHash": project_hash(&opened),
            "sourceAssetId": "mesh/kenney-wall-a",
            "sourcePath": "fixtures/voxel-conversion/kenney-wall-a.glb",
            "targetAssetId": "voxel-volume/kenney-wall-b",
            "licensePath": "fixtures/voxel-conversion/KENNEY-RETRO-URBAN-KIT-LICENSE.txt",
            "settings": conversion_settings(),
            "maxPreviewSamples": 32
        }),
    );
    assert_eq!(prepared["type"], "voxelConversionPrepared", "{prepared:#}");
    assert_eq!(
        prepared["plan"]["targetAssetId"],
        "voxel-volume/kenney-wall-b"
    );
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
            "protocolVersion": 3,
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
            "protocolVersion": 3,
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
        "fixtures/voxel-conversion/kenney-wall-a.glb"
    );
    assert_eq!(
        converted.provenance.converter,
        "rusty-engine.mesh-to-voxel.v1"
    );
    assert_eq!(converted.content_hash, output_hash);

    let missing_private_plan = send(
        &mut StudioAdapterService::new(),
        json!({
            "type": "applyVoxelConversion",
            "protocolVersion": 3,
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
fn material_asset_and_transformed_instance_lifecycle_is_atomic_and_projected() {
    let root = TestProjectRoot::new();
    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);

    let material = send(
        &mut service,
        json!({
            "type": "upsertMaterial",
            "protocolVersion": 3,
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
            "protocolVersion": 3,
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
            "protocolVersion": 3,
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
            "protocolVersion": 3,
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
            "protocolVersion": 3,
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
            "protocolVersion": 3,
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
            "protocolVersion": 3,
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
        "protocolVersion": 3,
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
            "protocolVersion": 3,
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
