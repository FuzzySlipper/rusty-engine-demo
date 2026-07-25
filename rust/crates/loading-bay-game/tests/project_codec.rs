use loading_bay_game::{
    admit_stored_project, decode_project_document, diagnostic_code, encode_project_document,
    StoredProject, MIGRATED_V6_PROJECT_ID, MIGRATED_V6_SCENE_ID, STORED_PROJECT_SCHEMA_VERSION,
};

const CURRENT_PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const LEGACY_PROJECT: &str =
    include_str!("../../../../content/generated/encounter-gate.project.json");

#[test]
fn canonical_encode_is_a_byte_stable_fixed_point() {
    let mut document = decode_project_document(CURRENT_PROJECT).unwrap().project;
    document.assets.reverse();
    document.scenes[0].entities.reverse();
    document.scenes[0].entities[0].translation = Some([-0.0, 0.0, -0.0]);

    let first = encode_project_document(&document).unwrap();
    let decoded = decode_project_document(&first).unwrap();
    let second = encode_project_document(&decoded.project).unwrap();

    assert_eq!(first, second);
    assert!(first.ends_with('\n'));
    assert!(first.find("mesh/control-panel").unwrap() < first.find("mesh/player-marker").unwrap());
    assert!(!first.contains("-0.0"));
    assert_eq!(decoded.source_schema_version, STORED_PROJECT_SCHEMA_VERSION);
    assert!(!decoded.was_migrated());
}

#[test]
fn real_schema_six_project_migrates_into_the_current_admitted_shape() {
    let decoded = decode_project_document(LEGACY_PROJECT).unwrap();

    assert_eq!(decoded.source_schema_version, 6);
    assert!(decoded.was_migrated());
    assert_eq!(
        decoded.project.schema_version,
        STORED_PROJECT_SCHEMA_VERSION
    );
    assert_eq!(decoded.project.project_id, MIGRATED_V6_PROJECT_ID);
    assert_eq!(decoded.project.entry_scene, MIGRATED_V6_SCENE_ID);
    assert!(decoded
        .project
        .assets
        .iter()
        .all(|asset| asset.id.starts_with("mesh/")));
    assert!(decoded.project.scenes[0]
        .entities
        .iter()
        .all(|entity| entity
            .renderable
            .as_ref()
            .is_none_or(|renderable| !renderable.asset.starts_with("primitive/"))));
    admit_stored_project(decoded.project).expect("migrated project admits");
}

#[test]
fn schema_seven_project_migrates_without_minting_new_beacon_meaning() {
    let mut previous: serde_json::Value = serde_json::from_str(CURRENT_PROJECT).unwrap();
    previous["schemaVersion"] = 7.into();
    previous["assets"]
        .as_array_mut()
        .unwrap()
        .retain(|asset| asset["id"] != "mesh/extraction-beacon");
    previous["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .retain(|entity| entity["id"] != 7);

    let decoded = decode_project_document(&serde_json::to_string(&previous).unwrap()).unwrap();

    assert_eq!(decoded.source_schema_version, 7);
    assert_eq!(
        decoded.project.schema_version,
        STORED_PROJECT_SCHEMA_VERSION
    );
    assert!(decoded.was_migrated());
    assert!(decoded.project.scenes[0]
        .entities
        .iter()
        .all(|entity| entity.extraction_beacon.is_none()));
    admit_stored_project(decoded.project).unwrap();
}

#[test]
fn schema_eight_project_migrates_with_empty_voxel_authoring_collections() {
    let mut previous: serde_json::Value = serde_json::from_str(CURRENT_PROJECT).unwrap();
    previous["schemaVersion"] = 8.into();

    let decoded = decode_project_document(&serde_json::to_string(&previous).unwrap()).unwrap();

    assert_eq!(decoded.source_schema_version, 8);
    assert_eq!(
        decoded.project.schema_version,
        STORED_PROJECT_SCHEMA_VERSION
    );
    assert!(decoded.was_migrated());
    assert!(decoded
        .project
        .scenes
        .iter()
        .all(|scene| scene.voxel_instances.is_empty()));
    assert!(decoded.project.assets.iter().all(|asset| {
        asset.voxel_edit_history.is_none()
            && asset.voxel_annotations.is_empty()
            && asset.material.is_none()
    }));
    admit_stored_project(decoded.project).unwrap();
}

#[test]
fn schema_nine_project_migrates_with_deterministic_root_order_and_identity_transforms() {
    let mut previous: serde_json::Value = serde_json::from_str(CURRENT_PROJECT).unwrap();
    previous["schemaVersion"] = 9.into();

    let decoded = decode_project_document(&serde_json::to_string(&previous).unwrap()).unwrap();

    assert_eq!(decoded.source_schema_version, 9);
    assert!(decoded.was_migrated());
    assert_eq!(
        decoded.project.schema_version,
        STORED_PROJECT_SCHEMA_VERSION
    );
    for scene in &decoded.project.scenes {
        for (index, entity) in scene.entities.iter().enumerate() {
            assert_eq!(entity.parent, None);
            assert_eq!(entity.child_order, index as u32);
            assert_eq!(entity.rotation, [0.0, 0.0, 0.0, 1.0]);
            assert_eq!(entity.scale, [1.0, 1.0, 1.0]);
            assert_eq!(entity.light, None);
        }
    }
    admit_stored_project(decoded.project).unwrap();
}

#[test]
fn migration_and_current_decode_reject_unknown_versions_fail_closed() {
    for schema_version in [0, 5, 11, 99] {
        let input = format!("{{\"schemaVersion\":{schema_version}}}");
        let error = decode_project_document(&input).unwrap_err();
        assert_eq!(error.diagnostic().code, diagnostic_code::UNSUPPORTED_SCHEMA);
        assert_eq!(error.diagnostic().path, "schemaVersion");
    }

    let error = decode_project_document("{}").unwrap_err();
    assert_eq!(error.diagnostic().code, diagnostic_code::DECODE);
    assert_eq!(error.diagnostic().path, "schemaVersion");
}

#[test]
fn migration_rejects_the_ambiguous_legacy_spatial_shape() {
    let mut legacy: serde_json::Value = serde_json::from_str(LEGACY_PROJECT).unwrap();
    legacy["voxelCollision"] = serde_json::json!({
        "voxelSize": 1,
        "chunkSize": 16,
        "solidVoxels": [[0, 0, 0]]
    });

    let error = decode_project_document(&serde_json::to_string(&legacy).unwrap()).unwrap_err();
    assert_eq!(error.diagnostic().code, diagnostic_code::MIGRATION);
    assert!(error.diagnostic().message.contains("both"));
}

#[test]
fn authored_project_codec_has_no_runtime_state_surface() {
    let project = decode_project_document(CURRENT_PROJECT).unwrap().project;
    let encoded = encode_project_document(&project).unwrap();
    let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();

    for runtime_root in ["tick", "scheduled", "events", "journal"] {
        assert!(
            value.get(runtime_root).is_none(),
            "unexpected {runtime_root}"
        );
    }
    for runtime_field in [
        "current",
        "ammoRemaining",
        "readyAtTick",
        "yawDegrees",
        "pitchDegrees",
    ] {
        assert!(
            !contains_object_key(&value, runtime_field),
            "unexpected {runtime_field}"
        );
    }
}

#[test]
fn canonical_encode_rejects_non_finite_authored_numbers() {
    let mut project: StoredProject = decode_project_document(CURRENT_PROJECT).unwrap().project;
    project.scenes[0].entities[0].translation.as_mut().unwrap()[1] = f32::NAN;

    let error = encode_project_document(&project).unwrap_err();
    assert_eq!(error.diagnostic().code, diagnostic_code::ENCODE);
    assert_eq!(
        error.diagnostic().path,
        "scenes[0].entities[0].translation[1]"
    );
}

fn contains_object_key(value: &serde_json::Value, needle: &str) -> bool {
    match value {
        serde_json::Value::Array(values) => values
            .iter()
            .any(|value| contains_object_key(value, needle)),
        serde_json::Value::Object(values) => {
            values.contains_key(needle)
                || values
                    .values()
                    .any(|value| contains_object_key(value, needle))
        }
        _ => false,
    }
}
