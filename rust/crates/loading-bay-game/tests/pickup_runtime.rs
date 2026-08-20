use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, GameEntityDefinition, GameEntityDefinitionError,
    GameRuntime, GameSession, PickupConfig, PickupDisposition, PickupState,
};
use rusty_engine::core_ids::EntityId;
use rusty_engine::entity_state::EntityDefinition;

const PROJECT: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");
const PLAYER: EntityId = EntityId::new(1);

#[test]
fn e1m1_pickups_admit_as_visible_available_or_hidden_dormant_entities() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let pickups = runtime.session().pickups().collect::<Vec<_>>();

    assert!(!pickups.is_empty());
    assert!(pickups
        .iter()
        .any(|pickup| pickup.state == PickupState::Available));
    assert!(pickups
        .iter()
        .any(|pickup| pickup.state == PickupState::Dormant));
    for pickup in pickups {
        let entity = runtime.session().entity(pickup.entity).unwrap();
        assert!(entity.transform.is_some());
        assert!(entity.bounds.is_some());
        assert!(entity.renderable.is_some());
        assert!(entity.collision.is_none());
        assert!(entity.kinematic.is_none());
        assert_eq!(
            entity.renderable.as_ref().unwrap().visible,
            pickup.state == PickupState::Available,
            "pickup {} visibility must follow its authored runtime state",
            pickup.entity
        );
    }
}

#[test]
fn dormant_enemy_drops_keep_trigger_definitions_but_never_retain_overlaps_or_facts() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let dormant = runtime
        .session()
        .pickups()
        .filter(|pickup| pickup.state == PickupState::Dormant)
        .map(|pickup| pickup.entity.raw())
        .collect::<std::collections::BTreeSet<_>>();
    let pickup_count = runtime.session().pickups().count();
    let available_pickup = available_pickup(&runtime);

    let phase = runtime.run_pickup_phase(PLAYER).unwrap();
    assert!(phase
        .trigger_facts
        .iter()
        .all(|fact| !dormant.contains(&fact.pair.trigger_id().raw())));

    let snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    let definitions = snapshot["pickupTriggers"]["definitions"]
        .as_array()
        .unwrap();
    assert_eq!(definitions.len(), pickup_count);
    assert!(definitions
        .iter()
        .any(|definition| dormant.contains(&definition["trigger"].as_u64().unwrap())));
    assert!(snapshot["pickupTriggers"]["activeOverlaps"]
        .as_array()
        .unwrap()
        .iter()
        .all(|overlap| !dormant.contains(&overlap["trigger"].as_u64().unwrap())));

    runtime = with_overlap(runtime, available_pickup);
    let receipt = runtime
        .collect_pickup(PLAYER, available_pickup, 4, 9)
        .unwrap();
    assert_eq!(receipt.disposition, PickupDisposition::Collected);
    let unavailable = runtime
        .session()
        .pickups()
        .filter(|pickup| pickup.state != PickupState::Available)
        .map(|pickup| pickup.entity.raw())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(receipt
        .trigger_facts
        .iter()
        .all(|fact| !unavailable.contains(&fact.pair.trigger_id().raw())));
    let reopened = decode_game_snapshot(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    assert!(matches!(
        reopened.session().pickup(available_pickup).unwrap().state,
        PickupState::Collected { .. }
    ));
    assert_eq!(
        reopened
            .session()
            .pickups()
            .filter(|pickup| pickup.state == PickupState::Dormant)
            .count(),
        dormant.len()
    );
}

#[test]
fn available_e1m1_pickup_collects_atomically_and_is_idempotent() {
    let pickup = available_pickup(&GameRuntime::from_stored_project(PROJECT).unwrap());
    let mut runtime = with_overlap(GameRuntime::from_stored_project(PROJECT).unwrap(), pickup);

    let receipt = runtime.collect_pickup(PLAYER, pickup, 4, 9).unwrap();
    assert_eq!(receipt.disposition, PickupDisposition::Collected);
    assert!(matches!(
        runtime.session().pickup(pickup).unwrap().state,
        PickupState::Collected {
            actor: PLAYER,
            collected_at_tick: 0,
            ..
        }
    ));
    assert_eq!(
        runtime.session().entity(pickup).unwrap().lifecycle,
        rusty_engine::entity_state::EntityLifecycle::Tombstoned
    );
    assert!(!runtime
        .readout()
        .projection
        .iter()
        .any(|node| node.entity == pickup));
    assert!(!receipt.facts.is_empty());

    let repeated = runtime.collect_pickup(PLAYER, pickup, 4, 9).unwrap();
    assert_eq!(repeated.disposition, PickupDisposition::Repeated);
    assert!(repeated.facts.is_empty());
    assert!(repeated.cues.is_empty());
}

#[test]
fn pickup_trigger_quota_is_rejected_before_runtime_construction_without_panicking() {
    let item = loading_bay_game::ItemDefinitionId::parse("ammo/quota-probe").unwrap();
    let definitions = (0..=rusty_engine::engine_spatial::MAX_TRIGGER_DEFINITIONS)
        .map(|index| {
            let entity = EntityId::new(index as u64 + 1);
            GameEntityDefinition::new(EntityDefinition::new(entity, format!("pickup-{index}")))
                .as_pickup(PickupConfig::new(item.clone(), 1))
        })
        .collect::<Vec<_>>();

    assert!(matches!(
        GameSession::from_definitions(definitions).unwrap_err(),
        GameEntityDefinitionError::TooManyPickups { count, limit }
            if count == rusty_engine::engine_spatial::MAX_TRIGGER_DEFINITIONS + 1
                && limit == rusty_engine::engine_spatial::MAX_TRIGGER_DEFINITIONS
    ));
}

#[test]
fn snapshot_pickup_trigger_quota_has_a_deterministic_typed_rejection() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    let template = snapshot["pickups"][0].clone();
    let pickups = snapshot["pickups"].as_array_mut().unwrap();
    while pickups.len() <= rusty_engine::engine_spatial::MAX_TRIGGER_DEFINITIONS {
        pickups.push(template.clone());
    }

    assert!(matches!(
        decode_game_snapshot(&snapshot.to_string()).unwrap_err(),
        loading_bay_game::GameSnapshotError::TooManyPickups { count, limit }
            if count == rusty_engine::engine_spatial::MAX_TRIGGER_DEFINITIONS + 1
                && limit == rusty_engine::engine_spatial::MAX_TRIGGER_DEFINITIONS
    ));
}

fn available_pickup(runtime: &GameRuntime) -> EntityId {
    runtime
        .session()
        .pickups()
        .find(|pickup| pickup.state == PickupState::Available)
        .map(|pickup| pickup.entity)
        .expect("E1M1 must include an available pickup")
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
