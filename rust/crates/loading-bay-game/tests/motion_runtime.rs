use loading_bay_game::{decode_game_snapshot, encode_game_snapshot, GameRuntime, MotionFact};
use loading_bay_game::{PlayerControlFact, ResolvedPlayerAction};
use rusty_engine::core_ids::EntityId;
use serde_json::{json, Value};

const MOTION_PROJECT: &str = include_str!("../../../../content/generated/motion-lab.project.json");
const LOADING_BAY_PROJECT: &str =
    include_str!("../../../../content/projects/loading-bay.project.json");
const BODY_COUNT: usize = 256;
const FIRST_BODY: u64 = 1_000;
const PLAYER: EntityId = EntityId::new(1);
const PHASES: usize = 180;
const DELTA_SECONDS: f32 = 1.0 / 60.0;

#[test]
fn authored_motion_workload_runs_as_one_central_phase_per_frame() {
    let mut runtime = GameRuntime::from_project_content(MOTION_PROJECT).expect("admit motion lab");
    let scene = runtime.collision_scene().expect("authored collision scene");
    assert_eq!(scene.solid_voxel_count(), BODY_COUNT);
    assert_eq!(scene.resident_chunk_count(), 32);
    assert_eq!(
        runtime.session().entities().kinematic_bodies().count(),
        BODY_COUNT
    );

    let mut blocked = 0usize;
    for _ in 0..PHASES {
        let receipt = runtime
            .run_motion_phase(DELTA_SECONDS)
            .expect("motion phase");
        assert_eq!(receipt.bodies_considered, BODY_COUNT);
        blocked += receipt
            .facts
            .iter()
            .filter(|fact| matches!(fact, MotionFact::Blocked { .. }))
            .count();
    }

    assert_eq!(
        blocked, BODY_COUNT,
        "every runner should meet its wall lane"
    );
    assert!(runtime.session().entities().revision() <= PHASES as u64);
    for raw in FIRST_BODY..FIRST_BODY + BODY_COUNT as u64 {
        let view = runtime
            .session()
            .entity(EntityId::new(raw))
            .expect("runner view");
        let transform = view.transform.expect("runner transform");
        let kinematic = view.kinematic.expect("runner kinematic");
        assert!(transform.translation.x + kinematic.half_extents.x < 8.0);
        assert_eq!(kinematic.velocity.x, 0.0);
    }
}

#[test]
fn snapshot_rebuilds_collision_projection_and_continues_identically() {
    let mut uninterrupted =
        GameRuntime::from_project_content(MOTION_PROJECT).expect("admit motion lab");
    for _ in 0..60 {
        uninterrupted
            .run_motion_phase(DELTA_SECONDS)
            .expect("warmup phase");
    }
    let saved = encode_game_snapshot(&uninterrupted).expect("save motion lab");
    let mut restored = decode_game_snapshot(&saved).expect("restore motion lab");

    for _ in 60..PHASES {
        uninterrupted
            .run_motion_phase(DELTA_SECONDS)
            .expect("uninterrupted phase");
        restored
            .run_motion_phase(DELTA_SECONDS)
            .expect("restored phase");
    }

    assert_eq!(
        restored.session().entities().revision(),
        uninterrupted.session().entities().revision()
    );
    assert_eq!(
        restored.session().entities().projection(),
        uninterrupted.session().entities().projection()
    );
    assert_eq!(
        restored
            .collision_scene()
            .expect("restored scene")
            .solid_voxels(),
        uninterrupted
            .collision_scene()
            .expect("original scene")
            .solid_voxels()
    );
}

#[test]
fn canonical_wall_faces_stop_the_real_player_head_on_and_at_high_delta() {
    let mut runtime = loading_bay_motion_runtime([1.26, 1.5, 3.5], 90.0, 2.0, 0.01, &[]);
    let player = runtime.session().entity(PLAYER).unwrap();
    assert_eq!(player.kinematic.unwrap().half_extents.to_array(), [0.25; 3]);

    move_player(&mut runtime, 1.0, 0.0);
    let receipt = move_player(&mut runtime, 1.0, 0.0);
    assert!(receipt.facts.iter().any(
        |fact| matches!(fact, PlayerControlFact::Blocked { entity, .. } if *entity == PLAYER)
    ));
    let position = player_position(&runtime);
    assert!(
        (position[0] - 1.25).abs() <= 1.0 / 32.0,
        "west stop must remain within the finest authored brush cell of the contact plane: {position:?}",
    );

    let mut high_delta = loading_bay_motion_runtime([5.5, 1.5, 3.5], 90.0, 100.0, 0.25, &[]);
    let receipt = move_player(&mut high_delta, 1.0, 0.0);
    assert!(receipt.facts.iter().any(
        |fact| matches!(fact, PlayerControlFact::Blocked { entity, .. } if *entity == PLAYER)
    ));
    assert!(
        player_position(&high_delta)[0] >= 1.249,
        "high-delta motion cannot tunnel through the west proxy",
    );
}

#[test]
fn canonical_wall_sliding_is_symmetric_and_corner_motion_cannot_tunnel() {
    for z_velocity in [-4.0_f32, 4.0] {
        let start = [1.26, 1.5, 10.5];
        let mut runtime = loading_bay_motion_runtime(start, 90.0, 6.0, 0.1, &[]);
        move_player(&mut runtime, 0.0, if z_velocity < 0.0 { 1.0 } else { -1.0 });
        let position = player_position(&runtime);
        assert!((position[0] - 1.25).abs() <= 1.0 / 32.0, "{position:?}");
        assert!(
            (position[2] - start[2] as f32).signum() == z_velocity.signum(),
            "tangential motion must survive either slide direction: {position:?}",
        );
    }

    let mut corner = loading_bay_motion_runtime([1.3, 1.5, 1.3], 90.0, 100.0, 0.25, &[]);
    move_player(&mut corner, 1.0, 1.0);
    let position = player_position(&corner);
    assert!(position[0] >= 1.249 && position[2] >= 1.249, "{position:?}");
}

#[test]
fn canonical_door_aperture_passes_only_inside_its_authored_proxy_opening() {
    for x in [20.251, 22.749] {
        let mut through = loading_bay_motion_runtime([x, 1.5, 47.5], 180.0, 20.0, 0.2, &[3]);
        assert_eq!(
            through
                .session()
                .entity(PLAYER)
                .unwrap()
                .kinematic
                .unwrap()
                .half_extents
                .to_array(),
            [0.25; 3],
        );
        move_player(&mut through, 1.0, 0.0);
        assert!(
            player_position(&through)[2] > 50.5,
            "player center {x} plus its real half extents must pass at the visible inner frame edge",
        );
    }

    for x in [20.249, 22.751] {
        let mut wall = loading_bay_motion_runtime([x, 1.5, 48.69], 180.0, 6.0, 0.01, &[3]);
        move_player(&mut wall, 1.0, 0.0);
        move_player(&mut wall, 1.0, 0.0);
        let position = player_position(&wall);
        assert!(
            (position[2] - 48.75).abs() <= 0.001,
            "center {x} just outside either real-half-extent edge must meet the adjacent wall: {position:?}",
        );
    }
}

#[test]
fn adjacent_canonical_door_apertures_preserve_both_real_extent_edges() {
    for x in [3.251, 5.749, 8.251, 14.749] {
        let mut through = loading_bay_motion_runtime([x, 1.5, 15.5], 180.0, 4.0, 0.2, &[11, 30]);
        move_player(&mut through, 1.0, 0.0);
        move_player(&mut through, 1.0, 0.0);
        let position = player_position(&through);
        assert!(
            position[2] > 17.0,
            "player center {x} plus its real half extents must pass through either z=17 aperture: {position:?}",
        );
    }

    for x in [2.749, 6.251, 7.749, 15.251] {
        let mut wall = loading_bay_motion_runtime([x, 1.5, 16.69], 180.0, 6.0, 0.01, &[11, 30]);
        move_player(&mut wall, 1.0, 0.0);
        let receipt = move_player(&mut wall, 1.0, 0.0);
        let position = player_position(&wall);
        assert!(
            position[2] <= 16.75 && receipt.facts.iter().any(
                |fact| matches!(fact, PlayerControlFact::Blocked { entity, .. } if *entity == PLAYER)
            ),
            "center {x} just outside either adjacent real-half-extent edge must meet its collision-backed wall: {position:?}",
        );
    }
}

fn loading_bay_motion_runtime(
    translation: [f64; 3],
    yaw_degrees: f64,
    speed: f64,
    step_seconds: f64,
    open_doors: &[u64],
) -> GameRuntime {
    let mut project: Value = serde_json::from_str(LOADING_BAY_PROJECT).unwrap();
    let scene = project["scenes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|scene| scene["id"] == "scene/loading-bay")
        .unwrap();
    let entities = scene["entities"].as_array_mut().unwrap();
    let player = entities
        .iter_mut()
        .find(|entity| entity["id"] == PLAYER.raw())
        .unwrap();
    player["translation"] = json!(translation);
    player["playerController"]["initialYawDegrees"] = json!(yaw_degrees);
    player["playerController"]["moveSpeedUnitsPerSecond"] = json!(speed);
    player["playerController"]["moveStepSeconds"] = json!(step_seconds);
    for door_id in open_doors {
        let door = entities
            .iter_mut()
            .find(|entity| entity["id"] == *door_id)
            .unwrap();
        door["collision"]["enabled"] = json!(false);
    }
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    set_player_yaw(&mut runtime, yaw_degrees as f32);
    runtime
}

fn set_player_yaw(runtime: &mut GameRuntime, target: f32) {
    loop {
        let current = runtime
            .session()
            .player_controller(PLAYER)
            .unwrap()
            .state
            .yaw_degrees;
        let remaining = (target - current + 180.0).rem_euclid(360.0) - 180.0;
        if remaining.abs() <= 0.000_1 {
            break;
        }
        runtime
            .apply_player_action(
                PLAYER,
                ResolvedPlayerAction::Look {
                    yaw_delta: (remaining / 12.0).clamp(-1.0, 1.0),
                    pitch_delta: 0.0,
                },
            )
            .unwrap();
    }
}

fn move_player(
    runtime: &mut GameRuntime,
    forward: f32,
    right: f32,
) -> loading_bay_game::PlayerControlReceipt {
    runtime
        .apply_player_action(PLAYER, ResolvedPlayerAction::Move { forward, right })
        .expect("real player collision-aware motion")
}

fn player_position(runtime: &GameRuntime) -> [f32; 3] {
    runtime
        .session()
        .entity(PLAYER)
        .unwrap()
        .transform
        .unwrap()
        .translation
        .to_array()
}
