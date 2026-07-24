use core_ids::EntityId;
use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, GameRuntime, NavigationFact, NavigationState,
};
use serde_json::{json, Value};

const PROJECT: &str = include_str!("../../../../content/generated/encounter-gate.project.json");
const NAVIGATOR: EntityId = EntityId::new(4);
const DELTA_SECONDS: f32 = 1.0 / 60.0;

#[test]
fn autonomous_enemy_replans_around_the_generated_voxel_pillar() {
    let mut runtime = GameRuntime::from_project_content(PROJECT).expect("admit navigation project");
    let mut facts = Vec::new();
    let mut positions = Vec::new();

    for _ in 0..240 {
        let receipt = runtime
            .run_navigation_phase(DELTA_SECONDS)
            .expect("navigation phase");
        facts.extend(receipt.facts);
        positions.push(navigator_position(&runtime));
        if runtime.session().navigation(NAVIGATOR).unwrap().state != NavigationState::Following {
            break;
        }
    }

    let view = runtime.session().navigation(NAVIGATOR).unwrap();
    assert_eq!(view.state, NavigationState::Arrived);
    assert_eq!(
        view.entity_view.transform.unwrap().translation,
        view.config.goal
    );
    assert!(positions.iter().any(|position| position.z > 5.0));
    assert!(facts.iter().any(
        |fact| matches!(fact, NavigationFact::Advanced { entity, .. } if *entity == NAVIGATOR)
    ));
    assert!(facts.iter().any(
        |fact| matches!(fact, NavigationFact::Arrived { entity, .. } if *entity == NAVIGATOR)
    ));
}

#[test]
fn solid_goal_commits_a_typed_unreachable_outcome() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    entity_mut(&mut project, NAVIGATOR.raw())["navigation"]["goal"] = json!([3.5, 0.5, 4.5]);
    let mut runtime = GameRuntime::from_project_content(&project.to_string()).unwrap();

    let receipt = runtime.run_navigation_phase(DELTA_SECONDS).unwrap();

    assert_eq!(
        runtime.session().navigation(NAVIGATOR).unwrap().state,
        NavigationState::Unreachable
    );
    assert_eq!(receipt.unreachable_agents, 1);
    assert!(receipt.facts.iter().any(
        |fact| matches!(fact, NavigationFact::Unreachable { entity, .. } if *entity == NAVIGATOR)
    ));
}

#[test]
fn collision_mismatch_fails_closed_as_blocked() {
    let project = json!({
        "schemaVersion": 6,
        "entities": [{
            "id": 4,
            "name": "wide-sentry",
            "translation": [0.5, 0.5, 0.5],
            "collision": { "enabled": true, "staticCollider": false },
            "renderable": { "asset": "mesh/security-sentry", "visible": true },
            "enemy": true,
            "kinematic": { "halfExtents": [0.6, 0.25, 0.6], "velocity": [0, 0, 0] },
            "navigation": { "goal": [2.5, 0.5, 0.5], "speedUnitsPerSecond": 4, "maxVisited": 512 }
        }],
        "voxelCollision": { "voxelSize": 1, "chunkSize": 8, "solidVoxels": [[1, 0, 1]] }
    });
    let mut runtime = GameRuntime::from_project_content(&project.to_string()).unwrap();

    let receipt = runtime.run_navigation_phase(DELTA_SECONDS).unwrap();

    assert_eq!(
        runtime.session().navigation(NAVIGATOR).unwrap().state,
        NavigationState::Blocked
    );
    assert_eq!(receipt.blocked_agents, 1);
    assert!(receipt.facts.iter().any(
        |fact| matches!(fact, NavigationFact::Blocked { entity, .. } if *entity == NAVIGATOR)
    ));
}

#[test]
fn dynamic_blocker_stops_motion_and_removal_allows_replanning() {
    let blocker = EntityId::new(5);
    let actor = EntityId::new(1);
    let project = json!({
        "schemaVersion": 6,
        "entities": [
            { "id": 1, "name": "player" },
            {
                "id": 4,
                "name": "moving-sentry",
                "translation": [0.5, 0.5, 0.5],
                "collision": { "enabled": true, "staticCollider": false },
                "renderable": { "asset": "mesh/security-sentry", "visible": true },
                "enemy": true,
                "kinematic": { "halfExtents": [0.25, 0.25, 0.25], "velocity": [0, 0, 0] },
                "navigation": { "goal": [4.5, 0.5, 0.5], "speedUnitsPerSecond": 4, "maxVisited": 512 }
            },
            {
                "id": 5,
                "name": "blocking-sentry",
                "translation": [1.5, 0.5, 0.5],
                "collision": { "enabled": true, "staticCollider": false },
                "renderable": { "asset": "mesh/security-sentry", "visible": true },
                "enemy": true,
                "kinematic": { "halfExtents": [0.25, 0.25, 0.25], "velocity": [0, 0, 0] }
            }
        ],
        "voxelCollision": { "voxelSize": 1, "chunkSize": 8, "solidVoxels": [[7, 7, 7]] }
    });
    let mut runtime = GameRuntime::from_project_content(&project.to_string()).unwrap();

    for _ in 0..30 {
        runtime.run_navigation_phase(DELTA_SECONDS).unwrap();
        if runtime.session().navigation(NAVIGATOR).unwrap().state == NavigationState::Blocked {
            break;
        }
    }
    assert_eq!(
        runtime.session().navigation(NAVIGATOR).unwrap().state,
        NavigationState::Blocked
    );

    runtime.defeat_enemy(actor, blocker).unwrap();
    for _ in 0..120 {
        runtime.run_navigation_phase(DELTA_SECONDS).unwrap();
        if runtime.session().navigation(NAVIGATOR).unwrap().state == NavigationState::Arrived {
            break;
        }
    }
    assert_eq!(
        runtime.session().navigation(NAVIGATOR).unwrap().state,
        NavigationState::Arrived
    );
}

#[test]
fn save_reopen_rebuilds_navigation_and_reaches_the_same_result() {
    let mut uninterrupted = GameRuntime::from_project_content(PROJECT).unwrap();
    for _ in 0..30 {
        uninterrupted.run_navigation_phase(DELTA_SECONDS).unwrap();
    }
    let encoded = encode_game_snapshot(&uninterrupted).unwrap();
    let mut reopened = decode_game_snapshot(&encoded).unwrap();

    for _ in 0..210 {
        uninterrupted.run_navigation_phase(DELTA_SECONDS).unwrap();
        reopened.run_navigation_phase(DELTA_SECONDS).unwrap();
    }

    assert_eq!(
        uninterrupted.session().navigation(NAVIGATOR),
        reopened.session().navigation(NAVIGATOR)
    );
    assert_eq!(
        uninterrupted.collision_scene().unwrap().navigation_hash(),
        reopened.collision_scene().unwrap().navigation_hash()
    );
}

#[test]
fn one_bounded_phase_advances_many_agents_without_scattered_updates() {
    let entities: Vec<_> = (0..32)
        .map(|index| {
            json!({
                "id": 100 + index,
                "name": format!("nav-agent-{index}"),
                "translation": [0.5, 0.5, 0.5],
                "collision": { "enabled": true, "staticCollider": false },
                "renderable": { "asset": "mesh/security-sentry", "visible": true },
                "enemy": true,
                "kinematic": { "halfExtents": [0.2, 0.2, 0.2], "velocity": [0, 0, 0] },
                "navigation": { "goal": [6.5, 0.5, 0.5], "speedUnitsPerSecond": 6, "maxVisited": 512 }
            })
        })
        .collect();
    let project = json!({
        "schemaVersion": 6,
        "entities": entities,
        "voxelCollision": { "voxelSize": 1, "chunkSize": 8, "solidVoxels": [[7, 7, 7]] }
    });
    let mut runtime = GameRuntime::from_project_content(&project.to_string()).unwrap();

    let receipt = runtime.run_navigation_phase(DELTA_SECONDS).unwrap();

    assert_eq!(receipt.agents_considered, 32);
    assert_eq!(receipt.advanced_agents, 32);
    assert_eq!(receipt.motion.bodies_considered, 32);
    assert_eq!(receipt.motion.moved_bodies, 32);
}

fn navigator_position(runtime: &GameRuntime) -> core_math::Vec3 {
    runtime
        .session()
        .navigation(NAVIGATOR)
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
