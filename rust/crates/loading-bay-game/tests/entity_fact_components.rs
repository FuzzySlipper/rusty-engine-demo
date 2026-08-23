use loading_bay_game::{
    combat::EnemyComponent, decode_game_snapshot, door::DoorComponent, encode_game_snapshot,
    pickup::PickupComponent, GameRuntime,
};
use rusty_engine::core_ids::EntityId;
use rusty_engine::entity_state::EntityLifecycle;
use serde_json::{json, Value};

const PROJECT: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");

fn entity_ids_with_kind(project: &str, marker: &str) -> Vec<EntityId> {
    let project: Value = serde_json::from_str(project).unwrap();
    project["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|scene| scene["entities"].as_array().unwrap())
        .filter_map(|entity| {
            entity
                .get(marker)
                .filter(|value| !value.is_null())
                .map(|_| EntityId::new(entity["id"].as_u64().unwrap()))
        })
        .collect()
}

/// Downstream gameplay facts are stored as registered Engine components with
/// stable identities, not in parallel side tables.
#[test]
fn downstream_facts_live_in_the_engine_component_store() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let entities = runtime.session().entities();

    let door = entity_ids_with_kind(PROJECT, "door")
        .first()
        .copied()
        .expect("canonical E1M1 door");
    let enemy = entity_ids_with_kind(PROJECT, "enemy")
        .first()
        .copied()
        .expect("canonical E1M1 enemy");

    assert!(entities.component::<DoorComponent>(door).unwrap().is_some());
    assert!(entities
        .component::<EnemyComponent>(enemy)
        .unwrap()
        .is_some());

    let inspection = entities.component_inspection();
    let loading_bay_kinds = inspection
        .kinds
        .iter()
        .filter(|kind| kind.type_id.as_str().starts_with("loading-bay."))
        .count();
    assert!(
        loading_bay_kinds > 0,
        "loading-bay facts are not registered"
    );
}

/// Durable downstream codecs round-trip through the embedded entity-state
/// snapshot, and the encoded payload carries the loading-bay registrations.
#[test]
fn downstream_fact_components_round_trip_through_snapshots() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let encoded = encode_game_snapshot(&runtime).unwrap();

    let payload: Value = serde_json::from_str(&encoded).unwrap();
    let registered = payload["entities"]["registeredComponents"]
        .as_array()
        .expect("current snapshots persist registered durable components");
    let loading_bay_types = registered
        .iter()
        .filter_map(|component| component["typeId"].as_str())
        .filter(|type_id| type_id.starts_with("loading-bay."))
        .count();
    assert!(loading_bay_types > 0, "no loading-bay facts were persisted");

    let reopened = decode_game_snapshot(&encoded).unwrap();
    assert_eq!(encode_game_snapshot(&reopened).unwrap(), encoded);

    let door = entity_ids_with_kind(PROJECT, "door")
        .first()
        .copied()
        .expect("canonical E1M1 door");
    assert!(reopened
        .session()
        .entities()
        .component::<DoorComponent>(door)
        .unwrap()
        .is_some());
}

/// Collecting a pickup destroys its entity and removes its gameplay fact
/// through the Engine component store; no stale side-table cleanup remains.
#[test]
fn destroyed_entities_drop_downstream_facts_atomically() {
    let player = EntityId::new(1);
    let base = GameRuntime::from_stored_project(PROJECT).unwrap();

    let available_pickup = entity_ids_with_kind(PROJECT, "pickup")
        .into_iter()
        .find(|pickup| {
            base.session().pickup(*pickup).is_some_and(|view| {
                matches!(view.state, loading_bay_game::pickup::PickupState::Available)
            })
        })
        .expect("canonical E1M1 starts with an available pickup");

    // Establish an actor/pickup trigger overlap through the documented
    // snapshot round-trip, matching the focused pickup runtime tests.
    let mut snapshot: Value = serde_json::from_str(&encode_game_snapshot(&base).unwrap()).unwrap();
    snapshot["pickupTriggers"]["revision"] = snapshot["pickupTriggers"]["revision"]
        .as_u64()
        .unwrap()
        .saturating_add(1)
        .into();
    snapshot["pickupTriggers"]["activeOverlaps"] =
        json!([{ "trigger": available_pickup.raw(), "subject": player.raw() }]);
    let mut runtime = decode_game_snapshot(&snapshot.to_string()).unwrap();
    let authored = loading_bay_game::decode_project_document(PROJECT)
        .expect("decode E1M1 authored program bindings")
        .project;
    runtime
        .reattach_authored_gameplay_programs(&authored)
        .expect("reattach admitted gameplay programs after snapshot restore");

    runtime
        .collect_pickup(player, available_pickup, 1, 1)
        .expect("overlapping available pickup collects");

    let entities = runtime.session().entities();
    assert!(matches!(
        entities.lifecycle(available_pickup),
        Some(EntityLifecycle::Tombstoned)
    ));
    assert!(entities
        .component::<PickupComponent>(available_pickup)
        .unwrap()
        .is_none());
}

/// A current-schema payload may not smuggle legacy side-table state.
#[test]
fn current_schema_rejects_legacy_fact_side_tables() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut payload: Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    payload["doors"] = json!([]);
    payload["doors"].as_array_mut().unwrap().push(json!({
        "entity": 424242,
        "state": "closed",
        "closedTranslation": [0.0, 0.0, 0.0],
        "openTranslation": [0.0, 4.0, 0.0],
        "motionDurationTicks": 1,
        "motionElapsedTicks": 0,
        "autoCloseAfterTicks": null
    }));

    let error = decode_game_snapshot(payload.to_string().as_str()).unwrap_err();
    assert!(
        error.to_string().contains("LegacyFactSideTables"),
        "unexpected error: {error}"
    );
}
