mod support;

use loading_bay_game::{
    decode_game_snapshot, diagnostic_code, encode_game_snapshot, EnemyAttackKind,
    EnemyAttackMissReason, EnemyCombatFact, EnemyCombatPosture, GameLoopEdgeCommand,
    GameLoopEdgeCommandKind, GameLoopFact, GameRestartMode, GameRuntime, LoadingBayGameLoop,
    PlayerInputCommand, PlayerInputIntent, ProgressionFact, RuntimeError, VitalityFact,
    VitalityState,
};
use rusty_engine::core_ids::EntityId;

const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const PLAYER: EntityId = EntityId::new(1);
const MELEE: EntityId = EntityId::new(4);
const RANGED: EntityId = EntityId::new(5);
const MAINTENANCE_BULKHEAD: EntityId = EntityId::new(30);

#[test]
fn authored_enemies_own_distinct_perception_and_attack_meanings() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let melee = runtime.session().enemy_combat(MELEE).unwrap();
    let ranged = runtime.session().enemy_combat(RANGED).unwrap();

    assert_eq!(melee.config.attack.kind, EnemyAttackKind::Melee);
    assert_eq!(melee.config.attack.presentation, "sentry-strike");
    assert_eq!(ranged.config.attack.kind, EnemyAttackKind::RangedHitscan);
    assert_eq!(ranged.config.attack.presentation, "sentry-pulse");
    assert_eq!(melee.state.posture, EnemyCombatPosture::Sleeping);
    assert_eq!(ranged.state.posture, EnemyCombatPosture::Sleeping);
}

#[test]
fn ranged_enemy_alerts_then_attacks_on_authoritative_ticks_with_cooldown() {
    let mut project = single_enemy_project(RANGED);
    entity_mut(&mut project, RANGED)["translation"] = serde_json::json!([1.5, 1.5, 5.5]);
    entity_mut(&mut project, RANGED)["enemyCombat"]["attack"]["damage"] = 7.into();
    entity_mut(&mut project, RANGED)["enemyCombat"]["attack"]["cooldownTicks"] = 4.into();
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();

    let alert = game_loop.run_fixed_tick().unwrap();
    assert!(alert.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::EnemyCombat(EnemyCombatFact::Alerted {
            enemy: RANGED,
            target: PLAYER,
            ..
        })
    )));
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .enemy_combat(RANGED)
            .unwrap()
            .state
            .posture,
        EnemyCombatPosture::Alert
    );

    let first_attack = game_loop.run_fixed_tick().unwrap();
    assert_enemy_damage(&first_attack.facts, RANGED, 7);
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .health(PLAYER)
            .unwrap()
            .current,
        93
    );
    for _ in 0..3 {
        let cooldown = game_loop.run_fixed_tick().unwrap();
        assert!(!cooldown.facts.iter().any(|fact| matches!(
            fact,
            GameLoopFact::EnemyCombat(EnemyCombatFact::AttackFired { enemy: RANGED, .. })
        )));
    }
    let second_attack = game_loop.run_fixed_tick().unwrap();
    assert_enemy_damage(&second_attack.facts, RANGED, 7);
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .health(PLAYER)
            .unwrap()
            .current,
        86
    );
}

#[test]
fn debug_awareness_edge_clears_target_memory_and_suppresses_attacks_until_reenabled() {
    let mut project = single_enemy_project(RANGED);
    entity_mut(&mut project, RANGED)["translation"] = serde_json::json!([1.5, 1.5, 5.5]);
    entity_mut(&mut project, RANGED)["enemyCombat"]["attack"]["damage"] = 7.into();
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();
    let generation = game_loop.start_connection().connection_generation;

    game_loop
        .submit_edge_command(GameLoopEdgeCommand {
            connection_generation: generation,
            sequence: 1,
            command: GameLoopEdgeCommandKind::SetEnemyAwareness { enabled: false },
        })
        .unwrap();
    for _ in 0..4 {
        let receipt = game_loop.run_fixed_tick().unwrap();
        assert!(!receipt.facts.iter().any(|fact| matches!(
            fact,
            GameLoopFact::EnemyCombat(EnemyCombatFact::AttackFired { enemy: RANGED, .. })
        )));
    }
    let unaware = game_loop.runtime().session().enemy_combat(RANGED).unwrap();
    assert!(!game_loop.enemy_awareness_enabled());
    assert_eq!(unaware.state.posture, EnemyCombatPosture::Sleeping);
    assert_eq!(unaware.state.last_known_target_position, None);
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .health(PLAYER)
            .unwrap()
            .current,
        100
    );

    game_loop
        .submit_edge_command(GameLoopEdgeCommand {
            connection_generation: generation,
            sequence: 2,
            command: GameLoopEdgeCommandKind::SetEnemyAwareness { enabled: true },
        })
        .unwrap();
    let alert = game_loop.run_fixed_tick().unwrap();
    assert!(game_loop.enemy_awareness_enabled());
    assert!(alert.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::EnemyCombat(EnemyCombatFact::Alerted {
            enemy: RANGED,
            target: PLAYER,
            ..
        })
    )));
    let attack = game_loop.run_fixed_tick().unwrap();
    assert_enemy_damage(&attack.facts, RANGED, 7);
}

#[test]
fn projectile_enemy_reuses_the_transient_engine_projectile_owner() {
    let mut project = single_enemy_project(RANGED);
    entity_mut(&mut project, RANGED)["translation"] = serde_json::json!([1.5, 1.5, 5.5]);
    entity_mut(&mut project, RANGED)["enemyCombat"]["attack"] = serde_json::json!({
        "kind": "projectile",
        "damage": 7,
        "range": 14,
        "cooldownTicks": 60,
        "originOffset": [0, 0.25, 0],
        "presentation": "fixture-orb",
        "projectile": {
            "mass": 0.2,
            "radius": 0.12,
            "impulse": 10,
            "gravityScale": 0,
            "lifetimeTicks": 120,
            "restitution": 0,
            "visualAsset": "mesh/physics-projectile"
        }
    });
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    assert_eq!(
        runtime
            .session()
            .enemy_combat(RANGED)
            .unwrap()
            .config
            .attack
            .kind,
        EnemyAttackKind::Projectile
    );
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();

    let alert = game_loop.run_fixed_tick().unwrap();
    assert!(alert.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::EnemyCombat(EnemyCombatFact::Alerted {
            enemy: RANGED,
            target: PLAYER,
            ..
        })
    )));
    let fired = game_loop.run_fixed_tick().unwrap();
    assert!(fired.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::EnemyCombat(EnemyCombatFact::ProjectileSpawned {
            enemy: RANGED,
            target: PLAYER,
            ..
        })
    )));
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .health(PLAYER)
            .unwrap()
            .current,
        100
    );
    assert!(game_loop
        .runtime()
        .readout()
        .projection
        .iter()
        .any(|node| node.asset == "mesh/physics-projectile"));

    let mut hit = false;
    for _ in 0..120 {
        let receipt = game_loop.run_fixed_tick().unwrap();
        hit |= receipt.facts.iter().any(|fact| {
            matches!(
                fact,
                GameLoopFact::Combat(loading_bay_game::CombatFact::Vitality(
                    VitalityFact::DamageApplied {
                        source: loading_bay_game::DamageSource::EnemyAttack { attacker },
                        target: PLAYER,
                        health_damage: 7,
                        ..
                    }
                )) if *attacker == RANGED
            )
        });
        if hit {
            break;
        }
    }
    assert!(
        hit,
        "enemy projectile reaches the selected player exactly once"
    );
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .health(PLAYER)
            .unwrap()
            .current,
        93
    );

    let snapshot = encode_game_snapshot(game_loop.runtime()).unwrap();
    let reopened = decode_game_snapshot(&snapshot).unwrap();
    assert!(!reopened
        .readout()
        .projection
        .iter()
        .any(|node| node.asset == "mesh/physics-projectile"));
    assert_eq!(
        reopened
            .session()
            .enemy_combat(RANGED)
            .unwrap()
            .config
            .attack
            .kind,
        EnemyAttackKind::Projectile
    );
}

#[test]
fn canonical_voxel_wall_blocks_sight_and_ranged_damage() {
    let mut project = single_enemy_project(RANGED);
    entity_mut(&mut project, PLAYER)["translation"] = serde_json::json!([4.5, 1.5, 7.5]);
    entity_mut(&mut project, RANGED)["translation"] = serde_json::json!([4.5, 1.5, 4.5]);
    entity_mut(&mut project, RANGED)["navigation"]["goal"] = serde_json::json!([4.5, 1.5, 7.5]);
    entity_mut(&mut project, RANGED)["enemyCombat"]["hearingRange"] = 0.into();
    project["scenes"][0]["voxelEnvironment"] = solid_room_with_wall();
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();

    for _ in 0..5 {
        let receipt = game_loop.run_fixed_tick().unwrap();
        assert!(!receipt.facts.iter().any(|fact| matches!(
            fact,
            GameLoopFact::EnemyCombat(
                EnemyCombatFact::Alerted { enemy: RANGED, .. }
                    | EnemyCombatFact::AttackFired { enemy: RANGED, .. }
            )
        )));
    }
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .enemy_combat(RANGED)
            .unwrap()
            .state
            .posture,
        EnemyCombatPosture::Sleeping
    );
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .health(PLAYER)
            .unwrap()
            .current,
        100
    );
}

#[test]
fn active_bulkhead_blocks_sight_and_damage_until_opened() {
    let mut project = single_enemy_project(RANGED);
    entity_mut(&mut project, PLAYER)["translation"] = serde_json::json!([2.5, 1.5, 4.5]);
    entity_mut(&mut project, RANGED)["translation"] = serde_json::json!([2.5, 1.5, 6.5]);
    entity_mut(&mut project, RANGED)["navigation"]["goal"] = serde_json::json!([2.5, 1.5, 6.5]);
    entity_mut(&mut project, RANGED)["enemyCombat"]["hearingRange"] = 0.into();
    entity_mut(&mut project, MAINTENANCE_BULKHEAD)["translation"] =
        serde_json::json!([2.5, 1.5, 5.5]);
    entity_mut(&mut project, MAINTENANCE_BULKHEAD)["bounds"] = serde_json::json!({
        "min": [-3.2, -1.5, -0.275],
        "max": [3.2, 1.5, 0.275]
    });
    entity_mut(&mut project, MAINTENANCE_BULKHEAD)["door"]["openTranslation"] =
        serde_json::json!([2.5, 5.5, 5.5]);
    entity_mut(&mut project, MAINTENANCE_BULKHEAD)["door"]["access"]["activationRadius"] = 4.into();
    entity_mut(&mut project, PLAYER)["inventory"]["startingStacks"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "item": "key/maintenance-pass",
            "quantity": 1
        }));
    project["scenes"][0]["voxelEnvironment"] = solid_room_floor();
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();

    for _ in 0..3 {
        let blocked = game_loop.run_fixed_tick().unwrap();
        assert!(!blocked.facts.iter().any(|fact| matches!(
            fact,
            GameLoopFact::EnemyCombat(
                EnemyCombatFact::Alerted { enemy: RANGED, .. }
                    | EnemyCombatFact::AttackFired { enemy: RANGED, .. }
            )
        )));
    }
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .enemy_combat(RANGED)
            .unwrap()
            .state
            .posture,
        EnemyCombatPosture::Sleeping
    );
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .health(PLAYER)
            .unwrap()
            .current,
        100
    );

    game_loop
        .runtime_mut()
        .open_keyed_door(PLAYER, MAINTENANCE_BULKHEAD)
        .unwrap();
    let door = game_loop
        .runtime()
        .session()
        .door(MAINTENANCE_BULKHEAD)
        .unwrap();
    assert!(!door.entity_view.collision.unwrap().enabled);

    let alerted = game_loop.run_fixed_tick().unwrap();
    assert!(alerted.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::EnemyCombat(EnemyCombatFact::Alerted {
            enemy: RANGED,
            target: PLAYER,
            ..
        })
    )));
    let attack = game_loop.run_fixed_tick().unwrap();
    assert_enemy_damage(&attack.facts, RANGED, 4);
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .health(PLAYER)
            .unwrap()
            .current,
        96
    );
}

#[test]
fn ranged_muzzle_occlusion_emits_typed_fired_and_miss_without_damage() {
    let mut project = single_enemy_project(RANGED);
    entity_mut(&mut project, PLAYER)["translation"] = serde_json::json!([2.5, 1.5, 6.5]);
    entity_mut(&mut project, RANGED)["translation"] = serde_json::json!([2.5, 1.5, 3.5]);
    entity_mut(&mut project, RANGED)["enemyCombat"]["attack"]["originOffset"] =
        serde_json::json!([1, 0, 0]);
    project["scenes"][0]["voxelEnvironment"] = solid_room_with_muzzle_block();
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();

    game_loop.run_fixed_tick().unwrap();
    let attack = game_loop.run_fixed_tick().unwrap();

    assert!(attack.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::EnemyCombat(EnemyCombatFact::AttackFired {
            enemy: RANGED,
            target: PLAYER,
            ..
        })
    )));
    assert!(attack.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::EnemyCombat(EnemyCombatFact::AttackMissed {
            enemy: RANGED,
            target: PLAYER,
            reason: EnemyAttackMissReason::WorldBlocked,
            ..
        })
    )));
    assert!(!attack.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::EnemyCombat(EnemyCombatFact::AttackHit { .. } | EnemyCombatFact::Vitality(_))
    )));
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .health(PLAYER)
            .unwrap()
            .current,
        100
    );
}

#[test]
fn invalid_enemy_combat_compositions_fail_at_the_authored_component() {
    let mut invalid_damage: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    entity_mut(&mut invalid_damage, MELEE)["enemyCombat"]["attack"]["damage"] = 0.into();
    let mut missing_health: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    entity_mut(&mut missing_health, MELEE)
        .as_object_mut()
        .unwrap()
        .remove("health");
    let mut missing_navigation: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    entity_mut(&mut missing_navigation, MELEE)
        .as_object_mut()
        .unwrap()
        .remove("navigation");

    for project in [invalid_damage, missing_health, missing_navigation] {
        let RuntimeError::StoredProject(error) =
            GameRuntime::from_stored_project(&project.to_string()).unwrap_err()
        else {
            panic!("enemy combat composition did not fail through project admission");
        };
        assert_eq!(error.diagnostic().code, diagnostic_code::INVALID_COMPONENT);
        assert_eq!(error.diagnostic().path, "scenes[0].entities[3].enemyCombat");
    }
}

#[test]
fn alerted_enemy_tracks_moving_player_through_transient_navigation_goal() {
    let mut project = single_enemy_project(MELEE);
    entity_mut(&mut project, MELEE)["translation"] = serde_json::json!([1.5, 1.5, 6.5]);
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();
    let generation = game_loop.start_connection().connection_generation;

    game_loop.run_fixed_tick().unwrap();
    let before = game_loop
        .runtime()
        .session()
        .enemy(MELEE)
        .unwrap()
        .entity_view
        .transform
        .unwrap()
        .translation;
    game_loop
        .submit_input(PlayerInputCommand {
            connection_generation: generation,
            sequence: 1,
            intent: PlayerInputIntent {
                movement: [1.0, 0.0],
                ..PlayerInputIntent::NEUTRAL
            },
        })
        .unwrap();
    game_loop.run_fixed_tick().unwrap();
    let after = game_loop
        .runtime()
        .session()
        .enemy(MELEE)
        .unwrap()
        .entity_view
        .transform
        .unwrap()
        .translation;
    let remembered = game_loop
        .runtime()
        .session()
        .enemy_combat(MELEE)
        .unwrap()
        .state
        .last_known_target_position
        .unwrap();

    assert_ne!(after, before);
    let player = game_loop
        .runtime()
        .session()
        .player_controller(PLAYER)
        .unwrap();
    let mut gameplay_position = player.entity_view.transform.unwrap().translation;
    gameplay_position.y += player.eye_offset_from_center - player.config.traversal.eye_height;
    assert_eq!(remembered, gameplay_position);
}

#[test]
fn simultaneous_attacks_apply_in_entity_order_and_kill_once() {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    strip_campaign_expansion(&mut project);
    activate_encounter_immediately(&mut project);
    entity_mut(&mut project, EntityId::new(2))["encounter"]["members"] =
        serde_json::json!([MELEE.raw(), RANGED.raw()]);
    for enemy in [MELEE, RANGED] {
        let entity = entity_mut(&mut project, enemy);
        entity["translation"] = if enemy == MELEE {
            serde_json::json!([4.5, 1.5, 3.5])
        } else {
            serde_json::json!([6.5, 1.5, 3.5])
        };
        entity["enemyCombat"]["attack"] = serde_json::json!({
            "kind": "rangedHitscan",
            "damage": 60,
            "range": 8,
            "cooldownTicks": 30,
            "originOffset": [0, 0, 0],
            "presentation": format!("simultaneous-{}", enemy.raw())
        });
    }
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();
    game_loop.run_fixed_tick().unwrap();
    let lethal = game_loop.run_fixed_tick().unwrap();

    let fired = lethal
        .facts
        .iter()
        .filter_map(|fact| match fact {
            GameLoopFact::EnemyCombat(EnemyCombatFact::AttackFired { enemy, .. }) => Some(*enemy),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(fired, vec![MELEE, RANGED]);
    assert_eq!(
        lethal
            .facts
            .iter()
            .filter(|fact| matches!(
                fact,
                GameLoopFact::EnemyCombat(EnemyCombatFact::Vitality(VitalityFact::Died {
                    entity: PLAYER,
                    ..
                }))
            ))
            .count(),
        1
    );
    assert_eq!(
        game_loop.runtime().session().health(PLAYER).unwrap().state,
        VitalityState::Dead
    );
}

#[test]
fn defeated_enemy_stays_dead_and_never_reawakens() {
    let project = single_enemy_project(RANGED);
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    runtime.defeat_enemy(PLAYER, RANGED).unwrap();
    let defeated_at = runtime
        .session()
        .enemy(RANGED)
        .unwrap()
        .entity_view
        .transform
        .unwrap()
        .translation;
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();

    for _ in 0..5 {
        let receipt = game_loop.run_fixed_tick().unwrap();
        assert!(!receipt.facts.iter().any(|fact| matches!(
            fact,
            GameLoopFact::EnemyCombat(EnemyCombatFact::Alerted { enemy: RANGED, .. })
        )));
    }
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .enemy_combat(RANGED)
            .unwrap()
            .state
            .posture,
        EnemyCombatPosture::Dead
    );
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .enemy(RANGED)
            .unwrap()
            .entity_view
            .transform
            .unwrap()
            .translation,
        defeated_at
    );
}

#[test]
fn snapshot_reopen_preserves_combat_memory_and_eventual_attack() {
    let project = single_enemy_project(RANGED);
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut uninterrupted = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();
    uninterrupted.run_fixed_tick().unwrap();
    let reopened =
        decode_game_snapshot(&encode_game_snapshot(uninterrupted.runtime()).unwrap()).unwrap();
    let mut reopened = LoadingBayGameLoop::new(reopened, PLAYER).unwrap();

    let expected = uninterrupted.run_fixed_tick().unwrap();
    let actual = reopened.run_fixed_tick().unwrap();
    assert_eq!(actual.facts, expected.facts);
    assert_eq!(
        encode_game_snapshot(reopened.runtime()).unwrap(),
        encode_game_snapshot(uninterrupted.runtime()).unwrap()
    );
}

#[test]
fn enemy_kill_clears_input_and_authoritative_restart_remains_available() {
    let mut project = single_enemy_project(RANGED);
    entity_mut(&mut project, RANGED)["enemyCombat"]["attack"]["damage"] = 100.into();
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();
    let generation = game_loop.start_connection().connection_generation;
    game_loop
        .submit_input(PlayerInputCommand {
            connection_generation: generation,
            sequence: 1,
            intent: PlayerInputIntent {
                movement: [1.0, 0.0],
                primary_fire_held: true,
                ..PlayerInputIntent::NEUTRAL
            },
        })
        .unwrap();
    game_loop.run_fixed_tick().unwrap();
    game_loop.run_fixed_tick().unwrap();
    assert_eq!(
        game_loop.runtime().session().health(PLAYER).unwrap().state,
        VitalityState::Dead
    );
    game_loop
        .submit_edge_command(GameLoopEdgeCommand {
            connection_generation: generation,
            sequence: 2,
            command: GameLoopEdgeCommandKind::RestartAuthoredBaseline,
        })
        .unwrap();
    let restart = game_loop.run_fixed_tick().unwrap();
    assert!(restart.facts.contains(&GameLoopFact::RestartRequested {
        sequence: 2,
        mode: GameRestartMode::AuthoredBaseline,
    }));
}

#[test]
fn legacy_snapshot_rejects_future_enemy_combat_state() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    snapshot["schemaVersion"] = 16.into();
    support::strip_future_gameplay_mechanics_state(&mut snapshot);

    assert!(matches!(
        decode_game_snapshot(&snapshot.to_string()),
        Err(loading_bay_game::GameSnapshotError::FutureEnemyCombatStateInLegacySnapshot)
    ));
}

#[test]
fn snapshot_rejects_enemy_cooldown_beyond_authored_cadence() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    let tick = snapshot["tick"].as_u64().unwrap();
    let combat = snapshot["enemyCombat"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|combat| combat["entity"] == RANGED.raw())
        .unwrap();
    let cooldown = combat["cooldownTicks"].as_u64().unwrap();
    combat["readyAtTick"] = (tick + cooldown + 1).into();

    assert!(matches!(
        decode_game_snapshot(&snapshot.to_string()),
        Err(loading_bay_game::GameSnapshotError::InvalidEnemyCombatState {
            entity
        }) if entity == RANGED.raw()
    ));
}

fn single_enemy_project(enemy: EntityId) -> serde_json::Value {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    strip_campaign_expansion(&mut project);
    entity_mut(&mut project, PLAYER)["translation"] = serde_json::json!([1.5, 1.5, 2.5]);
    entity_mut(&mut project, MAINTENANCE_BULKHEAD)
        .as_object_mut()
        .unwrap()
        .remove("bounds");
    activate_encounter_immediately(&mut project);
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .retain(|entity| {
            entity["id"]
                != if enemy == MELEE {
                    RANGED.raw()
                } else {
                    MELEE.raw()
                }
        });
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == 2)
        .unwrap()["encounter"]["members"] = serde_json::json!([enemy.raw()]);
    entity_mut(&mut project, enemy)["translation"] = serde_json::json!([1.5, 1.5, 5.5]);
    entity_mut(&mut project, enemy)["navigation"]["goal"] = serde_json::json!([1.5, 1.5, 5.5]);
    project
}

fn strip_campaign_expansion(project: &mut serde_json::Value) {
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .retain(|entity| {
            let id = entity["id"].as_u64().unwrap();
            !matches!(id, 40..=42 | 50..=54 | 60..=65)
        });
}

fn activate_encounter_immediately(project: &mut serde_json::Value) {
    entity_mut(project, EntityId::new(2))["encounter"]["activationRadius"] =
        serde_json::Value::Null;
}

fn entity_mut(project: &mut serde_json::Value, id: EntityId) -> &mut serde_json::Value {
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == id.raw())
        .unwrap()
}

fn solid_room_with_wall() -> serde_json::Value {
    let mut room = solid_room_floor();
    let voxels = room["solidVoxels"].as_array_mut().unwrap();
    for y in 1..=3 {
        voxels.push(serde_json::json!([4, y, 5]));
    }
    room
}

fn solid_room_floor() -> serde_json::Value {
    let mut voxels = Vec::new();
    for x in 0..=9 {
        for z in 0..=11 {
            voxels.push(serde_json::json!([x, 0, z]));
        }
    }
    serde_json::json!({
        "kind": "solid",
        "voxelSize": 1,
        "chunkSize": 16,
        "solidVoxels": voxels
    })
}

fn solid_room_with_muzzle_block() -> serde_json::Value {
    let mut room = solid_room_with_wall();
    let voxels = room["solidVoxels"].as_array_mut().unwrap();
    voxels.retain(|voxel| voxel != &serde_json::json!([4, 1, 5]));
    voxels.retain(|voxel| voxel != &serde_json::json!([4, 2, 5]));
    voxels.retain(|voxel| voxel != &serde_json::json!([4, 3, 5]));
    voxels.push(serde_json::json!([3, 1, 3]));
    room
}

fn assert_enemy_damage(facts: &[GameLoopFact], enemy: EntityId, damage: u32) {
    assert!(facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::EnemyCombat(EnemyCombatFact::Vitality(VitalityFact::DamageApplied {
            source: loading_bay_game::DamageSource::EnemyAttack { attacker },
            target: PLAYER,
            health_damage,
            ..
        })) if *attacker == enemy && *health_damage == damage
    )));
    assert!(!facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::Progression(ProgressionFact::LevelCompleted { .. })
    )));
}
