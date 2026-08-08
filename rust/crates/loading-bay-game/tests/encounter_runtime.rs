use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, DoorState, EncounterState, EnemyState, GameEvent,
    GameRuntime, ProjectContentError,
};
use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;

const ENCOUNTER_PROJECT: &str =
    include_str!("../../../../content/generated/encounter-gate.project.json");
const SOLO_PROJECT: &str =
    include_str!("../../../../content/generated/encounter-gate-solo.project.json");

const ACTOR: EntityId = EntityId::new(1);
const ENCOUNTER: EntityId = EntityId::new(2);
const EXIT: EntityId = EntityId::new(3);
const FIRST_ENEMY: EntityId = EntityId::new(4);
const SECOND_ENEMY: EntityId = EntityId::new(5);

#[test]
fn authored_content_materializes_legible_entities_and_relationships() {
    let runtime = GameRuntime::from_project_content(ENCOUNTER_PROJECT).expect("admit project");

    let encounter = runtime
        .session()
        .encounter(ENCOUNTER)
        .expect("encounter component");
    assert_eq!(encounter.members, vec![FIRST_ENEMY, SECOND_ENEMY]);
    assert_eq!(encounter.exit, EXIT);
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
    assert!(!defeated.entity_view.renderable.expect("renderable").visible);

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
            exit: EXIT
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
