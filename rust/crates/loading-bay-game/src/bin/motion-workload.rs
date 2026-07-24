use std::time::Instant;

use loading_bay_game::{encode_game_snapshot, GameRuntime, MotionFact};
use serde_json::{json, Value};

const PROJECT: &str = include_str!("../../../../../content/generated/motion-lab.project.json");
const DEFAULT_PHASES: usize = 180;
const DELTA_SECONDS: f32 = 1.0 / 60.0;

fn main() {
    let mut phases = DEFAULT_PHASES;
    let mut bodies = 256usize;
    let mut matrix = false;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--phases" => {
                phases = args
                    .next()
                    .expect("--phases needs a value")
                    .parse()
                    .expect("phases must be an integer")
            }
            "--bodies" => {
                bodies = args
                    .next()
                    .expect("--bodies needs a value")
                    .parse()
                    .expect("bodies must be an integer")
            }
            "--matrix" => matrix = true,
            _ => panic!("unknown motion-workload argument {argument}"),
        }
    }

    let result = if matrix {
        Value::Array(
            [32usize, 64, 128, 256]
                .into_iter()
                .map(|body_count| run_workload(body_count, phases))
                .collect(),
        )
    } else {
        run_workload(bodies, phases)
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&result).expect("encode workload result")
    );
}

fn run_workload(bodies: usize, phases: usize) -> Value {
    assert!((1..=256).contains(&bodies), "bodies must be in 1..=256");
    let project = project_with_body_count(bodies);

    let admission_started = Instant::now();
    let mut runtime = GameRuntime::from_project_content(&project).expect("admit motion workload");
    let admission_micros = admission_started.elapsed().as_micros();
    let admitted_bodies = runtime.session().entities().kinematic_bodies().count();

    let simulation_started = Instant::now();
    let mut moved_body_phases = 0usize;
    let mut blocked_axes = 0usize;
    let mut committed_entity_facts = 0usize;
    for _ in 0..phases {
        let receipt = runtime
            .run_motion_phase(DELTA_SECONDS)
            .expect("run motion phase");
        moved_body_phases += receipt.moved_bodies;
        blocked_axes += receipt
            .facts
            .iter()
            .filter(|fact| matches!(fact, MotionFact::Blocked { .. }))
            .count();
        committed_entity_facts += receipt.entity_facts.len();
    }
    let simulation_elapsed = simulation_started.elapsed();
    let simulation_micros = simulation_elapsed.as_micros();
    let phases_per_second = if simulation_elapsed.is_zero() {
        0.0
    } else {
        phases as f64 / simulation_elapsed.as_secs_f64()
    };
    let still_moving = runtime
        .session()
        .entities()
        .kinematic_bodies()
        .filter(|body| body.velocity.x != 0.0 || body.velocity.y != 0.0 || body.velocity.z != 0.0)
        .count();
    let projection_started = Instant::now();
    let mut projected_nodes = 0usize;
    for _ in 0..phases {
        projected_nodes += runtime.readout().projection.len();
    }
    let projection_micros = projection_started.elapsed().as_micros();
    let snapshot_bytes = encode_game_snapshot(&runtime)
        .expect("snapshot workload")
        .len();

    json!({
        "workload": "authored-voxel-wall-kinematic-lanes",
        "bodyCount": admitted_bodies,
        "phaseCount": phases,
        "deltaSeconds": DELTA_SECONDS,
        "authoredProjectBytes": project.len(),
        "admissionMicros": admission_micros,
        "simulationMicros": simulation_micros,
        "phasesPerSecond": phases_per_second,
        "nanosecondsPerBodyPhase": simulation_elapsed.as_nanos() as f64
            / (admitted_bodies * phases) as f64,
        "movedBodyPhases": moved_body_phases,
        "blockedAxes": blocked_axes,
        "committedEntityFacts": committed_entity_facts,
        "projectionPassMicros": projection_micros,
        "projectedNodeReads": projected_nodes,
        "snapshotBytes": snapshot_bytes,
        "stillMoving": still_moving,
        "entityRevision": runtime.session().entities().revision(),
        "projectionNodes": runtime.readout().projection.len(),
        "solidVoxels": runtime
            .collision_scene()
            .expect("collision scene")
            .solid_voxel_count(),
    })
}

fn project_with_body_count(bodies: usize) -> String {
    let mut project: Value = serde_json::from_str(PROJECT).expect("decode checked-in workload");
    project
        .get_mut("entities")
        .and_then(Value::as_array_mut)
        .expect("workload entities")
        .truncate(bodies);
    project
        .get_mut("voxelCollision")
        .and_then(|collision| collision.get_mut("solidVoxels"))
        .and_then(Value::as_array_mut)
        .expect("workload solid voxels")
        .truncate(bodies);
    serde_json::to_string(&project).expect("encode bounded workload")
}
