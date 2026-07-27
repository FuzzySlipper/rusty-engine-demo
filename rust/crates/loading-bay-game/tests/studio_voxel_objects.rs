use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use loading_bay_game::{
    admit_stored_project, decode_project_document, diagnostic_code, StudioAdapterService,
};
use serde_json::{json, Value};
use voxel_asset::{with_computed_voxel_object_hashes, VoxelObjectAnimationFrame, VoxelObjectClip};

const STATIC_PROJECT: &str =
    include_str!("../../../../content/projects/converted-wall.project.json");
const STATIC_SOURCE: &[u8] =
    include_bytes!("../../../../fixtures/voxel-conversion/kenney-wall-a.glb");
const STATIC_LICENSE: &str =
    include_str!("../../../../fixtures/voxel-conversion/KENNEY-RETRO-URBAN-KIT-LICENSE.txt");
const ANIMATED_PROJECT: &str =
    include_str!("../../../../content/projects/loading-bay.project.json");
const ANIMATED_SOURCE: &[u8] =
    include_bytes!("../../../../content/assets/kenney-retro-character-medium.glb");

#[test]
fn static_object_candidate_is_private_projected_atomic_and_restart_stable() {
    let root = TestRoot::static_project();
    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);
    assert_eq!(
        opened["project"]["voxelObjectAuthoring"]["assets"],
        json!([])
    );

    let host_inspection = send(
        &mut service,
        json!({
            "type": "inspectVoxelObjectSource",
            "protocolVersion": 7,
            "requestId": "inspect-static-host",
            "expectedProjectHash": project_hash(&opened),
            "sourceKind": "static",
            "sourceAssetId": "mesh/kenney-wall-a",
            "source": {
                "scope": "host",
                "path": root.static_source_file()
            },
            "meshPrimitive": "group/0"
        }),
    );
    assert_eq!(
        host_inspection["type"], "voxelObjectSourceInspected",
        "{host_inspection:#}"
    );

    let inspection = send(
        &mut service,
        json!({
            "type": "inspectVoxelObjectSource",
            "protocolVersion": 7,
            "requestId": "inspect-static",
            "expectedProjectHash": project_hash(&opened),
            "sourceKind": "static",
            "sourceAssetId": "mesh/kenney-wall-a",
            "source": {
                "scope": "project",
                "path": "fixtures/voxel-conversion/kenney-wall-a.glb"
            },
            "meshPrimitive": "group/0"
        }),
    );
    assert_eq!(
        inspection["type"], "voxelObjectSourceInspected",
        "{inspection:#}"
    );
    assert_eq!(inspection["inspection"]["sourceKind"], "static");
    assert!(inspection["inspection"]["metadata"]["groups"]
        .as_array()
        .is_some_and(|groups| groups.len() == 1));
    assert_eq!(
        inspection["inspection"]["diagnostics"][0]["code"],
        "voxelObject.staticSource"
    );

    let replaced = prepare_static(
        &mut service,
        &opened,
        "voxel-object/wall-preview-replaced",
        "prepare-replaced",
    );
    assert_candidate_projection(&replaced);
    let prepared_to_discard = prepare_static(
        &mut service,
        &opened,
        "voxel-object/wall-preview-discard",
        "prepare-discard",
    );
    let stale_preview = send(
        &mut service,
        json!({
            "type": "previewVoxelObjectConversion",
            "protocolVersion": 7,
            "requestId": "preview-replaced",
            "planId": replaced["plan"]["planId"],
            "expectedPlanHash": replaced["plan"]["planHash"],
            "frame": { "kind": "default" },
            "maxPreviewSamples": 32
        }),
    );
    assert_eq!(stale_preview["type"], "rejected", "{stale_preview:#}");
    assert_eq!(stale_preview["error"]["code"], "conversion.planMissing");
    let discarded = send(
        &mut service,
        json!({
            "type": "discardVoxelObjectConversion",
            "protocolVersion": 7,
            "requestId": "discard",
            "planId": prepared_to_discard["plan"]["planId"]
        }),
    );
    assert_eq!(
        discarded["type"], "voxelObjectConversionDiscarded",
        "{discarded:#}"
    );
    assert!(!has_op(&discarded["projection"], "defineVoxelObject"));

    let prepared = prepare_static(
        &mut service,
        &opened,
        "voxel-object/wall-preview",
        "prepare",
    );
    let plan_id = prepared["plan"]["planId"].as_str().unwrap().to_string();
    let plan_hash = prepared["plan"]["planHash"].as_str().unwrap().to_string();
    let output_hash = prepared["preview"]["outputHash"]
        .as_str()
        .unwrap()
        .to_string();
    let previewed = send(
        &mut service,
        json!({
            "type": "previewVoxelObjectConversion",
            "protocolVersion": 7,
            "requestId": "scrub-default",
            "planId": plan_id,
            "expectedPlanHash": plan_hash,
            "frame": { "kind": "default" },
            "maxPreviewSamples": 32
        }),
    );
    assert_eq!(
        previewed["type"], "voxelObjectConversionPreviewed",
        "{previewed:#}"
    );
    assert_candidate_projection(&previewed);

    let project_bytes_before = fs::read(root.project_file()).unwrap();
    fs::write(root.static_source_file(), b"source drift").unwrap();
    let drifted = apply_static(
        &mut service,
        &opened,
        &plan_id,
        &plan_hash,
        &output_hash,
        "apply-drifted",
    );
    assert_eq!(drifted["type"], "rejected");
    assert_eq!(drifted["error"]["code"], "conversion.staleSource");
    assert_eq!(fs::read(root.project_file()).unwrap(), project_bytes_before);
    fs::write(root.static_source_file(), STATIC_SOURCE).unwrap();

    let forged = apply_static(
        &mut service,
        &opened,
        &plan_id,
        &plan_hash,
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "apply-forged",
    );
    assert_eq!(forged["type"], "rejected");
    assert_eq!(forged["error"]["code"], "conversion.staleOutput");
    assert_eq!(fs::read(root.project_file()).unwrap(), project_bytes_before);

    let applied = apply_static(
        &mut service,
        &opened,
        &plan_id,
        &plan_hash,
        &output_hash,
        "apply",
    );
    assert_eq!(
        applied["receipt"]["kind"], "voxelObjectConversionApplied",
        "{applied:#}"
    );
    assert_eq!(
        applied["project"]["voxelObjectAuthoring"]["assets"][0]["contentHash"],
        output_hash
    );
    assert_eq!(
        applied["project"]["voxelObjectAuthoring"]["assets"][0]["provenance"]["licensePath"],
        "fixtures/voxel-conversion/KENNEY-RETRO-URBAN-KIT-LICENSE.txt"
    );
    assert_eq!(
        applied["project"]["voxelObjectAuthoring"]["assets"][0]["provenance"]["sourceClips"],
        json!([])
    );

    let attached = send(
        &mut service,
        json!({
            "type": "attachVoxelObjectInstance",
            "protocolVersion": 7,
            "requestId": "attach",
            "expectedProjectHash": project_hash(&applied),
            "sceneId": "scene/converted-wall",
            "instance": {
                "instanceId": "wall-object",
                "voxelObjectAssetId": "voxel-object/wall-preview",
                "frame": { "kind": "default" },
                "translation": [3.0, 2.0, 1.0],
                "rotation": [0.0, 0.0, 0.0, 1.0],
                "scale": [1.0, 2.0, 1.0],
                "materialOverrides": [{
                    "materialSlot": 7,
                    "materialAssetId": "material/concrete"
                }]
            }
        }),
    );
    assert_eq!(
        attached["receipt"]["kind"], "voxelObjectInstanceAttached",
        "{attached:#}"
    );
    assert_eq!(
        attached["project"]["voxelObjectAuthoring"]["instances"][0]["instance"]["translation"],
        json!([3.0, 2.0, 1.0])
    );
    assert!(has_op(
        &attached["project"]["projection"],
        "createVoxelObjectInstance"
    ));

    let stored = decode_project_document(&fs::read_to_string(root.project_file()).unwrap())
        .unwrap()
        .project;
    assert_eq!(stored.schema_version, 20);
    assert_eq!(
        stored
            .assets
            .iter()
            .filter(|asset| asset.voxel_object.is_some())
            .count(),
        1
    );
    assert_eq!(stored.scenes[0].voxel_object_instances.len(), 1);

    let mut malformed = stored.clone();
    malformed
        .assets
        .iter_mut()
        .find_map(|asset| asset.voxel_object.as_mut())
        .unwrap()
        .content_hash =
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
    let malformed_error = admit_stored_project(malformed).unwrap_err();
    assert_eq!(
        malformed_error.diagnostic().code,
        diagnostic_code::INVALID_VOXEL_OBJECT
    );

    let mut oversized_object = stored
        .assets
        .iter()
        .find_map(|asset| asset.voxel_object.clone())
        .unwrap();
    oversized_object.clips.push(VoxelObjectClip {
        id: "oversized".to_string(),
        name: Some("Oversized".to_string()),
        frames_per_second: 1.0,
        frames: vec![
            VoxelObjectAnimationFrame {
                duration_seconds: Some(1.0),
                frame: oversized_object.default_frame.clone(),
            };
            8_193
        ],
    });
    let oversized_error = with_computed_voxel_object_hashes(oversized_object).unwrap_err();
    assert!(oversized_error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code == "voxelObject.resourceLimit"
            && diagnostic.path == "clips"));

    let mut restarted = StudioAdapterService::new();
    let reopened = open(&mut restarted, &root);
    assert_eq!(
        reopened["project"]["identity"]["projectHash"],
        attached["project"]["identity"]["projectHash"]
    );
    assert_eq!(
        reopened["project"]["voxelObjectAuthoring"],
        attached["project"]["voxelObjectAuthoring"]
    );
    assert!(has_op(
        &reopened["project"]["projection"],
        "createVoxelObjectInstance"
    ));
}

#[test]
fn oversized_object_source_is_rejected_without_project_mutation() {
    let root = TestRoot::static_project();
    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);
    let project_bytes_before = fs::read(root.project_file()).unwrap();
    fs::write(
        root.path().join("fixtures/voxel-conversion/malformed.glb"),
        b"not a glb",
    )
    .unwrap();
    let malformed = send(
        &mut service,
        json!({
            "type": "inspectVoxelObjectSource",
            "protocolVersion": 7,
            "requestId": "inspect-malformed",
            "expectedProjectHash": project_hash(&opened),
            "sourceKind": "static",
            "sourceAssetId": "mesh/kenney-wall-a",
            "source": {
                "scope": "project",
                "path": "fixtures/voxel-conversion/malformed.glb"
            }
        }),
    );
    assert_eq!(malformed["type"], "rejected", "{malformed:#}");
    assert_eq!(malformed["error"]["code"], "conversion.invalidSource");
    assert_eq!(fs::read(root.project_file()).unwrap(), project_bytes_before);

    let oversized = root.path().join("fixtures/voxel-conversion/oversized.glb");
    fs::File::create(&oversized)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();

    let rejected = send(
        &mut service,
        json!({
            "type": "inspectVoxelObjectSource",
            "protocolVersion": 7,
            "requestId": "inspect-oversized",
            "expectedProjectHash": project_hash(&opened),
            "sourceKind": "static",
            "sourceAssetId": "mesh/kenney-wall-a",
            "source": {
                "scope": "project",
                "path": "fixtures/voxel-conversion/oversized.glb"
            }
        }),
    );
    assert_eq!(rejected["type"], "rejected", "{rejected:#}");
    assert_eq!(rejected["error"]["code"], "conversion.projectFileRejected");
    assert_eq!(fs::read(root.project_file()).unwrap(), project_bytes_before);
}

#[test]
fn animated_source_clips_are_inspected_converted_scrubbed_and_persisted() {
    let root = TestRoot::animated_project();
    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);
    let material = send(
        &mut service,
        json!({
            "type": "upsertMaterial",
            "protocolVersion": 7,
            "requestId": "material",
            "expectedProjectHash": project_hash(&opened),
            "assetId": "material/character-voxel",
            "definition": material_definition()
        }),
    );
    assert_eq!(
        material["receipt"]["kind"], "materialUpserted",
        "{material:#}"
    );

    let inspection = send(
        &mut service,
        json!({
            "type": "inspectVoxelObjectSource",
            "protocolVersion": 7,
            "requestId": "inspect-animated",
            "expectedProjectHash": project_hash(&material),
            "sourceKind": "animated",
            "sourceAssetId": "mesh-animation/kenney-retro-character-medium",
            "source": {
                "scope": "project",
                "path": "content/assets/kenney-retro-character-medium.glb"
            }
        }),
    );
    assert_eq!(
        inspection["type"], "voxelObjectSourceInspected",
        "{inspection:#}"
    );
    let clips = inspection["inspection"]["clips"].as_array().unwrap();
    assert!(clips.len() >= 3);
    assert_eq!(
        inspection["inspection"]["diagnostics"][0]["code"],
        "voxelObject.animatedSource"
    );
    let material_map = inspection["inspection"]["metadata"]["materialSlots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|slot| {
            json!({
                "sourceMaterialSlot": slot["sourceMaterialSlot"],
                "sourceMaterialName": slot["sourceMaterialName"],
                "voxelMaterialSlot": 1
            })
        })
        .collect::<Vec<_>>();
    let idle = clips.first().expect("animated source exposes a clip");
    let source_clip_name = idle["name"].as_str().unwrap();
    let end = idle["durationMicroseconds"].as_u64().unwrap().min(250_000);
    let prepared = send(
        &mut service,
        json!({
            "type": "prepareVoxelObjectConversion",
            "protocolVersion": 7,
            "requestId": "prepare-animated",
            "expectedProjectHash": project_hash(&material),
            "sourceKind": "animated",
            "sourceAssetId": "mesh-animation/kenney-retro-character-medium",
            "source": {
                "scope": "project",
                "path": "content/assets/kenney-retro-character-medium.glb"
            },
            "targetAssetId": "voxel-object/character-idle",
            "settings": object_settings(
                "material/character-voxel",
                material_map,
                [6, 6, 6]
            ),
            "clips": [{
                "sourceClipName": source_clip_name,
                "outputClipId": "idle",
                "outputName": "Idle",
                "sampleRateHz": 4,
                "startMicroseconds": 0,
                "endMicroseconds": end,
                "endPolicy": "includeClipEnd"
            }],
            "defaultClip": "idle",
            "frame": { "kind": "clip", "clipId": "idle", "frameIndex": 0 },
            "maxPreviewSamples": 64
        }),
    );
    assert_eq!(
        prepared["type"], "voxelObjectConversionPrepared",
        "{prepared:#}"
    );
    assert!(prepared["preview"]["storedFrameCount"].as_u64().unwrap() >= 1);
    assert_candidate_projection(&prepared);
    let scrubbed = send(
        &mut service,
        json!({
            "type": "previewVoxelObjectConversion",
            "protocolVersion": 7,
            "requestId": "scrub",
            "planId": prepared["plan"]["planId"],
            "expectedPlanHash": prepared["plan"]["planHash"],
            "frame": { "kind": "clip", "clipId": "idle", "frameIndex": 0 },
            "maxPreviewSamples": 64
        }),
    );
    assert_eq!(
        scrubbed["preview"]["selectedFrame"]["selection"]["frameIndex"], 0,
        "{scrubbed:#}"
    );

    let applied = send(
        &mut service,
        json!({
            "type": "applyVoxelObjectConversion",
            "protocolVersion": 7,
            "requestId": "apply-animated",
            "expectedProjectHash": project_hash(&material),
            "planId": prepared["plan"]["planId"],
            "expectedPlanHash": prepared["plan"]["planHash"],
            "expectedOutputHash": prepared["preview"]["outputHash"]
        }),
    );
    assert_eq!(
        applied["receipt"]["kind"], "voxelObjectConversionApplied",
        "{applied:#}"
    );
    let asset = &applied["project"]["voxelObjectAuthoring"]["assets"][0];
    assert_eq!(asset["defaultClip"], "idle");
    assert_eq!(asset["clips"][0]["clipId"], "idle");
    assert_eq!(
        asset["clips"][0]["frames"].as_array().unwrap().len(),
        prepared["preview"]["clips"][0]["storedFrameCount"]
            .as_u64()
            .unwrap() as usize
    );
    assert!(asset["clips"][0]["frames"][0]["durationMicroseconds"]
        .as_u64()
        .is_some_and(|duration| duration > 0));

    let attached = send(
        &mut service,
        json!({
            "type": "attachVoxelObjectInstance",
            "protocolVersion": 7,
            "requestId": "attach-animated",
            "expectedProjectHash": project_hash(&applied),
            "sceneId": "scene/loading-bay",
            "instance": {
                "instanceId": "character-object",
                "voxelObjectAssetId": "voxel-object/character-idle",
                "frame": { "kind": "clip", "clipId": "idle", "frameIndex": 0 },
                "translation": [4.0, 1.0, 8.0],
                "rotation": [0.0, 0.0, 0.0, 1.0],
                "scale": [0.5, 0.5, 0.5],
                "materialOverrides": []
            }
        }),
    );
    assert_eq!(
        attached["receipt"]["kind"], "voxelObjectInstanceAttached",
        "{attached:#}"
    );
    assert_eq!(attached["receipt"]["frameKind"], "clip");
    assert_eq!(
        attached["project"]["voxelObjectAuthoring"]["instances"][0]["instance"]
            ["materialOverrides"],
        json!([])
    );
    assert_candidate_projection(&json!({
        "projection": attached["project"]["projection"],
        "projectionReadout": attached["project"]["projectionReadout"]
    }));

    let mut restarted = StudioAdapterService::new();
    let reopened = open(&mut restarted, &root);
    assert_eq!(
        reopened["project"]["voxelObjectAuthoring"]["assets"][0],
        *asset
    );
    assert_eq!(
        reopened["project"]["voxelObjectAuthoring"],
        attached["project"]["voxelObjectAuthoring"]
    );
}

fn prepare_static(
    service: &mut StudioAdapterService,
    current: &Value,
    target_asset_id: &str,
    request_id: &str,
) -> Value {
    let response = send(
        service,
        json!({
            "type": "prepareVoxelObjectConversion",
            "protocolVersion": 7,
            "requestId": request_id,
            "expectedProjectHash": project_hash(current),
            "sourceKind": "static",
            "sourceAssetId": "mesh/kenney-wall-a",
            "source": {
                "scope": "project",
                "path": "fixtures/voxel-conversion/kenney-wall-a.glb"
            },
            "targetAssetId": target_asset_id,
            "license": {
                "scope": "project",
                "path": "fixtures/voxel-conversion/KENNEY-RETRO-URBAN-KIT-LICENSE.txt"
            },
            "meshPrimitive": "group/0",
            "settings": object_settings(
                "material/wall-lines",
                vec![json!({
                    "sourceMaterialSlot": 0,
                    "sourceMaterialName": "wall_lines",
                    "voxelMaterialSlot": 7
                })],
                [4, 3, 2]
            ),
            "clips": [],
            "frame": { "kind": "default" },
            "maxPreviewSamples": 32
        }),
    );
    assert_eq!(
        response["type"], "voxelObjectConversionPrepared",
        "{response:#}"
    );
    response
}

fn apply_static(
    service: &mut StudioAdapterService,
    current: &Value,
    plan_id: &str,
    plan_hash: &str,
    output_hash: &str,
    request_id: &str,
) -> Value {
    send(
        service,
        json!({
            "type": "applyVoxelObjectConversion",
            "protocolVersion": 7,
            "requestId": request_id,
            "expectedProjectHash": project_hash(current),
            "planId": plan_id,
            "expectedPlanHash": plan_hash,
            "expectedOutputHash": output_hash
        }),
    )
}

fn object_settings(material: &str, material_map: Vec<Value>, resolution: [u32; 3]) -> Value {
    json!({
        "mesh": {
            "conversion": {
                "resolution": resolution,
                "cellSize": 1.0,
                "chunkSize": 16,
                "origin": [0, 0, 0],
                "fitPolicy": "contain",
                "originPolicy": "sourceOrigin",
                "mode": "surface",
                "materialPalette": [{
                    "materialSlot": if material.ends_with("wall-lines") { 7 } else { 1 },
                    "materialAssetId": material,
                    "displayName": "Object material"
                }],
                "materialMap": material_map,
                "maxOutputVoxels": 512
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
        },
        "pivot": [0.0, 0.0, 0.0],
        "anchorPolicy": { "kind": "preserveSourceSpace" }
    })
}

fn material_definition() -> Value {
    json!({
        "authority": {
            "solid": true,
            "collidable": true,
            "occludes": true,
            "structuralClass": "solid"
        },
        "style": {
            "color": [0.25, 0.8, 0.45, 1.0],
            "texture": null,
            "textureTint": [1.0, 1.0, 1.0, 1.0],
            "emissionColor": [0.0, 0.0, 0.0, 1.0],
            "roughness": 0.7,
            "emissive": 0.0,
            "uvStrategy": "flat"
        }
    })
}

fn assert_candidate_projection(response: &Value) {
    assert!(has_op(&response["projection"], "defineVoxelObject"));
    assert!(has_op(&response["projection"], "createVoxelObjectInstance"));
    assert_eq!(response["projectionReadout"]["frameKind"], "complete");
}

fn has_op(frame: &Value, operation: &str) -> bool {
    frame["ops"]
        .as_array()
        .is_some_and(|ops| ops.iter().any(|entry| entry["op"] == operation))
}

fn open(service: &mut StudioAdapterService, root: &TestRoot) -> Value {
    let response = send(
        service,
        json!({
            "type": "openProject",
            "protocolVersion": 7,
            "requestId": "open",
            "root": root.path(),
            "projectFile": root.project_relative()
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

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestRoot {
    path: PathBuf,
    project_relative: &'static str,
}

impl TestRoot {
    fn static_project() -> Self {
        let root = Self::create("static", "content/projects/converted-wall.project.json");
        fs::create_dir_all(root.path.join("fixtures/voxel-conversion")).unwrap();
        fs::write(root.project_file(), STATIC_PROJECT).unwrap();
        fs::write(root.static_source_file(), STATIC_SOURCE).unwrap();
        fs::write(
            root.path
                .join("fixtures/voxel-conversion/KENNEY-RETRO-URBAN-KIT-LICENSE.txt"),
            STATIC_LICENSE,
        )
        .unwrap();
        root
    }

    fn animated_project() -> Self {
        let root = Self::create("animated", "content/projects/loading-bay.project.json");
        fs::create_dir_all(root.path.join("content/assets")).unwrap();
        fs::write(root.project_file(), ANIMATED_PROJECT).unwrap();
        fs::write(
            root.path
                .join("content/assets/kenney-retro-character-medium.glb"),
            ANIMATED_SOURCE,
        )
        .unwrap();
        root
    }

    fn create(label: &str, project_relative: &'static str) -> Self {
        let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rusty-engine-studio-voxel-object-{label}-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(
            path.join(
                Path::new(project_relative)
                    .parent()
                    .expect("project has a parent"),
            ),
        )
        .unwrap();
        Self {
            path,
            project_relative,
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn project_relative(&self) -> &str {
        self.project_relative
    }

    fn project_file(&self) -> PathBuf {
        self.path.join(self.project_relative)
    }

    fn static_source_file(&self) -> PathBuf {
        self.path
            .join("fixtures/voxel-conversion/kenney-wall-a.glb")
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}
