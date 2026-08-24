use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, GameRuntime, InventoryAction, InventoryCommand,
    InventoryRejection, ItemDefinitionId,
};
use rusty_engine::core_ids::EntityId;
use rusty_engine::gameplay_mechanics::{EquipmentComponent, ItemComponent};
use serde_json::Value;

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
fn player_setup_program_preserves_e1m1_loadout_slots_and_first_command_sequence() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let health = runtime.session().health(PLAYER).unwrap();
    assert_eq!(health.current, 100);
    assert_eq!(health.config.max, 200);
    assert_eq!(health.config.max_armor, 200);
    assert_eq!(health.config.armor_absorption_percent, 33);
    let controls = runtime.session().player_controller(PLAYER).unwrap();
    assert_eq!(controls.config.bindings.move_forward, "KeyW");
    assert_eq!(controls.config.bindings.primary_fire, "Mouse0");
    assert_eq!(
        controls.config.bindings.select_weapon,
        vec!["Digit1", "Digit2", "Digit3"]
    );
    let inventory = runtime.session().inventory(PLAYER).unwrap();
    assert_eq!(inventory.capacity_slots, 10);
    assert_eq!(
        inventory
            .stacks
            .iter()
            .map(|stack| (stack.item.as_str(), stack.quantity))
            .collect::<Vec<_>>(),
        vec![
            ("weapon/fist", 1),
            ("weapon/pistol", 1),
            ("ammo/bullets", 50),
        ]
    );
    assert_eq!(inventory.equipped_weapon.unwrap().as_str(), "weapon/pistol");
    assert_eq!(
        inventory
            .weapon_slots
            .iter()
            .map(|item| item.as_str())
            .collect::<Vec<_>>(),
        vec!["weapon/pistol", "weapon/shotgun", "weapon/fist"]
    );
    let programs = runtime.session().player_setup_programs();
    assert_eq!(programs.programs.len(), 2);
    assert_eq!(
        programs.bindings,
        vec![loading_bay_game::PlayerSetupProgramBinding {
            player: PLAYER.raw(),
            program_id: "player/e1m1-pistol-start".to_owned(),
        }]
    );

    // Admission applies setup directly to mechanics; it is not a ceremonial
    // inventory command and therefore leaves sequence one for the first live
    // mutation.
    let receipt = runtime
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
    assert_eq!(receipt.sequence, 1);
}

#[test]
fn changing_only_the_player_setup_binding_changes_rust_owned_initial_state() {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == 1)
        .unwrap()["inventory"]["setupProgram"] = serde_json::json!("player/shotgun-start");
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let inventory = runtime.session().inventory(PLAYER).unwrap();
    assert_eq!(
        inventory
            .stacks
            .iter()
            .map(|stack| (stack.item.as_str(), stack.quantity))
            .collect::<Vec<_>>(),
        vec!["weapon/fist", "weapon/shotgun", "ammo/shells"]
            .into_iter()
            .zip([1, 1, 8])
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        inventory.equipped_weapon.unwrap().as_str(),
        "weapon/shotgun"
    );
    assert_eq!(
        runtime.session().player_setup_programs().bindings[0].program_id,
        "player/shotgun-start"
    );
}

#[test]
fn malformed_player_setup_programs_fail_admission_before_a_session_exists() {
    let mut unknown_item: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    unknown_item["playerSetupPrograms"][0]["program"][0]["item"] =
        serde_json::json!("ammo/not-authored");
    assert!(GameRuntime::from_stored_project(&unknown_item.to_string()).is_err());

    let mut zero_quantity: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    zero_quantity["playerSetupPrograms"][0]["program"][0]["quantity"] = serde_json::json!(0);
    assert!(GameRuntime::from_stored_project(&zero_quantity.to_string()).is_err());

    let mut quantity_overflow: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    quantity_overflow["playerSetupPrograms"][0]["program"][2]["quantity"] = serde_json::json!(201);
    assert!(GameRuntime::from_stored_project(&quantity_overflow.to_string()).is_err());

    let mut equip_before_grant: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    let operations = equip_before_grant["playerSetupPrograms"][0]["program"]
        .as_array_mut()
        .unwrap();
    operations.swap(0, 3);
    assert!(GameRuntime::from_stored_project(&equip_before_grant.to_string()).is_err());

    let mut non_weapon_equip: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    non_weapon_equip["playerSetupPrograms"][0]["program"][3]["item"] =
        serde_json::json!("ammo/bullets");
    assert!(GameRuntime::from_stored_project(&non_weapon_equip.to_string()).is_err());

    let mut unknown_equipment: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    unknown_equipment["playerSetupPrograms"][0]["program"][3]["item"] =
        serde_json::json!("weapon/not-authored");
    assert!(GameRuntime::from_stored_project(&unknown_equipment.to_string()).is_err());

    let mut capacity_overflow: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    capacity_overflow["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == 1)
        .unwrap()["inventory"]["capacitySlots"] = serde_json::json!(1);
    assert!(GameRuntime::from_stored_project(&capacity_overflow.to_string()).is_err());

    let mut weapon_outside_slots: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    weapon_outside_slots["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == 1)
        .unwrap()["inventory"]["weaponSlots"] = serde_json::json!(["weapon/pistol", "weapon/fist"]);
    weapon_outside_slots["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == 1)
        .unwrap()["inventory"]["setupProgram"] = serde_json::json!("player/shotgun-start");
    assert!(GameRuntime::from_stored_project(&weapon_outside_slots.to_string()).is_err());
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

#[test]
fn unowned_weapon_slots_stay_reserved_absent_until_the_first_pickup_materializes_them() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let encoded = encode_game_snapshot(&runtime).unwrap();
    let snapshot: Value = serde_json::from_str(&encoded).unwrap();
    let mapping = snapshot["inventories"][0]["weaponEntities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|mapping| mapping["item"] == "weapon/shotgun")
        .unwrap();
    let shotgun = EntityId::new(mapping["entity"].as_u64().unwrap());
    assert_eq!(mapping["materialized"], false);
    assert!(!runtime.session().entities().contains(shotgun));
    assert_eq!(
        runtime
            .session()
            .entities()
            .components::<ItemComponent>()
            .unwrap()
            .count(),
        2,
        "only the owned fist and pistol are Engine items at startup"
    );
    assert_eq!(
        encode_game_snapshot(&decode_game_snapshot(&encoded).unwrap()).unwrap(),
        encoded
    );

    runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 1,
                action: InventoryAction::Grant {
                    item: ItemDefinitionId::parse("weapon/shotgun").unwrap(),
                    quantity: 1,
                },
            },
        )
        .unwrap();
    assert!(runtime.session().entities().contains(shotgun));
    assert_eq!(
        runtime.session().entities().contained_in(shotgun),
        Some(PLAYER)
    );
    assert_eq!(
        runtime
            .session()
            .entities()
            .component::<ItemComponent>(shotgun)
            .unwrap()
            .unwrap()
            .definition()
            .as_str(),
        "weapon.shotgun"
    );
    assert_eq!(
        runtime.session().inventory(PLAYER).unwrap().equipped_weapon,
        Some(ItemDefinitionId::parse("weapon/pistol").unwrap()),
        "materializing a pickup never implicitly equips it"
    );
}

#[test]
fn snapshot_distinguishes_reserved_absence_from_missing_materialized_weapon_mappings() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut snapshot: Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    let reserved = snapshot["inventories"][0]["weaponEntities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|mapping| mapping["item"] == "weapon/shotgun")
        .unwrap();
    reserved["materialized"] = serde_json::json!(true);
    assert!(decode_game_snapshot(&snapshot.to_string()).is_err());
}

#[test]
fn snapshot_rejects_one_reserved_weapon_entity_mapping_shared_by_two_inventory_owners() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    let player = project["scenes"][0]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["id"] == PLAYER.raw())
        .unwrap()
        .clone();
    let mut second_player = player;
    second_player["id"] = serde_json::json!(999);
    // The second player's position lives on its authored scene node below;
    // binding records carry no generic scene fields.
    {
        let player_node = project["scenes"][0]["authoredScene"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["id"] == PLAYER.raw())
            .unwrap()
            .clone();
        let mut second_node = player_node;
        second_node["id"] = serde_json::json!(999);
        second_node["transform"]["translation"] = serde_json::json!([0.0, 2.0, 0.0]);
        project["scenes"][0]["authoredScene"]["nodes"]
            .as_array_mut()
            .unwrap()
            .push(second_node);
    }
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .push(second_player);
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut snapshot: Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    let inventories = snapshot["inventories"].as_array_mut().unwrap();
    let first_pistol = inventories[0]["weaponEntities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|mapping| mapping["item"] == "weapon/pistol")
        .unwrap()["entity"]
        .clone();
    let second_pistol = inventories[1]["weaponEntities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|mapping| mapping["item"] == "weapon/pistol")
        .unwrap();
    second_pistol["entity"] = first_pistol;
    assert!(decode_game_snapshot(&snapshot.to_string()).is_err());
}

#[test]
fn pre_reserved_absence_current_snapshots_with_hidden_unowned_placeholders_still_reopen() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let shotgun = ItemDefinitionId::parse("weapon/shotgun").unwrap();
    runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 1,
                action: InventoryAction::Grant {
                    item: shotgun.clone(),
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
                action: InventoryAction::Consume {
                    item: shotgun,
                    quantity: 1,
                },
            },
        )
        .unwrap();
    let mut snapshot: Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    snapshot["schemaVersion"] = serde_json::json!(23);
    for mapping in snapshot["inventories"][0]["weaponEntities"]
        .as_array_mut()
        .unwrap()
    {
        mapping.as_object_mut().unwrap().remove("materialized");
    }
    let reopened = decode_game_snapshot(&snapshot.to_string()).unwrap();
    assert_eq!(
        reopened
            .session()
            .inventory(PLAYER)
            .unwrap()
            .stacks
            .iter()
            .find(|stack| stack.item.as_str() == "weapon/shotgun"),
        None
    );
    assert_eq!(
        reopened
            .session()
            .entities()
            .components::<ItemComponent>()
            .unwrap()
            .count(),
        3,
        "legacy hidden placeholder remains admitted for compatibility"
    );
}

#[test]
fn weapon_selection_uses_standard_equipment_and_selected_weapon_disposal_is_atomic() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let shotgun = ItemDefinitionId::parse("weapon/shotgun").unwrap();

    // Loading Bay retains this None-to-owner containment step because the item was authored and
    // materialized with player setup, not transferred from another inventory owner.
    runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 1,
                action: InventoryAction::Grant {
                    item: shotgun.clone(),
                    quantity: 1,
                },
            },
        )
        .unwrap();
    let shotgun_entity = unique_item_entity(&runtime, "weapon.shotgun");
    assert_eq!(
        runtime.session().entities().contained_in(shotgun_entity),
        Some(PLAYER)
    );

    // An occupied selection is a standard SwapUniqueItem: the canonical equipment component
    // moves from pistol to shotgun and survives the normal save/reopen path.
    runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 2,
                action: InventoryAction::SelectWeapon {
                    item: shotgun.clone(),
                },
            },
        )
        .unwrap();
    assert_eq!(
        runtime.session().inventory(PLAYER).unwrap().equipped_weapon,
        Some(shotgun.clone())
    );
    assert_eq!(
        runtime
            .session()
            .entities()
            .component::<EquipmentComponent>(PLAYER)
            .unwrap()
            .unwrap()
            .assignments()
            .iter()
            .map(|assignment| assignment.item)
            .collect::<Vec<_>>(),
        vec![shotgun_entity]
    );
    let reopened = decode_game_snapshot(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    assert_eq!(
        reopened
            .session()
            .inventory(PLAYER)
            .unwrap()
            .equipped_weapon,
        Some(shotgun.clone())
    );

    // Consume first performs standard UnequipUniqueItem, then the product's owner-to-None
    // disposal containment step in the same private inventory-command candidate. A successful
    // receipt therefore cannot leave a deleted weapon equipped.
    runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 3,
                action: InventoryAction::Consume {
                    item: shotgun.clone(),
                    quantity: 1,
                },
            },
        )
        .unwrap();
    assert_eq!(
        runtime.session().entities().contained_in(shotgun_entity),
        None
    );
    assert!(runtime
        .session()
        .entities()
        .component::<EquipmentComponent>(PLAYER)
        .unwrap()
        .unwrap()
        .assignments()
        .is_empty());
    assert_eq!(
        runtime.session().inventory(PLAYER).unwrap().equipped_weapon,
        None
    );

    // A rejected repeat leaves the saved product state untouched, including both the equipment
    // result and product disposal containment.
    let before_rejection = encode_game_snapshot(&runtime).unwrap();
    assert!(matches!(
        runtime.apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 3,
                action: InventoryAction::Consume {
                    item: shotgun,
                    quantity: 1,
                },
            },
        ),
        Err(loading_bay_game::RuntimeError::Inventory(
            InventoryRejection::RepeatedCommand { sequence: 3 }
        ))
    ));
    assert_eq!(encode_game_snapshot(&runtime).unwrap(), before_rejection);

    // Reacquisition only restores the explicit containment edge. The same admitted unique item
    // and ItemComponent remain authoritative; the product never rematerializes or destroys it.
    runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 4,
                action: InventoryAction::Grant {
                    item: ItemDefinitionId::parse("weapon/shotgun").unwrap(),
                    quantity: 1,
                },
            },
        )
        .unwrap();
    assert_eq!(
        runtime.session().entities().contained_in(shotgun_entity),
        Some(PLAYER)
    );
    assert_eq!(
        runtime
            .session()
            .entities()
            .component::<ItemComponent>(shotgun_entity)
            .unwrap()
            .unwrap()
            .definition()
            .as_str(),
        "weapon.shotgun"
    );
}

/// Fungible commands ride the Engine standard operation path through the one
/// product boundary; the typed product value policy (max quantity) is
/// preserved even though planning and mutation belong to the standard leaf.
#[test]
fn fungible_grant_through_the_public_route_preserves_product_overflow_policy() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let bullets = ItemDefinitionId::parse("ammo/bullets").unwrap();
    // Authored maxQuantity for ammo/bullets is 200.
    let unchanged = runtime.session().inventory(PLAYER).unwrap();
    let err = runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 1,
                action: InventoryAction::Grant {
                    item: bullets.clone(),
                    quantity: 201,
                },
            },
        )
        .unwrap_err();
    assert!(matches!(
        err,
        loading_bay_game::RuntimeError::Inventory(InventoryRejection::QuantityOverflow { .. })
    ));
    assert_eq!(runtime.session().inventory(PLAYER).unwrap(), unchanged);

    // Within the authored limit the same route succeeds and advances the
    // command sequence exactly once.
    runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 1,
                action: InventoryAction::Grant {
                    item: bullets,
                    quantity: 30,
                },
            },
        )
        .unwrap();
    let repeated = runtime.apply_inventory_command(
        PLAYER,
        InventoryCommand {
            sequence: 1,
            action: InventoryAction::Grant {
                item: ItemDefinitionId::parse("ammo/shells").unwrap(),
                quantity: 1,
            },
        },
    );
    assert!(matches!(
        repeated,
        Err(loading_bay_game::RuntimeError::Inventory(
            InventoryRejection::RepeatedCommand { .. }
        ))
    ));
}

fn unique_item_entity(runtime: &GameRuntime, definition: &str) -> EntityId {
    runtime
        .session()
        .entities()
        .components::<ItemComponent>()
        .unwrap()
        .find_map(|(entity, item)| (item.definition().as_str() == definition).then_some(entity))
        .expect("authored unique weapon entity")
}

fn quantity(stacks: &[loading_bay_game::InventoryStack], item: &ItemDefinitionId) -> u32 {
    stacks
        .iter()
        .find(|stack| &stack.item == item)
        .map(|stack| stack.quantity)
        .unwrap_or_default()
}
