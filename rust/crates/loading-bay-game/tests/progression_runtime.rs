use core_ids::EntityId;
use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, DoorAccessRejection, DoorState,
    EdgeCommandRejection, GameLoopEdgeCommand, GameLoopEdgeCommandKind, GameLoopFact,
    GameRestartMode, GameRuntime, GameSnapshotError, InputCommandRejection, ItemDefinitionId,
    ItemKind, LevelExitState, LoadingBayGameLoop, ProgressionFact, RequiredKeyPolicy,
    SecretRegionState,
};

const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const PLAYER: EntityId = EntityId::new(1);
const INTERLOCK_SWITCH: EntityId = EntityId::new(6);
const EXTRACTION_GATE: EntityId = EntityId::new(12);
const GENERATOR_DOOR: EntityId = EntityId::new(13);
const KEYED_DOOR: EntityId = EntityId::new(30);
const SECRET: EntityId = EntityId::new(31);
const LEVEL_EXIT: EntityId = EntityId::new(32);

fn item(value: &str) -> ItemDefinitionId {
    ItemDefinitionId::parse(value).unwrap()
}

fn project_at(position: [f32; 3]) -> serde_json::Value {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    entity_mut(&mut project, PLAYER)["translation"] = serde_json::json!(position);
    project
}

fn entity_mut(project: &mut serde_json::Value, id: EntityId) -> &mut serde_json::Value {
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == id.raw())
        .unwrap()
}

fn grant_starting_item(project: &mut serde_json::Value, item: &str) {
    entity_mut(project, PLAYER)["inventory"]["startingStacks"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"item": item, "quantity": 1}));
}

fn quantity(runtime: &GameRuntime, item: &str) -> u32 {
    runtime
        .session()
        .inventory(PLAYER)
        .unwrap()
        .stacks
        .iter()
        .find(|stack| stack.item.as_str() == item)
        .map_or(0, |stack| stack.quantity)
}

fn edge(generation: u64, sequence: u64, command: GameLoopEdgeCommandKind) -> GameLoopEdgeCommand {
    GameLoopEdgeCommand {
        connection_generation: generation,
        sequence,
        command,
    }
}

#[test]
fn authored_progression_is_object_owned_and_uses_inventory_keys() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let session = runtime.session();
    let required_key = item("key/maintenance-pass");

    assert!(matches!(
        session.item_definition(&required_key).unwrap().kind,
        ItemKind::AccessKey
    ));
    let access = session.door_access(KEYED_DOOR).unwrap();
    assert_eq!(access.config.required_key, required_key);
    assert_eq!(access.config.key_policy, RequiredKeyPolicy::Retain);
    assert_eq!(
        access.config.denied_presentation,
        "Maintenance pass required"
    );
    assert_eq!(
        session.switch(INTERLOCK_SWITCH).unwrap().controls_targets,
        vec![EXTRACTION_GATE]
    );
    assert_eq!(
        session.secret_region(SECRET).unwrap().state,
        SecretRegionState::Undiscovered
    );
    assert_eq!(
        session.level_exit(LEVEL_EXIT).unwrap().state,
        LevelExitState::Available
    );
}

#[test]
fn missing_or_wrong_key_rejects_without_mutating_runtime() {
    for wrong_key in [None, Some("key/inert-inspection-tag")] {
        let mut project = project_at([11.5, 1.5, 15.5]);
        if let Some(wrong_key) = wrong_key {
            grant_starting_item(&mut project, wrong_key);
        }
        let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
        let before = encode_game_snapshot(&runtime).unwrap();

        assert!(matches!(
            runtime.open_keyed_door(PLAYER, KEYED_DOOR),
            Err(loading_bay_game::RuntimeError::DoorAccess(
                DoorAccessRejection::MissingRequiredKey {
                    door: KEYED_DOOR,
                    ..
                }
            ))
        ));
        assert_eq!(encode_game_snapshot(&runtime).unwrap(), before);
    }
}

#[test]
fn keyed_door_retain_and_consume_policies_are_atomic_and_idempotent() {
    for (policy, expected_after) in [("retain", 1), ("consume", 0)] {
        let mut project = project_at([11.5, 1.5, 15.5]);
        grant_starting_item(&mut project, "key/maintenance-pass");
        entity_mut(&mut project, KEYED_DOOR)["door"]["access"]["keyPolicy"] = policy.into();
        let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();

        let (opened, events) = runtime.open_keyed_door(PLAYER, KEYED_DOOR).unwrap();
        assert!(matches!(
            opened.fact,
            Some(ProgressionFact::DoorAccessGranted {
                door: KEYED_DOOR,
                ..
            })
        ));
        assert_eq!(
            runtime.session().door(KEYED_DOOR).unwrap().state,
            DoorState::Open
        );
        assert_eq!(quantity(&runtime, "key/maintenance-pass"), expected_after);
        assert!(events.iter().any(|event| matches!(
            event,
            loading_bay_game::GameEvent::DoorOpened {
                door: KEYED_DOOR,
                ..
            }
        )));

        let before_repeat = encode_game_snapshot(&runtime).unwrap();
        let (repeated, repeated_events) = runtime.open_keyed_door(PLAYER, KEYED_DOOR).unwrap();
        assert!(repeated.fact.is_none());
        assert!(repeated.inventory.is_none());
        assert!(repeated_events.is_empty());
        assert_eq!(encode_game_snapshot(&runtime).unwrap(), before_repeat);
    }
}

#[test]
fn loading_bay_interlock_preserves_switch_consequences_and_scheduled_close() {
    let mut out_of_range = GameRuntime::from_stored_project(PROJECT).unwrap();
    let before = encode_game_snapshot(&out_of_range).unwrap();
    assert!(matches!(
        out_of_range.activate_loading_bay_interlock(PLAYER, INTERLOCK_SWITCH),
        Err(loading_bay_game::RuntimeError::LoadingBayInterlock(
            loading_bay_game::LoadingBayInterlockRejection::OutOfRange {
                actor: PLAYER,
                switch: INTERLOCK_SWITCH,
            }
        ))
    ));
    assert_eq!(encode_game_snapshot(&out_of_range).unwrap(), before);

    let mut project = project_at([11.5, 1.5, 20.5]);
    entity_mut(&mut project, EntityId::new(40))["encounter"]["activationRadius"] =
        serde_json::Value::Null;
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    for enemy in [5, 41, 42] {
        runtime.defeat_enemy(PLAYER, EntityId::new(enemy)).unwrap();
    }
    assert_eq!(
        runtime.session().door(GENERATOR_DOOR).unwrap().state,
        DoorState::Open
    );

    let receipt = runtime
        .activate_loading_bay_interlock(PLAYER, INTERLOCK_SWITCH)
        .unwrap();
    assert_eq!(
        runtime.session().door(GENERATOR_DOOR).unwrap().state,
        DoorState::Closed
    );
    assert_eq!(
        runtime.session().door(EXTRACTION_GATE).unwrap().state,
        DoorState::Open
    );
    assert!(matches!(
        receipt.events.as_slice(),
        [
            loading_bay_game::GameEvent::SwitchActivated {
                switch: INTERLOCK_SWITCH,
                ..
            },
            loading_bay_game::GameEvent::DoorClosed {
                door: GENERATOR_DOOR,
                ..
            },
            loading_bay_game::GameEvent::DoorOpened {
                door: EXTRACTION_GATE,
                ..
            }
        ]
    ));

    runtime.advance_by(90).unwrap();
    assert_eq!(
        runtime.session().door(GENERATOR_DOOR).unwrap().state,
        DoorState::Closed
    );
    assert_eq!(
        runtime.session().door(EXTRACTION_GATE).unwrap().state,
        DoorState::Open
    );
}

#[test]
fn secret_discovery_is_first_entry_only_and_survives_snapshot_reopen() {
    let project = project_at([3.5, 1.5, 24.5]);
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();

    let first = game_loop.run_fixed_tick().unwrap();
    assert_eq!(
        first
            .facts
            .iter()
            .filter(|fact| matches!(
                fact,
                GameLoopFact::Progression(ProgressionFact::SecretDiscovered { secret: SECRET, .. })
            ))
            .count(),
        1
    );
    assert!(matches!(
        game_loop
            .runtime()
            .session()
            .secret_region(SECRET)
            .unwrap()
            .state,
        SecretRegionState::Discovered { actor: PLAYER, .. }
    ));
    assert!(!game_loop
        .run_fixed_tick()
        .unwrap()
        .facts
        .iter()
        .any(|fact| {
            matches!(
                fact,
                GameLoopFact::Progression(ProgressionFact::SecretDiscovered { .. })
            )
        }));

    let reopened =
        decode_game_snapshot(&encode_game_snapshot(game_loop.runtime()).unwrap()).unwrap();
    let mut reopened_loop = LoadingBayGameLoop::new(reopened, PLAYER).unwrap();
    assert!(!reopened_loop
        .run_fixed_tick()
        .unwrap()
        .facts
        .iter()
        .any(|fact| {
            matches!(
                fact,
                GameLoopFact::Progression(ProgressionFact::SecretDiscovered { .. })
            )
        }));
    assert!(matches!(
        reopened_loop
            .runtime()
            .session()
            .secret_region(SECRET)
            .unwrap()
            .state,
        SecretRegionState::Discovered { actor: PLAYER, .. }
    ));
}

#[test]
fn later_over_cap_secret_query_commits_no_earlier_discovery() {
    let mut project = project_at([3.5, 1.5, 24.5]);
    let entities = project["scenes"][0]["entities"].as_array_mut().unwrap();
    entities.push(serde_json::json!({
        "id": 333,
        "name": "over-cap-secret",
        "translation": [8.5, 1.5, 8.5],
        "bounds": {
            "min": [-0.6, -0.6, -0.6],
            "max": [0.6, 0.6, 0.6]
        },
        "secretRegion": {
            "presentation": "Over-cap secret"
        }
    }));
    for offset in 0..129 {
        entities.push(serde_json::json!({
            "id": 1_000 + offset,
            "name": format!("secret-overlap-subject-{offset}"),
            "translation": [8.5, 1.5, 8.5],
            "bounds": {
                "min": [-0.1, -0.1, -0.1],
                "max": [0.1, 0.1, 0.1]
            },
            "collision": {
                "enabled": true,
                "staticCollider": false
            }
        }));
    }

    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let before: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    let before_triggers = before["progression"]["secretTriggers"].clone();
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();

    assert!(matches!(
        game_loop.run_fixed_tick(),
        Err(loading_bay_game::RuntimeError::Secret(_))
    ));
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .secret_region(SECRET)
            .unwrap()
            .state,
        SecretRegionState::Undiscovered
    );
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .secret_region(EntityId::new(333))
            .unwrap()
            .state,
        SecretRegionState::Undiscovered
    );
    let after: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(game_loop.runtime()).unwrap()).unwrap();
    assert_eq!(after["progression"]["secretTriggers"], before_triggers);
}

#[test]
fn level_completion_stops_simulation_across_reconnect_and_allows_only_authored_restart() {
    let project = project_at([21.5, 1.5, 50.5]);
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();
    let generation = game_loop.start_connection().connection_generation;
    game_loop
        .submit_edge_command(edge(
            generation,
            1,
            GameLoopEdgeCommandKind::Interact {
                target: LEVEL_EXIT.raw(),
            },
        ))
        .unwrap();
    let completed = game_loop.run_fixed_tick().unwrap();
    assert!(completed.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::Progression(ProgressionFact::LevelCompleted {
            exit: LEVEL_EXIT,
            actor: PLAYER,
            ..
        })
    )));
    assert!(game_loop.runtime().is_level_complete());

    assert!(game_loop.disconnect(generation));
    let generation = game_loop.start_connection().connection_generation;
    game_loop
        .submit_edge_command(edge(
            generation,
            2,
            GameLoopEdgeCommandKind::Interact {
                target: LEVEL_EXIT.raw(),
            },
        ))
        .unwrap();
    game_loop
        .submit_edge_command(edge(
            generation,
            3,
            GameLoopEdgeCommandKind::RestartAuthoredBaseline,
        ))
        .unwrap();
    let stopped = game_loop.run_fixed_tick().unwrap();
    assert!(!stopped.simulation_advanced);
    assert!(stopped.facts.contains(&GameLoopFact::EdgeCommandRejected {
        sequence: 2,
        reason: EdgeCommandRejection::LevelComplete,
    }));
    assert!(stopped.facts.contains(&GameLoopFact::RestartRequested {
        sequence: 3,
        mode: GameRestartMode::AuthoredBaseline,
    }));

    let reopened =
        decode_game_snapshot(&encode_game_snapshot(game_loop.runtime()).unwrap()).unwrap();
    assert!(reopened.is_level_complete());
}

#[test]
fn defeated_player_cannot_advance_progression_but_can_request_restart() {
    let mut project = project_at([4.5, 1.5, 4.5]);
    grant_starting_item(&mut project, "key/maintenance-pass");
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    let health = snapshot["health"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|health| health["entity"] == PLAYER.raw())
        .unwrap();
    health["current"] = 0.into();
    health["state"] = "dead".into();
    let defeated = decode_game_snapshot(&snapshot.to_string()).unwrap();
    let before = encode_game_snapshot(&defeated).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(defeated, PLAYER).unwrap();
    let generation = game_loop.start_connection().connection_generation;

    assert_eq!(
        game_loop
            .submit_edge_command(edge(
                generation,
                1,
                GameLoopEdgeCommandKind::Interact {
                    target: KEYED_DOOR.raw(),
                },
            ))
            .unwrap_err(),
        InputCommandRejection::PlayerDefeated
    );
    assert_eq!(encode_game_snapshot(game_loop.runtime()).unwrap(), before);
    game_loop
        .submit_edge_command(edge(
            generation,
            2,
            GameLoopEdgeCommandKind::RestartAuthoredBaseline,
        ))
        .unwrap();
    let receipt = game_loop.run_fixed_tick().unwrap();
    assert!(receipt.facts.contains(&GameLoopFact::RestartRequested {
        sequence: 2,
        mode: GameRestartMode::AuthoredBaseline,
    }));
}

#[test]
fn legacy_snapshots_reject_future_progression_state() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    snapshot["schemaVersion"] = 15.into();
    for definition in snapshot["itemDefinitions"].as_array_mut().unwrap() {
        let kind = definition["kind"].as_object_mut().unwrap();
        if kind.get("kind").and_then(serde_json::Value::as_str) == Some("weapon") {
            kind.insert("attackMode".into(), "hitscan".into());
            kind.remove("pelletCount");
            kind.remove("spreadDegrees");
        }
    }

    assert!(matches!(
        decode_game_snapshot(&snapshot.to_string()),
        Err(GameSnapshotError::FutureProgressionStateInLegacySnapshot)
    ));
}
