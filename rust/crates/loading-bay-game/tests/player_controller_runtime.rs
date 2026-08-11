use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, GameEntityDefinitionError, GameRuntime,
    PlayerControlFact, ProjectContentError, ResolvedPlayerAction, RuntimeError,
};
use rusty_engine::core_ids::EntityId;
use serde_json::{json, Value};

const PROJECT: &str = include_str!("../../../../content/generated/encounter-gate.project.json");
const PLAYER: EntityId = EntityId::new(1);

#[test]
fn semantic_move_actions_use_the_collision_aware_kinematic_path() {
    let mut runtime = GameRuntime::from_project_content(PROJECT).expect("admit player project");
    let before = player_position(&runtime);
    let mut moved = false;
    let mut blocked = false;

    for _ in 0..12 {
        let receipt = runtime
            .apply_player_action(
                PLAYER,
                ResolvedPlayerAction::Move {
                    forward: 1.0,
                    right: 0.0,
                },
            )
            .expect("move action");
        moved |= receipt
            .facts
            .iter()
            .any(|fact| matches!(fact, PlayerControlFact::Moved { .. }));
        blocked |= receipt
            .facts
            .iter()
            .any(|fact| matches!(fact, PlayerControlFact::Blocked { .. }));
    }

    let after = player_position(&runtime);
    assert!(moved, "the player should advance before reaching the wall");
    assert!(blocked, "the generated room shell should stop the player");
    assert!((after.x - before.x).abs() < 0.000_01);
    assert!(after.z < before.z);
    assert!(after.z > 1.0);
    assert_eq!(
        runtime
            .session()
            .entity(PLAYER)
            .unwrap()
            .kinematic
            .unwrap()
            .velocity,
        rusty_engine::core_math::Vec3::ZERO,
        "an action cannot leave polling-style velocity behind",
    );
}

#[test]
fn encounter_gate_blocks_its_canonical_aperture_closed_and_permits_passage_open() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    let player = entity_mut(&mut project, PLAYER.raw());
    player["translation"] = json!([4.5, 1.5, 10.25]);
    player["playerController"]["initialYawDegrees"] = json!(180);
    let mut runtime = GameRuntime::from_project_content(&project.to_string()).unwrap();
    let before = player_position(&runtime);

    let closed = runtime
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Move {
                forward: 1.0,
                right: 0.0,
            },
        )
        .unwrap();

    assert_eq!(player_position(&runtime), before);
    assert!(closed.facts.iter().any(
        |fact| matches!(fact, PlayerControlFact::Blocked { entity, .. } if *entity == PLAYER)
    ));

    runtime.defeat_enemy(PLAYER, EntityId::new(4)).unwrap();
    runtime.defeat_enemy(PLAYER, EntityId::new(5)).unwrap();
    for _ in 0..6 {
        runtime
            .apply_player_action(
                PLAYER,
                ResolvedPlayerAction::Move {
                    forward: 1.0,
                    right: 0.0,
                },
            )
            .unwrap();
    }

    assert!(
        player_position(&runtime).z > 12.0,
        "the opened entity gate must expose the canonical generated aperture"
    );
}

#[test]
fn semantic_look_action_updates_durable_controller_state_without_moving_the_entity() {
    let mut runtime = GameRuntime::from_project_content(PROJECT).unwrap();
    let before_position = player_position(&runtime);
    let before = runtime.session().player_controller(PLAYER).unwrap().state;

    let receipt = runtime
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Look {
                yaw_delta: 0.5,
                pitch_delta: -0.25,
            },
        )
        .unwrap();

    let after = runtime.session().player_controller(PLAYER).unwrap().state;
    assert_eq!(after.yaw_degrees, 6.0);
    assert_eq!(after.pitch_degrees, -13.0);
    assert_eq!(player_position(&runtime), before_position);
    assert!(receipt.motion.is_none());
    assert!(receipt.facts.iter().any(|fact| matches!(
        fact,
        PlayerControlFact::LookChanged {
            before_yaw_degrees,
            after_yaw_degrees,
            ..
        } if *before_yaw_degrees == before.yaw_degrees && *after_yaw_degrees == after.yaw_degrees
    )));
}

#[test]
fn malformed_or_unresolved_action_values_fail_closed() {
    let mut runtime = GameRuntime::from_project_content(PROJECT).unwrap();

    let error = runtime
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Move {
                forward: 1.01,
                right: 0.0,
            },
        )
        .unwrap_err();

    assert!(matches!(error, RuntimeError::InvalidPlayerAction { .. }));
}

#[test]
fn duplicate_authored_keyboard_controls_are_rejected_at_admission() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    let player = entity_mut(&mut project, PLAYER.raw());
    player["playerController"]["bindings"]["moveBackward"] = json!("KeyW");

    let error = GameRuntime::from_project_content(&project.to_string()).unwrap_err();

    assert!(matches!(
        error,
        RuntimeError::Content(ProjectContentError::Definition(
            GameEntityDefinitionError::InvalidPlayerControllerConfig { entity }
        )) if entity == PLAYER
    ));
}

#[test]
fn snapshot_reopen_preserves_player_pose_and_controller_but_derives_no_camera_state() {
    let mut runtime = GameRuntime::from_project_content(PROJECT).unwrap();
    runtime
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Move {
                forward: 1.0,
                right: 0.0,
            },
        )
        .unwrap();
    runtime
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Look {
                yaw_delta: -0.25,
                pitch_delta: 0.5,
            },
        )
        .unwrap();
    let encoded = encode_game_snapshot(&runtime).unwrap();

    assert!(!encoded.contains("camera"));
    let reopened = decode_game_snapshot(&encoded).unwrap();

    assert_eq!(player_position(&runtime), player_position(&reopened));
    assert_eq!(
        runtime.session().player_controller(PLAYER),
        reopened.session().player_controller(PLAYER),
    );
}

#[test]
fn grounded_traversal_steps_up_to_the_authored_limit_and_rejects_taller_walls() {
    let floor = (-1..=4).map(|x| [x, 0, 0]);
    let mut shallow = traversal_runtime(floor.clone().chain([[1, 1, 0]]).collect(), true, 0);
    let stepped = shallow
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Move {
                forward: 1.0,
                right: 0.0,
            },
        )
        .unwrap();
    let shallow_position = player_position(&shallow);
    assert!(shallow_position.x > 0.5 && shallow_position.y > 2.2);
    assert!(stepped
        .facts
        .iter()
        .any(|fact| matches!(fact, PlayerControlFact::Stepped { .. })));

    let floor = (-1..=4).map(|x| [x, 0, 0]);
    let mut tall = traversal_runtime(floor.chain([[1, 1, 0], [1, 2, 0]]).collect(), true, 0);
    let blocked = tall
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Move {
                forward: 1.0,
                right: 0.0,
            },
        )
        .unwrap();
    assert_eq!(player_position(&tall).x, 0.5);
    assert!(blocked
        .facts
        .iter()
        .any(|fact| matches!(fact, PlayerControlFact::Blocked { .. })));
}

#[test]
fn jump_is_grounded_deterministic_and_stops_at_head_collision() {
    let floor = || (-1..=1).map(|x| [x, 0, 0]).collect();
    let mut first = traversal_runtime(floor(), true, 0);
    let mut second = traversal_runtime(floor(), true, 0);

    let jumped = first
        .apply_player_action(PLAYER, ResolvedPlayerAction::Jump)
        .unwrap();
    second
        .apply_player_action(PLAYER, ResolvedPlayerAction::Jump)
        .unwrap();
    assert!(jumped
        .facts
        .iter()
        .any(|fact| matches!(fact, PlayerControlFact::Jumped { .. })));
    assert!(first
        .apply_player_action(PLAYER, ResolvedPlayerAction::Jump)
        .unwrap()
        .facts
        .is_empty());
    let airborne_snapshot = encode_game_snapshot(&first).unwrap();
    let reopened_airborne = decode_game_snapshot(&airborne_snapshot).unwrap();
    assert_eq!(
        first.session().player_controller(PLAYER),
        reopened_airborne.session().player_controller(PLAYER),
        "snapshot must retain grounded eligibility and the live jump velocity"
    );
    for _ in 0..12 {
        first
            .apply_player_action(
                PLAYER,
                ResolvedPlayerAction::Move {
                    forward: 0.0,
                    right: 0.0,
                },
            )
            .unwrap();
        second
            .apply_player_action(
                PLAYER,
                ResolvedPlayerAction::Move {
                    forward: 0.0,
                    right: 0.0,
                },
            )
            .unwrap();
    }
    assert_eq!(player_position(&first), player_position(&second));
    assert!(
        first
            .session()
            .player_controller(PLAYER)
            .unwrap()
            .state
            .grounded,
        "trajectory ended at {:?} with {:?}",
        player_position(&first),
        first.session().player_controller(PLAYER).unwrap().state
    );

    let mut ceiling = traversal_runtime(vec![[0, 0, 0], [0, 2, 0]], true, 0);
    ceiling
        .apply_player_action(PLAYER, ResolvedPlayerAction::Jump)
        .unwrap();
    ceiling
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Move {
                forward: 0.0,
                right: 0.0,
            },
        )
        .unwrap();
    assert_eq!(player_position(&ceiling).y, 1.251);
    assert_eq!(
        ceiling
            .session()
            .player_controller(PLAYER)
            .unwrap()
            .state
            .vertical_velocity,
        0.0
    );

    let mut air_jump = traversal_runtime(vec![[0, 0, 0]], true, 1);
    air_jump
        .apply_player_action(PLAYER, ResolvedPlayerAction::Jump)
        .unwrap();
    air_jump
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Move {
                forward: 0.0,
                right: 0.0,
            },
        )
        .unwrap();
    assert!(air_jump
        .apply_player_action(PLAYER, ResolvedPlayerAction::Jump)
        .unwrap()
        .facts
        .iter()
        .any(|fact| matches!(fact, PlayerControlFact::Jumped { .. })));
    assert!(air_jump
        .apply_player_action(PLAYER, ResolvedPlayerAction::Jump)
        .unwrap()
        .facts
        .is_empty());
}

#[test]
fn ground_probe_snaps_to_contact_and_ledge_departure_clears_grounded_state() {
    let mut runtime = traversal_runtime(vec![[0, 0, 0]], true, 0);
    runtime
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Move {
                forward: 0.0,
                right: 0.0,
            },
        )
        .unwrap();
    assert!((player_position(&runtime).y - 1.2501).abs() < 0.000_1);
    assert!(
        runtime
            .session()
            .player_controller(PLAYER)
            .unwrap()
            .state
            .grounded
    );

    for _ in 0..2 {
        runtime
            .apply_player_action(
                PLAYER,
                ResolvedPlayerAction::Move {
                    forward: 1.0,
                    right: 0.0,
                },
            )
            .unwrap();
    }
    assert!(
        !runtime
            .session()
            .player_controller(PLAYER)
            .unwrap()
            .state
            .grounded
    );
    let ledge_y = player_position(&runtime).y;
    runtime
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Move {
                forward: 0.0,
                right: 0.0,
            },
        )
        .unwrap();
    assert!(player_position(&runtime).y < ledge_y);
}

#[test]
fn downward_contact_never_snaps_through_a_dynamic_blocker() {
    let mut project = traversal_project(vec![[0, 0, 0]], true, 0);
    project["entities"][0]["translation"] = json!([0.5, 3.0, 0.5]);
    project["entities"][0]["playerController"]["traversal"]["gravityUnitsPerSecondSquared"] =
        json!(200);
    project["entities"].as_array_mut().unwrap().push(json!({
        "id": 2,
        "name": "dynamic-platform",
        "translation": [0.5, 1.6, 0.5],
        "collision": { "enabled": true, "staticCollider": false },
        "renderable": { "asset": "primitive/platform", "visible": true },
        "kinematic": { "halfExtents": [0.25, 0.25, 0.25], "velocity": [0, 0, 0] }
    }));
    let mut runtime = GameRuntime::from_project_content(&project.to_string()).unwrap();

    runtime
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Move {
                forward: 0.0,
                right: 0.0,
            },
        )
        .unwrap();

    assert_eq!(player_position(&runtime).y, 3.0);
    assert!(
        !runtime
            .session()
            .player_controller(PLAYER)
            .unwrap()
            .state
            .grounded
    );
}

fn traversal_runtime(
    solid_voxels: Vec<[i64; 3]>,
    jump_enabled: bool,
    max_air_jumps: u8,
) -> GameRuntime {
    let project = traversal_project(solid_voxels, jump_enabled, max_air_jumps);
    GameRuntime::from_project_content(&project.to_string()).unwrap()
}

fn traversal_project(solid_voxels: Vec<[i64; 3]>, jump_enabled: bool, max_air_jumps: u8) -> Value {
    json!({
        "schemaVersion": 6,
        "entities": [{
            "id": 1,
            "name": "player",
            "translation": [0.5, 1.251, 0.5],
            "collision": { "enabled": true, "staticCollider": false },
            "renderable": { "asset": "primitive/player-marker", "visible": true },
            "kinematic": { "halfExtents": [0.25, 0.25, 0.25], "velocity": [0, 0, 0] },
            "playerController": {
                "moveSpeedUnitsPerSecond": 4,
                "moveStepSeconds": 0.1,
                "lookDegreesPerUnit": 12,
                "initialYawDegrees": -90,
                "initialPitchDegrees": 0,
                "traversal": {
                    "maxStepHeight": 1,
                    "gravityUnitsPerSecondSquared": 24,
                    "jumpImpulseUnitsPerSecond": 8,
                    "groundProbeDistance": 0.05,
                    "eyeHeight": 1.2,
                    "manualJumpEnabled": jump_enabled,
                    "maxAirJumps": max_air_jumps
                },
                "bindings": {
                    "moveForward": "KeyW",
                    "moveBackward": "KeyS",
                    "moveLeft": "KeyA",
                    "moveRight": "KeyD",
                    "mouseLook": "pointer",
                    "primaryFire": "Mouse0",
                    "jump": "Space"
                }
            }
        }],
        "voxelCollision": {
            "voxelSize": 1,
            "chunkSize": 8,
            "solidVoxels": solid_voxels
        }
    })
}

fn player_position(runtime: &GameRuntime) -> rusty_engine::core_math::Vec3 {
    runtime
        .session()
        .player_controller(PLAYER)
        .unwrap()
        .entity_view
        .transform
        .unwrap()
        .translation
}

fn entity_mut(project: &mut Value, id: u64) -> &mut Value {
    project["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == id)
        .unwrap()
}
