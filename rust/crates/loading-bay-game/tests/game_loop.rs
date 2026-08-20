use std::time::Duration;

use loading_bay_game::{
    GameLoopEdgeCommand, GameLoopEdgeCommandKind, GameLoopFact, GameRuntime, LoadingBayGameLoop,
    PlayerInputCommand, PlayerInputIntent, FIXED_STEP_DURATION, FIXED_TICK_PHASE_ORDER,
};
use rusty_engine::core_ids::EntityId;

const PROJECT: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");
const PLAYER: EntityId = EntityId::new(1);

#[test]
fn e1m1_semantic_input_advances_the_authoritative_fixed_tick() {
    let mut game_loop = game_loop();
    let generation = game_loop.start_connection().connection_generation;
    let before = player_position(&game_loop);
    game_loop
        .submit_input(PlayerInputCommand {
            connection_generation: generation,
            sequence: 1,
            intent: PlayerInputIntent {
                movement: [1.0, 0.0],
                look_delta: [1.0, 0.0],
                ..PlayerInputIntent::NEUTRAL
            },
        })
        .unwrap();

    let receipt = game_loop.run_fixed_tick().unwrap();
    let after = player_position(&game_loop);
    assert_eq!(receipt.phases, FIXED_TICK_PHASE_ORDER);
    assert!(receipt.simulation_advanced);
    assert_ne!(before, after);
    assert!(receipt
        .facts
        .iter()
        .any(|fact| matches!(fact, GameLoopFact::PlayerControl(_))));
}

#[test]
fn authored_jump_edge_is_consumed_once_before_fixed_player_motion() {
    let mut game_loop = game_loop();
    let generation = game_loop.start_connection().connection_generation;
    game_loop
        .submit_edge_command(GameLoopEdgeCommand {
            connection_generation: generation,
            sequence: 1,
            command: GameLoopEdgeCommandKind::Jump,
        })
        .unwrap();

    let mut jumps = 0;
    for tick_index in 0..=30 {
        let ticks = if tick_index == 0 {
            game_loop
                .advance_elapsed(FIXED_STEP_DURATION)
                .unwrap()
                .fixed_ticks
        } else {
            vec![game_loop.run_fixed_tick().unwrap()]
        };
        jumps += ticks
            .iter()
            .flat_map(|tick| &tick.facts)
            .filter(|fact| {
                matches!(
                    fact,
                    GameLoopFact::PlayerControl(loading_bay_game::PlayerControlFact::Jumped { .. })
                )
            })
            .count();
    }
    assert_eq!(jumps, 1);
}

#[test]
fn fixed_tick_rejects_elapsed_debt_smaller_than_one_step_without_mutating() {
    let mut game_loop = game_loop();
    let before = game_loop.runtime().tick();
    let receipt = game_loop.advance_elapsed(Duration::from_millis(1)).unwrap();
    assert!(receipt.fixed_ticks.is_empty());
    assert_eq!(game_loop.runtime().tick(), before);
}

fn game_loop() -> LoadingBayGameLoop {
    LoadingBayGameLoop::new(GameRuntime::from_stored_project(PROJECT).unwrap(), PLAYER).unwrap()
}

fn player_position(game_loop: &LoadingBayGameLoop) -> [f32; 3] {
    game_loop
        .runtime()
        .session()
        .player_controller(PLAYER)
        .unwrap()
        .entity_view
        .transform
        .unwrap()
        .translation
        .to_array()
}
