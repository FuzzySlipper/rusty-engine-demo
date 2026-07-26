use std::time::Duration;

use core_ids::EntityId;
use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, GameLoopEdgeCommand, GameLoopEdgeCommandKind,
    GameLoopFact, GameRuntime, InputCommandDisposition, InputCommandRejection, InventoryAction,
    InventoryCommand, InventoryFact, ItemDefinitionId, LoadingBayGameLoop, PlayerInputCommand,
    PlayerInputIntent, FIXED_STEP_DURATION, FIXED_TICK_PHASE_ORDER, MAX_CATCH_UP_TICKS,
    MAX_EDGE_COMMANDS,
};

const PLAYER: EntityId = EntityId::new(1);
const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");

fn game_loop() -> LoadingBayGameLoop {
    LoadingBayGameLoop::new(
        GameRuntime::from_stored_project(PROJECT).expect("admit Loading Bay"),
        PLAYER,
    )
    .expect("create Loading Bay loop")
}

fn player_position(game_loop: &LoadingBayGameLoop) -> [f32; 3] {
    game_loop
        .runtime()
        .session()
        .player_controller(PLAYER)
        .expect("player")
        .entity_view
        .transform
        .expect("player transform")
        .translation
        .to_array()
}

fn input(
    generation: u64,
    sequence: u64,
    movement: [f32; 2],
    look_delta: [f32; 2],
    primary_fire_held: bool,
) -> PlayerInputCommand {
    PlayerInputCommand {
        connection_generation: generation,
        sequence,
        intent: PlayerInputIntent {
            movement,
            look_delta,
            primary_fire_held,
        },
    }
}

fn edge(generation: u64, sequence: u64, command: GameLoopEdgeCommandKind) -> GameLoopEdgeCommand {
    GameLoopEdgeCommand {
        connection_generation: generation,
        sequence,
        command,
    }
}

#[test]
fn fixed_tick_integrates_velocity_instead_of_applying_an_authored_request_step() {
    let mut game_loop = game_loop();
    let generation = game_loop.start_connection().connection_generation;
    let before = player_position(&game_loop);
    game_loop
        .submit_input(input(generation, 1, [1.0, 0.0], [0.0, 0.0], false))
        .unwrap();

    let receipt = game_loop.run_fixed_tick().unwrap();
    let after = player_position(&game_loop);
    let distance = before
        .into_iter()
        .zip(after)
        .map(|(before, after)| (after - before).powi(2))
        .sum::<f32>()
        .sqrt();

    assert_eq!(receipt.phases, FIXED_TICK_PHASE_ORDER);
    assert!(receipt.simulation_advanced);
    assert!(distance > 0.06 && distance < 0.07, "{distance}");
    assert!(receipt
        .facts
        .iter()
        .any(|fact| matches!(fact, GameLoopFact::PlayerControl(_))));
}

#[test]
fn fixed_pickup_phase_collects_non_solid_overlap_and_reports_capacity_rejection() {
    let mut game_loop = game_loop();
    let generation = game_loop.start_connection().connection_generation;
    let mut facts = Vec::new();
    for sequence in 1..=40 {
        game_loop
            .submit_input(input(generation, sequence, [0.0, 1.0], [0.0, 0.0], false))
            .unwrap();
        facts.extend(game_loop.run_fixed_tick().unwrap().facts);
    }

    assert!(
        player_position(&game_loop)[0] > 3.5,
        "the player traverses both non-solid pickup volumes"
    );
    assert!(
        facts.iter().any(|fact| matches!(
            fact,
            GameLoopFact::Pickup(loading_bay_game::PickupFact::Collected {
                pickup,
                item,
                quantity: 160,
                ..
            }) if *pickup == EntityId::new(20) && item.as_str() == "ammo/energy-cell"
        )),
        "{facts:#?}"
    );
    assert!(facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::PickupRejected {
            pickup,
            reason: loading_bay_game::PickupRejection::Inventory(
                loading_bay_game::InventoryRejection::QuantityOverflow {
                    current: 200,
                    requested: 1,
                    limit: 200,
                    ..
                }
            )
        } if *pickup == EntityId::new(21)
    )));
    assert!(matches!(
        game_loop
            .runtime()
            .session()
            .pickup(EntityId::new(20))
            .unwrap()
            .state,
        loading_bay_game::PickupState::Collected { .. }
    ));
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .pickup(EntityId::new(21))
            .unwrap()
            .state,
        loading_bay_game::PickupState::Available
    );
}

#[test]
fn weapon_slot_edges_reject_precisely_and_select_only_owned_items_on_fixed_ticks() {
    let mut game_loop = game_loop();
    let generation = game_loop.start_connection().connection_generation;
    let original = game_loop.runtime().session().inventory(PLAYER).unwrap();

    game_loop
        .submit_edge_command(edge(
            generation,
            1,
            GameLoopEdgeCommandKind::SelectWeaponSlot { slot: 1 },
        ))
        .unwrap();
    let unowned = game_loop.run_fixed_tick().unwrap();
    assert!(unowned.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::EdgeCommandRejected {
            sequence: 1,
            reason: loading_bay_game::EdgeCommandRejection::WeaponNotOwned,
        }
    )));
    assert_eq!(
        game_loop.runtime().session().inventory(PLAYER).unwrap(),
        original
    );

    game_loop
        .submit_edge_command(edge(
            generation,
            2,
            GameLoopEdgeCommandKind::SelectWeaponSlot { slot: 9 },
        ))
        .unwrap();
    let invalid = game_loop.run_fixed_tick().unwrap();
    assert!(invalid.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::EdgeCommandRejected {
            sequence: 2,
            reason: loading_bay_game::EdgeCommandRejection::InvalidWeaponSlot,
        }
    )));

    game_loop
        .submit_edge_command(edge(
            generation,
            3,
            GameLoopEdgeCommandKind::SelectWeaponSlot { slot: 0 },
        ))
        .unwrap();
    let already = game_loop.run_fixed_tick().unwrap();
    assert!(already.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::EdgeCommandRejected {
            sequence: 3,
            reason: loading_bay_game::EdgeCommandRejection::WeaponAlreadySelected,
        }
    )));

    let breach = ItemDefinitionId::parse("weapon/breach-scattergun").unwrap();
    game_loop
        .runtime_mut()
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 1,
                action: InventoryAction::Grant {
                    item: breach.clone(),
                    quantity: 1,
                },
            },
        )
        .unwrap();
    game_loop
        .submit_edge_command(edge(
            generation,
            4,
            GameLoopEdgeCommandKind::SelectWeaponSlot { slot: 1 },
        ))
        .unwrap();
    let selected = game_loop.run_fixed_tick().unwrap();
    assert!(selected.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::Inventory(InventoryFact::EquippedWeaponChanged {
            after: Some(item),
            ..
        }) if item == &breach
    )));
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .inventory(PLAYER)
            .unwrap()
            .equipped_weapon,
        Some(breach)
    );

    assert_eq!(
        game_loop
            .submit_edge_command(edge(
                generation,
                4,
                GameLoopEdgeCommandKind::SelectWeaponSlot { slot: 0 },
            ))
            .unwrap()
            .disposition,
        InputCommandDisposition::Repeated
    );
    for sequence in 5..=70 {
        game_loop
            .submit_input(input(generation, sequence, [0.0, 0.0], [0.0, 0.0], false))
            .unwrap();
        game_loop.run_fixed_tick().unwrap();
    }
    let selected_inventory = game_loop.runtime().session().inventory(PLAYER).unwrap();
    assert_eq!(
        game_loop.submit_edge_command(edge(
            generation,
            3,
            GameLoopEdgeCommandKind::SelectWeaponSlot { slot: 0 },
        )),
        Err(InputCommandRejection::StaleSequence {
            acknowledged: 70,
            actual: 3,
        })
    );
    assert_eq!(
        game_loop.runtime().session().inventory(PLAYER).unwrap(),
        selected_inventory
    );
}

#[test]
fn defeated_player_cannot_change_equipped_weapon() {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    project["scenes"][0]["entities"][0]["health"] = serde_json::json!({
        "max": 1,
        "hitboxHalfExtents": [0.25, 0.25, 0.25]
    });
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    snapshot["health"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|health| health["entity"] == PLAYER.raw())
        .unwrap()["current"] = 0.into();
    let defeated = decode_game_snapshot(&snapshot.to_string()).unwrap();
    let before = defeated.session().inventory(PLAYER).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(defeated, PLAYER).unwrap();
    let generation = game_loop.start_connection().connection_generation;

    game_loop
        .submit_edge_command(edge(
            generation,
            1,
            GameLoopEdgeCommandKind::SelectWeaponSlot { slot: 0 },
        ))
        .unwrap();
    let receipt = game_loop.run_fixed_tick().unwrap();

    assert!(receipt.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::EdgeCommandRejected {
            sequence: 1,
            reason: loading_bay_game::EdgeCommandRejection::PlayerDefeated,
        }
    )));
    assert_eq!(
        game_loop.runtime().session().inventory(PLAYER).unwrap(),
        before
    );
}

#[test]
fn elapsed_cadence_is_deterministic_and_large_debt_is_bounded() {
    let mut combined = game_loop();
    let combined_generation = combined.start_connection().connection_generation;
    combined
        .submit_input(input(combined_generation, 1, [1.0, 0.0], [0.0, 0.0], false))
        .unwrap();

    let mut sliced = game_loop();
    let sliced_generation = sliced.start_connection().connection_generation;
    sliced
        .submit_input(input(sliced_generation, 1, [1.0, 0.0], [0.0, 0.0], false))
        .unwrap();

    let combined_receipt = combined
        .advance_elapsed(FIXED_STEP_DURATION.saturating_mul(2))
        .unwrap();
    sliced.advance_elapsed(FIXED_STEP_DURATION).unwrap();
    sliced.advance_elapsed(FIXED_STEP_DURATION).unwrap();
    assert_eq!(combined_receipt.fixed_ticks.len(), 2);
    assert_eq!(
        encode_game_snapshot(combined.runtime()).unwrap(),
        encode_game_snapshot(sliced.runtime()).unwrap()
    );

    let mut delayed = game_loop();
    let receipt = delayed
        .advance_elapsed(FIXED_STEP_DURATION.saturating_mul(20))
        .unwrap();
    assert_eq!(receipt.fixed_ticks.len(), MAX_CATCH_UP_TICKS);
    assert_eq!(receipt.dropped_ticks, 15);
    assert!(delayed
        .advance_elapsed(Duration::ZERO)
        .unwrap()
        .fixed_ticks
        .is_empty());
}

#[test]
fn look_is_coalesced_without_losing_small_deltas_and_rejects_invalid_input() {
    let mut game_loop = game_loop();
    let generation = game_loop.start_connection().connection_generation;
    let before = game_loop
        .runtime()
        .session()
        .player_controller(PLAYER)
        .unwrap()
        .state;

    game_loop
        .submit_input(input(generation, 1, [0.0, 0.0], [0.001, 0.002], false))
        .unwrap();
    game_loop
        .submit_input(input(generation, 2, [0.0, 0.0], [0.002, 0.003], false))
        .unwrap();
    game_loop.run_fixed_tick().unwrap();
    let after = game_loop
        .runtime()
        .session()
        .player_controller(PLAYER)
        .unwrap()
        .state;
    assert!((after.yaw_degrees - before.yaw_degrees - 0.036).abs() < 0.000_1);
    assert!((after.pitch_degrees - before.pitch_degrees - 0.06).abs() < 0.000_1);

    assert_eq!(
        game_loop.submit_input(input(generation, 3, [f32::NAN, 0.0], [0.0, 0.0], false,)),
        Err(InputCommandRejection::InvalidInput)
    );
    assert_eq!(game_loop.input_session().acknowledged_sequence, 2);
}

#[test]
fn coalesced_look_clamps_at_the_action_boundary_in_both_directions() {
    let mut positive = game_loop();
    let positive_generation = positive.start_connection().connection_generation;
    let positive_before = positive
        .runtime()
        .session()
        .player_controller(PLAYER)
        .unwrap()
        .state
        .yaw_degrees;
    positive
        .submit_input(input(
            positive_generation,
            1,
            [0.0, 0.0],
            [0.75, 0.0],
            false,
        ))
        .unwrap();
    positive
        .submit_input(input(
            positive_generation,
            2,
            [0.0, 0.0],
            [0.75, 0.0],
            false,
        ))
        .unwrap();
    positive.run_fixed_tick().unwrap();
    let positive_after = positive
        .runtime()
        .session()
        .player_controller(PLAYER)
        .unwrap()
        .state
        .yaw_degrees;
    assert!((positive_after - positive_before - 12.0).abs() < 0.000_1);
    assert_eq!(positive.input_session().consumed_sequence, 2);

    let mut negative = game_loop();
    let negative_generation = negative.start_connection().connection_generation;
    let negative_before = negative
        .runtime()
        .session()
        .player_controller(PLAYER)
        .unwrap()
        .state
        .yaw_degrees;
    negative
        .submit_input(input(
            negative_generation,
            1,
            [0.0, 0.0],
            [-0.75, 0.0],
            false,
        ))
        .unwrap();
    negative
        .submit_input(input(
            negative_generation,
            2,
            [0.0, 0.0],
            [-0.75, 0.0],
            false,
        ))
        .unwrap();
    negative.run_fixed_tick().unwrap();
    let negative_after = negative
        .runtime()
        .session()
        .player_controller(PLAYER)
        .unwrap()
        .state
        .yaw_degrees;
    assert!((negative_after - negative_before + 12.0).abs() < 0.000_1);
    assert_eq!(negative.input_session().consumed_sequence, 2);
}

#[test]
fn disconnect_stale_input_and_reconnect_cannot_stick_or_resurrect_movement() {
    let mut game_loop = game_loop();
    let first_generation = game_loop.start_connection().connection_generation;
    let before = player_position(&game_loop);
    game_loop
        .submit_input(input(first_generation, 1, [1.0, 0.0], [0.0, 0.0], true))
        .unwrap();
    assert!(game_loop.disconnect(first_generation));
    game_loop.run_fixed_tick().unwrap();
    assert_eq!(player_position(&game_loop), before);

    let second_generation = game_loop.start_connection().connection_generation;
    assert_ne!(second_generation, first_generation);
    assert!(matches!(
        game_loop.submit_input(input(first_generation, 2, [1.0, 0.0], [0.0, 0.0], false,)),
        Err(InputCommandRejection::WrongConnectionGeneration { .. })
    ));
    game_loop
        .submit_input(input(second_generation, 1, [1.0, 0.0], [0.0, 0.0], false))
        .unwrap();
    game_loop.run_fixed_tick().unwrap();
    assert_ne!(player_position(&game_loop), before);

    game_loop.run_fixed_tick().unwrap();
    let after_two_ticks = player_position(&game_loop);
    let expired = game_loop.run_fixed_tick().unwrap();
    assert_eq!(player_position(&game_loop), after_two_ticks);
    assert!(expired
        .facts
        .contains(&GameLoopFact::InputExpired { sequence: 1 }));
}

#[test]
fn edge_queue_saturation_is_atomic_and_independent_of_continuous_input() {
    let mut game_loop = game_loop();
    let generation = game_loop.start_connection().connection_generation;
    for sequence in 1..=MAX_EDGE_COMMANDS as u64 {
        let receipt = game_loop
            .submit_edge_command(GameLoopEdgeCommand {
                connection_generation: generation,
                sequence,
                command: GameLoopEdgeCommandKind::Interact { target: 2 },
            })
            .unwrap();
        assert_eq!(receipt.disposition, InputCommandDisposition::Accepted);
    }
    let before = game_loop.input_session();
    assert_eq!(
        game_loop.submit_edge_command(GameLoopEdgeCommand {
            connection_generation: generation,
            sequence: MAX_EDGE_COMMANDS as u64 + 1,
            command: GameLoopEdgeCommandKind::Interact { target: 2 },
        }),
        Err(InputCommandRejection::EdgeQueueSaturated {
            capacity: MAX_EDGE_COMMANDS,
        })
    );
    assert_eq!(game_loop.input_session(), before);

    let continuous = game_loop
        .submit_input(input(
            generation,
            MAX_EDGE_COMMANDS as u64 + 2,
            [1.0, 0.0],
            [0.0, 0.0],
            false,
        ))
        .unwrap();
    assert_eq!(continuous.disposition, InputCommandDisposition::Accepted);
    assert_eq!(
        game_loop.input_session().queued_edge_commands,
        MAX_EDGE_COMMANDS
    );
}

#[test]
fn reopening_a_snapshot_preserves_world_state_but_starts_without_transient_input() {
    let mut game_loop = game_loop();
    let generation = game_loop.start_connection().connection_generation;
    game_loop
        .submit_input(input(generation, 1, [1.0, 0.0], [0.5, 0.25], true))
        .unwrap();
    game_loop.run_fixed_tick().unwrap();
    let snapshot = encode_game_snapshot(game_loop.runtime()).unwrap();

    let reopened_runtime = decode_game_snapshot(&snapshot).unwrap();
    let reopened = LoadingBayGameLoop::new(reopened_runtime, PLAYER).unwrap();
    assert_eq!(encode_game_snapshot(reopened.runtime()).unwrap(), snapshot);
    assert_eq!(reopened.input_session().connection_generation, 0);
    assert!(!reopened.input_session().connected);
    assert_eq!(reopened.input_session().acknowledged_sequence, 0);
    assert_eq!(reopened.input_session().queued_edge_commands, 0);
}

#[test]
fn pause_and_session_replacement_clear_intent_without_resurrection() {
    let mut game_loop = game_loop();
    let generation = game_loop.start_connection().connection_generation;
    let before = player_position(&game_loop);
    game_loop
        .submit_input(input(generation, 1, [1.0, 0.0], [0.0, 0.0], true))
        .unwrap();
    game_loop
        .submit_edge_command(GameLoopEdgeCommand {
            connection_generation: generation,
            sequence: 2,
            command: GameLoopEdgeCommandKind::SetPaused { paused: true },
        })
        .unwrap();
    let paused = game_loop.run_fixed_tick().unwrap();
    assert!(!paused.simulation_advanced);
    assert!(game_loop.input_session().paused);
    assert_eq!(player_position(&game_loop), before);

    game_loop
        .submit_input(input(generation, 3, [1.0, 0.0], [1.0, 1.0], true))
        .unwrap();
    game_loop
        .submit_edge_command(GameLoopEdgeCommand {
            connection_generation: generation,
            sequence: 4,
            command: GameLoopEdgeCommandKind::SetPaused { paused: false },
        })
        .unwrap();
    let resumed = game_loop.run_fixed_tick().unwrap();
    assert!(resumed.simulation_advanced);
    assert!(!game_loop.input_session().paused);
    assert_eq!(player_position(&game_loop), before);

    let replacement_generation = game_loop
        .start_connection_after(generation)
        .connection_generation;
    assert!(replacement_generation > generation);
    assert!(matches!(
        game_loop.submit_input(input(generation, 5, [1.0, 0.0], [0.0, 0.0], false)),
        Err(InputCommandRejection::WrongConnectionGeneration { .. })
    ));
}
