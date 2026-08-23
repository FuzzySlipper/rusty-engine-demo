use loading_bay_game::{FloorActionState, GameRuntime, LiftState};
use rusty_engine::core_ids::EntityId;
use serde_json::Value;

const PROJECT: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");
const PLAYER: EntityId = EntityId::new(1);

fn activated_runtime() -> (GameRuntime, EntityId, EntityId) {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    // Generic scene transforms live only on authored scene nodes.
    let entities = project["scenes"][0]["entities"].as_array().unwrap();
    let floor_id = EntityId::new(
        entities
            .iter()
            .find(|entity| entity.get("floorAction").is_some())
            .unwrap()["id"]
            .as_u64()
            .unwrap(),
    );
    let lift_id = EntityId::new(
        entities
            .iter()
            .find(|entity| entity.get("lift").is_some())
            .unwrap()["id"]
            .as_u64()
            .unwrap(),
    );
    let player_translation = authored_translation(&project, PLAYER);
    for id in [floor_id, lift_id] {
        sync_authored_translation(&mut project, id, &player_translation);
    }
    (
        GameRuntime::from_stored_project(&project.to_string()).unwrap(),
        floor_id,
        lift_id,
    )
}

fn authored_translation(project: &Value, id: EntityId) -> Value {
    project["scenes"][0]["authoredScene"]["nodes"]
        .as_array()
        .expect("authored scene nodes")
        .iter()
        .find(|node| node["id"] == id.raw())
        .expect("authored scene node for entity")["transform"]["translation"]
        .clone()
}

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
fn canonical_walk_trigger_programs_preserve_e1m1_phase_timing() {
    let (mut runtime, floor, lift) = activated_runtime();
    let receipt = runtime.run_walk_trigger_phase(PLAYER).unwrap();
    assert_eq!(receipt.floor_action.activations.len(), 1);
    assert_eq!(receipt.lift.activations.len(), 1);
    assert_eq!(
        runtime.session().floor_action(floor).unwrap().state,
        FloorActionState::Lowering
    );
    assert_eq!(
        runtime.session().lift(lift).unwrap().state,
        LiftState::Lowering
    );

    runtime.run_walk_trigger_motion_phase().unwrap();
    assert_eq!(
        runtime
            .session()
            .floor_action(floor)
            .unwrap()
            .motion_elapsed()
            .raw(),
        1
    );
    assert_eq!(
        runtime.session().lift(lift).unwrap().motion_elapsed().raw(),
        1
    );

    for _ in 1..59 {
        runtime.run_walk_trigger_motion_phase().unwrap();
    }
    assert_eq!(
        runtime.session().floor_action(floor).unwrap().state,
        FloorActionState::Lowered
    );
    assert_eq!(
        runtime.session().lift(lift).unwrap().state,
        LiftState::Lowering
    );
    for _ in 59..65 {
        runtime.run_walk_trigger_motion_phase().unwrap();
    }
    assert_eq!(
        runtime.session().lift(lift).unwrap().state,
        LiftState::Waiting
    );
    for _ in 0..180 {
        runtime.run_walk_trigger_motion_phase().unwrap();
    }
    assert_eq!(
        runtime.session().lift(lift).unwrap().state,
        LiftState::Raising
    );
    for _ in 0..65 {
        runtime.run_walk_trigger_motion_phase().unwrap();
    }
    assert_eq!(
        runtime.session().lift(lift).unwrap().state,
        LiftState::Raised
    );

    let floors = runtime.session().floor_action_programs();
    assert_eq!(floors.programs.len(), 1);
    assert_eq!(floors.bindings.len(), 1);
    let lifts = runtime.session().lift_programs();
    assert_eq!(lifts.programs.len(), 1);
    assert_eq!(lifts.bindings.len(), 1);
    assert!(runtime.session().gameplay_outcome().is_none());
}

#[test]
fn changing_only_floor_program_composition_changes_rust_owned_transition() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    let activation_steps = project["floorActionPrograms"][0]["program"]["steps"][0]["thenProgram"]
        ["steps"]
        .as_array_mut()
        .unwrap();
    activation_steps.retain(|step| step["operation"] != "requestLowerBoundPlatform");
    let floor_id = EntityId::new(
        project["scenes"][0]["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entity| entity.get("floorAction").is_some())
            .unwrap()["id"]
            .as_u64()
            .unwrap(),
    );
    let player_translation = authored_translation(&project, PLAYER);
    sync_authored_translation(&mut project, floor_id, &player_translation);
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();

    let receipt = runtime.run_walk_trigger_phase(PLAYER).unwrap();
    assert_eq!(receipt.floor_action.activations.len(), 1);
    assert_eq!(
        runtime.session().floor_action(floor_id).unwrap().state,
        FloorActionState::Armed
    );
    assert!(runtime.session().gameplay_outcome().is_none());
}

#[test]
fn lift_return_is_program_selected_and_motion_predicates_use_frozen_state() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity.get("lift").is_some())
        .unwrap()["lift"]["motionDurationTicks"] = Value::from(1);
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity.get("lift").is_some())
        .unwrap()["lift"]["loweredWaitTicks"] = Value::from(0);
    let root_steps = project["liftPrograms"][0]["program"]["steps"]
        .as_array_mut()
        .unwrap();
    root_steps.retain(|step| step["thenProgram"]["operation"] != "advanceRaising");
    let lift_id = EntityId::new(
        project["scenes"][0]["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entity| entity.get("lift").is_some())
            .unwrap()["id"]
            .as_u64()
            .unwrap(),
    );
    let player_translation = authored_translation(&project, PLAYER);
    sync_authored_translation(&mut project, lift_id, &player_translation);
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();

    runtime.run_walk_trigger_phase(PLAYER).unwrap();
    runtime.run_walk_trigger_motion_phase().unwrap();
    assert_eq!(
        runtime.session().lift(lift_id).unwrap().state,
        LiftState::Waiting
    );
    runtime.run_walk_trigger_motion_phase().unwrap();
    assert_eq!(
        runtime.session().lift(lift_id).unwrap().state,
        LiftState::Raising
    );
    runtime.run_walk_trigger_motion_phase().unwrap();
    assert_eq!(
        runtime.session().lift(lift_id).unwrap().state,
        LiftState::Raising
    );
}

#[test]
fn late_floor_program_failure_rolls_back_both_walk_trigger_families_and_trigger_revision() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    project["floorActionPrograms"][0]["program"]["steps"][0]["thenProgram"]["steps"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({ "kind": "operation", "operation": "advanceLowering" }));
    let entities = project["scenes"][0]["entities"].as_array().unwrap();
    let floor_id = EntityId::new(
        entities
            .iter()
            .find(|entity| entity.get("floorAction").is_some())
            .unwrap()["id"]
            .as_u64()
            .unwrap(),
    );
    let lift_id = EntityId::new(
        entities
            .iter()
            .find(|entity| entity.get("lift").is_some())
            .unwrap()["id"]
            .as_u64()
            .unwrap(),
    );
    let player_translation = authored_translation(&project, PLAYER);
    for id in [floor_id, lift_id] {
        sync_authored_translation(&mut project, id, &player_translation);
    }
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();

    assert!(runtime.run_walk_trigger_phase(PLAYER).is_err());
    assert_eq!(
        runtime.session().floor_action(floor_id).unwrap().state,
        FloorActionState::Armed
    );
    assert_eq!(
        runtime.session().lift(lift_id).unwrap().state,
        LiftState::Raised
    );
    assert!(runtime.run_walk_trigger_phase(PLAYER).is_err());
}

#[test]
fn missing_or_wrong_family_walk_trigger_binding_is_rejected_before_session_admission() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    let floor = project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity.get("floorAction").is_some())
        .unwrap();
    floor["floorAction"]["program"] = Value::from("lift/e1m1-cycle");
    assert!(GameRuntime::from_stored_project(&project.to_string()).is_err());
}
