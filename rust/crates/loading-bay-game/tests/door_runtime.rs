use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, security_door_definitions, DoorState,
    GameEntityDefinition, GameEntityDefinitionError, GameEvent, GameRuntime, RuntimeError,
    SwitchConfig, SwitchEffect, GAME_SNAPSHOT_SCHEMA_VERSION,
};
use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::core_time::{Tick, TickDelta};
use rusty_engine::entity_state::EntityDefinition;

fn timed_security_door(
    motion_duration: TickDelta,
    auto_close_after: Option<TickDelta>,
) -> (loading_bay_game::SecurityDoorIds, GameRuntime) {
    let (ids, mut definitions) = security_door_definitions(auto_close_after);
    definitions
        .iter_mut()
        .find(|definition| definition.entity.id == ids.door)
        .expect("door definition")
        .door
        .as_mut()
        .expect("door config")
        .motion_duration = motion_duration;
    let session =
        loading_bay_game::GameSession::from_definitions(definitions).expect("timed door fixture");
    (ids, GameRuntime::new(session))
}

#[test]
fn timed_door_opens_closes_and_respects_collision_boundaries() {
    let (ids, mut runtime) = timed_security_door(TickDelta::new(3), Some(TickDelta::new(2)));

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
    let opening = runtime.session().door(ids.door).expect("door");
    assert_eq!(opening.state, DoorState::Opening);
    assert_eq!(opening.motion_elapsed(), TickDelta::ZERO);
    assert_eq!(
        opening
            .entity_view
            .transform
            .expect("transform")
            .translation,
        Vec3::ZERO
    );
    assert!(opening.entity_view.collision.expect("collision").enabled);
    assert_eq!(runtime.readout().pending_schedules, 1);

    assert!(matches!(
        runtime.interact(ids.actor, ids.switch),
        Err(RuntimeError::SwitchUnavailable { .. })
    ));
    assert_eq!(
        runtime.session().door(ids.door).expect("door").state,
        DoorState::Opening
    );

    runtime.advance_by(1).expect("first motion tick");
    let intermediate = runtime.session().door(ids.door).expect("door");
    assert_eq!(intermediate.state, DoorState::Opening);
    assert_eq!(intermediate.motion_elapsed(), TickDelta::new(1));
    assert_eq!(
        intermediate
            .entity_view
            .transform
            .expect("transform")
            .translation,
        Vec3::new(0.0, 1.0, 0.0)
    );
    assert!(
        intermediate
            .entity_view
            .collision
            .expect("collision")
            .enabled
    );

    runtime.advance_by(2).expect("complete opening");
    let open = runtime.session().door(ids.door).expect("door");
    assert_eq!(open.state, DoorState::Open);
    assert_eq!(open.motion_elapsed(), TickDelta::new(3));
    assert_eq!(
        open.entity_view.transform.expect("transform").translation,
        Vec3::new(0.0, 3.0, 0.0)
    );
    assert!(!open.entity_view.collision.expect("collision").enabled);

    let waiting = runtime.advance_by(1).expect("auto-close wait");
    assert!(waiting.events.is_empty());
    assert_eq!(
        runtime.session().door(ids.door).expect("door").state,
        DoorState::Open
    );

    let closing = runtime.advance_by(1).expect("scheduled close");
    assert_eq!(closing.tick, Tick::new(5));
    assert_eq!(closing.events.len(), 1);
    assert!(matches!(closing.events[0], GameEvent::DoorClosed { .. }));
    let door = runtime.session().door(ids.door).expect("door");
    assert_eq!(door.state, DoorState::Closing);
    assert_eq!(door.motion_elapsed(), TickDelta::ZERO);
    assert_eq!(
        door.entity_view.transform.expect("transform").translation,
        Vec3::new(0.0, 3.0, 0.0)
    );
    assert!(door.entity_view.collision.expect("collision").enabled);

    runtime.advance_by(2).expect("intermediate closing");
    let closing = runtime.session().door(ids.door).expect("door");
    assert_eq!(closing.state, DoorState::Closing);
    assert_eq!(closing.motion_elapsed(), TickDelta::new(2));
    assert_eq!(
        closing
            .entity_view
            .transform
            .expect("transform")
            .translation,
        Vec3::new(0.0, 1.0, 0.0)
    );
    assert!(closing.entity_view.collision.expect("collision").enabled);

    let closed = runtime.advance_by(1).expect("complete closing");
    assert!(closed.events.is_empty());
    let door = runtime.session().door(ids.door).expect("door");
    assert_eq!(door.state, DoorState::Closed);
    assert_eq!(door.motion_elapsed(), TickDelta::ZERO);
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
    let (ids, mut runtime) = timed_security_door(TickDelta::new(4), Some(TickDelta::new(5)));
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
        DoorState::Opening
    );
    assert_eq!(
        restored
            .session()
            .door(ids.door)
            .expect("door")
            .motion_elapsed(),
        TickDelta::new(2)
    );
    assert!(encoded.contains("\"motionDurationTicks\": 4"));
    assert!(encoded.contains("\"motionElapsedTicks\": 2"));

    restored.advance_by(2).expect("complete opening");
    assert_eq!(
        restored.session().door(ids.door).expect("door").state,
        DoorState::Open
    );
    let receipt = restored.advance_by(5).expect("run due close");
    assert!(matches!(receipt.events[0], GameEvent::DoorClosed { .. }));
    assert_eq!(
        restored.session().door(ids.door).expect("door").state,
        DoorState::Closing
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

#[test]
fn reusable_switch_policy_enforces_range_repeatability_effects_and_reopen() {
    let actor = EntityId::new(201);
    let inner_gate = EntityId::new(202);
    let outer_gate = EntityId::new(203);
    let prime_panel = EntityId::new(204);
    let transfer_panel = EntityId::new(205);
    let remote_panel = EntityId::new(206);
    let door = |id, name: &str, closed: Vec3| {
        let config =
            loading_bay_game::DoorConfig::new(closed, closed + Vec3::new(0.0, 3.0, 0.0), None);
        GameEntityDefinition::new(
            EntityDefinition::new(id, name)
                .with_transform(closed)
                .with_collision(true, false)
                .with_renderable("mesh/airlock", true),
        )
        .as_door(config)
    };
    let definitions = vec![
        GameEntityDefinition::new(EntityDefinition::new(actor, "pilot").with_transform(Vec3::ZERO)),
        door(inner_gate, "inner-airlock", Vec3::new(2.0, 0.0, 0.0)),
        door(outer_gate, "outer-airlock", Vec3::new(3.0, 0.0, 0.0)),
        GameEntityDefinition::new(
            EntityDefinition::new(prime_panel, "prime-panel").with_transform(Vec3::ZERO),
        )
        .with_switch_config(SwitchConfig::new(
            1.5,
            "Prime outer airlock",
            "Prime panel unavailable",
            true,
            [SwitchEffect::OpenDoor(outer_gate)],
        )),
        GameEntityDefinition::new(
            EntityDefinition::new(transfer_panel, "transfer-panel")
                .with_transform(Vec3::new(1.0, 0.0, 0.0)),
        )
        .with_switch_config(SwitchConfig::new(
            2.0,
            "Transfer airlock",
            "Transfer already complete",
            false,
            [
                SwitchEffect::OpenDoor(inner_gate),
                SwitchEffect::CloseDoor(outer_gate),
            ],
        )),
        GameEntityDefinition::new(
            EntityDefinition::new(remote_panel, "remote-panel")
                .with_transform(Vec3::new(10.0, 0.0, 0.0)),
        )
        .with_switch_config(SwitchConfig::new(
            2.0,
            "Remote close",
            "Remote close unavailable",
            true,
            [SwitchEffect::CloseDoor(inner_gate)],
        )),
    ];
    let mut runtime = GameRuntime::new(
        loading_bay_game::GameSession::from_definitions(definitions).expect("fixture"),
    );

    runtime
        .interact(actor, prime_panel)
        .expect("prime outer gate");
    runtime.advance_by(1).expect("complete outer opening");
    assert_eq!(
        runtime.session().door(outer_gate).expect("outer").state,
        DoorState::Open
    );
    runtime
        .interact(actor, transfer_panel)
        .expect("one-shot transfer");
    runtime.advance_by(1).expect("complete transfer motions");
    assert_eq!(
        runtime.session().door(inner_gate).expect("inner").state,
        DoorState::Open
    );
    assert_eq!(
        runtime.session().door(outer_gate).expect("outer").state,
        DoorState::Closed
    );

    let revision = runtime.session().entities().revision();
    assert!(matches!(
        runtime.interact(actor, remote_panel),
        Err(RuntimeError::SwitchOutOfRange { .. })
    ));
    assert_eq!(runtime.session().entities().revision(), revision);

    let encoded = encode_game_snapshot(&runtime).expect("snapshot");
    let mut restored = decode_game_snapshot(&encoded).expect("reopen");
    let restored_switch = restored.session().switch(transfer_panel).expect("switch");
    assert_eq!(restored_switch.activation_count, 1);
    assert_eq!(restored_switch.config.effects.len(), 2);
    assert!(matches!(
        restored.interact(actor, transfer_panel),
        Err(RuntimeError::SwitchUnavailable { presentation, .. })
            if presentation == "Transfer already complete"
    ));
}
