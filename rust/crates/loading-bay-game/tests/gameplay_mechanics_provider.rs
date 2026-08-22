use loading_bay_game::{decode_game_snapshot, encode_game_snapshot, GameRuntime};
use rusty_engine::core_ids::EntityId;
use rusty_engine::gameplay_mechanics::{
    EquipmentComponent, InventoryComponent, ItemComponent, TracksComponent,
};
use serde_json::{json, Value};

const PROJECT: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");
const PLAYER: EntityId = EntityId::new(1);

#[test]
fn e1m1_runtime_uses_canonical_engine_mechanics_components_and_round_trips_them() {
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
    assert!(!session
        .entities()
        .components::<ItemComponent>()
        .unwrap()
        .collect::<Vec<_>>()
        .is_empty());

    let encoded = encode_game_snapshot(&runtime).unwrap();
    let reopened = decode_game_snapshot(&encoded).unwrap();
    assert_eq!(encode_game_snapshot(&reopened).unwrap(), encoded);
}

#[test]
fn future_valid_explosive_prop_capacity_is_inspectable_and_snapshot_stable() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    let (prop, health) = project["scenes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .flat_map(|scene| scene["entities"].as_array_mut().unwrap())
        .find_map(|entity| {
            entity.get("explosiveProp").is_some().then(|| {
                (
                    EntityId::new(entity["id"].as_u64().unwrap()),
                    &mut entity["health"],
                )
            })
        })
        .expect("canonical E1M1 explosive prop health");
    health["max"] = json!(51);

    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let inspection = runtime.session().developer_inspect_mechanics(prop).unwrap();
    assert_eq!(inspection.catalog_version, "loading-bay-v1");
    assert!(inspection.catalog_fingerprint.starts_with("sha256:"));

    let reopened = decode_game_snapshot(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    let reopened_inspection = reopened
        .session()
        .developer_inspect_mechanics(prop)
        .unwrap();
    assert_eq!(
        reopened_inspection.catalog_fingerprint,
        inspection.catalog_fingerprint
    );
}
