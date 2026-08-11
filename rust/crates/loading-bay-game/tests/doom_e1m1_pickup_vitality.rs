use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, ArmorGrantMode, ArmorTransition, DamageCommand,
    DamageService, DamageSource, GameLoopFact, GameRuntime, GameSnapshotError, HazardFact,
    ItemKind, LoadingBayGameLoop, PickupRejection, PickupState, RuntimeError, VitalityFact,
    VitalityRejection, VitalityState,
};
use rusty_engine::core_ids::EntityId;

const PROJECT: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");
const PLAYER: EntityId = EntityId::new(1);

#[test]
fn authored_e1m1_vitality_vocabulary_and_incidence_are_exact() {
    let runtime = GameRuntime::from_stored_project(PROJECT).expect("admit E1M1");
    let session = runtime.session();
    let health = session.health(PLAYER).unwrap();
    assert_eq!((health.current, health.config.max), (100, 200));
    assert_eq!(health.config.max_armor, 200);
    assert_eq!(health.armor, 0);

    let definition = |id: &str| {
        session
            .item_definitions()
            .find(|definition| definition.id.as_str() == id)
            .unwrap()
            .kind
    };
    assert_eq!(
        definition("supply/stimpack"),
        ItemKind::HealthSupply {
            restore_health: 10,
            maximum_health: Some(100),
            automatic_use: true,
        }
    );
    assert_eq!(
        definition("supply/medikit"),
        ItemKind::HealthSupply {
            restore_health: 25,
            maximum_health: Some(100),
            automatic_use: true,
        }
    );
    assert_eq!(
        definition("supply/health-bonus"),
        ItemKind::HealthSupply {
            restore_health: 1,
            maximum_health: Some(200),
            automatic_use: true,
        }
    );
    assert_eq!(
        definition("armor/bonus"),
        ItemKind::Armor {
            protection: 1,
            maximum_armor: Some(200),
            absorption_percent: Some(33),
            grant_mode: ArmorGrantMode::Add,
            transition: ArmorTransition::Preserve,
        }
    );
    assert_eq!(
        definition("armor/green"),
        ItemKind::Armor {
            protection: 100,
            maximum_armor: Some(200),
            absorption_percent: Some(33),
            grant_mode: ArmorGrantMode::SetMinimum,
            transition: ArmorTransition::Replace,
        }
    );
    assert_eq!(
        definition("armor/blue"),
        ItemKind::Armor {
            protection: 200,
            maximum_armor: Some(200),
            absorption_percent: Some(50),
            grant_mode: ArmorGrantMode::SetMinimum,
            transition: ArmorTransition::Replace,
        }
    );

    assert_eq!(pickup_count(&runtime, "supply/stimpack"), 1);
    assert_eq!(pickup_count(&runtime, "supply/medikit"), 3);
    assert_eq!(pickup_count(&runtime, "supply/health-bonus"), 13);
    assert_eq!(pickup_count(&runtime, "armor/bonus"), 25);
    assert_eq!(pickup_count(&runtime, "armor/green"), 1);
    assert_eq!(pickup_count(&runtime, "armor/blue"), 1);
    assert_eq!(session.hazards().len(), 4);
    assert!(session
        .hazards()
        .all(|hazard| hazard.config.damage == 5 && hazard.config.cooldown_ticks == 55));
    assert!(!session
        .item_definitions()
        .any(|definition| matches!(definition.kind, ItemKind::AccessKey)));
}

#[test]
fn authored_nukage_uses_the_bounded_hazard_owner() {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    let entities = project["scenes"][0]["entities"].as_array_mut().unwrap();
    let player_translation = entities
        .iter()
        .find(|entity| entity["id"] == PLAYER.raw())
        .unwrap()["translation"]
        .clone();
    for entity in entities.iter_mut() {
        entity.as_object_mut().unwrap().remove("enemyCombat");
        entity.as_object_mut().unwrap().remove("navigation");
    }
    entities
        .iter_mut()
        .find(|entity| entity.get("hazard").is_some())
        .unwrap()["translation"] = player_translation;

    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();
    game_loop.start_connection();
    let tick = game_loop.run_fixed_tick().unwrap();
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .health(PLAYER)
            .unwrap()
            .current,
        95
    );
    assert!(tick.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::Hazard(HazardFact::Damage(VitalityFact::DamageApplied {
            target: PLAYER,
            incoming: 5,
            health_after: 95,
            ..
        }))
    )));
}

#[test]
fn snapshot_schema_twenty_one_rejects_future_vitality_item_policy() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut legacy: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    legacy["schemaVersion"] = 21.into();
    assert!(matches!(
        decode_game_snapshot(&legacy.to_string()).unwrap_err(),
        GameSnapshotError::FutureVitalityStateInLegacySnapshot
    ));

    for definition in legacy["itemDefinitions"].as_array_mut().unwrap() {
        let kind = definition["kind"].as_object_mut().unwrap();
        for field in [
            "maximumHealth",
            "automaticUse",
            "maximumArmor",
            "absorptionPercent",
            "grantMode",
            "transition",
        ] {
            kind.remove(field);
        }
    }
    decode_game_snapshot(&legacy.to_string()).unwrap();
}

#[test]
fn walk_over_health_pickups_apply_atomically_and_rejections_leave_the_world_intact() {
    let mut full = bounded_runtime();
    let stimpack = pickup(&full, "supply/stimpack");
    full = with_overlap(full, stimpack);
    let before = encode_game_snapshot(&full).unwrap();
    assert!(matches!(
        full.collect_pickup(PLAYER, stimpack, 1, 1),
        Err(RuntimeError::Pickup(PickupRejection::Vitality(
            VitalityRejection::HealthFull { player: PLAYER }
        )))
    ));
    assert_eq!(encode_game_snapshot(&full).unwrap(), before);
    assert_eq!(
        full.session().pickup(stimpack).unwrap().state,
        PickupState::Available
    );

    let mut bonus = bounded_runtime();
    let health_bonus = pickup(&bonus, "supply/health-bonus");
    bonus = with_overlap(bonus, health_bonus);
    let receipt = bonus.collect_pickup(PLAYER, health_bonus, 1, 1).unwrap();
    assert!(receipt
        .vitality_facts
        .contains(&VitalityFact::HealthRestored {
            entity: PLAYER,
            item: item("supply/health-bonus"),
            amount: 1,
            before: 100,
            after: 101,
        }));
    assert_eq!(bonus.session().health(PLAYER).unwrap().current, 101);
    assert_eq!(quantity(&bonus, "supply/health-bonus"), 0);
    assert!(matches!(
        bonus.session().pickup(health_bonus).unwrap().state,
        PickupState::Collected { .. }
    ));

    let base = bounded_runtime();
    let mut session = base.session().clone();
    DamageService::apply(
        &mut session,
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: PLAYER,
            amount: 30,
        },
    )
    .unwrap();
    let mut medikit_runtime = with_session(session);
    let medikit = pickup(&medikit_runtime, "supply/medikit");
    medikit_runtime = with_overlap(medikit_runtime, medikit);
    medikit_runtime
        .collect_pickup(PLAYER, medikit, 1, 1)
        .unwrap();
    assert_eq!(
        medikit_runtime.session().health(PLAYER).unwrap().current,
        95
    );
}

#[test]
fn armor_classes_order_damage_and_survive_snapshot_while_death_and_restart_are_clean() {
    let mut runtime = bounded_runtime();
    let green = pickup(&runtime, "armor/green");
    runtime = with_overlap(runtime, green);
    runtime.collect_pickup(PLAYER, green, 1, 1).unwrap();
    assert_eq!(runtime.session().health(PLAYER).unwrap().armor, 100);
    assert_eq!(
        runtime
            .session()
            .health(PLAYER)
            .unwrap()
            .armor_item
            .unwrap()
            .as_str(),
        "armor/green"
    );

    let mut session = runtime.session().clone();
    let green_damage = DamageService::apply(
        &mut session,
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: PLAYER,
            amount: 30,
        },
    )
    .unwrap();
    assert!(green_damage.facts.contains(&VitalityFact::DamageApplied {
        source: DamageSource::Direct { actor: PLAYER },
        target: PLAYER,
        incoming: 30,
        armor_absorbed: 9,
        health_damage: 21,
        health_before: 100,
        health_after: 79,
        armor_before: 100,
        armor_after: 91,
    }));

    runtime = with_session(session);
    let bonus = pickup(&runtime, "armor/bonus");
    runtime = with_overlap(runtime, bonus);
    runtime.collect_pickup(PLAYER, bonus, 1, 2).unwrap();
    let after_bonus = runtime.session().health(PLAYER).unwrap();
    assert_eq!(after_bonus.armor, 92);
    assert_eq!(after_bonus.armor_item.unwrap().as_str(), "armor/green");

    let blue = pickup(&runtime, "armor/blue");
    runtime = with_overlap(runtime, blue);
    runtime.collect_pickup(PLAYER, blue, 1, 3).unwrap();
    assert_eq!(runtime.session().health(PLAYER).unwrap().armor, 200);
    assert_eq!(
        runtime
            .session()
            .health(PLAYER)
            .unwrap()
            .armor_item
            .unwrap()
            .as_str(),
        "armor/blue"
    );

    let mut session = runtime.session().clone();
    let blue_damage = DamageService::apply(
        &mut session,
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: PLAYER,
            amount: 30,
        },
    )
    .unwrap();
    assert!(blue_damage.facts.contains(&VitalityFact::DamageApplied {
        source: DamageSource::Direct { actor: PLAYER },
        target: PLAYER,
        incoming: 30,
        armor_absorbed: 15,
        health_damage: 15,
        health_before: 79,
        health_after: 64,
        armor_before: 200,
        armor_after: 185,
    }));

    runtime = with_session(session);
    let encoded = encode_game_snapshot(&runtime).unwrap();
    let reopened = decode_game_snapshot(&encoded).unwrap();
    assert_eq!(encode_game_snapshot(&reopened).unwrap(), encoded);
    assert_eq!(reopened.session().health(PLAYER).unwrap().current, 64);
    assert_eq!(reopened.session().health(PLAYER).unwrap().armor, 185);
    assert_eq!(
        reopened
            .session()
            .health(PLAYER)
            .unwrap()
            .armor_item
            .unwrap()
            .as_str(),
        "armor/blue"
    );

    let mut stronger = bounded_runtime();
    let blue = pickup(&stronger, "armor/blue");
    stronger = with_overlap(stronger, blue);
    stronger.collect_pickup(PLAYER, blue, 1, 1).unwrap();
    let green = pickup(&stronger, "armor/green");
    stronger = with_overlap(stronger, green);
    let before_rejected_green = encode_game_snapshot(&stronger).unwrap();
    assert!(matches!(
        stronger.collect_pickup(PLAYER, green, 1, 2),
        Err(RuntimeError::Pickup(PickupRejection::Vitality(
            VitalityRejection::ArmorFull { player: PLAYER }
        )))
    ));
    assert_eq!(
        encode_game_snapshot(&stronger).unwrap(),
        before_rejected_green
    );
    assert_eq!(
        stronger.session().pickup(green).unwrap().state,
        PickupState::Available
    );

    let mut lethal_session = reopened.session().clone();
    DamageService::apply(
        &mut lethal_session,
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: PLAYER,
            amount: 500,
        },
    )
    .unwrap();
    assert_eq!(
        lethal_session.health(PLAYER).unwrap().state,
        VitalityState::Dead
    );

    let restarted = GameRuntime::from_stored_project(PROJECT).unwrap();
    let restarted_health = restarted.session().health(PLAYER).unwrap();
    assert_eq!((restarted_health.current, restarted_health.armor), (100, 0));
    assert!(restarted
        .session()
        .pickups()
        .all(|pickup| { !matches!(pickup.state, PickupState::Collected { .. }) }));
}

fn pickup(runtime: &GameRuntime, item: &str) -> EntityId {
    runtime
        .session()
        .pickups()
        .find(|pickup| {
            pickup.config.item.as_str() == item && pickup.state == PickupState::Available
        })
        .unwrap()
        .entity
}

fn bounded_runtime() -> GameRuntime {
    GameRuntime::from_stored_project(PROJECT).unwrap()
}

fn with_session(session: loading_bay_game::GameSession) -> GameRuntime {
    let mut admitted = loading_bay_game::decode_and_admit_stored_project(PROJECT).unwrap();
    admitted.session = session;
    GameRuntime::from_admitted_project(admitted)
}

fn pickup_count(runtime: &GameRuntime, item: &str) -> usize {
    runtime
        .session()
        .pickups()
        .filter(|pickup| pickup.config.item.as_str() == item)
        .count()
}

fn with_overlap(runtime: GameRuntime, pickup: EntityId) -> GameRuntime {
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    snapshot["pickupTriggers"]["revision"] = snapshot["pickupTriggers"]["revision"]
        .as_u64()
        .unwrap()
        .saturating_add(1)
        .into();
    snapshot["pickupTriggers"]["activeOverlaps"] =
        serde_json::json!([{ "trigger": pickup.raw(), "subject": PLAYER.raw() }]);
    decode_game_snapshot(&snapshot.to_string()).unwrap()
}

fn quantity(runtime: &GameRuntime, item: &str) -> u32 {
    runtime
        .session()
        .inventory(PLAYER)
        .unwrap()
        .stacks
        .iter()
        .find(|stack| stack.item.as_str() == item)
        .map_or(0, |stack| stack.quantity)
}

fn item(value: &str) -> loading_bay_game::ItemDefinitionId {
    loading_bay_game::ItemDefinitionId::parse(value).unwrap()
}
