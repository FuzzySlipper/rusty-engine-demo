mod support;

use core_ids::EntityId;
use loading_bay_game::{
    decode_game_snapshot, decode_project_document, diagnostic_code, encode_game_snapshot,
    DamageCommand, DamageDisposition, DamageService, DamageSource, EdgeCommandRejection, GameEvent,
    GameLoopEdgeCommand, GameLoopEdgeCommandKind, GameLoopFact, GameRestartMode, GameRuntime,
    GameSnapshotError, InputCommandRejection, InventoryAction, InventoryCommand, InventoryService,
    ItemDefinitionId, LoadingBayGameLoop, PlayerInputCommand, PlayerInputIntent, VitalityFact,
    VitalityRejection, VitalityState, STORED_PROJECT_SCHEMA_VERSION,
};
use serde_json::{json, Value};

const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const PLAYER: EntityId = EntityId::new(1);
const HAZARD: EntityId = EntityId::new(27);

#[test]
fn armor_absorbs_damage_and_lethal_damage_is_exactly_once() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut session = runtime.session().clone();
    let armor_item = item("armor/impact-vest");
    InventoryService::apply(
        &mut session,
        PLAYER,
        InventoryCommand {
            sequence: 1,
            action: InventoryAction::Grant {
                item: armor_item.clone(),
                quantity: 1,
            },
        },
    )
    .unwrap();

    let armor = DamageService::grant_armor(&mut session, PLAYER, armor_item.clone()).unwrap();
    assert!(armor.facts.contains(&VitalityFact::ArmorGranted {
        entity: PLAYER,
        item: armor_item.clone(),
        amount: 100,
        before: 0,
        after: 100,
    }));
    assert_eq!(quantity(&session, "armor/impact-vest"), 0);

    let first = DamageService::apply(
        &mut session,
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: PLAYER,
            amount: 30,
        },
    )
    .unwrap();
    assert_eq!(first.disposition, DamageDisposition::Applied);
    assert!(first.facts.contains(&VitalityFact::DamageApplied {
        source: DamageSource::Direct { actor: PLAYER },
        target: PLAYER,
        incoming: 30,
        armor_absorbed: 15,
        health_damage: 15,
        health_before: 100,
        health_after: 85,
        armor_before: 100,
        armor_after: 85,
    }));

    let lethal = DamageService::apply(
        &mut session,
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: PLAYER,
            amount: 200,
        },
    )
    .unwrap();
    assert_eq!(session.health(PLAYER).unwrap().current, 0);
    assert_eq!(session.health(PLAYER).unwrap().state, VitalityState::Dead);
    assert_eq!(
        lethal
            .facts
            .iter()
            .filter(|fact| matches!(fact, VitalityFact::Died { entity, .. } if *entity == PLAYER))
            .count(),
        1
    );
    assert!(matches!(
        lethal.event,
        Some(GameEvent::PlayerDied { player: PLAYER, .. })
    ));

    let repeated = DamageService::apply(
        &mut session,
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: PLAYER,
            amount: 1,
        },
    )
    .unwrap();
    assert_eq!(repeated.disposition, DamageDisposition::AlreadyDead);
    assert!(repeated.facts.is_empty());
    assert!(repeated.event.is_none());
}

#[test]
fn healing_is_bounded_consumes_one_item_and_rejections_preserve_inventory() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut session = runtime.session().clone();
    let supply = item("supply/med-patch");
    let before_full = session.inventory(PLAYER).unwrap();

    assert_eq!(
        DamageService::use_health_supply(&mut session, PLAYER, supply.clone()).unwrap_err(),
        VitalityRejection::HealthFull { player: PLAYER }
    );
    assert_eq!(session.inventory(PLAYER).unwrap(), before_full);

    DamageService::apply(
        &mut session,
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: PLAYER,
            amount: 10,
        },
    )
    .unwrap();
    let healed = DamageService::use_health_supply(&mut session, PLAYER, supply.clone()).unwrap();
    assert!(healed.facts.contains(&VitalityFact::HealthRestored {
        entity: PLAYER,
        item: supply,
        amount: 10,
        before: 90,
        after: 100,
    }));
    assert_eq!(quantity(&session, "supply/med-patch"), 0);
}

#[test]
fn fixed_tick_hazard_uses_authored_cadence() {
    let mut game_loop = hazard_loop(20, 2, 100);
    game_loop.start_connection();

    let first = game_loop.run_fixed_tick().unwrap();
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .health(PLAYER)
            .unwrap()
            .current,
        80
    );
    assert!(first.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::Hazard(loading_bay_game::HazardFact::Damage(
            VitalityFact::DamageApplied {
                source: DamageSource::Hazard { hazard: HAZARD },
                target: PLAYER,
                health_after: 80,
                ..
            }
        ))
    )));

    let cooling_down = game_loop.run_fixed_tick().unwrap();
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .health(PLAYER)
            .unwrap()
            .current,
        80
    );
    assert!(!cooling_down
        .facts
        .iter()
        .any(|fact| matches!(fact, GameLoopFact::Hazard(_))));

    game_loop.run_fixed_tick().unwrap();
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .health(PLAYER)
            .unwrap()
            .current,
        60
    );
}

#[test]
fn death_clears_intent_rejects_same_tick_edges_and_exposes_distinct_restart_modes() {
    let mut game_loop = hazard_loop(100, 60, 100);
    let generation = game_loop.start_connection().connection_generation;
    game_loop
        .submit_edge_command(edge(
            generation,
            1,
            GameLoopEdgeCommandKind::UseItem {
                item: "supply/med-patch".to_owned(),
            },
        ))
        .unwrap();

    let death = game_loop.run_fixed_tick().unwrap();
    assert_eq!(
        game_loop.runtime().session().health(PLAYER).unwrap().state,
        VitalityState::Dead
    );
    assert_eq!(
        quantity(game_loop.runtime().session(), "supply/med-patch"),
        1
    );
    assert_eq!(
        death
            .facts
            .iter()
            .filter(|fact| matches!(
                fact,
                GameLoopFact::Event(GameEvent::PlayerDied { player: PLAYER, .. })
            ))
            .count(),
        1
    );
    assert!(death.facts.contains(&GameLoopFact::EdgeCommandRejected {
        sequence: 1,
        reason: EdgeCommandRejection::PlayerDefeated,
    }));
    assert_eq!(
        game_loop
            .submit_input(PlayerInputCommand {
                connection_generation: generation,
                sequence: 2,
                intent: PlayerInputIntent {
                    movement: [1.0, 0.0],
                    look_delta: [0.5, 0.5],
                    primary_fire_held: true,
                },
            })
            .unwrap_err(),
        InputCommandRejection::PlayerDefeated
    );

    game_loop
        .submit_edge_command(edge(
            generation,
            2,
            GameLoopEdgeCommandKind::RestartCheckpoint,
        ))
        .unwrap();
    let checkpoint = game_loop.run_fixed_tick().unwrap();
    assert!(checkpoint
        .facts
        .contains(&GameLoopFact::EdgeCommandRejected {
            sequence: 2,
            reason: EdgeCommandRejection::CheckpointUnavailable,
        }));
    assert_eq!(
        checkpoint
            .facts
            .iter()
            .filter(|fact| matches!(fact, GameLoopFact::Event(GameEvent::PlayerDied { .. })))
            .count(),
        0
    );

    game_loop
        .submit_edge_command(edge(
            generation,
            3,
            GameLoopEdgeCommandKind::RestartAuthoredBaseline,
        ))
        .unwrap();
    assert!(game_loop
        .run_fixed_tick()
        .unwrap()
        .facts
        .contains(&GameLoopFact::RestartRequested {
            sequence: 3,
            mode: GameRestartMode::AuthoredBaseline,
        }));
}

#[test]
fn snapshot_round_trip_preserves_dead_posture_and_rejects_future_vitality_state() {
    let mut game_loop = hazard_loop(100, 1, 100);
    game_loop.run_fixed_tick().unwrap();
    assert_eq!(
        game_loop.runtime().session().health(PLAYER).unwrap().state,
        VitalityState::Dead
    );
    let runtime = game_loop.into_runtime();
    let dead_snapshot: Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();

    let reopened = decode_game_snapshot(&dead_snapshot.to_string()).unwrap();
    assert_eq!(
        reopened.session().health(PLAYER).unwrap().state,
        VitalityState::Dead
    );
    let mut reconnected = LoadingBayGameLoop::new(reopened, PLAYER).unwrap();
    let generation = reconnected.start_connection().connection_generation;
    assert_eq!(
        reconnected
            .submit_input(PlayerInputCommand {
                connection_generation: generation,
                sequence: 1,
                intent: PlayerInputIntent::NEUTRAL,
            })
            .unwrap_err(),
        InputCommandRejection::PlayerDefeated
    );

    let mut legacy: Value = serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    legacy["schemaVersion"] = 13.into();
    support::strip_future_gameplay_mechanics_state(&mut legacy);
    assert!(matches!(
        decode_game_snapshot(&legacy.to_string()).unwrap_err(),
        GameSnapshotError::FutureVitalityStateInLegacySnapshot
    ));
}

#[test]
fn project_and_snapshot_admission_fail_closed_for_future_hazard_state() {
    let mut legacy_project: Value = serde_json::from_str(PROJECT).unwrap();
    legacy_project["schemaVersion"] = 14.into();
    let rejection = decode_project_document(&legacy_project.to_string()).unwrap_err();
    assert_eq!(rejection.diagnostic().code, diagnostic_code::MIGRATION);

    legacy_project["assets"]
        .as_array_mut()
        .unwrap()
        .retain(|asset| asset.get("voxelObject").is_none());
    for scene in legacy_project["scenes"].as_array_mut().unwrap() {
        scene
            .as_object_mut()
            .unwrap()
            .remove("voxelObjectInstances");
    }
    legacy_project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .retain(|entity| entity.get("hazard").is_none());
    for definition in legacy_project["itemDefinitions"].as_array_mut().unwrap() {
        if let Some(weapon) = definition
            .get_mut("kind")
            .and_then(Value::as_object_mut)
            .filter(|kind| kind.get("kind") == Some(&Value::String("weapon".to_string())))
        {
            weapon.insert(
                "attackMode".to_string(),
                Value::String("hitscan".to_string()),
            );
            weapon.remove("pelletCount");
            weapon.remove("spreadDegrees");
        }
    }
    for entity in legacy_project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
    {
        entity.as_object_mut().unwrap().remove("enemyCombat");
        entity.as_object_mut().unwrap().remove("defeatDrop");
        entity.as_object_mut().unwrap().remove("secretRegion");
        entity.as_object_mut().unwrap().remove("levelExit");
        if let Some(encounter) = entity.get_mut("encounter").and_then(Value::as_object_mut) {
            encounter.remove("activationRadius");
        }
        if let Some(door) = entity.get_mut("door").and_then(Value::as_object_mut) {
            door.remove("access");
        }
        if let Some(switch) = entity.get_mut("switch").and_then(Value::as_object_mut) {
            switch.remove("loadingBayInterlock");
        }
        if let Some(health) = entity.get_mut("health").and_then(Value::as_object_mut) {
            health.remove("maxArmor");
            health.remove("armorAbsorptionPercent");
        }
    }
    let migrated = decode_project_document(&legacy_project.to_string()).unwrap();
    assert_eq!(migrated.source_schema_version, 14);
    assert_eq!(
        migrated.project.schema_version,
        STORED_PROJECT_SCHEMA_VERSION
    );

    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut snapshot: Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    let tick = snapshot["tick"].as_u64().unwrap();
    let hazard = snapshot["hazards"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|hazard| hazard["entity"] == HAZARD.raw())
        .unwrap();
    let cooldown = hazard["cooldownTicks"].as_u64().unwrap();
    hazard["readyAtTick"] = json!(tick + cooldown + 1);
    assert!(matches!(
        decode_game_snapshot(&snapshot.to_string()).unwrap_err(),
        GameSnapshotError::InvalidHazardConfig { entity } if entity == HAZARD.raw()
    ));
}

#[test]
fn overlapping_hazard_without_player_health_is_rejected_before_loop_mutation() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    strip_enemy_combat(&mut project);
    let hazard_translation = entity(&project, HAZARD.raw())["translation"].clone();
    let player = entity_mut(&mut project, PLAYER.raw());
    player["translation"] = hazard_translation;
    player.as_object_mut().unwrap().remove("health");

    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let before = encode_game_snapshot(&runtime).unwrap();
    assert!(matches!(
        LoadingBayGameLoop::validate_runtime(&runtime, PLAYER),
        Err(loading_bay_game::RuntimeError::HazardPlayerMissingVitality { player: PLAYER })
    ));
    assert_eq!(runtime.tick().raw(), 0);
    assert_eq!(encode_game_snapshot(&runtime).unwrap(), before);
    assert!(matches!(
        LoadingBayGameLoop::new(runtime, PLAYER),
        Err(loading_bay_game::RuntimeError::HazardPlayerMissingVitality { player: PLAYER })
    ));

    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .retain(|entity| entity.get("hazard").is_none());
    LoadingBayGameLoop::new(
        GameRuntime::from_stored_project(&project.to_string()).unwrap(),
        PLAYER,
    )
    .unwrap();
}

fn hazard_loop(damage: u32, cooldown_ticks: u64, max_health: u32) -> LoadingBayGameLoop {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    strip_enemy_combat(&mut project);
    let hazard_translation = entity(&project, HAZARD.raw())["translation"].clone();
    let player = entity_mut(&mut project, PLAYER.raw());
    player["translation"] = hazard_translation;
    player["health"]["max"] = max_health.into();
    let hazard = entity_mut(&mut project, HAZARD.raw());
    hazard["hazard"]["damage"] = damage.into();
    hazard["hazard"]["cooldownTicks"] = cooldown_ticks.into();
    LoadingBayGameLoop::new(
        GameRuntime::from_stored_project(&project.to_string()).unwrap(),
        PLAYER,
    )
    .unwrap()
}

fn strip_enemy_combat(project: &mut Value) {
    for entity in project["scenes"][0]["entities"].as_array_mut().unwrap() {
        entity.as_object_mut().unwrap().remove("enemyCombat");
    }
}

fn edge(
    connection_generation: u64,
    sequence: u64,
    command: GameLoopEdgeCommandKind,
) -> GameLoopEdgeCommand {
    GameLoopEdgeCommand {
        connection_generation,
        sequence,
        command,
    }
}

fn item(value: &str) -> ItemDefinitionId {
    ItemDefinitionId::parse(value).unwrap()
}

fn quantity(session: &loading_bay_game::GameSession, item_id: &str) -> u32 {
    session
        .inventory(PLAYER)
        .unwrap()
        .stacks
        .iter()
        .find(|stack| stack.item.as_str() == item_id)
        .map_or(0, |stack| stack.quantity)
}

fn entity(project: &Value, id: u64) -> &Value {
    project["scenes"][0]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["id"] == id)
        .unwrap()
}

fn entity_mut(project: &mut Value, id: u64) -> &mut Value {
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == id)
        .unwrap()
}
