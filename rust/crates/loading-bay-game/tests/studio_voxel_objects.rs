use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use loading_bay_game::{
    admit_stored_project, decode_project_document, diagnostic_code, encode_project_document,
    StoredAsset, StoredVoxelObjectFrameSelection, StoredVoxelObjectInstance, StudioAdapterService,
    MAX_PROJECT_VOXEL_OBJECT_INSTANCES, MAX_PROJECT_VOXEL_OBJECT_RESOLVED_CELLS,
};
use serde_json::{json, Value};
use voxel_asset::{
    with_computed_voxel_object_hashes, VoxelAssetBounds, VoxelAssetMaterialBinding,
    VoxelAssetMaterialMapping, VoxelCoordinateSystem, VoxelFrame, VoxelObjectAnimationFrame,
    VoxelObjectAsset, VoxelObjectClip, VoxelObjectGrid, VoxelObjectProvenance,
    VoxelObjectProvenanceKind, VoxelRepresentation, VoxelRepresentationKind, VoxelSparseRun,
    VOXEL_OBJECT_SCHEMA_VERSION,
};

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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
    let owner_entity_id = attached["project"]["voxelObjectAuthoring"]["instances"][0]
        ["ownerEntityId"]
        .as_u64()
        .unwrap();
    assert!(attached["project"]["sceneHierarchy"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["entityId"] == owner_entity_id));
    assert!(attached["project"]["projection"]["ops"]
        .as_array()
        .unwrap()
        .iter()
        .any(|operation| operation["op"] == "createVoxelObjectInstance"
            && operation["instance"]["metadata"]["sourceEntity"] == owner_entity_id));
    assert!(has_op(
        &attached["project"]["projection"],
        "createVoxelObjectInstance"
    ));

    let stored = decode_project_document(&fs::read_to_string(root.project_file()).unwrap())
        .unwrap()
        .project;
    assert_eq!(stored.schema_version, 21);
    assert_eq!(
        stored
            .assets
            .iter()
            .filter(|asset| asset.voxel_object.is_some())
            .count(),
        1
    );
    assert_eq!(stored.scenes[0].voxel_object_instances.len(), 1);
    assert_eq!(
        stored.scenes[0].voxel_object_instances[0].owner_entity_id,
        owner_entity_id
    );

    let mut missing_owner: Value =
        serde_json::from_str(&encode_project_document(&stored).unwrap()).unwrap();
    missing_owner["scenes"][0]["voxelObjectInstances"][0]
        .as_object_mut()
        .unwrap()
        .remove("ownerEntityId");
    let missing_owner_error =
        decode_project_document(&serde_json::to_string(&missing_owner).unwrap()).unwrap_err();
    assert_eq!(
        missing_owner_error.diagnostic().code,
        diagnostic_code::DECODE
    );

    let mut dangling_owner = stored.clone();
    dangling_owner.scenes[0].voxel_object_instances[0].owner_entity_id = 9_999_999;
    let dangling_owner_error = admit_stored_project(dangling_owner).unwrap_err();
    assert_eq!(
        dangling_owner_error.diagnostic().code,
        diagnostic_code::INVALID_VOXEL_OBJECT_INSTANCE
    );
    assert!(dangling_owner_error
        .diagnostic()
        .path
        .ends_with("ownerEntityId"));

    let mut duplicate_owner = stored.clone();
    let mut second_instance = duplicate_owner.scenes[0].voxel_object_instances[0].clone();
    second_instance.instance_id = "wall-object-duplicate-owner".to_string();
    duplicate_owner.scenes[0]
        .voxel_object_instances
        .push(second_instance);
    let duplicate_owner_error = admit_stored_project(duplicate_owner).unwrap_err();
    assert_eq!(
        duplicate_owner_error.diagnostic().code,
        diagnostic_code::INVALID_VOXEL_OBJECT_INSTANCE
    );
    assert!(duplicate_owner_error
        .diagnostic()
        .message
        .contains("already owns"));

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
fn applied_playback_is_incremental_bounded_transient_and_lifecycle_scoped() {
    let root = TestRoot::static_project();
    let mut project = decode_project_document(STATIC_PROJECT).unwrap().project;
    project
        .assets
        .push(stored_animated_object("voxel-object/two-frame"));
    fs::write(
        root.project_file(),
        encode_project_document(&project).unwrap(),
    )
    .unwrap();

    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);
    let attached = send(
        &mut service,
        json!({
            "type": "attachVoxelObjectInstance",
            "protocolVersion": 9,
            "requestId": "attach-two-frame",
            "expectedProjectHash": project_hash(&opened),
            "sceneId": "scene/converted-wall",
            "instance": {
                "instanceId": "two-frame-object",
                "voxelObjectAssetId": "voxel-object/two-frame",
                "frame": { "kind": "default" },
                "translation": [0.0, 0.0, 0.0],
                "rotation": [0.0, 0.0, 0.0, 1.0],
                "scale": [1.0, 1.0, 1.0],
                "materialOverrides": []
            }
        }),
    );
    assert_eq!(attached["type"], "projectMutationApplied", "{attached:#}");
    let durable_bytes = fs::read(root.project_file()).unwrap();
    let complete_retained_entities =
        attached["project"]["projectionReadout"]["retainedEntities"].clone();

    let scrubbed = preview_applied(
        &mut service,
        &attached,
        "scrub-repeat",
        0,
        json!({
            "kind": "scrub",
            "clipId": "cycle",
            "clipFrame": 1,
            "loopMode": "repeat"
        }),
    );
    assert_eq!(scrubbed["playback"]["status"], "paused");
    assert_eq!(scrubbed["playback"]["runtimeFrame"], 2);
    assert!(has_op(&scrubbed["projection"], "setVoxelObjectFrame"));
    assert!(!has_op(
        &scrubbed["projection"],
        "createVoxelObjectInstance"
    ));
    assert_eq!(
        scrubbed["projectionReadout"]["retainedEntities"],
        complete_retained_entities
    );

    let playing = preview_applied(
        &mut service,
        &attached,
        "play-repeat",
        100,
        json!({ "kind": "play" }),
    );
    assert_eq!(playing["playback"]["status"], "playing");
    assert_eq!(playing["projection"]["ops"], json!([]));
    let repeated = preview_applied(
        &mut service,
        &attached,
        "sample-repeat",
        500_100,
        json!({ "kind": "sample" }),
    );
    assert_eq!(repeated["playback"]["loopMode"], "repeat");
    assert_eq!(repeated["playback"]["runtimeFrame"], 1);
    assert!(has_op(&repeated["projection"], "setVoxelObjectFrame"));
    assert_eq!(
        repeated["projectionReadout"]["retainedEntities"],
        complete_retained_entities
    );

    let ping_pong = preview_applied(
        &mut service,
        &attached,
        "scrub-ping-pong",
        1_000_000,
        json!({
            "kind": "scrub",
            "clipId": "cycle",
            "clipFrame": 0,
            "loopMode": "pingPong"
        }),
    );
    assert_eq!(ping_pong["playback"]["loopMode"], "pingPong");
    let ping_pong_play = preview_applied(
        &mut service,
        &attached,
        "play-ping-pong",
        1_000_000,
        json!({ "kind": "play" }),
    );
    assert_eq!(ping_pong_play["playback"]["status"], "playing");
    let ping_pong_sample = preview_applied(
        &mut service,
        &attached,
        "sample-ping-pong",
        1_500_000,
        json!({ "kind": "sample" }),
    );
    assert_eq!(ping_pong_sample["playback"]["runtimeFrame"], 2);
    assert!(!ping_pong_sample["playback"]["ended"].as_bool().unwrap());

    let once = preview_applied(
        &mut service,
        &attached,
        "scrub-once",
        2_000_000,
        json!({
            "kind": "scrub",
            "clipId": "cycle",
            "clipFrame": 0,
            "loopMode": "once"
        }),
    );
    assert_eq!(once["playback"]["status"], "paused");
    let once_play = preview_applied(
        &mut service,
        &attached,
        "play-once",
        2_000_000,
        json!({ "kind": "play" }),
    );
    assert_eq!(once_play["playback"]["status"], "playing");
    let ended = preview_applied(
        &mut service,
        &attached,
        "sample-once-ended",
        3_000_000,
        json!({ "kind": "sample" }),
    );
    assert_eq!(ended["playback"]["status"], "paused");
    assert!(ended["playback"]["ended"].as_bool().unwrap());
    assert_eq!(ended["playback"]["runtimeFrame"], 2);

    let stopped = preview_applied(
        &mut service,
        &attached,
        "stop",
        3_000_000,
        json!({ "kind": "stop" }),
    );
    assert_eq!(stopped["playback"]["status"], "stopped");
    assert_eq!(stopped["playback"]["runtimeFrame"], 0);
    assert!(has_op(&stopped["projection"], "setVoxelObjectFrame"));
    assert_eq!(fs::read(root.project_file()).unwrap(), durable_bytes);

    let selected = preview_applied(
        &mut service,
        &attached,
        "select-before-reread",
        4_000_000,
        json!({
            "kind": "scrub",
            "clipId": "cycle",
            "clipFrame": 0,
            "loopMode": "once"
        }),
    );
    assert_eq!(selected["playback"]["status"], "paused");
    let reread = send(
        &mut service,
        json!({
            "type": "readProject",
            "protocolVersion": 9,
            "requestId": "reread-clears"
        }),
    );
    assert_eq!(reread["type"], "projectRead");
    let after_clear = preview_applied(
        &mut service,
        &reread,
        "sample-after-reread",
        4_000_001,
        json!({ "kind": "sample" }),
    );
    assert_eq!(after_clear["type"], "rejected", "{after_clear:#}");
    assert_eq!(
        after_clear["error"]["code"],
        "voxelObject.playbackNotSelected"
    );
    assert_eq!(fs::read(root.project_file()).unwrap(), durable_bytes);
}

#[test]
fn aggregate_object_budget_preflights_projects_and_preserves_private_candidate() {
    let root = TestRoot::static_project();
    let mut exact = decode_project_document(STATIC_PROJECT).unwrap().project;
    let half = u32::try_from(MAX_PROJECT_VOXEL_OBJECT_RESOLVED_CELLS / 2).unwrap();
    assert_eq!(u64::from(half) * 2, MAX_PROJECT_VOXEL_OBJECT_RESOLVED_CELLS);
    exact
        .assets
        .push(stored_budget_object("voxel-object/budget-a", half));
    exact
        .assets
        .push(stored_budget_object("voxel-object/budget-b", half));
    let exact_bytes = encode_project_document(&exact).unwrap();
    fs::write(root.project_file(), &exact_bytes).unwrap();

    let mut service = StudioAdapterService::new();
    let opened = open(&mut service, &root);
    assert_eq!(
        opened["project"]["voxelObjectAuthoring"]["assets"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let mut one_over = exact.clone();
    one_over
        .assets
        .push(stored_budget_object("voxel-object/budget-over", 1));
    let admission_error = admit_stored_project(one_over.clone()).unwrap_err();
    assert_eq!(
        admission_error.diagnostic().code,
        diagnostic_code::VOXEL_OBJECT_AGGREGATE_LIMIT
    );

    let mut too_many_instances = exact.clone();
    too_many_instances.scenes[0].voxel_object_instances = (0..=MAX_PROJECT_VOXEL_OBJECT_INSTANCES)
        .map(|index| StoredVoxelObjectInstance {
            owner_entity_id: index + 1,
            instance_id: format!("budget-instance-{index}"),
            voxel_object_asset_id: "voxel-object/budget-a".to_string(),
            frame: StoredVoxelObjectFrameSelection::Default,
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
            material_overrides: Vec::new(),
        })
        .collect();
    let instance_error = admit_stored_project(too_many_instances).unwrap_err();
    assert_eq!(
        instance_error.diagnostic().code,
        diagnostic_code::VOXEL_OBJECT_AGGREGATE_LIMIT
    );
    let mut one_over_bytes = serde_json::to_string_pretty(&one_over).unwrap();
    one_over_bytes.push('\n');
    fs::write(root.project_file(), &one_over_bytes).unwrap();

    let read_rejected = send(
        &mut service,
        json!({
            "type": "readProject",
            "protocolVersion": 9,
            "requestId": "read-one-over"
        }),
    );
    assert_eq!(read_rejected["type"], "rejected", "{read_rejected:#}");
    assert_eq!(
        read_rejected["error"]["code"],
        diagnostic_code::VOXEL_OBJECT_AGGREGATE_LIMIT
    );
    assert_eq!(
        fs::read_to_string(root.project_file()).unwrap(),
        one_over_bytes
    );

    let mut fresh = StudioAdapterService::new();
    let open_rejected = send(
        &mut fresh,
        json!({
            "type": "openProject",
            "protocolVersion": 9,
            "requestId": "open-one-over",
            "root": root.path(),
            "projectFile": root.project_relative()
        }),
    );
    assert_eq!(open_rejected["type"], "rejected", "{open_rejected:#}");
    assert_eq!(
        open_rejected["error"]["code"],
        diagnostic_code::VOXEL_OBJECT_AGGREGATE_LIMIT
    );

    fs::write(root.project_file(), &exact_bytes).unwrap();
    let restored = send(
        &mut service,
        json!({
            "type": "readProject",
            "protocolVersion": 9,
            "requestId": "read-restored"
        }),
    );
    assert_eq!(restored["type"], "projectRead", "{restored:#}");

    let retained = prepare_static(
        &mut service,
        &restored,
        "voxel-object/budget-a",
        "prepare-replacement",
    );
    let rejected_candidate = request_prepare_static(
        &mut service,
        &restored,
        "voxel-object/budget-over",
        "prepare-one-over",
    );
    assert_eq!(
        rejected_candidate["type"], "rejected",
        "{rejected_candidate:#}"
    );
    assert_eq!(
        rejected_candidate["error"]["code"],
        diagnostic_code::VOXEL_OBJECT_AGGREGATE_LIMIT
    );
    assert_eq!(
        fs::read_to_string(root.project_file()).unwrap(),
        exact_bytes
    );

    let retained_preview = send(
        &mut service,
        json!({
            "type": "previewVoxelObjectConversion",
            "protocolVersion": 9,
            "requestId": "preview-retained",
            "planId": retained["plan"]["planId"],
            "expectedPlanHash": retained["plan"]["planHash"],
            "frame": { "kind": "default" },
            "maxPreviewSamples": 32
        }),
    );
    assert_eq!(
        retained_preview["type"], "voxelObjectConversionPreviewed",
        "{retained_preview:#}"
    );
    assert_candidate_projection(&retained_preview);

    let applied = apply_static(
        &mut service,
        &restored,
        retained["plan"]["planId"].as_str().unwrap(),
        retained["plan"]["planHash"].as_str().unwrap(),
        retained["preview"]["outputHash"].as_str().unwrap(),
        "apply-retained",
    );
    assert_eq!(
        applied["receipt"]["kind"], "voxelObjectConversionApplied",
        "{applied:#}"
    );
    let attached = send(
        &mut service,
        json!({
            "type": "attachVoxelObjectInstance",
            "protocolVersion": 9,
            "requestId": "attach-after-budget-preflight",
            "expectedProjectHash": project_hash(&applied),
            "sceneId": "scene/converted-wall",
            "instance": {
                "instanceId": "budget-object",
                "voxelObjectAssetId": "voxel-object/budget-a",
                "frame": { "kind": "default" },
                "translation": [0.0, 0.0, 0.0],
                "rotation": [0.0, 0.0, 0.0, 1.0],
                "scale": [1.0, 1.0, 1.0],
                "materialOverrides": []
            }
        }),
    );
    assert_eq!(
        attached["receipt"]["kind"], "voxelObjectInstanceAttached",
        "{attached:#}"
    );
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
            "protocolVersion": 9,
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
    let owner_entity_id = attached["project"]["voxelObjectAuthoring"]["instances"][0]
        ["ownerEntityId"]
        .as_u64()
        .unwrap();
    assert!(attached["project"]["sceneHierarchy"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["entityId"] == owner_entity_id));

    let project_bytes_before_playback = fs::read(root.project_file()).unwrap();
    let frames = asset["clips"][0]["frames"].as_array().unwrap();
    assert!(!frames.is_empty());
    let scrub_frame = frames.len() - 1;
    let scrubbed_applied = send(
        &mut service,
        json!({
            "type": "previewVoxelObjectInstance",
            "protocolVersion": 9,
            "requestId": "scrub-applied",
            "expectedProjectHash": project_hash(&attached),
            "sceneId": "scene/loading-bay",
            "instanceId": "character-object",
            "nowMicroseconds": 1_000,
            "command": {
                "kind": "scrub",
                "clipId": "idle",
                "clipFrame": scrub_frame,
                "loopMode": "repeat"
            }
        }),
    );
    assert_eq!(
        scrubbed_applied["type"], "voxelObjectInstancePreviewed",
        "{scrubbed_applied:#}"
    );
    assert_eq!(scrubbed_applied["playback"]["status"], "paused");
    assert_eq!(scrubbed_applied["playback"]["clipFrame"], scrub_frame);
    assert_eq!(scrubbed_applied["playback"]["loopMode"], "repeat");
    if scrub_frame == 0 {
        assert_eq!(scrubbed_applied["projection"]["ops"], json!([]));
    } else {
        assert!(has_op(
            &scrubbed_applied["projection"],
            "setVoxelObjectFrame"
        ));
    }
    assert!(!has_op(
        &scrubbed_applied["projection"],
        "createVoxelObjectInstance"
    ));

    let playing = send(
        &mut service,
        json!({
            "type": "previewVoxelObjectInstance",
            "protocolVersion": 9,
            "requestId": "play-applied",
            "expectedProjectHash": project_hash(&attached),
            "sceneId": "scene/loading-bay",
            "instanceId": "character-object",
            "nowMicroseconds": 2_000,
            "command": { "kind": "play" }
        }),
    );
    assert_eq!(playing["playback"]["status"], "playing", "{playing:#}");
    assert_eq!(playing["projection"]["ops"], json!([]));

    let frame_duration = frames[scrub_frame]["durationMicroseconds"]
        .as_u64()
        .unwrap();
    let sampled = send(
        &mut service,
        json!({
            "type": "previewVoxelObjectInstance",
            "protocolVersion": 9,
            "requestId": "sample-applied",
            "expectedProjectHash": project_hash(&attached),
            "sceneId": "scene/loading-bay",
            "instanceId": "character-object",
            "nowMicroseconds": 2_000 + frame_duration,
            "command": { "kind": "sample" }
        }),
    );
    assert_eq!(sampled["playback"]["status"], "playing", "{sampled:#}");
    assert_eq!(sampled["playback"]["loopMode"], "repeat");

    let ping_pong = send(
        &mut service,
        json!({
            "type": "previewVoxelObjectInstance",
            "protocolVersion": 9,
            "requestId": "scrub-ping-pong",
            "expectedProjectHash": project_hash(&attached),
            "sceneId": "scene/loading-bay",
            "instanceId": "character-object",
            "nowMicroseconds": 20_000,
            "command": {
                "kind": "scrub",
                "clipId": "idle",
                "clipFrame": 0,
                "loopMode": "pingPong"
            }
        }),
    );
    assert_eq!(ping_pong["playback"]["loopMode"], "pingPong");
    assert_eq!(ping_pong["playback"]["status"], "paused");

    let stopped = send(
        &mut service,
        json!({
            "type": "previewVoxelObjectInstance",
            "protocolVersion": 9,
            "requestId": "stop-applied",
            "expectedProjectHash": project_hash(&attached),
            "sceneId": "scene/loading-bay",
            "instanceId": "character-object",
            "nowMicroseconds": 20_000,
            "command": { "kind": "stop" }
        }),
    );
    assert_eq!(stopped["playback"]["status"], "stopped", "{stopped:#}");
    assert_eq!(stopped["playback"]["runtimeFrame"], 1);
    assert_eq!(
        fs::read(root.project_file()).unwrap(),
        project_bytes_before_playback
    );

    let selected_again = send(
        &mut service,
        json!({
            "type": "previewVoxelObjectInstance",
            "protocolVersion": 9,
            "requestId": "select-before-read",
            "expectedProjectHash": project_hash(&attached),
            "sceneId": "scene/loading-bay",
            "instanceId": "character-object",
            "nowMicroseconds": 30_000,
            "command": {
                "kind": "scrub",
                "clipId": "idle",
                "clipFrame": 0,
                "loopMode": "once"
            }
        }),
    );
    assert_eq!(selected_again["playback"]["status"], "paused");
    let reread = send(
        &mut service,
        json!({
            "type": "readProject",
            "protocolVersion": 9,
            "requestId": "read-clears-playback"
        }),
    );
    assert_eq!(reread["type"], "projectRead", "{reread:#}");
    let cleared_sample = send(
        &mut service,
        json!({
            "type": "previewVoxelObjectInstance",
            "protocolVersion": 9,
            "requestId": "sample-after-read",
            "expectedProjectHash": project_hash(&reread),
            "sceneId": "scene/loading-bay",
            "instanceId": "character-object",
            "nowMicroseconds": 31_000,
            "command": { "kind": "sample" }
        }),
    );
    assert_eq!(cleared_sample["type"], "rejected", "{cleared_sample:#}");
    assert_eq!(
        cleared_sample["error"]["code"],
        "voxelObject.playbackNotSelected"
    );
    assert_eq!(
        fs::read(root.project_file()).unwrap(),
        project_bytes_before_playback
    );

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
    let response = request_prepare_static(service, current, target_asset_id, request_id);
    assert_eq!(
        response["type"], "voxelObjectConversionPrepared",
        "{response:#}"
    );
    response
}

fn request_prepare_static(
    service: &mut StudioAdapterService,
    current: &Value,
    target_asset_id: &str,
    request_id: &str,
) -> Value {
    send(
        service,
        json!({
            "type": "prepareVoxelObjectConversion",
            "protocolVersion": 9,
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
    )
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
            "protocolVersion": 9,
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

fn preview_applied(
    service: &mut StudioAdapterService,
    current: &Value,
    request_id: &str,
    now_microseconds: u64,
    command: Value,
) -> Value {
    send(
        service,
        json!({
            "type": "previewVoxelObjectInstance",
            "protocolVersion": 9,
            "requestId": request_id,
            "expectedProjectHash": project_hash(current),
            "sceneId": "scene/converted-wall",
            "instanceId": "two-frame-object",
            "nowMicroseconds": now_microseconds,
            "command": command
        }),
    )
}

fn stored_animated_object(asset_id: &str) -> StoredAsset {
    let bounds = VoxelAssetBounds {
        min: [0, 0, 0],
        max: [1, 0, 0],
    };
    let object = with_computed_voxel_object_hashes(VoxelObjectAsset {
        schema_version: VOXEL_OBJECT_SCHEMA_VERSION,
        asset_id: asset_id.to_string(),
        grid: VoxelObjectGrid {
            coordinate_system: VoxelCoordinateSystem::RightHandedYUp,
            cell_size: 1.0,
            chunk_size: 16,
            pivot: [0.0, 0.0, 0.0],
        },
        bounds,
        default_frame: stored_animation_frame(bounds, 0),
        clips: vec![VoxelObjectClip {
            id: "cycle".to_string(),
            name: Some("Cycle".to_string()),
            frames_per_second: 2.0,
            frames: vec![
                VoxelObjectAnimationFrame {
                    duration_seconds: Some(0.5),
                    frame: stored_animation_frame(bounds, 0),
                },
                VoxelObjectAnimationFrame {
                    duration_seconds: Some(0.5),
                    frame: stored_animation_frame(bounds, 1),
                },
            ],
        }],
        default_clip: Some("cycle".to_string()),
        material_palette: vec![VoxelAssetMaterialBinding {
            material_slot: 7,
            material_asset_id: "material/wall-lines".to_string(),
            display_name: Some("Animated fixture".to_string()),
        }],
        material_map: vec![VoxelAssetMaterialMapping {
            source_material_slot: 0,
            source_material_name: Some("animated".to_string()),
            voxel_material_slot: 7,
        }],
        provenance: VoxelObjectProvenance {
            kind: VoxelObjectProvenanceKind::Authored,
            source_path: "generated/two-frame".to_string(),
            source_sha256:
                "sha256:3333333333333333333333333333333333333333333333333333333333333333"
                    .to_string(),
            source_byte_count: 2,
            converter: "rusty-engine-demo-tests".to_string(),
            settings_sha256:
                "sha256:4444444444444444444444444444444444444444444444444444444444444444"
                    .to_string(),
            license_path: None,
            source_clips: Vec::new(),
        },
        content_hash: String::new(),
    })
    .unwrap();
    StoredAsset {
        id: asset_id.to_string(),
        catalog: None,
        static_mesh: None,
        animated_mesh: None,
        import: None,
        voxel_volume: None,
        voxel_object: Some(object),
        voxel_edit_history: None,
        voxel_annotations: Vec::new(),
        material: None,
    }
}

fn stored_animation_frame(_bounds: VoxelAssetBounds, x: i64) -> VoxelFrame {
    VoxelFrame {
        bounds: VoxelAssetBounds {
            min: [x, 0, 0],
            max: [x, 0, 0],
        },
        representation: VoxelRepresentation {
            kind: VoxelRepresentationKind::SparseRuns,
            sparse_runs: vec![VoxelSparseRun {
                start: [x, 0, 0],
                length: 1,
                material_slot: 7,
            }],
        },
        voxel_data_hash: String::new(),
    }
}

fn stored_budget_object(asset_id: &str, cells: u32) -> StoredAsset {
    assert!(cells > 0);
    let bounds = VoxelAssetBounds {
        min: [0, 0, 0],
        max: [i64::from(cells) - 1, 0, 0],
    };
    let object = with_computed_voxel_object_hashes(VoxelObjectAsset {
        schema_version: VOXEL_OBJECT_SCHEMA_VERSION,
        asset_id: asset_id.to_string(),
        grid: VoxelObjectGrid {
            coordinate_system: VoxelCoordinateSystem::RightHandedYUp,
            cell_size: 1.0,
            chunk_size: 16,
            pivot: [0.0, 0.0, 0.0],
        },
        bounds,
        default_frame: VoxelFrame {
            bounds,
            representation: VoxelRepresentation {
                kind: VoxelRepresentationKind::SparseRuns,
                sparse_runs: vec![VoxelSparseRun {
                    start: [0, 0, 0],
                    length: cells,
                    material_slot: 7,
                }],
            },
            voxel_data_hash: String::new(),
        },
        clips: Vec::new(),
        default_clip: None,
        material_palette: vec![VoxelAssetMaterialBinding {
            material_slot: 7,
            material_asset_id: "material/wall-lines".to_string(),
            display_name: Some("Budget fixture".to_string()),
        }],
        material_map: vec![VoxelAssetMaterialMapping {
            source_material_slot: 0,
            source_material_name: Some("budget".to_string()),
            voxel_material_slot: 7,
        }],
        provenance: VoxelObjectProvenance {
            kind: VoxelObjectProvenanceKind::Authored,
            source_path: "generated/aggregate-budget".to_string(),
            source_sha256:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_string(),
            source_byte_count: 1,
            converter: "rusty-engine-demo-tests".to_string(),
            settings_sha256:
                "sha256:2222222222222222222222222222222222222222222222222222222222222222"
                    .to_string(),
            license_path: None,
            source_clips: Vec::new(),
        },
        content_hash: String::new(),
    })
    .unwrap();
    StoredAsset {
        id: asset_id.to_string(),
        catalog: None,
        static_mesh: None,
        animated_mesh: None,
        import: None,
        voxel_volume: None,
        voxel_object: Some(object),
        voxel_edit_history: None,
        voxel_annotations: Vec::new(),
        material: None,
    }
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
            "protocolVersion": 9,
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
