use loading_bay_game::{
    diagnostic_code, DoorState, EncounterState, GameEvent, GameRuntime, RuntimeError,
};
use rusty_engine::core_ids::EntityId;
use serde_json::{json, Value};

const PROJECT: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");
const PLAYER: EntityId = EntityId::new(1);

fn sync_authored_translation(project: &mut Value, id: EntityId, translation: &Value) {
    let nodes = project["scenes"][0]["authoredScene"]["nodes"]
        .as_array_mut()
        .expect("authored scene nodes");
    let node = nodes
        .iter_mut()
        .find(|node| node["id"] == id.raw())
        .expect("authored scene node for entity");
    node["transform"]["translation"] = translation.clone();
}

#[test]
fn canonical_e1m1_encounter_program_activates_members_in_rust() {
    let (project, encounter, member, _) = project_with_one_member_and_optional_exit();
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();

    let events = runtime.run_encounter_activation_phase(PLAYER).unwrap();

    assert_eq!(
        runtime.session().encounter(encounter).unwrap().state,
        EncounterState::Active
    );
    assert!(
        runtime
            .session()
            .enemy_combat(member)
            .unwrap()
            .state
            .ready_at_tick
            .raw()
            > 0
    );
    assert!(events.iter().any(|event| matches!(
        event,
        GameEvent::EncounterActivated { encounter: observed, player }
            if *observed == encounter && *player == PLAYER
    )));
    assert!(runtime.session().gameplay_outcome().is_none());
}

#[test]
fn activation_program_variant_omitting_member_activation_preserves_dormant_member_cadence() {
    let (mut project, encounter, member, _) = project_with_one_member_and_optional_exit();
    encounter_program_mut(&mut project)["activation"] = json!({
        "kind": "when",
        "predicate": "activationEligible",
        "thenProgram": {
            "kind": "sequence",
            "steps": [
                { "kind": "operation", "operation": "recordEncounterActivation" },
                { "kind": "operation", "operation": "emitEncounterFeedback" }
            ]
        }
    });
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();

    runtime.run_encounter_activation_phase(PLAYER).unwrap();

    assert_eq!(
        runtime.session().encounter(encounter).unwrap().state,
        EncounterState::Active
    );
    assert_eq!(
        runtime
            .session()
            .enemy_combat(member)
            .unwrap()
            .state
            .ready_at_tick
            .raw(),
        0
    );
}

#[test]
fn clear_program_variant_controls_typed_optional_exit_without_inventing_an_e1m1_relation() {
    let (project, encounter, member, exit) = project_with_one_member_and_optional_exit();
    let mut canonical = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    canonical.run_encounter_activation_phase(PLAYER).unwrap();
    let canonical_events = canonical.defeat_enemy(PLAYER, member).unwrap().events;

    assert_eq!(
        canonical.session().encounter(encounter).unwrap().state,
        EncounterState::Cleared
    );
    assert_eq!(
        canonical.session().door(exit).unwrap().state,
        DoorState::Opening
    );
    assert_event_order(&canonical_events, member, encounter, exit);

    let mut without_exit_operation = project;
    encounter_program_mut(&mut without_exit_operation)["clear"] = json!({
        "kind": "when",
        "predicate": "membersDefeated",
        "thenProgram": {
            "kind": "operation",
            "operation": "recordEncounterCleared"
        }
    });
    let mut variant =
        GameRuntime::from_stored_project(&without_exit_operation.to_string()).unwrap();
    variant.run_encounter_activation_phase(PLAYER).unwrap();
    let variant_events = variant.defeat_enemy(PLAYER, member).unwrap().events;

    assert_eq!(
        variant.session().encounter(encounter).unwrap().state,
        EncounterState::Cleared
    );
    assert_eq!(
        variant.session().door(exit).unwrap().state,
        DoorState::Closed
    );
    assert!(variant_events.iter().any(|event| matches!(
        event,
        GameEvent::EncounterCleared { encounter: observed, exit: Some(observed_exit) }
            if *observed == encounter && *observed_exit == exit
    )));
    assert!(!variant_events.iter().any(|event| matches!(
        event,
        GameEvent::DoorOpened { door, .. } if *door == exit
    )));
}

#[test]
fn canonical_clear_program_treats_missing_bound_exit_as_a_typed_no_op() {
    let (mut project, encounter, member, door) = project_with_one_member_and_optional_exit();
    entity_mut(&mut project, encounter)["encounter"]
        .as_object_mut()
        .unwrap()
        .remove("exit");
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();

    runtime.run_encounter_activation_phase(PLAYER).unwrap();
    let events = runtime.defeat_enemy(PLAYER, member).unwrap().events;

    assert_eq!(
        runtime.session().encounter(encounter).unwrap().state,
        EncounterState::Cleared
    );
    assert_eq!(
        runtime.session().door(door).unwrap().state,
        DoorState::Closed
    );
    assert_eq!(events.len(), 2);
    assert!(matches!(
        &events[0],
        GameEvent::EnemyDefeated { enemy, .. } if *enemy == member
    ));
    assert!(matches!(
        &events[1],
        GameEvent::EncounterCleared { encounter: observed, exit: None } if *observed == encounter
    ));
    assert!(!events
        .iter()
        .any(|event| matches!(event, GameEvent::DoorOpened { .. })));
}

#[test]
fn ordered_late_clear_failure_rolls_back_encounter_exit_event_and_journal_but_not_defeat() {
    let (mut project, encounter, member, exit) = project_with_one_member_and_optional_exit();
    encounter_program_mut(&mut project)["clear"] = json!({
        "kind": "when",
        "predicate": "membersDefeated",
        "thenProgram": {
            "kind": "sequence",
            "steps": [
                { "kind": "operation", "operation": "recordEncounterCleared" },
                { "kind": "operation", "operation": "openBoundExit" },
                { "kind": "operation", "operation": "recordEncounterCleared" }
            ]
        }
    });
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    runtime.run_encounter_activation_phase(PLAYER).unwrap();
    let journal_before = runtime.readout().journal;

    let error = runtime.defeat_enemy(PLAYER, member).unwrap_err();

    assert!(matches!(
        error,
        RuntimeError::EncounterProgram(
            loading_bay_game::encounter::EncounterProgramRejection::DuplicateClearRecord {
                encounter: observed,
            }
        ) if observed == encounter
    ));
    assert_eq!(
        runtime.session().enemy(member).unwrap().state,
        loading_bay_game::EnemyState::Defeated
    );
    assert_eq!(
        runtime.session().encounter(encounter).unwrap().state,
        EncounterState::Active
    );
    assert_eq!(
        runtime.session().door(exit).unwrap().state,
        DoorState::Closed
    );
    assert_eq!(runtime.readout().journal, journal_before);
}

#[test]
fn encounter_bindings_reject_missing_and_wrong_family_ids() {
    let (mut wrong_family, encounter, _, _) = project_with_one_member_and_optional_exit();
    entity_mut(&mut wrong_family, encounter)["encounter"]["program"] = json!("hazard/nukage");
    let RuntimeError::StoredProject(error) =
        GameRuntime::from_stored_project(&wrong_family.to_string()).unwrap_err()
    else {
        panic!("wrong-family encounter id did not fail admission");
    };
    assert_eq!(error.diagnostic().code, diagnostic_code::INVALID_VALUE);
    assert!(error.diagnostic().path.ends_with("encounter.program"));

    let (mut missing, encounter, _, _) = project_with_one_member_and_optional_exit();
    entity_mut(&mut missing, encounter)["encounter"]
        .as_object_mut()
        .unwrap()
        .remove("program");
    assert!(GameRuntime::from_stored_project(&missing.to_string()).is_err());
}

fn project_with_one_member_and_optional_exit() -> (Value, EntityId, EntityId, EntityId) {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    let entities = project["scenes"][0]["entities"].as_array().unwrap();
    let encounter = entities
        .iter()
        .find(|entity| entity.get("encounter").is_some())
        .expect("E1M1 has an authored encounter");
    let encounter_id = EntityId::new(encounter["id"].as_u64().unwrap());
    let member = EntityId::new(encounter["encounter"]["members"][0].as_u64().unwrap());
    let position = encounter["translation"].clone();
    let exit = EntityId::new(
        entities
            .iter()
            .find(|entity| entity.get("door").is_some())
            .and_then(|entity| entity["id"].as_u64())
            .expect("E1M1 has a door usable as an encounter fixture exit"),
    );

    entity_mut(&mut project, PLAYER)["translation"] = position.clone();
    sync_authored_translation(&mut project, PLAYER, &position);
    let encounter_component = &mut entity_mut(&mut project, encounter_id)["encounter"];
    encounter_component["members"] = json!([member.raw()]);
    encounter_component["exit"] = json!(exit.raw());
    (project, encounter_id, member, exit)
}

fn encounter_program_mut(project: &mut Value) -> &mut Value {
    project["encounterPrograms"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|program| program["id"] == "encounter/e1m1")
        .expect("E1M1 authors the canonical encounter program")
}

fn entity_mut(project: &mut Value, id: EntityId) -> &mut Value {
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == id.raw())
        .expect("entity exists")
}

fn assert_event_order(events: &[GameEvent], member: EntityId, encounter: EntityId, exit: EntityId) {
    let defeat = events
        .iter()
        .position(
            |event| matches!(event, GameEvent::EnemyDefeated { enemy, .. } if *enemy == member),
        )
        .expect("defeat event");
    let cleared = events
        .iter()
        .position(|event| matches!(event, GameEvent::EncounterCleared { encounter: observed, .. } if *observed == encounter))
        .expect("clear event");
    let opened = events
        .iter()
        .position(|event| matches!(event, GameEvent::DoorOpened { door, .. } if *door == exit))
        .expect("door event");
    assert!(defeat < cleared && cleared < opened);
}
