mod support;

use core_ids::EntityId;
use gameplay_mechanics::{EquipmentComponent, InventoryComponent, ItemComponent, TracksComponent};
use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, GameRuntime, GameSnapshotError,
    GAME_SNAPSHOT_SCHEMA_VERSION,
};

const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const PLAYER: EntityId = EntityId::new(1);

#[test]
fn engine_components_are_canonical_and_schema_nineteen_rejects_projection_drift() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let session = runtime.session();
    assert!(session
        .entities()
        .component::<InventoryComponent>(PLAYER)
        .unwrap()
        .is_some());
    assert!(session
        .entities()
        .component::<EquipmentComponent>(PLAYER)
        .unwrap()
        .is_some());
    assert!(session
        .entities()
        .component::<TracksComponent>(PLAYER)
        .unwrap()
        .is_some());
    let unique_items = session
        .entities()
        .components::<ItemComponent>()
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(unique_items.len(), 3);

    let encoded = encode_game_snapshot(&runtime).unwrap();
    let snapshot: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        snapshot["schemaVersion"],
        serde_json::json!(GAME_SNAPSHOT_SCHEMA_VERSION)
    );
    let registered = snapshot["entities"]["registeredComponents"]
        .as_array()
        .unwrap();
    assert!(registered
        .iter()
        .any(|component| component["typeId"] == "rusty.mechanics.inventory"));
    assert!(registered
        .iter()
        .any(|component| component["typeId"] == "rusty.mechanics.tracks"));

    let mut drifted_health = snapshot.clone();
    let health = drifted_health["health"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|health| health["entity"] == PLAYER.raw())
        .unwrap();
    health["current"] = 1.into();
    assert!(matches!(
        decode_game_snapshot(&drifted_health.to_string()),
        Err(GameSnapshotError::Mechanics { .. })
    ));

    let mut drifted_inventory = snapshot;
    let inventory = drifted_inventory["inventories"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|inventory| inventory["owner"] == PLAYER.raw())
        .unwrap();
    inventory["stacks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|stack| stack["item"] == "ammo/energy-cell")
        .unwrap()["quantity"] = 23.into();
    assert!(matches!(
        decode_game_snapshot(&drifted_inventory.to_string()),
        Err(GameSnapshotError::Mechanics { .. })
    ));
}

#[test]
fn schema_eighteen_rejects_future_provider_state_before_migration() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut relabeled: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    relabeled["schemaVersion"] = 18.into();
    assert!(matches!(
        decode_game_snapshot(&relabeled.to_string()),
        Err(GameSnapshotError::FutureGameplayMechanicsStateInLegacySnapshot)
    ));

    relabeled["entities"]["registeredComponents"] = serde_json::json!([]);
    assert!(relabeled["inventories"]
        .as_array()
        .unwrap()
        .iter()
        .any(|inventory| !inventory["weaponEntities"].as_array().unwrap().is_empty()));
    assert!(matches!(
        decode_game_snapshot(&relabeled.to_string()),
        Err(GameSnapshotError::FutureGameplayMechanicsStateInLegacySnapshot)
    ));
}

#[test]
fn schema_eighteen_save_migrates_into_the_canonical_provider_store() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let before_health = runtime.session().health(PLAYER).unwrap();
    let before_inventory = runtime.session().inventory(PLAYER).unwrap();
    let mut legacy: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    legacy["schemaVersion"] = 18.into();
    support::strip_future_gameplay_mechanics_state(&mut legacy);

    let migrated = decode_game_snapshot(&legacy.to_string()).unwrap();
    assert_eq!(migrated.session().health(PLAYER).unwrap(), before_health);
    assert_eq!(
        migrated.session().inventory(PLAYER).unwrap(),
        before_inventory
    );
    assert!(migrated
        .session()
        .entities()
        .component::<TracksComponent>(PLAYER)
        .unwrap()
        .is_some());
    assert!(migrated
        .session()
        .entities()
        .component::<InventoryComponent>(PLAYER)
        .unwrap()
        .is_some());
    let reencoded: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&migrated).unwrap()).unwrap();
    assert_eq!(
        reencoded["schemaVersion"],
        serde_json::json!(GAME_SNAPSHOT_SCHEMA_VERSION)
    );
}
