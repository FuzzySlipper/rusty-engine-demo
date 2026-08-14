use loading_bay_game::{GameLoopFact, GameRuntime, LoadingBayGameLoop, PickupFact, PickupState};
use rusty_engine::core_ids::EntityId;
use serde_json::{json, Value};

const PROJECT: &str = include_str!("../../../../content/projects/doom-pickup-room.project.json");
const PLAYER: EntityId = EntityId::new(1);
const SHOTGUN: EntityId = EntityId::new(10);
const SOURCE_PICKUP_RADIUS: f64 = 20.0 / 28.0;
const PLAYER_HALF_EXTENT: f64 = 0.25;
const EDGE_EPSILON: f64 = 0.01;

#[test]
fn pickup_trigger_is_centered_forgiving_and_independent_of_sprite_rotation() {
    let center_limit = SOURCE_PICKUP_RADIUS + PLAYER_HALF_EXTENT;
    let approaches = [
        ([center_limit - EDGE_EPSILON, 0.0], true),
        ([-center_limit + EDGE_EPSILON, 0.0], true),
        ([0.0, center_limit - EDGE_EPSILON], true),
        ([0.0, -center_limit + EDGE_EPSILON], true),
        ([center_limit + EDGE_EPSILON, 0.0], false),
        ([-center_limit - EDGE_EPSILON, 0.0], false),
        ([0.0, center_limit + EDGE_EPSILON], false),
        ([0.0, -center_limit - EDGE_EPSILON], false),
    ];

    for (index, (offset, should_collect)) in approaches.into_iter().enumerate() {
        let mut project: Value = serde_json::from_str(PROJECT).unwrap();
        let entities = project["scenes"][0]["entities"].as_array_mut().unwrap();
        let pickup_position = entities
            .iter()
            .find(|entity| entity["id"] == SHOTGUN.raw())
            .unwrap()["translation"]
            .as_array()
            .unwrap()
            .iter()
            .map(Value::as_f64)
            .collect::<Option<Vec<_>>>()
            .unwrap();

        let player = entities
            .iter_mut()
            .find(|entity| entity["id"] == PLAYER.raw())
            .unwrap();
        player["translation"] = json!([
            pickup_position[0] + offset[0],
            pickup_position[1] + 0.25,
            pickup_position[2] + offset[1],
        ]);

        let pickup = entities
            .iter_mut()
            .find(|entity| entity["id"] == SHOTGUN.raw())
            .unwrap();
        pickup["renderable"]["localTransform"]["rotation"] = if index % 2 == 0 {
            json!([0.0, 0.70710677, 0.0, 0.70710677])
        } else {
            json!([0.0, -0.70710677, 0.0, 0.70710677])
        };

        let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
        let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();
        let tick = game_loop.run_fixed_tick().unwrap();
        let collected = tick.facts.iter().any(|fact| {
            matches!(
                fact,
                GameLoopFact::Pickup(PickupFact::Collected { pickup, actor, .. })
                    if *pickup == SHOTGUN && *actor == PLAYER
            )
        });

        assert_eq!(
            collected, should_collect,
            "approach {offset:?} disagreed at the authored trigger boundary"
        );
        assert_eq!(
            matches!(
                game_loop.runtime().session().pickup(SHOTGUN).unwrap().state,
                PickupState::Collected { .. }
            ),
            should_collect,
        );
    }
}
