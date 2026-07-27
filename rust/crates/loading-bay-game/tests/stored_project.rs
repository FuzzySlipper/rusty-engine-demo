use core_ids::EntityId;
use loading_bay_game::{
    decode_game_snapshot, decode_stored_project, diagnostic_code, encode_game_snapshot,
    GameRuntime, ProjectDiagnostic, ResolvedPlayerAction, RuntimeError, StoredItemKind,
};

const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");

#[test]
fn hand_authored_project_is_static_typed_multi_family_content() {
    let project = decode_stored_project(PROJECT).expect("stored project");
    assert_eq!(project.project_id, "loading-bay");
    assert_eq!(project.entry_scene, "scene/loading-bay");
    assert_eq!(project.assets.len(), 15);
    assert!(project
        .assets
        .iter()
        .any(|asset| asset.id.as_str() == "mesh-animation/kenney-retro-character-medium"));
    assert_eq!(project.scenes.len(), 1);

    let entities = &project.scenes[0].entities;
    assert!(entities
        .iter()
        .any(|entity| entity.player_controller.is_some()));
    assert!(entities.iter().all(|entity| entity.weapon.is_none()));
    assert!(project
        .item_definitions
        .iter()
        .any(|definition| matches!(definition.kind, StoredItemKind::Weapon { .. })));
    assert_eq!(
        entities
            .iter()
            .find_map(|entity| entity.inventory.as_ref())
            .unwrap()
            .weapon_slots
            .len(),
        3
    );
    assert!(entities.iter().any(|entity| entity.navigation.is_some()));
    assert!(entities.iter().any(|entity| entity.health.is_some()));
    assert!(entities.iter().any(|entity| entity.encounter.is_some()));
    assert_eq!(
        entities
            .iter()
            .filter(|entity| entity.defeat_drop.is_some())
            .count(),
        8
    );
    assert!(entities.iter().any(|entity| entity.door.is_some()));
    assert!(entities.iter().any(|entity| entity.switch.is_some()));
    assert!(entities
        .iter()
        .any(|entity| entity.extraction_beacon.is_some()));
    assert_eq!(
        entities
            .iter()
            .filter(|entity| entity.pickup.is_some())
            .count(),
        16
    );
    assert_eq!(
        entities
            .iter()
            .filter(|entity| entity.hazard.is_some())
            .count(),
        1
    );
}

#[test]
fn invalid_asset_identity_reports_the_exact_catalog_path() {
    let invalid = mutate(|project| project["assets"][0]["id"] = "primitive/panel".into());
    let error = decode_stored_project(&invalid).unwrap_err();

    assert_eq!(error.diagnostic().code, diagnostic_code::INVALID_ASSET_ID);
    assert_eq!(error.diagnostic().path, "assets[0].id");
    assert!(error.diagnostic().message.contains("unknown kind"));
}

#[test]
fn duplicate_asset_identity_reports_both_declarations() {
    let invalid = mutate(|project| {
        project["assets"][1]["id"] = project["assets"][0]["id"].clone();
    });
    let error = decode_stored_project(&invalid).unwrap_err();

    assert_eq!(error.diagnostic().code, diagnostic_code::DUPLICATE_ASSET);
    assert_eq!(error.diagnostic().path, "assets[1].id");
    assert!(error.diagnostic().message.contains("assets[0].id"));
}

#[test]
fn entry_scene_requires_a_scene_identity_and_declared_document() {
    let wrong_kind = mutate(|project| project["entryScene"] = "mesh/player-marker".into());
    let wrong_kind = decode_stored_project(&wrong_kind).unwrap_err();
    assert_eq!(
        wrong_kind.diagnostic().code,
        diagnostic_code::WRONG_ASSET_KIND
    );
    assert_eq!(wrong_kind.diagnostic().path, "entryScene");

    let missing = mutate(|project| project["entryScene"] = "scene/not-declared".into());
    let missing = decode_stored_project(&missing).unwrap_err();
    assert_eq!(
        missing.diagnostic().code,
        diagnostic_code::MISSING_ENTRY_SCENE
    );
    assert_eq!(missing.diagnostic().path, "entryScene");
}

#[test]
fn structural_decode_error_retains_the_scene_source_path() {
    let invalid = mutate(|project| project["scenes"][0]["unexpected"] = true.into());
    let error = decode_stored_project(&invalid).unwrap_err();

    assert_eq!(error.diagnostic().code, diagnostic_code::DECODE);
    assert!(error.diagnostic().path.starts_with("scenes[0]"));
}

#[test]
fn stored_project_admits_every_settled_component_family_atomically() {
    let runtime = GameRuntime::from_stored_project(PROJECT).expect("admitted runtime");
    let session = runtime.session();

    assert!(session.player_controller(EntityId::new(1)).is_some());
    assert!(session.weapon(EntityId::new(1)).is_some());
    assert!(session.encounter(EntityId::new(2)).is_some());
    assert!(session.door(EntityId::new(3)).is_some());
    assert!(session.enemy(EntityId::new(4)).is_some());
    assert!(session.health(EntityId::new(4)).is_some());
    assert!(session.navigation(EntityId::new(4)).is_some());
    assert!(session.extraction_beacon(EntityId::new(7)).is_some());
    assert!(session.hazard(EntityId::new(27)).is_some());
    assert_eq!(
        session
            .switch(EntityId::new(6))
            .expect("switch")
            .controls_targets,
        [EntityId::new(12)]
    );
    let collision = runtime.collision_scene().expect("spatial projection");
    assert!(collision.solid_voxel_count() > 0);
    assert!(!collision.mesh_chunks().is_empty());
}

#[test]
fn renderables_require_declared_mesh_assets() {
    let wrong_kind = mutate(|project| {
        let assets = project["assets"].as_array_mut().unwrap();
        let control_panel = assets
            .iter()
            .position(|asset| asset["id"] == "mesh/control-panel")
            .expect("control panel asset");
        assets[control_panel]["id"] = "audio/control-panel".into();
        project["scenes"][0]["entities"][5]["renderable"]["asset"] = "audio/control-panel".into();
    });
    let wrong_kind = admission_diagnostic(&wrong_kind);
    assert_eq!(wrong_kind.code, diagnostic_code::WRONG_ASSET_KIND);
    assert_eq!(wrong_kind.path, "scenes[0].entities[5].renderable.asset");

    let missing = mutate(|project| {
        let assets = project["assets"].as_array_mut().unwrap();
        let player_marker = assets
            .iter()
            .position(|asset| asset["id"] == "mesh/player-marker")
            .expect("player marker asset");
        assets.remove(player_marker);
    });
    let missing = admission_diagnostic(&missing);
    assert_eq!(missing.code, diagnostic_code::MISSING_ASSET);
    assert_eq!(missing.path, "scenes[0].entities[0].renderable.asset");
}

#[test]
fn non_entry_scenes_receive_the_same_semantic_admission() {
    let invalid = mutate(|project| {
        let mut second_scene = project["scenes"][0].clone();
        second_scene["id"] = "scene/storage-wing".into();
        second_scene["name"] = "Storage Wing".into();
        second_scene["entities"][0]["renderable"]["asset"] = "mesh/not-declared".into();
        project["scenes"].as_array_mut().unwrap().push(second_scene);
    });
    let diagnostic = admission_diagnostic(&invalid);

    assert_eq!(diagnostic.code, diagnostic_code::MISSING_ASSET);
    assert_eq!(diagnostic.path, "scenes[1].entities[0].renderable.asset");
}

#[test]
fn duplicate_entity_identity_fails_before_session_construction() {
    let invalid = mutate(|project| project["scenes"][0]["entities"][1]["id"] = 1.into());
    let diagnostic = admission_diagnostic(&invalid);

    assert_eq!(diagnostic.code, diagnostic_code::DUPLICATE_ENTITY);
    assert_eq!(diagnostic.path, "scenes[0].entities[1].id");
    assert!(diagnostic.message.contains("entities[0].id"));
}

#[test]
fn bad_relationship_reports_the_owning_component_path() {
    let invalid = mutate(|project| {
        project["scenes"][0]["entities"][5]["switch"]["controls"] = serde_json::json!([999]);
    });
    let diagnostic = admission_diagnostic(&invalid);

    assert_eq!(diagnostic.code, diagnostic_code::INVALID_RELATIONSHIP);
    assert_eq!(diagnostic.path, "scenes[0].entities[5].switch.controls");
}

#[test]
fn component_and_spatial_failures_retain_source_paths() {
    let invalid_component = mutate(|project| {
        project["scenes"][0]["entities"][0]["playerController"]["moveSpeedUnitsPerSecond"] =
            0.into();
    });
    let component = admission_diagnostic(&invalid_component);
    assert_eq!(component.code, diagnostic_code::INVALID_COMPONENT);
    assert_eq!(component.path, "scenes[0].entities[0].playerController");

    let invalid_spatial = mutate(|project| {
        project["scenes"][0]["voxelEnvironment"]["chunkSize"] = 65.into();
    });
    let spatial = admission_diagnostic(&invalid_spatial);
    assert_eq!(spatial.code, diagnostic_code::INVALID_SPATIAL);
    assert_eq!(spatial.path, "scenes[0].voxelEnvironment");
}

#[test]
fn material_only_project_edit_changes_canonical_spatial_behavior() {
    let variation = mutate(|project| {
        project["scenes"][0]["voxelEnvironment"]["materialVoxels"][0]["materialSlot"] = 9.into();
    });
    let first = GameRuntime::from_stored_project(PROJECT).unwrap();
    let second = GameRuntime::from_stored_project(&variation).unwrap();

    let first = first.collision_scene().unwrap();
    let second = second.collision_scene().unwrap();
    assert_ne!(first.material_voxels(), second.material_voxels());
    assert_ne!(first.mesh_chunks(), second.mesh_chunks());
}

#[test]
fn runtime_snapshot_reopens_without_becoming_authored_project_content() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    runtime
        .apply_player_action(
            EntityId::new(1),
            ResolvedPlayerAction::Look {
                yaw_delta: 0.5,
                pitch_delta: -0.5,
            },
        )
        .unwrap();

    let snapshot = encode_game_snapshot(&runtime).unwrap();
    let reopened = decode_game_snapshot(&snapshot).unwrap();
    assert_eq!(encode_game_snapshot(&reopened).unwrap(), snapshot);

    let project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    let snapshot_value: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
    assert!(project.get("entryScene").is_some());
    assert!(project.get("assets").is_some());
    assert!(project.get("scenes").is_some());
    assert!(project.get("tick").is_none());
    assert!(snapshot_value.get("tick").is_some());
    assert!(snapshot_value.get("entryScene").is_none());
    assert!(snapshot_value.get("assets").is_none());
    assert!(snapshot_value.get("scenes").is_none());
    assert!(!snapshot.contains("\"events\""));
    assert!(!snapshot.contains("TypeScript"));
}

fn mutate(change: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    change(&mut project);
    serde_json::to_string(&project).unwrap()
}

fn admission_diagnostic(input: &str) -> ProjectDiagnostic {
    match GameRuntime::from_stored_project(input).unwrap_err() {
        RuntimeError::StoredProject(error) => error.diagnostic().clone(),
        error => panic!("unexpected runtime error: {error:?}"),
    }
}
