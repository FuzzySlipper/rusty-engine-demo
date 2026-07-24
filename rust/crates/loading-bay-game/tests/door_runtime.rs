use core_ids::EntityId;
use core_math::Vec3;
use core_time::{Tick, TickDelta};
use entity_state::EntityDefinition;
use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, security_door_definitions, DoorState,
    GameEntityDefinition, GameEntityDefinitionError, GameEvent, GameRuntime,
    GAME_SNAPSHOT_SCHEMA_VERSION,
};

#[test]
fn switch_opens_and_scheduled_intent_closes_door() {
    let (ids, mut runtime) = GameRuntime::security_door(Some(TickDelta::new(3))).expect("fixture");

    let opened = runtime
        .interact(ids.actor, ids.switch)
        .expect("interaction");
    assert_eq!(opened.tick, Tick::ZERO);
    assert_eq!(opened.events.len(), 2);
    assert!(matches!(
        opened.events[0],
        GameEvent::SwitchActivated { .. }
    ));
    assert!(matches!(opened.events[1], GameEvent::DoorOpened { .. }));
    let open = runtime.session().door(ids.door).expect("door");
    assert_eq!(open.state, DoorState::Open);
    assert_eq!(
        open.entity_view.transform.expect("transform").translation,
        Vec3::new(0.0, 3.0, 0.0)
    );
    assert!(!open.entity_view.collision.expect("collision").enabled);
    assert_eq!(runtime.readout().pending_schedules, 1);

    let before_due = runtime.advance_by(2).expect("advance");
    assert!(before_due.events.is_empty());
    assert_eq!(
        runtime.session().door(ids.door).expect("door").state,
        DoorState::Open
    );

    let closed = runtime.advance_by(1).expect("due close");
    assert_eq!(closed.tick, Tick::new(3));
    assert_eq!(closed.events.len(), 1);
    assert!(matches!(closed.events[0], GameEvent::DoorClosed { .. }));
    let door = runtime.session().door(ids.door).expect("door");
    assert_eq!(door.state, DoorState::Closed);
    assert_eq!(
        door.entity_view.transform.expect("transform").translation,
        Vec3::ZERO
    );
    assert!(door.entity_view.collision.expect("collision").enabled);
    assert_eq!(runtime.readout().pending_schedules, 0);
}

#[test]
fn latched_door_is_a_data_only_configuration_variation() {
    let (ids, mut runtime) = GameRuntime::security_door(None).expect("fixture");
    runtime
        .interact(ids.actor, ids.switch)
        .expect("interaction");
    runtime.advance_by(20).expect("advance");

    assert_eq!(
        runtime.session().door(ids.door).expect("door").state,
        DoorState::Open
    );
    assert_eq!(runtime.readout().pending_schedules, 0);
}

#[test]
fn save_reopen_preserves_pending_close_without_event_history() {
    let (ids, mut runtime) = GameRuntime::security_door(Some(TickDelta::new(5))).expect("fixture");
    runtime
        .interact(ids.actor, ids.switch)
        .expect("interaction");
    runtime.advance_by(2).expect("advance");
    let encoded = encode_game_snapshot(&runtime).expect("save");
    assert!(encoded.contains(&format!(
        "\"schemaVersion\": {GAME_SNAPSHOT_SCHEMA_VERSION}"
    )));
    assert!(encoded.contains("\"entities\""));
    assert!(!encoded.contains("\"world\""));

    let mut restored = decode_game_snapshot(&encoded).expect("restore");
    assert_eq!(restored.tick(), Tick::new(2));
    assert_eq!(restored.readout().pending_schedules, 1);
    assert!(restored.readout().journal.is_empty());
    assert_eq!(
        restored.session().door(ids.door).expect("door").state,
        DoorState::Open
    );

    let receipt = restored.advance_by(3).expect("run due close");
    assert!(matches!(receipt.events[0], GameEvent::DoorClosed { .. }));
    assert_eq!(
        restored.session().door(ids.door).expect("door").state,
        DoorState::Closed
    );
}

#[test]
fn invalid_control_relationship_fails_before_runtime() {
    let (ids, mut definitions) = security_door_definitions(None);
    definitions.push(GameEntityDefinition::new(EntityDefinition::new(
        EntityId::new(99),
        "not-a-door",
    )));
    let switch = definitions
        .iter_mut()
        .find(|definition| definition.entity.id == ids.switch)
        .expect("switch definition");
    switch.controls_targets = vec![EntityId::new(99)];

    let error =
        loading_bay_game::GameSession::from_definitions(definitions).expect_err("invalid target");
    assert!(matches!(
        error,
        GameEntityDefinitionError::ControlTargetIsNotDoor { .. }
    ));
}

#[test]
fn rejected_interaction_does_not_mutate_runtime() {
    let (ids, mut runtime) = GameRuntime::security_door(None).expect("fixture");
    let revision = runtime.session().entities().revision();
    assert!(runtime.interact(ids.actor, EntityId::new(404)).is_err());
    assert_eq!(runtime.session().entities().revision(), revision);
    assert_eq!(
        runtime.session().door(ids.door).expect("door").state,
        DoorState::Closed
    );
    assert!(runtime.readout().journal.is_empty());
}
