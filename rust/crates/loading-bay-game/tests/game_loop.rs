mod support;

use std::time::Duration;

use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, CombatFact, CombatRejectionReason,
    GameLoopEdgeCommand, GameLoopEdgeCommandKind, GameLoopFact, GameRuntime,
    InputCommandDisposition, InputCommandRejection, InventoryAction, InventoryCommand,
    InventoryFact, ItemDefinitionId, LoadingBayGameLoop, PlayerInputCommand, PlayerInputIntent,
    SaveSlotId, WeaponAttackMode, FIXED_STEP_DURATION, FIXED_TICK_PHASE_ORDER, MAX_CATCH_UP_TICKS,
    MAX_EDGE_COMMANDS,
};
use rusty_engine::core_ids::EntityId;

const PLAYER: EntityId = EntityId::new(1);
const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const DOOM_PROJECT: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");
const DOOM_SPRITE_ORBIT_ROOM_PROJECT: &str =
    include_str!("../../../../content/projects/doom-sprite-orbit-room.project.json");
const DOOM_FX_ROOM_PROJECT: &str =
    include_str!("../../../../content/projects/doom-fx-room.project.json");

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

fn horizontal_position(game_loop: &LoadingBayGameLoop) -> [f32; 2] {
    let position = player_position(game_loop);
    [position[0], position[2]]
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
            jump_held: false,
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
fn authored_jump_edge_is_consumed_once_before_fixed_player_motion() {
    let mut game_loop = LoadingBayGameLoop::new(
        GameRuntime::from_stored_project(DOOM_PROJECT).expect("admit Doom traversal project"),
        PLAYER,
    )
    .unwrap();
    let generation = game_loop.start_connection().connection_generation;
    game_loop
        .submit_edge_command(edge(generation, 1, GameLoopEdgeCommandKind::Jump))
        .unwrap();

    let mut jump_observation = None;
    let mut jump_fact_count = 0;
    for tick_index in 0..=30 {
        let ticks = if tick_index == 0 {
            game_loop
                .advance_elapsed(FIXED_STEP_DURATION)
                .unwrap()
                .fixed_ticks
        } else {
            vec![game_loop.run_fixed_tick().unwrap()]
        };
        for tick in ticks {
            if tick.facts.iter().any(|fact| {
                matches!(
                    fact,
                    GameLoopFact::PlayerControl(loading_bay_game::PlayerControlFact::Jumped { .. })
                )
            }) {
                jump_fact_count += 1;
                jump_observation = Some(
                    game_loop
                        .runtime()
                        .session()
                        .player_controller(PLAYER)
                        .unwrap()
                        .state,
                );
            }
        }
    }
    assert_eq!(jump_fact_count, 1, "jump edge must resolve exactly once");
    let state = jump_observation.expect("jump edge must resolve after canonical settling");
    assert!(
        !state.grounded,
        "jump ended grounded at position {:?} with state {:?}",
        player_position(&game_loop),
        state
    );
    assert!(state.vertical_velocity > 0.0);

    let mut paused = LoadingBayGameLoop::new(
        GameRuntime::from_stored_project(DOOM_PROJECT).unwrap(),
        PLAYER,
    )
    .unwrap();
    let generation = paused.start_connection().connection_generation;
    paused
        .submit_edge_command(edge(
            generation,
            1,
            GameLoopEdgeCommandKind::SetPaused { paused: true },
        ))
        .unwrap();
    paused
        .submit_edge_command(edge(generation, 2, GameLoopEdgeCommandKind::Jump))
        .unwrap();
    let paused_tick = paused
        .advance_elapsed(FIXED_STEP_DURATION)
        .unwrap()
        .fixed_ticks
        .into_iter()
        .next()
        .unwrap();
    assert!(!paused_tick.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::PlayerControl(loading_bay_game::PlayerControlFact::Jumped { .. })
    )));
    assert_eq!(
        paused
            .runtime()
            .session()
            .player_controller(PLAYER)
            .unwrap()
            .state
            .vertical_velocity,
        0.0
    );

    let mut jump_then_pause = LoadingBayGameLoop::new(
        GameRuntime::from_stored_project(DOOM_PROJECT).unwrap(),
        PLAYER,
    )
    .unwrap();
    let generation = jump_then_pause.start_connection().connection_generation;
    jump_then_pause
        .submit_edge_command(edge(generation, 1, GameLoopEdgeCommandKind::Jump))
        .unwrap();
    jump_then_pause
        .submit_edge_command(edge(
            generation,
            2,
            GameLoopEdgeCommandKind::SetPaused { paused: true },
        ))
        .unwrap();
    let reverse_tick = jump_then_pause
        .advance_elapsed(FIXED_STEP_DURATION)
        .unwrap()
        .fixed_ticks
        .into_iter()
        .next()
        .unwrap();
    assert!(!reverse_tick.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::PlayerControl(loading_bay_game::PlayerControlFact::Jumped { .. })
    )));
}

#[test]
fn sampled_frame_looks_before_camera_relative_motion_and_uses_canonical_velocity() {
    let mut game_loop = game_loop();
    let generation = game_loop.start_connection().connection_generation;
    let before = player_position(&game_loop);
    let expected_distance = game_loop
        .runtime()
        .session()
        .player_controller(PLAYER)
        .expect("player")
        .config
        .move_speed_units_per_second
        * FIXED_STEP_DURATION.as_secs_f32();
    game_loop
        .submit_input(input(generation, 1, [1.0, 0.0], [1.0, 0.0], false))
        .unwrap();

    let receipt = game_loop.run_fixed_tick().unwrap();
    let after = player_position(&game_loop);
    let state = game_loop
        .runtime()
        .session()
        .player_controller(PLAYER)
        .expect("player")
        .state;
    let distance = before
        .into_iter()
        .zip(after)
        .map(|(before, after)| (after - before).powi(2))
        .sum::<f32>()
        .sqrt();

    assert_eq!(receipt.phases, FIXED_TICK_PHASE_ORDER);
    assert!(receipt.simulation_advanced);
    assert!(
        distance > 0.0 && distance < expected_distance,
        "canonical acceleration must stay within one fixed-step request: {distance}"
    );
    assert!(
        (normalized_angle_delta(state.yaw_degrees, 180.0) - 12.0).abs() < 0.000_1,
        "canonical yaw after sampled look: {state:?}"
    );
    assert!(
        after[0] < before[0] && after[2] > before[2],
        "{before:?} -> {after:?}"
    );
    assert!(receipt
        .facts
        .iter()
        .any(|fact| matches!(fact, GameLoopFact::PlayerControl(_))));
}

#[test]
fn doom_sprite_orbit_room_floor_supports_a_real_movement_tick() {
    let mut game_loop = LoadingBayGameLoop::new(
        GameRuntime::from_stored_project(DOOM_SPRITE_ORBIT_ROOM_PROJECT)
            .expect("admit Doom directional-sprite orbit room"),
        PLAYER,
    )
    .unwrap();
    let generation = game_loop.start_connection().connection_generation;
    for tick in 0..120 {
        if tick % 2 == 0 {
            game_loop
                .submit_input(input(
                    generation,
                    tick / 2 + 1,
                    [1.0, 0.0],
                    [0.0, 0.0],
                    false,
                ))
                .unwrap();
        }
        game_loop.run_fixed_tick().unwrap();
    }

    let position = player_position(&game_loop);
    let controller = game_loop
        .runtime()
        .session()
        .player_controller(PLAYER)
        .unwrap();
    assert!(controller.state.grounded, "player state: {controller:#?}");
    assert!((position[1] - 1.15).abs() < 0.001, "{position:?}");
    assert!(
        position[2] > -3.0 && position[2] < 0.0,
        "orbit-room player did not make bounded forward progress: {position:?}"
    );
}

#[test]
fn fixed_pickup_phase_collects_non_solid_overlap_and_reports_capacity_rejection() {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    let entities = project["scenes"][0]["entities"].as_array_mut().unwrap();
    let player = entities
        .iter_mut()
        .find(|entity| entity["id"] == 1)
        .unwrap();
    player["translation"] = serde_json::json!([1.5, 1.5, 2.5]);
    player["playerController"]["initialYawDegrees"] = 0.into();
    let fill = entities
        .iter_mut()
        .find(|entity| entity["id"] == 20)
        .unwrap();
    fill["translation"] = serde_json::json!([2.5, 1.5, 2.5]);
    fill["pickup"]["quantity"] = 182.into();
    let overflow = entities
        .iter_mut()
        .find(|entity| entity["id"] == 21)
        .unwrap();
    overflow["translation"] = serde_json::json!([3.5, 1.5, 2.5]);
    overflow["pickup"]["item"] = "ammo/energy-cell".into();
    let mut game_loop = LoadingBayGameLoop::new(
        GameRuntime::from_stored_project(&project.to_string()).unwrap(),
        PLAYER,
    )
    .unwrap();
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
                quantity: 182,
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
fn single_press_automatic_hold_and_dry_fire_have_distinct_fixed_tick_semantics() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 1,
                action: InventoryAction::Grant {
                    item: ItemDefinitionId::parse("weapon/rivet-carbine").unwrap(),
                    quantity: 1,
                },
            },
        )
        .unwrap();
    runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 2,
                action: InventoryAction::SelectWeapon {
                    item: ItemDefinitionId::parse("weapon/rivet-carbine").unwrap(),
                },
            },
        )
        .unwrap();
    let mut automatic = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();
    let generation = automatic.start_connection().connection_generation;
    let mut automatic_fired = Vec::new();
    for sequence in 1..=5 {
        automatic
            .submit_input(input(generation, sequence, [0.0, 0.0], [0.0, 0.0], true))
            .unwrap();
        automatic_fired.extend(
            automatic
                .run_fixed_tick()
                .unwrap()
                .facts
                .into_iter()
                .filter(|fact| {
                    matches!(
                        fact,
                        GameLoopFact::Combat(CombatFact::AttackFired {
                            attack_mode: WeaponAttackMode::Automatic,
                            ..
                        })
                    )
                }),
        );
    }
    assert_eq!(automatic_fired.len(), 2);
    assert_eq!(ammunition(&automatic, "ammo/energy-cell"), 16);

    let mut single = game_loop();
    let generation = single.start_connection().connection_generation;
    for sequence in 1..=5 {
        single
            .submit_input(input(generation, sequence, [0.0, 0.0], [0.0, 0.0], true))
            .unwrap();
        single.run_fixed_tick().unwrap();
    }
    assert_eq!(ammunition(&single, "ammo/energy-cell"), 17);
    single
        .submit_input(input(generation, 6, [0.0, 0.0], [0.0, 0.0], false))
        .unwrap();
    single.run_fixed_tick().unwrap();
    single
        .submit_input(input(generation, 7, [0.0, 0.0], [0.0, 0.0], true))
        .unwrap();
    assert!(single
        .run_fixed_tick()
        .unwrap()
        .facts
        .iter()
        .any(|fact| matches!(
            fact,
            GameLoopFact::Combat(CombatFact::AttackFired {
                attack_mode: WeaponAttackMode::Hitscan,
                ..
            })
        )));
    assert_eq!(ammunition(&single, "ammo/energy-cell"), 16);

    single
        .runtime_mut()
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 3,
                action: InventoryAction::Consume {
                    item: ItemDefinitionId::parse("ammo/energy-cell").unwrap(),
                    quantity: 16,
                },
            },
        )
        .unwrap();
    single
        .submit_input(input(generation, 8, [0.0, 0.0], [0.0, 0.0], false))
        .unwrap();
    single.run_fixed_tick().unwrap();
    single
        .submit_input(input(generation, 9, [0.0, 0.0], [0.0, 0.0], true))
        .unwrap();
    assert!(single
        .run_fixed_tick()
        .unwrap()
        .facts
        .contains(&GameLoopFact::CombatRejected {
            attacker: PLAYER,
            weapon: Some(ItemDefinitionId::parse("weapon/arc-pistol").unwrap()),
            presentation: Some("arc-pistol".to_owned()),
            reason: CombatRejectionReason::NoAmmo,
        }));
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
    downgrade_to_schema_eighteen(&mut snapshot);
    snapshot["health"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|health| health["entity"] == PLAYER.raw())
        .unwrap()["current"] = 0.into();
    snapshot["health"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|health| health["entity"] == PLAYER.raw())
        .unwrap()["state"] = "dead".into();
    let defeated = decode_game_snapshot(&snapshot.to_string()).unwrap();
    let before = defeated.session().inventory(PLAYER).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(defeated, PLAYER).unwrap();
    let generation = game_loop.start_connection().connection_generation;

    let rejection = game_loop
        .submit_edge_command(edge(
            generation,
            1,
            GameLoopEdgeCommandKind::SelectWeaponSlot { slot: 0 },
        ))
        .unwrap_err();
    assert_eq!(
        rejection,
        loading_bay_game::InputCommandRejection::PlayerDefeated
    );
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
    assert!(
        (normalized_angle_delta(after.yaw_degrees, before.yaw_degrees) - 0.036).abs() < 0.000_1
    );
    assert!((after.pitch_degrees - before.pitch_degrees - 0.06).abs() < 0.000_1);

    assert_eq!(
        game_loop.submit_input(input(generation, 3, [f32::NAN, 0.0], [0.0, 0.0], false,)),
        Err(InputCommandRejection::InvalidInput)
    );
    assert_eq!(game_loop.input_session().acknowledged_sequence, 2);
}

#[test]
fn coalesced_look_preserves_the_canonical_sampled_delta_in_both_directions() {
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
    assert!(
        (normalized_angle_delta(positive_after, positive_before) - 18.0).abs() < 0.000_1,
        "before={positive_before} after={positive_after}"
    );
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
    assert!((normalized_angle_delta(negative_after, negative_before) + 18.0).abs() < 0.000_1);
    assert_eq!(negative.input_session().consumed_sequence, 2);
}

#[test]
fn disconnect_stale_input_and_reconnect_cannot_stick_or_resurrect_movement() {
    let mut game_loop = game_loop();
    let first_generation = game_loop.start_connection().connection_generation;
    let before = player_position(&game_loop);
    let before_horizontal = horizontal_position(&game_loop);
    game_loop
        .submit_input(input(first_generation, 1, [1.0, 0.0], [0.0, 0.0], true))
        .unwrap();
    assert!(game_loop.disconnect(first_generation));
    game_loop.run_fixed_tick().unwrap();
    assert_eq!(horizontal_position(&game_loop), before_horizontal);

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
    let expired = game_loop.run_fixed_tick().unwrap();
    assert!(expired
        .facts
        .contains(&GameLoopFact::InputExpired { sequence: 1 }));
    assert_eq!(expired.consumed_sequence, 1);
    assert_eq!(game_loop.input_session().acknowledged_sequence, 1);
}

#[test]
fn explicit_initial_enemy_awareness_is_retained_until_a_debug_edge_changes_it() {
    let game_loop = LoadingBayGameLoop::new_with_enemy_awareness(
        GameRuntime::from_stored_project(PROJECT).unwrap(),
        PLAYER,
        false,
    )
    .unwrap();

    assert!(!game_loop.enemy_awareness_enabled());
}

#[test]
fn doom_fx_room_awareness_enters_direct_combat_without_navigation_failures() {
    let mut game_loop = LoadingBayGameLoop::new_with_enemy_awareness(
        GameRuntime::from_stored_project(DOOM_FX_ROOM_PROJECT).unwrap(),
        PLAYER,
        false,
    )
    .unwrap();
    let generation = game_loop.start_connection().connection_generation;
    game_loop
        .submit_edge_command(edge(
            generation,
            1,
            GameLoopEdgeCommandKind::SetEnemyAwareness { enabled: true },
        ))
        .unwrap();

    let facts = (0..4)
        .flat_map(|_| game_loop.run_fixed_tick().unwrap().facts)
        .collect::<Vec<_>>();
    assert!(
        facts.iter().any(|fact| matches!(
            fact,
            GameLoopFact::EnemyCombat(loading_bay_game::EnemyCombatFact::AttackFired { .. })
        )),
        "{facts:#?}"
    );
    assert!(
        !facts.iter().any(|fact| matches!(
            fact,
            GameLoopFact::Navigation(loading_bay_game::NavigationFact::Unreachable { .. })
        )),
        "{facts:#?}"
    );
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
    assert_eq!(horizontal_position(&game_loop), [before[0], before[2]]);

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
    assert_eq!(horizontal_position(&game_loop), [before[0], before[2]]);

    let replacement_generation = game_loop
        .start_connection_after(generation)
        .connection_generation;
    assert!(replacement_generation > generation);
    assert!(matches!(
        game_loop.submit_input(input(generation, 5, [1.0, 0.0], [0.0, 0.0], false)),
        Err(InputCommandRejection::WrongConnectionGeneration { .. })
    ));
}

#[test]
fn save_and_load_edges_have_fixed_tick_meaning_while_live_paused_dead_or_complete() {
    let mut live = game_loop();
    let generation = live.start_connection().connection_generation;
    live.submit_edge_command(edge(
        generation,
        1,
        GameLoopEdgeCommandKind::SaveGame {
            slot: SaveSlotId::Slot1,
        },
    ))
    .unwrap();
    let receipt = live.run_fixed_tick().unwrap();
    assert!(receipt.simulation_advanced);
    assert!(receipt.facts.contains(&GameLoopFact::SaveRequested {
        sequence: 1,
        slot: SaveSlotId::Slot1,
    }));

    live.submit_edge_command(edge(
        generation,
        2,
        GameLoopEdgeCommandKind::SetPaused { paused: true },
    ))
    .unwrap();
    live.run_fixed_tick().unwrap();
    live.submit_edge_command(edge(
        generation,
        3,
        GameLoopEdgeCommandKind::SaveGame {
            slot: SaveSlotId::Checkpoint,
        },
    ))
    .unwrap();
    let paused = live.run_fixed_tick().unwrap();
    assert!(!paused.simulation_advanced);
    assert!(paused.facts.contains(&GameLoopFact::SaveRequested {
        sequence: 3,
        slot: SaveSlotId::Checkpoint,
    }));

    let mut dead_snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(game_loop().runtime()).unwrap()).unwrap();
    downgrade_to_schema_eighteen(&mut dead_snapshot);
    let health = dead_snapshot["health"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|health| health["entity"] == PLAYER.raw())
        .unwrap();
    health["current"] = 0.into();
    health["state"] = "dead".into();
    let mut dead = LoadingBayGameLoop::new(
        decode_game_snapshot(&dead_snapshot.to_string()).unwrap(),
        PLAYER,
    )
    .unwrap();
    let generation = dead.start_connection().connection_generation;
    dead.submit_edge_command(edge(
        generation,
        1,
        GameLoopEdgeCommandKind::LoadGame {
            slot: SaveSlotId::Checkpoint,
        },
    ))
    .unwrap();
    let receipt = dead.run_fixed_tick().unwrap();
    assert!(receipt.facts.contains(&GameLoopFact::LoadRequested {
        sequence: 1,
        slot: SaveSlotId::Checkpoint,
    }));

    let mut complete_snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(game_loop().runtime()).unwrap()).unwrap();
    complete_snapshot["progression"]["levelExits"][0]["state"] = serde_json::json!({
        "state": "completed",
        "actor": PLAYER.raw(),
        "completedAtTick": 0
    });
    let mut complete = LoadingBayGameLoop::new(
        decode_game_snapshot(&complete_snapshot.to_string()).unwrap(),
        PLAYER,
    )
    .unwrap();
    let generation = complete.start_connection().connection_generation;
    complete
        .submit_edge_command(edge(
            generation,
            1,
            GameLoopEdgeCommandKind::SaveGame {
                slot: SaveSlotId::Slot3,
            },
        ))
        .unwrap();
    let receipt = complete.run_fixed_tick().unwrap();
    assert!(!receipt.simulation_advanced);
    assert!(receipt.facts.contains(&GameLoopFact::SaveRequested {
        sequence: 1,
        slot: SaveSlotId::Slot3,
    }));
}

fn downgrade_to_schema_eighteen(snapshot: &mut serde_json::Value) {
    let controllers = snapshot["playerControllers"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for controller in controllers {
        let entity_id = controller["entity"].as_u64().unwrap();
        let standing_height = controller["canonicalStandingHeight"]
            .as_f64()
            .unwrap_or(1.8);
        let radius = controller["canonicalRadius"].as_f64().unwrap_or(0.25);
        let eye_height = controller["traversal"]["eyeHeight"].as_f64().unwrap();
        let eye_offset_from_center = controller["eyeOffsetFromCenter"].as_f64().unwrap();
        let center_lift = eye_height - eye_offset_from_center;
        let authored_half_height = standing_height * 0.5 - center_lift;
        let entity = snapshot["entities"]["entities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entity| entity["id"] == entity_id)
            .unwrap();
        entity["transform"]["translation"][1] = serde_json::json!(
            entity["transform"]["translation"][1].as_f64().unwrap() - center_lift
        );
        entity["kinematic"] = serde_json::json!({
            "halfExtents": [radius, authored_half_height, radius],
            "velocity": [0.0, 0.0, 0.0]
        });
    }
    snapshot["schemaVersion"] = 18.into();
    support::strip_future_gameplay_mechanics_state(snapshot);
}

fn ammunition(game_loop: &LoadingBayGameLoop, item: &str) -> u32 {
    game_loop
        .runtime()
        .session()
        .inventory(PLAYER)
        .unwrap()
        .stacks
        .into_iter()
        .find(|stack| stack.item.as_str() == item)
        .map_or(0, |stack| stack.quantity)
}

fn normalized_angle_delta(after: f32, before: f32) -> f32 {
    (after - before + 180.0).rem_euclid(360.0) - 180.0
}
