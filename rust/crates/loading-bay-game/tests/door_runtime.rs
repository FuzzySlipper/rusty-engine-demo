use loading_bay_game::{
    decode_game_snapshot, decode_project_document, encode_game_snapshot, DoorState, GameEvent,
    GameRuntime, RuntimeError, GAME_SNAPSHOT_SCHEMA_VERSION,
};
use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::core_time::{Tick, TickDelta};
use serde_json::Value;

const PLAYER: EntityId = EntityId::new(1);
const DOOR_SWITCH: EntityId = EntityId::new(141);
const E1M1: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");

fn door_runtime(
    motion_duration: u64,
    auto_close_after: Option<u64>,
    repeatable: Option<bool>,
) -> GameRuntime {
    let mut project: Value = serde_json::from_str(E1M1).expect("E1M1 project");
    let door_translation = authored_translation(&project, DOOR_SWITCH);
    let entities = project["scenes"][0]["entities"]
        .as_array_mut()
        .expect("entry entities");
    let door = entities
        .iter_mut()
        .find(|entity| entity["id"] == DOOR_SWITCH.raw())
        .expect("canonical E1M1 door/switch");
    door["door"]["motionDurationTicks"] = motion_duration.into();
    match auto_close_after {
        Some(ticks) => door["door"]["autoCloseAfterTicks"] = ticks.into(),
        None => {
            door["door"]
                .as_object_mut()
                .expect("door object")
                .remove("autoCloseAfterTicks");
        }
    }
    if let Some(repeatable) = repeatable {
        door["switch"]["repeatable"] = repeatable.into();
    }
    sync_authored_translation(&mut project, PLAYER, &door_translation);
    GameRuntime::from_stored_project(&project.to_string()).expect("current authored fixture")
}

/// Reads an authored scene-node transform; generic scene fields live only
/// on nodes, so fixture mutations target nodes exclusively.
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
fn current_authored_door_program_opens_closes_and_respects_collision_boundaries() {
    let mut runtime = door_runtime(3, Some(2), None);

    let opened = runtime.interact(PLAYER, DOOR_SWITCH).expect("interaction");
    assert_eq!(opened.tick, Tick::ZERO);
    assert!(matches!(
        opened.events.as_slice(),
        [
            GameEvent::SwitchActivated { .. },
            GameEvent::DoorOpened { .. }
        ]
    ));
    let opening = runtime.session().door(DOOR_SWITCH).expect("door");
    assert_eq!(opening.state, DoorState::Opening);
    assert_eq!(opening.motion_elapsed(), TickDelta::ZERO);
    assert!(opening.entity_view.collision.expect("collision").enabled);

    runtime.advance_by(1).expect("first motion tick");
    assert_eq!(
        runtime
            .session()
            .door(DOOR_SWITCH)
            .expect("door")
            .motion_elapsed(),
        TickDelta::new(1)
    );
    runtime.advance_by(2).expect("complete opening");
    let open = runtime.session().door(DOOR_SWITCH).expect("door");
    assert_eq!(open.state, DoorState::Open);
    assert!(!open.entity_view.collision.expect("collision").enabled);

    assert!(runtime.advance_by(1).expect("wait").events.is_empty());
    let closing = runtime.advance_by(1).expect("scheduled close");
    assert!(matches!(
        closing.events.as_slice(),
        [GameEvent::DoorClosed { .. }]
    ));
    assert_eq!(
        runtime.session().door(DOOR_SWITCH).expect("door").state,
        DoorState::Closing
    );
    assert!(
        runtime
            .session()
            .door(DOOR_SWITCH)
            .expect("door")
            .entity_view
            .collision
            .expect("collision")
            .enabled
    );
    runtime.advance_by(3).expect("complete closing");
    assert_eq!(
        runtime.session().door(DOOR_SWITCH).expect("door").state,
        DoorState::Closed
    );
}

#[test]
fn current_authored_door_program_honors_repeatability() {
    let mut runtime = door_runtime(1, None, Some(false));

    runtime.interact(PLAYER, DOOR_SWITCH).unwrap();
    runtime.advance_by(1).unwrap();
    assert_eq!(
        runtime.session().door(DOOR_SWITCH).unwrap().state,
        DoorState::Open
    );
    assert!(matches!(
        runtime.interact(PLAYER, DOOR_SWITCH),
        Err(RuntimeError::SwitchUnavailable { .. })
    ));
}

#[test]
fn current_authored_door_snapshot_preserves_pending_close_without_event_history() {
    let mut runtime = door_runtime(4, Some(5), None);
    runtime.interact(PLAYER, DOOR_SWITCH).expect("interaction");
    runtime.advance_by(2).expect("advance");
    let encoded = encode_game_snapshot(&runtime).expect("save");
    assert!(encoded.contains(&format!(
        "\"schemaVersion\": {GAME_SNAPSHOT_SCHEMA_VERSION}"
    )));

    let mut restored = decode_game_snapshot(&encoded).expect("raw snapshot restore");
    assert_eq!(restored.tick(), Tick::new(2));
    assert_eq!(restored.readout().pending_schedules, 1);
    assert!(restored.readout().journal.is_empty());
    assert_eq!(
        restored.session().door(DOOR_SWITCH).expect("door").state,
        DoorState::Opening
    );
    let authored = decode_project_document(E1M1)
        .expect("current authored project")
        .project;
    restored
        .reattach_authored_gameplay_programs(&authored)
        .expect("product snapshot boundary reattaches authored catalogs");
    restored.advance_by(2).expect("complete opening");
    assert_eq!(
        restored.session().door(DOOR_SWITCH).expect("door").state,
        DoorState::Open
    );
    let receipt = restored.advance_by(5).expect("run due close");
    assert!(matches!(
        receipt.events.as_slice(),
        [GameEvent::DoorClosed { .. }]
    ));
}

#[test]
fn rejected_current_authored_door_interaction_does_not_mutate_runtime() {
    let mut runtime = door_runtime(1, None, None);
    let revision = runtime.session().entities().revision();
    assert!(runtime.interact(PLAYER, EntityId::new(404)).is_err());
    assert_eq!(runtime.session().entities().revision(), revision);
    assert_eq!(
        runtime.session().door(DOOR_SWITCH).unwrap().state,
        DoorState::Closed
    );
    assert!(runtime.readout().journal.is_empty());

    let translation = runtime
        .session()
        .door(DOOR_SWITCH)
        .unwrap()
        .entity_view
        .transform
        .unwrap()
        .translation;
    assert_eq!(translation, Vec3::new(144.5, 9.0, 148.0));
}
