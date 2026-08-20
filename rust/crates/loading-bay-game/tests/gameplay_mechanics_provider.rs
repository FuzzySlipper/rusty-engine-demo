use loading_bay_game::{decode_game_snapshot, encode_game_snapshot, GameRuntime};
use rusty_engine::core_ids::EntityId;
use rusty_engine::gameplay_mechanics::{
    EquipmentComponent, InventoryComponent, ItemComponent, TracksComponent,
};

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
