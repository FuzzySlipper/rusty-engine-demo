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
    strip_future_voxel_objects(&mut previous);
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
    strip_future_voxel_objects(&mut previous);
    previous["assets"]
        .as_array_mut()
        .unwrap()
        .retain(|asset| asset.get("material").is_none());
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
    strip_future_voxel_objects(&mut previous);
    strip_future_inventory_and_pickups(&mut previous);
    for entity in previous["scenes"][0]["entities"].as_array_mut().unwrap() {
        entity.as_object_mut().unwrap().remove("light");
        entity.as_object_mut().unwrap().remove("rotation");
        entity.as_object_mut().unwrap().remove("scale");
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
    for schema_version in [0, 5, 25, 99] {
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
fn schema_twenty_three_migrates_without_renderable_transforms_and_rejects_the_future_field() {
    let mut previous: serde_json::Value = serde_json::from_str(CURRENT_PROJECT).unwrap();
    previous["schemaVersion"] = 23.into();
    for scene in previous["scenes"].as_array_mut().unwrap() {
        for entity in scene["entities"].as_array_mut().unwrap() {
            if let Some(renderable) = entity.get_mut("renderable") {
                renderable.as_object_mut().unwrap().remove("localTransform");
            }
        }
    }
    let decoded = decode_project_document(&serde_json::to_string(&previous).unwrap()).unwrap();
    assert_eq!(decoded.source_schema_version, 23);
    assert!(decoded.was_migrated());
    assert!(decoded
        .project
        .scenes
        .iter()
        .flat_map(|scene| &scene.entities)
        .all(|entity| entity
            .renderable
            .as_ref()
            .is_none_or(|renderable| renderable.local_transform.is_none())));

    let first_renderable = previous["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find_map(|entity| entity.get_mut("renderable"))
        .unwrap();
    first_renderable["localTransform"] = serde_json::json!({
        "translation": [0.0, -1.0, 0.0],
        "rotation": [0.0, 0.0, 0.0, 1.0],
        "scale": [1.0, 1.0, 1.0]
    });
    let error = decode_project_document(&serde_json::to_string(&previous).unwrap()).unwrap_err();
    assert_eq!(error.diagnostic().code, diagnostic_code::DECODE);
    assert_eq!(
        error.diagnostic().path,
        "scenes[].entities[].renderable.localTransform"
    );
}

#[test]
fn schema_twenty_two_migrates_without_visual_bindings_and_rejects_the_future_field() {
    let mut previous: serde_json::Value = serde_json::from_str(CURRENT_PROJECT).unwrap();
    previous["schemaVersion"] = 22.into();
    for scene in previous["scenes"].as_array_mut().unwrap() {
        for entity in scene["entities"].as_array_mut().unwrap() {
            if let Some(renderable) = entity
                .get_mut("renderable")
                .and_then(serde_json::Value::as_object_mut)
            {
                renderable.remove("visualBinding");
            }
        }
    }

    let decoded = decode_project_document(&previous.to_string()).unwrap();
    assert_eq!(decoded.source_schema_version, 22);
    assert_eq!(
        decoded.project.schema_version,
        STORED_PROJECT_SCHEMA_VERSION
    );
    assert!(decoded.was_migrated());
    assert!(decoded
        .project
        .scenes
        .iter()
        .all(|scene| scene.entities.iter().all(|entity| entity
            .renderable
            .as_ref()
            .is_none_or(|renderable| renderable.visual_binding.is_none()))));

    previous["scenes"][0]["entities"][2]["renderable"]["visualBinding"] =
        serde_json::json!({"version": 1, "states": []});
    let error = decode_project_document(&previous.to_string()).unwrap_err();
    assert_eq!(error.diagnostic().code, diagnostic_code::MIGRATION);
    assert_eq!(
        error.diagnostic().path,
        "scenes[].entities[].renderable.visualBinding"
    );
}

#[test]
fn schema_twenty_one_migrates_an_absent_proxy_role_and_rejects_the_future_field() {
    let mut previous: serde_json::Value = serde_json::from_str(CURRENT_PROJECT).unwrap();
    previous["schemaVersion"] = 21.into();
    let environment = previous["scenes"][0]["voxelEnvironment"]
        .as_object_mut()
        .unwrap();
    environment.remove("gameplayProxy");

    let decoded = decode_project_document(&previous.to_string()).unwrap();
    assert_eq!(decoded.source_schema_version, 21);
    assert_eq!(
        decoded.project.schema_version,
        STORED_PROJECT_SCHEMA_VERSION
    );
    assert!(decoded.was_migrated());
    assert!(!decoded.project.scenes[0]
        .voxel_environment
        .as_ref()
        .unwrap()
        .gameplay_proxy());

    previous["scenes"][0]["voxelEnvironment"]["gameplayProxy"] = true.into();
    let error = decode_project_document(&previous.to_string()).unwrap_err();
    assert_eq!(error.diagnostic().code, diagnostic_code::UNSUPPORTED_SCHEMA);
    assert_eq!(
        error.diagnostic().path,
        "scenes[].voxelEnvironment.gameplayProxy"
    );
}

#[test]
fn schema_twenty_migrates_without_instances_and_rejects_unowned_instances() {
    let mut previous: serde_json::Value = serde_json::from_str(CURRENT_PROJECT).unwrap();
    previous["schemaVersion"] = 20.into();
    for scene in previous["scenes"].as_array_mut().unwrap() {
        scene
            .as_object_mut()
            .unwrap()
            .remove("voxelObjectInstances");
    }
    let decoded = decode_project_document(&serde_json::to_string(&previous).unwrap()).unwrap();
    assert_eq!(decoded.source_schema_version, 20);
    assert_eq!(
        decoded.project.schema_version,
        STORED_PROJECT_SCHEMA_VERSION
    );

    previous["scenes"][0]["voxelObjectInstances"] = serde_json::json!([{
        "instanceId": "legacy-unowned-object",
        "voxelObjectAssetId": "voxel-object/future",
        "frame": { "kind": "default" },
        "translation": [0, 0, 0],
        "rotation": [0, 0, 0, 1],
        "scale": [1, 1, 1],
        "materialOverrides": []
    }]);
    let error = decode_project_document(&serde_json::to_string(&previous).unwrap()).unwrap_err();
    assert_eq!(error.diagnostic().code, diagnostic_code::MIGRATION);
    assert_eq!(error.diagnostic().path, "scenes");
    assert!(error.diagnostic().message.contains("explicit entity owner"));
}

#[test]
fn schema_eighteen_rejects_future_archetype_fields_and_migrates_when_absent() {
    let mut previous: serde_json::Value = serde_json::from_str(CURRENT_PROJECT).unwrap();
    previous["schemaVersion"] = 18.into();
    strip_future_voxel_objects(&mut previous);

    let error = decode_project_document(&previous.to_string()).unwrap_err();
    assert_eq!(error.diagnostic().code, diagnostic_code::MIGRATION);
    assert_eq!(error.diagnostic().path, "scenes");

    strip_future_enemy_archetypes(&mut previous);
    let decoded = decode_project_document(&previous.to_string()).unwrap();
    assert_eq!(decoded.source_schema_version, 18);
    assert_eq!(
        decoded.project.schema_version,
        STORED_PROJECT_SCHEMA_VERSION
    );
    assert!(decoded.was_migrated());
    assert!(decoded
        .project
        .scenes
        .iter()
        .all(|scene| scene
            .entities
            .iter()
            .all(|entity| entity.defeat_drop.is_none()
                && entity
                    .encounter
                    .as_ref()
                    .is_none_or(|encounter| encounter.activation_radius.is_none()))));
}

#[test]
fn schema_nineteen_migrates_without_objects_and_rejects_future_object_fields() {
    let mut previous: serde_json::Value = serde_json::from_str(CURRENT_PROJECT).unwrap();
    previous["schemaVersion"] = 19.into();
    strip_future_voxel_objects(&mut previous);
    let decoded = decode_project_document(&serde_json::to_string(&previous).unwrap()).unwrap();
    assert_eq!(decoded.source_schema_version, 19);
    assert_eq!(
        decoded.project.schema_version,
        STORED_PROJECT_SCHEMA_VERSION
    );
    assert!(decoded
        .project
        .scenes
        .iter()
        .all(|scene| scene.voxel_object_instances.is_empty()));

    previous["scenes"][0]["voxelObjectInstances"] = serde_json::json!([{
        "ownerEntityId": 1,
        "instanceId": "future-object",
        "voxelObjectAssetId": "voxel-object/future",
        "frame": { "kind": "default" },
        "translation": [0, 0, 0],
        "rotation": [0, 0, 0, 1],
        "scale": [1, 1, 1],
        "materialOverrides": []
    }]);
    let error = decode_project_document(&serde_json::to_string(&previous).unwrap()).unwrap_err();
    assert_eq!(error.diagnostic().code, diagnostic_code::MIGRATION);
    assert!(error.diagnostic().message.contains("schema 19"));
}

#[test]
fn schema_seventeen_rejects_future_enemy_combat_and_migrates_when_absent() {
    let mut previous: serde_json::Value = serde_json::from_str(CURRENT_PROJECT).unwrap();
    previous["schemaVersion"] = 17.into();
    strip_future_voxel_objects(&mut previous);

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
    strip_future_voxel_objects(&mut previous);
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
    strip_future_voxel_objects(&mut future);
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
            for field in [
                "projectileMass",
                "projectileRadius",
                "projectileImpulse",
                "projectileGravityScale",
                "projectileLifetimeTicks",
                "projectileRestitution",
            ] {
                kind.remove(field);
            }
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

fn strip_future_voxel_objects(project: &mut serde_json::Value) {
    project["assets"]
        .as_array_mut()
        .unwrap()
        .retain(|asset| asset.get("voxelObject").is_none());
    for scene in project["scenes"].as_array_mut().unwrap() {
        scene
            .as_object_mut()
            .unwrap()
            .remove("voxelObjectInstances");
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
                "projectileMass",
                "projectileRadius",
                "projectileImpulse",
                "projectileGravityScale",
                "projectileLifetimeTicks",
                "projectileRestitution",
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
    strip_future_enemy_archetypes(project);
    for scene in project["scenes"].as_array_mut().unwrap() {
        for entity in scene["entities"].as_array_mut().unwrap() {
            entity.as_object_mut().unwrap().remove("enemyCombat");
        }
    }
}

fn strip_future_enemy_archetypes(project: &mut serde_json::Value) {
    for scene in project["scenes"].as_array_mut().unwrap() {
        for entity in scene["entities"].as_array_mut().unwrap() {
            entity.as_object_mut().unwrap().remove("defeatDrop");
            if let Some(encounter) = entity
                .get_mut("encounter")
                .and_then(serde_json::Value::as_object_mut)
            {
                encounter.remove("activationRadius");
            }
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
