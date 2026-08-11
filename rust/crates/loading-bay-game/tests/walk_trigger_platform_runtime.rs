use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, FloorActionConfig, FloorActionState,
    GameEntityDefinition, GameEntityDefinitionError, GameRuntime, GameSession, LiftConfig,
    LiftState,
};
use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::core_time::TickDelta;
use rusty_engine::entity_state::EntityDefinition;

const ACTOR: EntityId = EntityId::new(1);
const FLOOR_ACTION: EntityId = EntityId::new(2);
const LIFT: EntityId = EntityId::new(3);
const FLOOR_PLATFORM: EntityId = EntityId::new(4);
const LIFT_PLATFORM: EntityId = EntityId::new(5);

fn trigger_entity(entity: EntityId, name: &str) -> EntityDefinition {
    EntityDefinition::new(entity, name)
        .with_transform(Vec3::ZERO)
        .with_bounds(Vec3::new(-2.0, -2.0, -2.0), Vec3::new(2.0, 2.0, 2.0))
}

fn platform_entity(entity: EntityId, name: &str, translation: Vec3) -> EntityDefinition {
    EntityDefinition::new(entity, name)
        .with_transform(translation)
        .with_bounds(Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5))
        .with_collision(false, false)
}

fn definitions(floor_target: EntityId) -> Vec<GameEntityDefinition> {
    let floor_upper = Vec3::new(10.0, 4.0, 0.0);
    let floor_lowered = Vec3::new(10.0, 1.0, 0.0);
    let lift_raised = Vec3::new(20.0, 5.0, 0.0);
    let lift_lowered = Vec3::new(20.0, 2.0, 0.0);

    vec![
        GameEntityDefinition::new(
            EntityDefinition::new(ACTOR, "actor")
                .with_transform(Vec3::ZERO)
                .with_bounds(Vec3::new(-0.25, -0.25, -0.25), Vec3::new(0.25, 0.25, 0.25))
                .with_collision(true, false),
        ),
        GameEntityDefinition::new(trigger_entity(FLOOR_ACTION, "floor-action")).with_floor_action(
            FloorActionConfig::new(
                floor_target,
                floor_upper,
                floor_lowered,
                TickDelta::new(3),
                "Lower floor",
                "Floor lowering",
                "test.floor-action",
            ),
        ),
        GameEntityDefinition::new(trigger_entity(LIFT, "lift")).with_lift(
            LiftConfig::new(
                LIFT_PLATFORM,
                lift_raised,
                lift_lowered,
                TickDelta::new(2),
                TickDelta::new(3),
            )
            .with_metadata("Use lift", "Lift moving", "test.lift"),
        ),
        GameEntityDefinition::new(platform_entity(
            FLOOR_PLATFORM,
            "floor-platform",
            floor_upper,
        )),
        GameEntityDefinition::new(platform_entity(LIFT_PLATFORM, "lift-platform", lift_raised)),
    ]
}

fn runtime() -> GameRuntime {
    GameRuntime::new(GameSession::from_definitions(definitions(FLOOR_PLATFORM)).unwrap())
}

#[test]
fn named_walk_triggers_follow_exact_one_shot_and_repeatable_timing() {
    let mut runtime = runtime();

    let activation = runtime.run_walk_trigger_phase(ACTOR).unwrap();
    assert_eq!(activation.floor_action.activations.len(), 1);
    assert_eq!(activation.lift.activations.len(), 1);
    assert_eq!(
        runtime.session().floor_action(FLOOR_ACTION).unwrap().state,
        FloorActionState::Lowering
    );
    assert_eq!(
        runtime.session().lift(LIFT).unwrap().state,
        LiftState::Lowering
    );

    let repeated_while_inside = runtime.run_walk_trigger_phase(ACTOR).unwrap();
    assert!(repeated_while_inside.floor_action.activations.is_empty());
    assert!(repeated_while_inside.lift.activations.is_empty());

    runtime.advance_by(1).unwrap();
    let floor_mid = runtime.session().floor_action(FLOOR_ACTION).unwrap();
    assert_eq!(floor_mid.motion_elapsed(), TickDelta::new(1));
    assert_eq!(
        floor_mid
            .target_platform_view
            .transform
            .unwrap()
            .translation,
        Vec3::new(10.0, 3.0, 0.0)
    );
    let lift_mid = runtime.session().lift(LIFT).unwrap();
    assert_eq!(lift_mid.motion_elapsed(), TickDelta::new(1));
    assert_eq!(
        lift_mid.target_platform_view.transform.unwrap().translation,
        Vec3::new(20.0, 3.5, 0.0)
    );

    runtime.advance_by(1).unwrap();
    assert_eq!(
        runtime.session().floor_action(FLOOR_ACTION).unwrap().state,
        FloorActionState::Lowering
    );
    assert_eq!(
        runtime
            .session()
            .floor_action(FLOOR_ACTION)
            .unwrap()
            .target_platform_view
            .transform
            .unwrap()
            .translation,
        Vec3::new(10.0, 2.0, 0.0)
    );
    let lift_waiting = runtime.session().lift(LIFT).unwrap();
    assert_eq!(lift_waiting.state, LiftState::Waiting);
    assert_eq!(lift_waiting.wait_elapsed(), TickDelta::ZERO);
    assert_eq!(
        lift_waiting
            .target_platform_view
            .transform
            .unwrap()
            .translation,
        Vec3::new(20.0, 2.0, 0.0)
    );

    runtime.advance_by(2).unwrap();
    assert_eq!(
        runtime.session().lift(LIFT).unwrap().state,
        LiftState::Waiting
    );
    assert_eq!(
        runtime.session().lift(LIFT).unwrap().wait_elapsed(),
        TickDelta::new(2)
    );
    runtime.advance_by(1).unwrap();
    assert_eq!(
        runtime.session().lift(LIFT).unwrap().state,
        LiftState::Raising
    );
    assert_eq!(
        runtime.session().lift(LIFT).unwrap().wait_elapsed(),
        TickDelta::ZERO
    );
    runtime.advance_by(1).unwrap();
    assert_eq!(
        runtime.session().lift(LIFT).unwrap().state,
        LiftState::Raising
    );
    assert_eq!(
        runtime.session().lift(LIFT).unwrap().motion_elapsed(),
        TickDelta::new(1)
    );
    runtime.advance_by(1).unwrap();

    let floor_lowered = runtime.session().floor_action(FLOOR_ACTION).unwrap();
    assert_eq!(floor_lowered.state, FloorActionState::Lowered);
    assert_eq!(floor_lowered.motion_elapsed(), TickDelta::ZERO);
    assert_eq!(
        floor_lowered
            .target_platform_view
            .transform
            .unwrap()
            .translation,
        Vec3::new(10.0, 1.0, 0.0)
    );
    let lift_raised = runtime.session().lift(LIFT).unwrap();
    assert_eq!(lift_raised.state, LiftState::Raised);
    assert_eq!(lift_raised.motion_elapsed(), TickDelta::ZERO);
    assert_eq!(
        lift_raised
            .target_platform_view
            .transform
            .unwrap()
            .translation,
        Vec3::new(20.0, 5.0, 0.0)
    );

    let mut reopened_snapshot = runtime.snapshot();
    reopened_snapshot
        .lift_triggers
        .as_mut()
        .unwrap()
        .active_overlaps
        .clear();
    let mut reopened = GameRuntime::from_snapshot(reopened_snapshot).unwrap();
    let retriggered = reopened.run_walk_trigger_phase(ACTOR).unwrap();
    assert!(retriggered.floor_action.activations.is_empty());
    assert_eq!(retriggered.lift.activations.len(), 1);
    assert_eq!(
        reopened.session().lift(LIFT).unwrap().state,
        LiftState::Lowering
    );
}

#[test]
fn invalid_named_walk_trigger_target_is_rejected_during_admission() {
    let error = GameSession::from_definitions(definitions(EntityId::new(99))).unwrap_err();
    assert!(matches!(
        error,
        GameEntityDefinitionError::UnknownFloorActionTarget {
            action: FLOOR_ACTION,
            target_platform,
        } if target_platform == EntityId::new(99)
    ));
}

#[test]
fn named_walk_trigger_snapshot_reopens_mid_cycle() {
    let mut runtime = runtime();
    runtime.run_walk_trigger_phase(ACTOR).unwrap();
    runtime.advance_by(1).unwrap();
    let encoded = encode_game_snapshot(&runtime).unwrap();
    let mut reopened = decode_game_snapshot(&encoded).unwrap();

    assert_eq!(encode_game_snapshot(&reopened).unwrap(), encoded);
    assert_eq!(
        reopened.session().floor_action(FLOOR_ACTION).unwrap().state,
        FloorActionState::Lowering
    );
    assert_eq!(
        reopened
            .session()
            .floor_action(FLOOR_ACTION)
            .unwrap()
            .motion_elapsed(),
        TickDelta::new(1)
    );
    assert_eq!(
        reopened.session().lift(LIFT).unwrap().state,
        LiftState::Lowering
    );
    assert_eq!(
        reopened.session().lift(LIFT).unwrap().motion_elapsed(),
        TickDelta::new(1)
    );
    assert_eq!(
        reopened
            .session()
            .floor_action(FLOOR_ACTION)
            .unwrap()
            .target_platform_view
            .transform
            .unwrap()
            .translation,
        Vec3::new(10.0, 3.0, 0.0)
    );

    reopened.advance_by(2).unwrap();
    assert_eq!(
        reopened.session().floor_action(FLOOR_ACTION).unwrap().state,
        FloorActionState::Lowered
    );
    assert_eq!(
        reopened
            .session()
            .floor_action(FLOOR_ACTION)
            .unwrap()
            .target_platform_view
            .transform
            .unwrap()
            .translation,
        Vec3::new(10.0, 1.0, 0.0)
    );
}
