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
    let contains_negative_zero = first.match_indices("-0.0").any(|(index, _)| {
        first
            .as_bytes()
            .get(index + "-0.0".len())
            .is_none_or(|next| !next.is_ascii_digit())
    });
    assert!(!contains_negative_zero);
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
    strip_future_inventory_and_pickups(&mut previous);
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
    strip_future_inventory_and_pickups(&mut previous);

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
    strip_future_inventory_and_pickups(&mut previous);
    for entity in previous["scenes"][0]["entities"].as_array_mut().unwrap() {
        entity.as_object_mut().unwrap().remove("light");
    }

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
    for schema_version in [0, 5, 19, 99] {
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
fn schema_seventeen_rejects_future_enemy_combat_and_migrates_when_absent() {
    let mut previous: serde_json::Value = serde_json::from_str(CURRENT_PROJECT).unwrap();
    previous["schemaVersion"] = 17.into();

    let error = decode_project_document(&previous.to_string()).unwrap_err();
    assert_eq!(error.diagnostic().code, diagnostic_code::MIGRATION);
    assert_eq!(error.diagnostic().path, "scenes");

    strip_future_enemy_combat(&mut previous);
    let decoded = decode_project_document(&previous.to_string()).unwrap();
    assert_eq!(decoded.source_schema_version, 17);
    assert_eq!(
        decoded.project.schema_version,
        STORED_PROJECT_SCHEMA_VERSION
    );
    assert!(decoded.was_migrated());
    assert!(decoded.project.scenes.iter().all(|scene| scene
        .entities
        .iter()
        .all(|entity| entity.enemy_combat.is_none())));
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
fn schema_thirteen_multi_scene_weapon_migration_rejects_conflicting_authority() {
    let mut previous: serde_json::Value = serde_json::from_str(CURRENT_PROJECT).unwrap();
    previous["schemaVersion"] = 13.into();
    strip_future_vitality(&mut previous);
    strip_current_weapon_fields(&mut previous);

    let mut second_scene = previous["scenes"][0].clone();
    second_scene["id"] = "scene/second-loading-bay".into();
    second_scene["name"] = "Second Loading Bay".into();
    previous["scenes"]
        .as_array_mut()
        .unwrap()
        .push(second_scene);

    let first_player = previous["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == 1)
        .unwrap();
    first_player["weapon"] = legacy_weapon(60);
    let second_player = previous["scenes"][1]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == 1)
        .unwrap();
    second_player["weapon"] = legacy_weapon(17);

    let error = decode_project_document(&serde_json::to_string(&previous).unwrap()).unwrap_err();

    assert_eq!(error.diagnostic().code, diagnostic_code::MIGRATION);
    assert_eq!(error.diagnostic().path, "scenes[1].entities[0].weapon");
    assert!(error.diagnostic().message.contains("conflicts"));
}

#[test]
fn schema_fifteen_rejects_future_weapon_modes_and_migrates_hitscan_only_content() {
    let mut future: serde_json::Value = serde_json::from_str(CURRENT_PROJECT).unwrap();
    future["schemaVersion"] = 15.into();
    strip_future_progression(&mut future);
    let error = decode_project_document(&future.to_string()).unwrap_err();
    assert_eq!(error.diagnostic().code, diagnostic_code::MIGRATION);
    assert_eq!(error.diagnostic().path, "itemDefinitions");

    for definition in future["itemDefinitions"].as_array_mut().unwrap() {
        let kind = definition["kind"].as_object_mut().unwrap();
        if kind.get("kind").and_then(serde_json::Value::as_str) == Some("weapon") {
            kind.insert("attackMode".to_owned(), "hitscan".into());
            kind.remove("pelletCount");
            kind.remove("spreadDegrees");
        }
    }
    let migrated = decode_project_document(&future.to_string()).unwrap();
    assert_eq!(migrated.source_schema_version, 15);
    assert_eq!(
        migrated.project.schema_version,
        STORED_PROJECT_SCHEMA_VERSION
    );
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

fn strip_future_inventory_and_pickups(project: &mut serde_json::Value) {
    project.as_object_mut().unwrap().remove("itemDefinitions");
    strip_future_enemy_combat(project);
    for scene in project["scenes"].as_array_mut().unwrap() {
        scene["entities"]
            .as_array_mut()
            .unwrap()
            .retain(|entity| entity.get("pickup").is_none() && entity.get("hazard").is_none());
        for entity in scene["entities"].as_array_mut().unwrap() {
            entity.as_object_mut().unwrap().remove("inventory");
            entity.as_object_mut().unwrap().remove("bounds");
            entity.as_object_mut().unwrap().remove("secretRegion");
            entity.as_object_mut().unwrap().remove("levelExit");
            if let Some(door) = entity
                .get_mut("door")
                .and_then(serde_json::Value::as_object_mut)
            {
                door.remove("access");
            }
            if let Some(switch) = entity
                .get_mut("switch")
                .and_then(serde_json::Value::as_object_mut)
            {
                switch.remove("loadingBayInterlock");
            }
            if let Some(health) = entity
                .get_mut("health")
                .and_then(serde_json::Value::as_object_mut)
            {
                health.remove("maxArmor");
                health.remove("armorAbsorptionPercent");
            }
            if let Some(controller) = entity.get_mut("playerController") {
                controller["bindings"]
                    .as_object_mut()
                    .unwrap()
                    .remove("selectWeapon");
            }
        }
    }
}

fn strip_future_vitality(project: &mut serde_json::Value) {
    strip_future_enemy_combat(project);
    for scene in project["scenes"].as_array_mut().unwrap() {
        scene["entities"]
            .as_array_mut()
            .unwrap()
            .retain(|entity| entity.get("hazard").is_none());
        for entity in scene["entities"].as_array_mut().unwrap() {
            entity.as_object_mut().unwrap().remove("secretRegion");
            entity.as_object_mut().unwrap().remove("levelExit");
            if let Some(door) = entity
                .get_mut("door")
                .and_then(serde_json::Value::as_object_mut)
            {
                door.remove("access");
            }
            if let Some(switch) = entity
                .get_mut("switch")
                .and_then(serde_json::Value::as_object_mut)
            {
                switch.remove("loadingBayInterlock");
            }
            if let Some(health) = entity
                .get_mut("health")
                .and_then(serde_json::Value::as_object_mut)
            {
                health.remove("maxArmor");
                health.remove("armorAbsorptionPercent");
            }
        }
    }
}

fn strip_current_weapon_fields(project: &mut serde_json::Value) {
    for definition in project["itemDefinitions"].as_array_mut().unwrap() {
        let kind = definition["kind"].as_object_mut().unwrap();
        if kind.get("kind").and_then(serde_json::Value::as_str) == Some("weapon") {
            for field in [
                "attackMode",
                "pelletCount",
                "spreadDegrees",
                "damage",
                "maxDistance",
                "cooldownTicks",
                "ammunitionCost",
                "muzzleOffset",
                "presentation",
            ] {
                kind.remove(field);
            }
        }
    }
    for scene in project["scenes"].as_array_mut().unwrap() {
        for entity in scene["entities"].as_array_mut().unwrap() {
            if let Some(inventory) = entity
                .get_mut("inventory")
                .and_then(serde_json::Value::as_object_mut)
            {
                inventory.remove("weaponSlots");
            }
            if let Some(bindings) = entity
                .get_mut("playerController")
                .and_then(|controller| controller.get_mut("bindings"))
                .and_then(serde_json::Value::as_object_mut)
            {
                bindings.remove("selectWeapon");
            }
            if let Some(pickup) = entity
                .get_mut("pickup")
                .and_then(serde_json::Value::as_object_mut)
            {
                pickup.remove("starterAmmunition");
            }
        }
    }
}

fn strip_future_progression(project: &mut serde_json::Value) {
    strip_future_enemy_combat(project);
    for scene in project["scenes"].as_array_mut().unwrap() {
        for entity in scene["entities"].as_array_mut().unwrap() {
            entity.as_object_mut().unwrap().remove("secretRegion");
            entity.as_object_mut().unwrap().remove("levelExit");
            if let Some(door) = entity
                .get_mut("door")
                .and_then(serde_json::Value::as_object_mut)
            {
                door.remove("access");
            }
            if let Some(switch) = entity
                .get_mut("switch")
                .and_then(serde_json::Value::as_object_mut)
            {
                switch.remove("loadingBayInterlock");
            }
        }
    }
}

fn strip_future_enemy_combat(project: &mut serde_json::Value) {
    for scene in project["scenes"].as_array_mut().unwrap() {
        for entity in scene["entities"].as_array_mut().unwrap() {
            entity.as_object_mut().unwrap().remove("enemyCombat");
        }
    }
}

fn legacy_weapon(damage: u32) -> serde_json::Value {
    serde_json::json!({
        "damage": damage,
        "maxDistance": 20,
        "cooldownTicks": 2,
        "ammoCapacity": 200,
        "muzzleOffset": [0, 0, 0]
    })
}
