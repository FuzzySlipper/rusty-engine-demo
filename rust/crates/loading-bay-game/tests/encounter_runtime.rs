use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, DoorState, EncounterState, EnemyState, GameEvent,
    GameLoopFact, GameRuntime, LoadingBayGameLoop, ProjectContentError,
};
use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;

const ENCOUNTER_PROJECT: &str =
    include_str!("../../../../content/generated/encounter-gate.project.json");
const SOLO_PROJECT: &str =
    include_str!("../../../../content/generated/encounter-gate-solo.project.json");
const STORED_PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");

const ACTOR: EntityId = EntityId::new(1);
const ENCOUNTER: EntityId = EntityId::new(2);
const EXIT: EntityId = EntityId::new(3);
const FIRST_ENEMY: EntityId = EntityId::new(4);
const SECOND_ENEMY: EntityId = EntityId::new(5);
const GENERATOR_ENCOUNTER: EntityId = EntityId::new(40);

#[test]
fn authored_content_materializes_legible_entities_and_relationships() {
    let runtime = GameRuntime::from_project_content(ENCOUNTER_PROJECT).expect("admit project");

    let encounter = runtime
        .session()
        .encounter(ENCOUNTER)
        .expect("encounter component");
    assert_eq!(encounter.members, vec![FIRST_ENEMY, SECOND_ENEMY]);
    assert_eq!(encounter.exit, Some(EXIT));
    assert_eq!(encounter.state, EncounterState::Active);
    assert_eq!(
        runtime.session().door(EXIT).expect("exit door").state,
        DoorState::Closed
    );
    assert_eq!(
        runtime.session().enemy(FIRST_ENEMY).expect("enemy").state,
        EnemyState::Alive
    );
}

#[test]
fn committed_enemy_facts_clear_the_encounter_and_open_the_exit() {
    let mut runtime = GameRuntime::from_project_content(ENCOUNTER_PROJECT).expect("admit project");

    let first = runtime
        .defeat_enemy(ACTOR, FIRST_ENEMY)
        .expect("defeat first enemy");
    assert_eq!(first.events.len(), 1);
    assert!(matches!(
        first.events[0],
        GameEvent::EnemyDefeated {
            enemy: FIRST_ENEMY,
            ..
        }
    ));
    assert_eq!(
        runtime
            .session()
            .encounter(ENCOUNTER)
            .expect("encounter")
            .state,
        EncounterState::Active
    );
    assert_eq!(
        runtime.session().door(EXIT).expect("exit").state,
        DoorState::Closed
    );
    let defeated = runtime.session().enemy(FIRST_ENEMY).expect("enemy");
    assert_eq!(defeated.state, EnemyState::Defeated);
    assert!(!defeated.entity_view.collision.expect("collision").enabled);
    assert!(defeated.entity_view.renderable.expect("renderable").visible);

    let second = runtime
        .defeat_enemy(ACTOR, SECOND_ENEMY)
        .expect("defeat second enemy");
    assert_eq!(second.events.len(), 3);
    assert!(matches!(
        second.events[0],
        GameEvent::EnemyDefeated {
            enemy: SECOND_ENEMY,
            ..
        }
    ));
    assert!(matches!(
        second.events[1],
        GameEvent::EncounterCleared {
            encounter: ENCOUNTER,
            exit: Some(EXIT)
        }
    ));
    assert!(matches!(
        second.events[2],
        GameEvent::DoorOpened { door: EXIT, .. }
    ));
    assert_eq!(
        runtime
            .session()
            .encounter(ENCOUNTER)
            .expect("encounter")
            .state,
        EncounterState::Cleared
    );
    let exit = runtime.session().door(EXIT).expect("exit");
    assert_eq!(exit.state, DoorState::Open);
    assert_eq!(
        exit.entity_view.transform.expect("transform").translation,
        Vec3::new(4.5, 4.0, 11.0)
    );
    assert_eq!(runtime.readout().journal.len(), 4);
}

#[test]
fn enemy_count_is_a_content_only_gate_variation() {
    let mut runtime = GameRuntime::from_project_content(SOLO_PROJECT).expect("admit solo project");
    let receipt = runtime
        .defeat_enemy(ACTOR, FIRST_ENEMY)
        .expect("defeat only enemy");

    assert_eq!(receipt.events.len(), 3);
    assert!(matches!(
        receipt.events[1],
        GameEvent::EncounterCleared { .. }
    ));
    assert_eq!(
        runtime.session().door(EXIT).expect("exit").state,
        DoorState::Open
    );
}

#[test]
fn save_reopen_preserves_partial_encounter_progress() {
    let mut runtime = GameRuntime::from_project_content(ENCOUNTER_PROJECT).expect("admit project");
    runtime
        .defeat_enemy(ACTOR, FIRST_ENEMY)
        .expect("defeat first enemy");
    let snapshot = encode_game_snapshot(&runtime).expect("save");

    let mut restored = decode_game_snapshot(&snapshot).expect("reopen");
    assert_eq!(
        restored.session().enemy(FIRST_ENEMY).expect("enemy").state,
        EnemyState::Defeated
    );
    assert_eq!(
        restored
            .session()
            .encounter(ENCOUNTER)
            .expect("encounter")
            .state,
        EncounterState::Active
    );
    assert!(restored.readout().journal.is_empty());

    let receipt = restored
        .defeat_enemy(ACTOR, SECOND_ENEMY)
        .expect("finish encounter");
    assert!(matches!(
        receipt.events[1],
        GameEvent::EncounterCleared { .. }
    ));
    assert_eq!(
        restored.session().door(EXIT).expect("exit").state,
        DoorState::Open
    );
}

#[test]
fn no_exit_encounter_activates_clears_and_round_trips_without_a_door_consequence() {
    let mut project: serde_json::Value = serde_json::from_str(STORED_PROJECT).unwrap();
    stored_entity_mut(&mut project, ACTOR)["translation"] = serde_json::json!([7.5, 1.5, 8.5]);
    stored_entity_mut(&mut project, ENCOUNTER)["encounter"]
        .as_object_mut()
        .unwrap()
        .remove("exit");

    let runtime = GameRuntime::from_stored_project(&project.to_string()).expect("admit project");
    assert_eq!(runtime.session().encounter(ENCOUNTER).unwrap().exit, None);
    assert_eq!(
        runtime.session().encounter(ENCOUNTER).unwrap().state,
        EncounterState::Dormant
    );

    let mut game_loop = LoadingBayGameLoop::new(runtime, ACTOR).unwrap();
    let activation = game_loop.run_fixed_tick().expect("activate encounter");
    assert!(activation.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::Event(GameEvent::EncounterActivated {
            encounter: ENCOUNTER,
            player: ACTOR
        })
    )));
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .encounter(ENCOUNTER)
            .unwrap()
            .state,
        EncounterState::Active
    );

    let mut runtime = game_loop.into_runtime();
    let cleared = runtime
        .defeat_enemy(ACTOR, FIRST_ENEMY)
        .expect("clear no-exit encounter");
    assert_eq!(cleared.events.len(), 2);
    assert!(matches!(
        cleared.events[0],
        GameEvent::EnemyDefeated {
            enemy: FIRST_ENEMY,
            ..
        }
    ));
    assert!(matches!(
        cleared.events[1],
        GameEvent::EncounterCleared {
            encounter: ENCOUNTER,
            exit: None
        }
    ));
    assert!(!cleared
        .events
        .iter()
        .any(|event| matches!(event, GameEvent::DoorOpened { .. })));
    assert_eq!(
        runtime.session().encounter(ENCOUNTER).unwrap().state,
        EncounterState::Cleared
    );
    assert_eq!(runtime.session().encounter(ENCOUNTER).unwrap().exit, None);

    let snapshot = encode_game_snapshot(&runtime).expect("save");
    let snapshot_value: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
    let encounter_snapshot = snapshot_value["encounters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|encounter| encounter["entity"].as_u64() == Some(ENCOUNTER.raw()))
        .expect("encounter snapshot");
    assert!(encounter_snapshot.get("exit").is_none());

    let restored = decode_game_snapshot(&snapshot).expect("reopen");
    let encounter = restored.session().encounter(ENCOUNTER).unwrap();
    assert_eq!(encounter.state, EncounterState::Cleared);
    assert_eq!(encounter.exit, None);
}

#[test]
fn project_content_rejects_unknown_contract_fields() {
    let invalid = ENCOUNTER_PROJECT.replacen(
        "\"schemaVersion\": 6",
        "\"schemaVersion\": 6, \"runtimeBehavior\": \"not-content\"",
        1,
    );
    assert!(matches!(
        GameRuntime::from_project_content(&invalid),
        Err(loading_bay_game::RuntimeError::Content(
            ProjectContentError::Decode(_)
        ))
    ));
}

#[test]
fn encounter_activation_spreads_first_attacks_over_authored_enemy_cadence() {
    let mut project: serde_json::Value = serde_json::from_str(STORED_PROJECT).unwrap();
    let encounter_position =
        stored_entity_mut(&mut project, GENERATOR_ENCOUNTER)["translation"].clone();
    stored_entity_mut(&mut project, ACTOR)["translation"] = encounter_position;
    let runtime = GameRuntime::from_stored_project(&project.to_string()).expect("admit project");
    let mut game_loop = LoadingBayGameLoop::new(runtime, ACTOR).unwrap();

    let activation = game_loop.run_fixed_tick().expect("activate encounter");
    assert!(activation.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::Event(GameEvent::EncounterActivated {
            encounter: GENERATOR_ENCOUNTER,
            player: ACTOR
        })
    )));
    let session = game_loop.runtime().session();
    let ready_at = [5, 41, 42].map(|enemy| {
        session
            .enemy_combat(EntityId::new(enemy))
            .expect("encounter combatant")
            .state
            .ready_at_tick
            .raw()
    });
    assert_eq!(ready_at, [121, 241, 361]);
}

#[test]
fn project_content_rejects_kinematics_without_a_collision_scene() {
    let invalid = r#"{
        "schemaVersion": 6,
      "entities": [{
        "id": 1,
        "name": "unbounded-runner",
        "translation": [0, 0, 0],
        "kinematic": { "halfExtents": [0.5, 0.5, 0.5], "velocity": [1, 0, 0] }
      }]
    }"#;

    assert!(matches!(
        GameRuntime::from_project_content(invalid),
        Err(loading_bay_game::RuntimeError::Content(
            ProjectContentError::KinematicMissingCollisionScene { entity: ACTOR }
        ))
    ));
}

fn stored_entity_mut(project: &mut serde_json::Value, id: EntityId) -> &mut serde_json::Value {
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"].as_u64() == Some(id.raw()))
        .expect("stored entity")
}
