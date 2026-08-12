use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, GameEntityDefinitionError, GameRuntime,
    PlayerControlFact, PlayerControlReceipt, ProjectContentError, ResolvedPlayerAction,
    RuntimeError,
};
use rusty_engine::core_ids::EntityId;
use rusty_engine::engine_spatial::{CharacterBlockKind, CharacterContactKind};
use serde_json::{json, Value};

const PROJECT: &str = include_str!("../../../../content/generated/encounter-gate.project.json");
const PLAYER: EntityId = EntityId::new(1);

#[test]
fn player_admission_installs_canonical_character_motion_authority() {
    let runtime = GameRuntime::from_project_content(PROJECT).expect("admit player project");
    let entity = runtime.session().entity(PLAYER).unwrap();
    let player = runtime.session().player_controller(PLAYER).unwrap();
    let motion = runtime
        .session()
        .entities()
        .character_motion(PLAYER)
        .expect("admitted player has Engine character motion");

    assert!(
        entity.kinematic.is_none(),
        "legacy kinematic state is not authoritative"
    );
    assert_eq!(entity.character_motion, Some(*motion));
    assert!(
        entity.bounds.is_some(),
        "canonical character bounds are admitted"
    );
    assert_eq!(player.entity_view.character_motion, Some(*motion));
    assert_eq!(player.state.grounded, motion.grounded);
    assert_eq!(
        player.state.vertical_velocity,
        (motion.controlled_velocity + motion.external_velocity).y
    );
}

#[test]
fn admission_and_semantic_input_validation_reject_invalid_contracts() {
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
fn semantic_look_updates_fps_orientation_without_moving_the_character() {
    let mut runtime = GameRuntime::from_project_content(PROJECT).unwrap();
    let before_position = player_position(&runtime);
    let before = runtime.session().player_controller(PLAYER).unwrap().state;

    let receipt = runtime
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Look {
                yaw_delta: 0.5,
                pitch_delta: 0.25,
            },
        )
        .unwrap();

    let after = runtime.session().player_controller(PLAYER).unwrap().state;
    assert!(after.yaw_degrees > before.yaw_degrees);
    assert!(after.pitch_degrees > before.pitch_degrees);
    assert!((-89.0..=89.0).contains(&after.pitch_degrees));
    assert_eq!(player_position(&runtime), before_position);
    assert!(
        receipt.motion.is_none(),
        "look does not fabricate a motion receipt"
    );
    assert!(receipt.facts.iter().any(|fact| matches!(
        fact,
        PlayerControlFact::LookChanged {
            before_yaw_degrees,
            after_yaw_degrees,
            before_pitch_degrees,
            after_pitch_degrees,
            ..
        } if *after_yaw_degrees > *before_yaw_degrees
            && *after_pitch_degrees > *before_pitch_degrees
    )));
}

#[test]
fn snapshot_reopen_preserves_pose_and_canonical_motion_state() {
    let floor = floor_voxels(-2..=2, -2..=2);
    let mut runtime = traversal_runtime(floor, false, 0.0);
    let moved = runtime
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Move {
                forward: 0.5,
                right: 0.25,
            },
        )
        .unwrap();
    assert_canonical_motion_authority(&runtime, &moved);
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
    assert!(
        !encoded.contains("camera"),
        "camera presentation is not saved as gameplay state"
    );
    let reopened = decode_game_snapshot(&encoded).unwrap();

    assert_eq!(player_position(&runtime), player_position(&reopened));
    assert_eq!(
        runtime.session().player_controller(PLAYER),
        reopened.session().player_controller(PLAYER),
    );
    assert_eq!(
        runtime.session().entities().character_motion(PLAYER),
        reopened.session().entities().character_motion(PLAYER),
    );
    assert!(reopened
        .session()
        .entity(PLAYER)
        .unwrap()
        .kinematic
        .is_none());
}

#[test]
fn wall_contact_preserves_tangent_progress_without_penetration() {
    let mut voxels = floor_voxels(-2..=4, -32..=32);
    for y in 1..=3 {
        for z in -32..=32 {
            voxels.push([1, y, z]);
        }
    }
    let mut runtime = traversal_runtime(voxels, false, -90.0);
    let start = player_position(&runtime);
    let mut saw_wall = false;
    let mut saw_tangent_progress = false;

    for _ in 0..30 {
        let receipt = runtime
            .apply_player_action(
                PLAYER,
                ResolvedPlayerAction::Move {
                    forward: 1.0,
                    right: 0.5,
                },
            )
            .unwrap();
        assert_canonical_motion_authority(&runtime, &receipt);
        let motion = receipt.motion.as_ref().unwrap();
        saw_wall |= motion.blocks.contains(&CharacterBlockKind::Wall)
            || motion
                .contacts
                .iter()
                .any(|contact| contact.kind == CharacterContactKind::Wall);
        saw_tangent_progress |= player_position(&runtime).z > start.z;
    }

    let end = player_position(&runtime);
    let bounds = runtime.session().entity(PLAYER).unwrap().bounds.unwrap();
    assert!(
        saw_wall,
        "the Engine receipt should report the wall contact"
    );
    assert!(
        saw_tangent_progress,
        "wall collision should preserve useful tangent motion"
    );
    assert!(
        end.x + bounds.max.x <= 1.0 + 0.05,
        "character crossed the wall plane: position={end:?}, bounds={bounds:?}"
    );
}

#[test]
fn grounded_motion_accepts_a_step_and_rejects_a_taller_obstacle() {
    let mut short_step = traversal_project(floor_voxels(-2..=2, -4..=2), false, 0.0);
    short_step["entities"][0]["playerController"]["traversal"]["maxStepHeight"] = json!(0.4);
    short_step["entities"].as_array_mut().unwrap().push(json!({
        "id": 2,
        "name": "low-step",
        "translation": [0.5, 1.125, -1.0],
        "collision": { "enabled": true, "staticCollider": false },
        "renderable": { "asset": "primitive/step", "visible": true },
        "kinematic": { "halfExtents": [0.5, 0.125, 0.5], "velocity": [0, 0, 0] }
    }));
    let mut runtime = GameRuntime::from_project_content(&short_step.to_string()).unwrap();
    let start = player_position(&runtime);
    let mut stepped = None;

    for _ in 0..30 {
        let receipt = runtime
            .apply_player_action(
                PLAYER,
                ResolvedPlayerAction::Move {
                    forward: 1.0,
                    right: 0.0,
                },
            )
            .unwrap();
        assert_canonical_motion_authority(&runtime, &receipt);
        if receipt
            .facts
            .iter()
            .any(|fact| matches!(fact, PlayerControlFact::Stepped { .. }))
        {
            stepped = Some(receipt);
            break;
        }
    }

    let stepped = stepped.expect("Engine controller should report the accepted step");
    let stepped_motion = stepped.motion.as_ref().unwrap();
    assert!(stepped_motion.motion_after.grounded);
    assert!(stepped.facts.iter().any(|fact| matches!(
        fact,
        PlayerControlFact::Stepped { before, after, .. } if after.y > before.y
    )));
    assert!(player_position(&runtime).z < start.z);

    let mut tall_wall = floor_voxels(-2..=4, -2..=2);
    tall_wall.extend([[1, 1, 0], [1, 2, 0]]);
    let mut runtime = traversal_runtime(tall_wall, false, -90.0);
    let mut saw_block = false;
    let mut saw_accepted_step = false;
    for _ in 0..30 {
        let receipt = runtime
            .apply_player_action(
                PLAYER,
                ResolvedPlayerAction::Move {
                    forward: 1.0,
                    right: 0.0,
                },
            )
            .unwrap();
        assert_canonical_motion_authority(&runtime, &receipt);
        let motion = receipt.motion.as_ref().unwrap();
        saw_block |= motion.blocks.contains(&CharacterBlockKind::Wall);
        saw_accepted_step |= receipt
            .facts
            .iter()
            .any(|fact| matches!(fact, PlayerControlFact::Stepped { .. }));
    }

    let position = player_position(&runtime);
    let bounds = runtime.session().entity(PLAYER).unwrap().bounds.unwrap();
    assert!(saw_block, "the tall obstacle should be reported as blocked");
    assert!(
        !saw_accepted_step,
        "the taller obstacle must not be accepted as a step"
    );
    assert!(position.x + bounds.max.x <= 1.0 + 0.05);
}

#[test]
fn floor_support_is_stable_and_ledge_departure_becomes_airborne() {
    let floor = floor_voxels(-2..=2, 0..=4);
    let mut runtime = traversal_runtime(floor, false, 0.0);
    let first = runtime
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Move {
                forward: 0.0,
                right: 0.0,
            },
        )
        .unwrap();
    assert_canonical_motion_authority(&runtime, &first);
    assert!(first.motion.as_ref().unwrap().motion_after.grounded);
    assert!(has_ground_support(&first));
    let first_y = player_position(&runtime).y;

    let second = runtime
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Move {
                forward: 0.0,
                right: 0.0,
            },
        )
        .unwrap();
    assert_canonical_motion_authority(&runtime, &second);
    assert!(second.motion.as_ref().unwrap().motion_after.grounded);
    assert!(has_ground_support(&second));
    assert!((player_position(&runtime).y - first_y).abs() < 0.01);

    let mut departure = None;
    for _ in 0..40 {
        let receipt = runtime
            .apply_player_action(
                PLAYER,
                ResolvedPlayerAction::Move {
                    forward: 1.0,
                    right: 0.0,
                },
            )
            .unwrap();
        assert_canonical_motion_authority(&runtime, &receipt);
        if !receipt.motion.as_ref().unwrap().motion_after.grounded {
            departure = Some(receipt);
            break;
        }
    }

    let departure = departure.expect("walking beyond the finite floor should lose support");
    let departure_y = player_position(&runtime).y;
    assert!(departure.motion.as_ref().unwrap().ground.is_none());
    let falling = runtime
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Move {
                forward: 0.0,
                right: 0.0,
            },
        )
        .unwrap();
    assert_canonical_motion_authority(&runtime, &falling);
    assert!(player_position(&runtime).y < departure_y);
    assert!(!falling.motion.as_ref().unwrap().motion_after.grounded);
}

#[test]
fn jump_uses_engine_motion_and_respects_a_ceiling() {
    let floor = floor_voxels(-2..=2, -2..=2);
    let mut runtime = traversal_runtime(floor.clone(), true, 0.0);
    let start = player_position(&runtime);
    let jumped = runtime
        .apply_player_action(PLAYER, ResolvedPlayerAction::Jump)
        .unwrap();
    assert_canonical_motion_authority(&runtime, &jumped);
    let jumped_motion = jumped.motion.as_ref().unwrap();
    assert!(jumped.facts.iter().any(|fact| matches!(
        fact,
        PlayerControlFact::Jumped { entity, impulse } if *entity == PLAYER && *impulse > 0.0
    )));
    assert!(!jumped_motion.motion_after.grounded);
    assert!(jumped_motion.motion_after.controlled_velocity.y > 0.0);

    let mut landed = false;
    for _ in 0..30 {
        let receipt = runtime
            .apply_player_action(
                PLAYER,
                ResolvedPlayerAction::Move {
                    forward: 0.0,
                    right: 0.0,
                },
            )
            .unwrap();
        assert_canonical_motion_authority(&runtime, &receipt);
        landed |= receipt.motion.as_ref().unwrap().motion_after.grounded;
        if landed {
            break;
        }
    }
    assert!(
        landed,
        "jump should return to Engine-reported ground support"
    );
    assert!((player_position(&runtime).y - start.y).abs() < 0.1);

    let mut ceiling_voxels = floor;
    ceiling_voxels.push([0, 3, 0]);
    let mut ceiling = traversal_runtime(ceiling_voxels, true, 0.0);
    let jumped_under_ceiling = ceiling
        .apply_player_action(PLAYER, ResolvedPlayerAction::Jump)
        .unwrap();
    let mut ceiling_hit = has_blocked_fact(&jumped_under_ceiling).then_some(jumped_under_ceiling);
    for _ in 0..20 {
        if ceiling_hit.is_some() {
            break;
        }
        let receipt = ceiling
            .apply_player_action(
                PLAYER,
                ResolvedPlayerAction::Move {
                    forward: 0.0,
                    right: 0.0,
                },
            )
            .unwrap();
        assert_canonical_motion_authority(&ceiling, &receipt);
        if receipt
            .motion
            .as_ref()
            .unwrap()
            .blocks
            .contains(&CharacterBlockKind::Ceiling)
            || has_blocked_fact(&receipt)
        {
            ceiling_hit = Some(receipt);
            break;
        }
    }

    let ceiling_hit = ceiling_hit.expect("Engine receipt should report the ceiling contact");
    let ceiling_motion = ceiling_hit.motion.as_ref().unwrap();
    assert!(ceiling_motion.motion_after.controlled_velocity.y <= 0.0);
    let ceiling_position = player_position(&ceiling);
    let bounds = ceiling.session().entity(PLAYER).unwrap().bounds.unwrap();
    assert!(ceiling_position.y + bounds.max.y <= 3.0 + 0.05);
}

fn assert_canonical_motion_authority(runtime: &GameRuntime, receipt: &PlayerControlReceipt) {
    let motion_receipt = receipt
        .motion
        .as_ref()
        .expect("movement action returns an Engine motion receipt");
    let motion = runtime
        .session()
        .entities()
        .character_motion(PLAYER)
        .expect("player retains Engine character motion");
    assert_eq!(motion_receipt.motion_after, *motion);
    assert!(runtime
        .session()
        .entity(PLAYER)
        .unwrap()
        .kinematic
        .is_none());
    let controller = runtime.session().player_controller(PLAYER).unwrap();
    assert_eq!(controller.state.grounded, motion.grounded);
    assert_eq!(
        controller.state.vertical_velocity,
        (motion.controlled_velocity + motion.external_velocity).y
    );
}

fn has_ground_support(receipt: &PlayerControlReceipt) -> bool {
    let motion = receipt.motion.as_ref().unwrap();
    motion.ground.is_some()
        || motion
            .floor_probe
            .as_ref()
            .is_some_and(|probe| probe.accepted_support.is_some())
}

fn has_blocked_fact(receipt: &PlayerControlReceipt) -> bool {
    receipt
        .facts
        .iter()
        .any(|fact| matches!(fact, PlayerControlFact::Blocked { .. }))
}

fn traversal_runtime(
    solid_voxels: Vec<[i64; 3]>,
    jump_enabled: bool,
    initial_yaw_degrees: f32,
) -> GameRuntime {
    let project = traversal_project(solid_voxels, jump_enabled, initial_yaw_degrees);
    GameRuntime::from_project_content(&project.to_string()).unwrap()
}

fn traversal_project(
    solid_voxels: Vec<[i64; 3]>,
    jump_enabled: bool,
    initial_yaw_degrees: f32,
) -> Value {
    json!({
        "schemaVersion": 6,
        "entities": [{
            "id": 1,
            "name": "player",
            "translation": [0.5, 1.25, 0.5],
            "collision": { "enabled": true, "staticCollider": false },
            "renderable": { "asset": "primitive/player-marker", "visible": true },
            "kinematic": { "halfExtents": [0.25, 0.25, 0.25], "velocity": [0, 0, 0] },
            "playerController": {
                "moveSpeedUnitsPerSecond": 4,
                "moveStepSeconds": 0.1,
                "lookDegreesPerUnit": 12,
                "initialYawDegrees": initial_yaw_degrees,
                "initialPitchDegrees": 0,
                "traversal": {
                    "maxStepHeight": 1,
                    "gravityUnitsPerSecondSquared": 24,
                    "jumpImpulseUnitsPerSecond": 8,
                    "groundProbeDistance": 0.05,
                    "eyeHeight": 1.2,
                    "manualJumpEnabled": jump_enabled,
                    "maxAirJumps": 0
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

fn floor_voxels(
    x_range: std::ops::RangeInclusive<i64>,
    z_range: std::ops::RangeInclusive<i64>,
) -> Vec<[i64; 3]> {
    x_range
        .flat_map(|x| z_range.clone().map(move |z| [x, 0, z]))
        .collect()
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
