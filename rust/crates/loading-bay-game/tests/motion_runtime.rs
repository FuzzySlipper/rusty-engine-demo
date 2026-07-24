use core_ids::EntityId;
use loading_bay_game::{decode_game_snapshot, encode_game_snapshot, GameRuntime, MotionFact};

const MOTION_PROJECT: &str = include_str!("../../../../content/generated/motion-lab.project.json");
const BODY_COUNT: usize = 256;
const FIRST_BODY: u64 = 1_000;
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
