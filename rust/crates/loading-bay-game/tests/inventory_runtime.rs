mod support;

use loading_bay_game::{
    decode_game_snapshot, decode_project_document, diagnostic_code, encode_game_snapshot,
    encode_project_document, GameRuntime, InventoryAction, InventoryCommand, InventoryFact,
    InventoryRejection, ItemDefinitionId, ItemKind, RuntimeError, STORED_PROJECT_SCHEMA_VERSION,
};
use rusty_engine::core_ids::EntityId;
const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const PLAYER: EntityId = EntityId::new(1);

#[test]
fn authored_item_vocabulary_and_starting_inventory_admit_as_read_only_views() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let session = runtime.session();
    let ids: Vec<_> = session
        .item_definitions()
        .map(|definition| definition.id.as_str().to_string())
        .collect();

    assert_eq!(
        ids,
        [
            "ammo/energy-cell",
            "ammo/kinetic-slug",
            "ammo/scatter-shell",
            "armor/impact-vest",
            "key/inert-inspection-tag",
            "key/maintenance-pass",
            "supply/med-patch",
            "weapon/arc-pistol",
            "weapon/breach-scattergun",
            "weapon/kinetic-launcher",
            "weapon/rivet-carbine",
        ]
    );
    assert!(matches!(
        session
            .item_definition(&item("weapon/arc-pistol"))
            .unwrap()
            .kind,
        ItemKind::Weapon(definition)
            if definition.ammunition == item("ammo/energy-cell")
    ));
    assert!(matches!(
        session
            .item_definition(&item("ammo/energy-cell"))
            .unwrap()
            .kind,
        ItemKind::Ammunition
    ));
    assert!(matches!(
        session
            .item_definition(&item("key/maintenance-pass"))
            .unwrap()
            .kind,
        ItemKind::AccessKey
    ));
    assert!(matches!(
        session
            .item_definition(&item("supply/med-patch"))
            .unwrap()
            .kind,
        ItemKind::HealthSupply {
            restore_health: 25,
            ..
        }
    ));
    assert!(matches!(
        session
            .item_definition(&item("armor/impact-vest"))
            .unwrap()
            .kind,
        ItemKind::Armor {
            protection: 100,
            ..
        }
    ));

    let inventory = session.inventory(PLAYER).unwrap();
    assert_eq!(inventory.capacity_slots, 10);
    assert_eq!(
        inventory
            .stacks
            .iter()
            .map(|stack| (stack.item.as_str(), stack.quantity))
            .collect::<Vec<_>>(),
        [
            ("weapon/arc-pistol", 1),
            ("ammo/energy-cell", 18),
            ("supply/med-patch", 1),
            ("weapon/kinetic-launcher", 1),
            ("ammo/kinetic-slug", 8),
        ]
    );
    assert_eq!(
        inventory
            .equipped_weapon
            .as_ref()
            .map(ItemDefinitionId::as_str),
        Some("weapon/arc-pistol")
    );

    // This authored definition is deliberately unused. Its admission proves
    // that adding another identity of an existing concrete kind is data-only.
    assert!(session
        .item_definition(&item("key/inert-inspection-tag"))
        .is_some());
    assert!(inventory
        .stacks
        .iter()
        .all(|stack| stack.item != item("key/inert-inspection-tag")));

    let mut detached_view = inventory;
    detached_view.stacks[1].quantity = 0;
    assert_eq!(
        quantity(
            &runtime.session().inventory(PLAYER).unwrap().stacks,
            "ammo/energy-cell"
        ),
        18
    );
}

#[test]
fn inventory_commands_are_atomic_ordered_and_report_exact_before_after_state() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();

    let receipt = apply(
        &mut runtime,
        1,
        InventoryAction::Grant {
            item: item("ammo/energy-cell"),
            quantity: 10,
        },
    )
    .unwrap();
    assert_eq!(quantity(&receipt.before.stacks, "ammo/energy-cell"), 18);
    assert_eq!(quantity(&receipt.after.stacks, "ammo/energy-cell"), 28);
    assert_eq!(
        receipt.facts,
        [InventoryFact::QuantityChanged {
            owner: PLAYER,
            item: item("ammo/energy-cell"),
            before: 18,
            after: 28,
        }]
    );

    let before_overflow = runtime.session().inventory(PLAYER).unwrap();
    let overflow = apply(
        &mut runtime,
        2,
        InventoryAction::Grant {
            item: item("ammo/energy-cell"),
            quantity: 173,
        },
    )
    .unwrap_err();
    assert_eq!(
        overflow,
        InventoryRejection::QuantityOverflow {
            item: item("ammo/energy-cell"),
            current: 28,
            requested: 173,
            limit: 200,
        }
    );
    assert_eq!(
        runtime.session().inventory(PLAYER).unwrap(),
        before_overflow
    );

    // A rejected command does not consume its sequence or partially mutate.
    let consume = apply(
        &mut runtime,
        2,
        InventoryAction::Consume {
            item: item("ammo/energy-cell"),
            quantity: 5,
        },
    )
    .unwrap();
    assert_eq!(quantity(&consume.after.stacks, "ammo/energy-cell"), 23);

    let moved = apply(
        &mut runtime,
        3,
        InventoryAction::MoveStack {
            from_index: 2,
            to_index: 0,
        },
    )
    .unwrap();
    assert_eq!(moved.after.stacks[0].item, item("supply/med-patch"));
    assert_eq!(
        moved.facts,
        [InventoryFact::StackMoved {
            owner: PLAYER,
            item: item("supply/med-patch"),
            from_index: 2,
            to_index: 0,
        }]
    );

    let before_rejections = runtime.session().inventory(PLAYER).unwrap();
    assert_eq!(
        apply(
            &mut runtime,
            4,
            InventoryAction::Consume {
                item: item("ammo/scatter-shell"),
                quantity: 1,
            },
        )
        .unwrap_err(),
        InventoryRejection::QuantityUnderflow {
            item: item("ammo/scatter-shell"),
            current: 0,
            requested: 1,
        }
    );
    assert_eq!(
        apply(
            &mut runtime,
            4,
            InventoryAction::SelectWeapon {
                item: item("supply/med-patch"),
            },
        )
        .unwrap_err(),
        InventoryRejection::IncompatibleSelection {
            item: item("supply/med-patch"),
        }
    );
    assert_eq!(
        apply(
            &mut runtime,
            4,
            InventoryAction::Grant {
                item: item("ammo/not-defined"),
                quantity: 1,
            },
        )
        .unwrap_err(),
        InventoryRejection::MissingDefinition {
            item: item("ammo/not-defined"),
        }
    );
    assert_eq!(
        runtime.session().inventory(PLAYER).unwrap(),
        before_rejections
    );

    apply(
        &mut runtime,
        4,
        InventoryAction::Grant {
            item: item("weapon/breach-scattergun"),
            quantity: 1,
        },
    )
    .unwrap();
    let selected = apply(
        &mut runtime,
        5,
        InventoryAction::SelectWeapon {
            item: item("weapon/breach-scattergun"),
        },
    )
    .unwrap();
    assert_eq!(
        selected.after.equipped_weapon,
        Some(item("weapon/breach-scattergun"))
    );
    assert_eq!(
        apply(
            &mut runtime,
            5,
            InventoryAction::SelectWeapon {
                item: item("weapon/arc-pistol"),
            },
        )
        .unwrap_err(),
        InventoryRejection::RepeatedCommand { sequence: 5 }
    );
    assert_eq!(
        apply(
            &mut runtime,
            3,
            InventoryAction::SelectWeapon {
                item: item("weapon/arc-pistol"),
            },
        )
        .unwrap_err(),
        InventoryRejection::StaleCommand {
            sequence: 3,
            last_applied: 5,
        }
    );

    let consumed_weapon = apply(
        &mut runtime,
        6,
        InventoryAction::Consume {
            item: item("weapon/breach-scattergun"),
            quantity: 1,
        },
    )
    .unwrap();
    assert_eq!(
        consumed_weapon.facts,
        [
            InventoryFact::QuantityChanged {
                owner: PLAYER,
                item: item("weapon/breach-scattergun"),
                before: 1,
                after: 0,
            },
            InventoryFact::EquippedWeaponChanged {
                owner: PLAYER,
                before: Some(item("weapon/breach-scattergun")),
                after: None,
            },
        ]
    );
    assert_eq!(consumed_weapon.after.equipped_weapon, None);
}

#[test]
fn capacity_and_authored_definition_failures_leave_no_partial_runtime() {
    let mut full: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    full["scenes"][0]["entities"][0]["inventory"]["capacitySlots"] = 5.into();
    let mut runtime = GameRuntime::from_stored_project(&full.to_string()).unwrap();
    let before = runtime.session().inventory(PLAYER).unwrap();
    assert_eq!(
        apply(
            &mut runtime,
            1,
            InventoryAction::Grant {
                item: item("key/maintenance-pass"),
                quantity: 1,
            },
        )
        .unwrap_err(),
        InventoryRejection::InventoryFull { capacity_slots: 5 }
    );
    assert_eq!(runtime.session().inventory(PLAYER).unwrap(), before);

    let duplicate = mutate(|project| {
        let item = project["itemDefinitions"][0].clone();
        project["itemDefinitions"]
            .as_array_mut()
            .unwrap()
            .push(item);
    });
    let error = GameRuntime::from_stored_project(&duplicate).unwrap_err();
    assert_eq!(
        stored_diagnostic(error).code,
        diagnostic_code::DUPLICATE_ITEM_DEFINITION
    );

    let missing_ammo = mutate(|project| {
        let arc_pistol = project["itemDefinitions"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|definition| definition["id"] == "weapon/arc-pistol")
            .unwrap();
        arc_pistol["kind"]["ammunition"] = "ammo/not-defined".into();
    });
    let error = GameRuntime::from_stored_project(&missing_ammo).unwrap_err();
    assert_eq!(
        stored_diagnostic(error).code,
        diagnostic_code::MISSING_ITEM_DEFINITION
    );

    let duplicate_stack = mutate(|project| {
        let stack = project["scenes"][0]["entities"][0]["inventory"]["startingStacks"][0].clone();
        project["scenes"][0]["entities"][0]["inventory"]["startingStacks"]
            .as_array_mut()
            .unwrap()
            .push(stack);
    });
    let error = GameRuntime::from_stored_project(&duplicate_stack).unwrap_err();
    assert_eq!(
        stored_diagnostic(error).code,
        diagnostic_code::DUPLICATE_INVENTORY_STACK
    );
}

#[test]
fn snapshot_reopen_preserves_canonical_inventory_but_not_command_history() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    apply(
        &mut runtime,
        7,
        InventoryAction::Grant {
            item: item("weapon/breach-scattergun"),
            quantity: 1,
        },
    )
    .unwrap();
    apply(
        &mut runtime,
        8,
        InventoryAction::SelectWeapon {
            item: item("weapon/breach-scattergun"),
        },
    )
    .unwrap();
    apply(
        &mut runtime,
        9,
        InventoryAction::MoveStack {
            from_index: 3,
            to_index: 1,
        },
    )
    .unwrap();
    apply(
        &mut runtime,
        10,
        InventoryAction::Consume {
            item: item("ammo/energy-cell"),
            quantity: 7,
        },
    )
    .unwrap();

    let encoded = encode_game_snapshot(&runtime).unwrap();
    let mut reopened = decode_game_snapshot(&encoded).unwrap();
    assert_eq!(
        reopened.session().inventory(PLAYER),
        runtime.session().inventory(PLAYER)
    );
    let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        value["itemDefinitions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|definition| definition["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "ammo/energy-cell",
            "ammo/kinetic-slug",
            "ammo/scatter-shell",
            "armor/impact-vest",
            "key/inert-inspection-tag",
            "key/maintenance-pass",
            "supply/med-patch",
            "weapon/arc-pistol",
            "weapon/breach-scattergun",
            "weapon/kinetic-launcher",
            "weapon/rivet-carbine",
        ]
    );

    // Command sequencing is session-transient rather than durable gameplay.
    apply(
        &mut reopened,
        1,
        InventoryAction::Consume {
            item: item("ammo/energy-cell"),
            quantity: 1,
        },
    )
    .unwrap();
}

#[test]
fn schema_eleven_migrates_with_no_invented_inventory_and_authored_truth_stays_static() {
    let mut legacy: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    legacy["schemaVersion"] = 11.into();
    let future_fields = decode_project_document(&legacy.to_string()).unwrap_err();
    assert_eq!(future_fields.diagnostic().code, diagnostic_code::MIGRATION);
    legacy.as_object_mut().unwrap().remove("itemDefinitions");
    legacy["assets"]
        .as_array_mut()
        .unwrap()
        .retain(|asset| asset.get("voxelObject").is_none());
    legacy["scenes"][0]
        .as_object_mut()
        .unwrap()
        .remove("voxelObjectInstances");
    legacy["scenes"][0]["entities"][0]
        .as_object_mut()
        .unwrap()
        .remove("inventory");
    legacy["scenes"][0]["entities"][0]["playerController"]["bindings"]
        .as_object_mut()
        .unwrap()
        .remove("selectWeapon");
    legacy["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .retain(|entity| entity.get("pickup").is_none() && entity.get("hazard").is_none());
    for entity in legacy["scenes"][0]["entities"].as_array_mut().unwrap() {
        entity.as_object_mut().unwrap().remove("bounds");
        entity.as_object_mut().unwrap().remove("enemyCombat");
        entity.as_object_mut().unwrap().remove("defeatDrop");
        entity.as_object_mut().unwrap().remove("secretRegion");
        entity.as_object_mut().unwrap().remove("levelExit");
        if let Some(encounter) = entity
            .get_mut("encounter")
            .and_then(serde_json::Value::as_object_mut)
        {
            encounter.remove("activationRadius");
        }
        if let Some(door) = entity
            .get_mut("door")
            .and_then(serde_json::Value::as_object_mut)
        {
            door.remove("access");
        }
        if let Some(switch) = entity
            .get_mut("switch")
            .and_then(serde_json::Value::as_object_mut)
        {
            switch.remove("loadingBayInterlock");
        }
        if let Some(health) = entity
            .get_mut("health")
            .and_then(serde_json::Value::as_object_mut)
        {
            health.remove("maxArmor");
            health.remove("armorAbsorptionPercent");
        }
    }
    let decoded = decode_project_document(&legacy.to_string()).unwrap();
    assert_eq!(decoded.source_schema_version, 11);
    assert_eq!(
        decoded.project.schema_version,
        STORED_PROJECT_SCHEMA_VERSION
    );
    assert!(decoded.project.item_definitions.is_empty());
    assert!(decoded.project.scenes[0]
        .entities
        .iter()
        .all(|entity| entity.inventory.is_none()));
    GameRuntime::from_stored_project(&encode_project_document(&decoded.project).unwrap()).unwrap();

    let authored = decode_project_document(PROJECT).unwrap().project;
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    apply(
        &mut runtime,
        1,
        InventoryAction::Consume {
            item: item("ammo/energy-cell"),
            quantity: 9,
        },
    )
    .unwrap();
    assert_eq!(
        authored.scenes[0].entities[0]
            .inventory
            .as_ref()
            .unwrap()
            .starting_stacks[1]
            .quantity,
        18
    );
    assert_eq!(
        quantity(
            &runtime.session().inventory(PLAYER).unwrap().stacks,
            "ammo/energy-cell"
        ),
        9
    );
    assert!(!encode_project_document(&authored)
        .unwrap()
        .contains("ammoRemaining"));

    let mut previous_snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    previous_snapshot["schemaVersion"] = 10.into();
    support::strip_future_gameplay_mechanics_state(&mut previous_snapshot);
    previous_snapshot
        .as_object_mut()
        .unwrap()
        .remove("itemDefinitions");
    previous_snapshot
        .as_object_mut()
        .unwrap()
        .remove("inventories");
    previous_snapshot.as_object_mut().unwrap().remove("pickups");
    previous_snapshot
        .as_object_mut()
        .unwrap()
        .remove("pickupTriggers");
    previous_snapshot.as_object_mut().unwrap().remove("hazards");
    previous_snapshot
        .as_object_mut()
        .unwrap()
        .remove("hazardTriggers");
    previous_snapshot
        .as_object_mut()
        .unwrap()
        .remove("progression");
    previous_snapshot
        .as_object_mut()
        .unwrap()
        .remove("enemyCombat");
    previous_snapshot
        .as_object_mut()
        .unwrap()
        .remove("enemyDrops");
    for encounter in previous_snapshot["encounters"].as_array_mut().unwrap() {
        encounter
            .as_object_mut()
            .unwrap()
            .remove("activationRadius");
        encounter["state"] = "active".into();
    }
    for health in previous_snapshot["health"].as_array_mut().unwrap() {
        let health = health.as_object_mut().unwrap();
        health.remove("maxArmor");
        health.remove("armorAbsorptionPercent");
        health.remove("armor");
        health.remove("armorItem");
        health.remove("state");
    }
    for controller in previous_snapshot["playerControllers"]
        .as_array_mut()
        .unwrap()
    {
        controller["bindings"]
            .as_object_mut()
            .unwrap()
            .remove("selectWeapon");
    }
    let migrated_snapshot = decode_game_snapshot(&previous_snapshot.to_string()).unwrap();
    assert!(migrated_snapshot.session().inventory(PLAYER).is_none());
    assert!(migrated_snapshot
        .session()
        .item_definitions()
        .next()
        .is_none());
}

#[test]
fn schema_ten_snapshot_rejects_future_inventory_fields() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    snapshot["schemaVersion"] = 10.into();
    support::strip_future_gameplay_mechanics_state(&mut snapshot);

    let error = decode_game_snapshot(&snapshot.to_string()).unwrap_err();
    assert!(matches!(
        error,
        loading_bay_game::GameSnapshotError::FutureInventoryStateInLegacySnapshot
    ));
}

fn apply(
    runtime: &mut GameRuntime,
    sequence: u64,
    action: InventoryAction,
) -> Result<loading_bay_game::InventoryReceipt, InventoryRejection> {
    runtime
        .apply_inventory_command(PLAYER, InventoryCommand { sequence, action })
        .map_err(|error| match error {
            RuntimeError::Inventory(rejection) => rejection,
            other => panic!("unexpected runtime error: {other:?}"),
        })
}

fn item(value: &str) -> ItemDefinitionId {
    ItemDefinitionId::parse(value.to_string()).unwrap()
}

fn quantity(stacks: &[loading_bay_game::InventoryStack], item: &str) -> u32 {
    stacks
        .iter()
        .find(|stack| stack.item.as_str() == item)
        .map_or(0, |stack| stack.quantity)
}

fn mutate(mutation: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    mutation(&mut project);
    project.to_string()
}

fn stored_diagnostic(error: RuntimeError) -> loading_bay_game::ProjectDiagnostic {
    match error {
        RuntimeError::StoredProject(error) => error.diagnostic().clone(),
        other => panic!("unexpected runtime error: {other:?}"),
    }
}
