use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, GameRuntime, InventoryAction, InventoryCommand,
    InventoryRejection, ItemDefinitionId,
};
use rusty_engine::core_ids::EntityId;

const PROJECT: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");
const PLAYER: EntityId = EntityId::new(1);

#[test]
fn e1m1_authored_inventory_is_read_only_and_uses_doom_item_vocabulary() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let session = runtime.session();
    let ids = session
        .item_definitions()
        .map(|definition| definition.id.as_str().to_owned())
        .collect::<Vec<_>>();
    assert!(ids.contains(&"weapon/pistol".to_owned()));
    assert!(ids.contains(&"weapon/shotgun".to_owned()));
    assert!(ids.contains(&"ammo/bullets".to_owned()));
    assert!(ids.contains(&"supply/medikit".to_owned()));

    let inventory = session.inventory(PLAYER).unwrap();
    assert!(inventory
        .stacks
        .iter()
        .any(|stack| stack.item.as_str() == "weapon/pistol"));
    let mut detached = inventory;
    detached.stacks[0].quantity = 0;
    assert_ne!(session.inventory(PLAYER).unwrap(), detached);
}

#[test]
fn e1m1_inventory_commands_are_atomic_and_preserve_rejected_sequences() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let bullets = ItemDefinitionId::parse("ammo/bullets").unwrap();
    let before = runtime.session().inventory(PLAYER).unwrap();
    let receipt = runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 1,
                action: InventoryAction::Grant {
                    item: bullets.clone(),
                    quantity: 10,
                },
            },
        )
        .unwrap();
    assert_eq!(
        quantity(&receipt.after.stacks, &bullets),
        quantity(&before.stacks, &bullets) + 10
    );

    let unchanged = runtime.session().inventory(PLAYER).unwrap();
    assert!(matches!(
        runtime.apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 2,
                action: InventoryAction::Grant {
                    item: ItemDefinitionId::parse("ammo/not-authored").unwrap(),
                    quantity: 1,
                },
            },
        ),
        Err(loading_bay_game::RuntimeError::Inventory(
            InventoryRejection::MissingDefinition { .. }
        ))
    ));
    assert_eq!(runtime.session().inventory(PLAYER).unwrap(), unchanged);
    assert!(runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 2,
                action: InventoryAction::Consume {
                    item: bullets,
                    quantity: 1,
                },
            },
        )
        .is_ok());
}

#[test]
fn e1m1_inventory_snapshot_round_trips_without_command_history() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 1,
                action: InventoryAction::Grant {
                    item: ItemDefinitionId::parse("ammo/bullets").unwrap(),
                    quantity: 1,
                },
            },
        )
        .unwrap();
    let encoded = encode_game_snapshot(&runtime).unwrap();
    let reopened = decode_game_snapshot(&encoded).unwrap();
    assert_eq!(encode_game_snapshot(&reopened).unwrap(), encoded);
}

fn quantity(stacks: &[loading_bay_game::InventoryStack], item: &ItemDefinitionId) -> u32 {
    stacks
        .iter()
        .find(|stack| &stack.item == item)
        .map(|stack| stack.quantity)
        .unwrap_or_default()
}
