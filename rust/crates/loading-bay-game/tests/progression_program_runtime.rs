use loading_bay_game::{
    diagnostic_code, GameRuntime, LevelExitRejection, LevelExitState, ProgressionFact,
    RuntimeError, SecretRegionState, SecretRejection,
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
fn canonical_e1m1_secret_and_exit_programs_preserve_once_only_progression() {
    let (project, secret, exit) = project_with_secret_and_exit_at_player();
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();

    let secret_receipt = runtime.run_secret_phase(PLAYER).unwrap();
    assert_eq!(secret_receipt.facts.len(), 1);
    assert!(matches!(
        &secret_receipt.facts[0],
        ProgressionFact::SecretDiscovered { secret: observed, actor, .. }
            if *observed == secret && *actor == PLAYER
    ));
    assert!(matches!(
        runtime.session().secret_region(secret).unwrap().state,
        SecretRegionState::Discovered { actor, .. } if actor == PLAYER
    ));
    assert!(runtime.run_secret_phase(PLAYER).unwrap().facts.is_empty());

    let exit_fact = runtime.complete_level(PLAYER, exit).unwrap();
    assert!(matches!(
        exit_fact,
        Some(ProgressionFact::LevelCompleted { exit: observed, actor, .. })
            if observed == exit && actor == PLAYER
    ));
    assert!(matches!(
        runtime.session().level_exit(exit).unwrap().state,
        LevelExitState::Completed { actor, .. } if actor == PLAYER
    ));
    assert!(runtime.complete_level(PLAYER, exit).unwrap().is_none());

    let secrets = runtime.session().secret_programs();
    assert_eq!(secrets.programs.len(), 1);
    assert_eq!(secrets.bindings.len(), 3);
    assert_eq!(secrets.bindings[0].program_id, "secret/e1m1-discovery");
    let exits = runtime.session().level_exit_programs();
    assert_eq!(exits.programs.len(), 1);
    assert_eq!(exits.bindings.len(), 1);
    assert_eq!(exits.bindings[0].program_id, "level-exit/e1m1-completion");
    assert!(runtime.session().gameplay_outcome().is_none());
}

#[test]
fn record_only_secret_and_exit_variants_change_state_without_presentation_fact() {
    let (mut project, secret, exit) = project_with_secret_and_exit_at_player();
    secret_program_mut(&mut project)["program"] = json!({
        "kind": "operation",
        "operation": "recordDiscovery"
    });
    level_exit_program_mut(&mut project)["program"] = json!({
        "kind": "operation",
        "operation": "recordCompletion"
    });
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();

    assert!(runtime.run_secret_phase(PLAYER).unwrap().facts.is_empty());
    assert!(matches!(
        runtime.session().secret_region(secret).unwrap().state,
        SecretRegionState::Discovered { actor, .. } if actor == PLAYER
    ));
    assert!(runtime.complete_level(PLAYER, exit).unwrap().is_none());
    assert!(matches!(
        runtime.session().level_exit(exit).unwrap().state,
        LevelExitState::Completed { actor, .. } if actor == PLAYER
    ));
}

#[test]
fn late_secret_order_failure_rolls_back_state_and_trigger_candidate() {
    let (mut project, secret, _) = project_with_secret_and_exit_at_player();
    secret_program_mut(&mut project)["program"] = json!({
        "kind": "sequence",
        "steps": [
            { "kind": "operation", "operation": "recordDiscovery" },
            { "kind": "operation", "operation": "emitSecretPresentation" },
            { "kind": "operation", "operation": "recordDiscovery" }
        ]
    });
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();

    let error = runtime.run_secret_phase(PLAYER).unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::Secret(SecretRejection::DiscoveryAlreadyRecorded { secret: observed })
            if observed == secret
    ));
    assert_eq!(
        runtime.session().secret_region(secret).unwrap().state,
        SecretRegionState::Undiscovered
    );
    assert!(matches!(
        runtime.run_secret_phase(PLAYER),
        Err(RuntimeError::Secret(SecretRejection::DiscoveryAlreadyRecorded { secret: observed }))
            if observed == secret
    ));
}

#[test]
fn emit_before_record_is_rejected_without_secret_or_exit_mutation() {
    let (mut project, secret, exit) = project_with_secret_and_exit_at_player();
    secret_program_mut(&mut project)["program"] = json!({
        "kind": "operation",
        "operation": "emitSecretPresentation"
    });
    level_exit_program_mut(&mut project)["program"] = json!({
        "kind": "operation",
        "operation": "emitCompletionPresentation"
    });
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();

    assert!(matches!(
        runtime.run_secret_phase(PLAYER),
        Err(RuntimeError::Secret(SecretRejection::SecretPresentationBeforeRecord { secret: observed }))
            if observed == secret
    ));
    assert_eq!(
        runtime.session().secret_region(secret).unwrap().state,
        SecretRegionState::Undiscovered
    );
    assert!(matches!(
        runtime.complete_level(PLAYER, exit),
        Err(RuntimeError::LevelExit(LevelExitRejection::CompletionPresentationBeforeRecord { exit: observed }))
            if observed == exit
    ));
    assert_eq!(
        runtime.session().level_exit(exit).unwrap().state,
        LevelExitState::Available
    );
}

#[test]
fn duplicate_exit_record_rolls_back_completion() {
    let (mut project, _, exit) = project_with_secret_and_exit_at_player();
    level_exit_program_mut(&mut project)["program"] = json!({
        "kind": "sequence",
        "steps": [
            { "kind": "operation", "operation": "recordCompletion" },
            { "kind": "operation", "operation": "recordCompletion" }
        ]
    });
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();

    assert!(matches!(
        runtime.complete_level(PLAYER, exit),
        Err(RuntimeError::LevelExit(LevelExitRejection::CompletionAlreadyRecorded { exit: observed }))
            if observed == exit
    ));
    assert_eq!(
        runtime.session().level_exit(exit).unwrap().state,
        LevelExitState::Available
    );
}

#[test]
fn secret_and_exit_bindings_reject_missing_or_wrong_family_ids() {
    let (mut wrong_family, secret, _) = project_with_secret_and_exit_at_player();
    entity_mut(&mut wrong_family, secret)["secretRegion"]["program"] = json!("hazard/nukage");
    let RuntimeError::StoredProject(error) =
        GameRuntime::from_stored_project(&wrong_family.to_string()).unwrap_err()
    else {
        panic!("wrong-family secret program did not fail admission");
    };
    assert_eq!(error.diagnostic().code, diagnostic_code::INVALID_VALUE);
    assert!(error.diagnostic().path.ends_with("secretRegion.program"));

    let (mut missing, _, exit) = project_with_secret_and_exit_at_player();
    entity_mut(&mut missing, exit)["levelExit"]
        .as_object_mut()
        .unwrap()
        .remove("program");
    assert!(GameRuntime::from_stored_project(&missing.to_string()).is_err());
}

fn project_with_secret_and_exit_at_player() -> (Value, EntityId, EntityId) {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    let player_translation = entity_mut(&mut project, PLAYER)["translation"].clone();
    let secret = project["scenes"][0]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity.get("secretRegion").is_some())
        .unwrap();
    let secret_id = EntityId::new(secret["id"].as_u64().unwrap());
    let exit = project["scenes"][0]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity.get("levelExit").is_some())
        .unwrap();
    let exit_id = EntityId::new(exit["id"].as_u64().unwrap());
    entity_mut(&mut project, secret_id)["translation"] = player_translation.clone();
    entity_mut(&mut project, exit_id)["translation"] = player_translation.clone();
    sync_authored_translation(&mut project, secret_id, &player_translation);
    sync_authored_translation(&mut project, exit_id, &player_translation);
    (project, secret_id, exit_id)
}

fn secret_program_mut(project: &mut Value) -> &mut Value {
    project["secretPrograms"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|program| program["id"] == "secret/e1m1-discovery")
        .unwrap()
}

fn level_exit_program_mut(project: &mut Value) -> &mut Value {
    project["levelExitPrograms"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|program| program["id"] == "level-exit/e1m1-completion")
        .unwrap()
}

fn entity_mut(project: &mut Value, id: EntityId) -> &mut Value {
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == id.raw())
        .unwrap()
}
