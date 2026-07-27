mod support;

use core_ids::EntityId;
use entity_state::{EntityDefinition, EntityLifecycle};
use loading_bay_game::{
    decode_game_snapshot, decode_project_document, diagnostic_code, encode_game_snapshot,
    encode_project_document, GameEntityDefinition, GameEntityDefinitionError, GameRuntime,
    GameSession, InventoryAction, InventoryCommand, InventoryRejection, ItemDefinitionId,
    PickupConfig, PickupDisposition, PickupFact, PickupRejection, PickupState, RuntimeError,
};

const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const PLAYER: EntityId = EntityId::new(1);
const ENERGY_FILL: EntityId = EntityId::new(20);
const ENERGY_OVERFLOW: EntityId = EntityId::new(21);

#[test]
fn authored_pickups_are_visible_non_solid_objects_with_explicit_item_quantities() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let pickups = runtime.session().pickups().collect::<Vec<_>>();

    assert_eq!(pickups.len(), 16);
    assert_eq!(
        pickups
            .iter()
            .map(|pickup| (
                pickup.entity.raw(),
                pickup.config.item.as_str(),
                pickup.config.quantity,
            ))
            .collect::<Vec<_>>(),
        [
            (20, "ammo/energy-cell", 12),
            (21, "supply/med-patch", 1),
            (22, "ammo/scatter-shell", 6),
            (23, "weapon/breach-scattergun", 1),
            (24, "supply/med-patch", 2),
            (25, "armor/impact-vest", 1),
            (26, "key/maintenance-pass", 1),
            (28, "weapon/rivet-carbine", 1),
            (33, "supply/med-patch", 1),
            (34, "ammo/energy-cell", 10),
            (60, "supply/med-patch", 1),
            (61, "ammo/energy-cell", 10),
            (62, "supply/med-patch", 1),
            (63, "ammo/energy-cell", 10),
            (64, "supply/med-patch", 1),
            (65, "ammo/energy-cell", 10),
        ]
    );
    for pickup in pickups {
        let entity = runtime.session().entity(pickup.entity).unwrap();
        if pickup.entity.raw() >= 33 {
            assert_eq!(pickup.state, PickupState::Dormant);
            assert!(!entity.renderable.as_ref().unwrap().visible);
        } else {
            assert_eq!(pickup.state, PickupState::Available);
            assert!(entity.renderable.as_ref().unwrap().visible);
        }
        assert!(entity.transform.is_some());
        assert!(entity.bounds.is_some());
        assert!(entity.renderable.is_some());
        assert!(entity.collision.is_none());
        assert!(entity.kinematic.is_none());
    }
}

#[test]
fn pickup_collection_atomically_grants_inventory_consumes_world_and_is_idempotent() {
    let mut runtime = with_overlap(
        GameRuntime::from_stored_project(PROJECT).unwrap(),
        ENERGY_FILL,
    );
    let receipt = runtime.collect_pickup(PLAYER, ENERGY_FILL, 4, 9).unwrap();

    assert_eq!(receipt.disposition, PickupDisposition::Collected);
    assert_eq!(
        quantity(&runtime, "ammo/energy-cell"),
        30,
        "the existing ammunition stack is incremented exactly"
    );
    assert!(matches!(
        runtime.session().pickup(ENERGY_FILL).unwrap().state,
        PickupState::Collected {
            actor: PLAYER,
            collected_at_tick: 0,
            ..
        }
    ));
    assert_eq!(
        runtime.session().entity(ENERGY_FILL).unwrap().lifecycle,
        EntityLifecycle::Tombstoned
    );
    assert!(!runtime
        .readout()
        .projection
        .iter()
        .any(|node| node.entity == ENERGY_FILL));
    assert!(matches!(
        receipt.facts.as_slice(),
        [PickupFact::Collected {
            pickup: ENERGY_FILL,
            actor: PLAYER,
            quantity: 12,
            ..
        }]
    ));
    assert_eq!(receipt.cues.len(), 1);

    let repeated = runtime.collect_pickup(PLAYER, ENERGY_FILL, 4, 9).unwrap();
    assert_eq!(repeated.disposition, PickupDisposition::Repeated);
    assert!(repeated.facts.is_empty());
    assert!(repeated.cues.is_empty());
    assert_eq!(quantity(&runtime, "ammo/energy-cell"), 30);
}

#[test]
fn capacity_rejection_leaves_inventory_pickup_and_snapshot_byte_identical() {
    let project = project_with_energy_overflow();
    let mut runtime = with_overlap(
        GameRuntime::from_stored_project(&project.to_string()).unwrap(),
        ENERGY_FILL,
    );
    runtime.collect_pickup(PLAYER, ENERGY_FILL, 1, 1).unwrap();
    let mut runtime = with_overlap(runtime, ENERGY_OVERFLOW);
    let before = encode_game_snapshot(&runtime).unwrap();

    let error = runtime
        .collect_pickup(PLAYER, ENERGY_OVERFLOW, 1, 2)
        .unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::Pickup(PickupRejection::Inventory(
            InventoryRejection::QuantityOverflow {
                current: 30,
                requested: 171,
                limit: 200,
                ..
            }
        ))
    ));
    assert_eq!(encode_game_snapshot(&runtime).unwrap(), before);
    assert_eq!(
        runtime.session().pickup(ENERGY_OVERFLOW).unwrap().state,
        PickupState::Available
    );
    assert_eq!(
        runtime.session().entity(ENERGY_OVERFLOW).unwrap().lifecycle,
        EntityLifecycle::Active
    );
}

#[test]
fn every_pickup_family_round_trips_and_authored_restart_restores_availability() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut armor_receipts = None;
    for pickup in [22, 23, 24, 25, 26, 28] {
        runtime = with_overlap(runtime, EntityId::new(pickup));
        let receipt = runtime
            .collect_pickup(PLAYER, EntityId::new(pickup), 2, pickup)
            .unwrap();
        if pickup == 25 {
            armor_receipts = Some(receipt.inventory);
        }
    }

    assert_eq!(quantity(&runtime, "ammo/scatter-shell"), 14);
    assert_eq!(quantity(&runtime, "weapon/breach-scattergun"), 1);
    assert_eq!(quantity(&runtime, "weapon/rivet-carbine"), 1);
    assert_eq!(quantity(&runtime, "ammo/energy-cell"), 18);
    assert_eq!(quantity(&runtime, "supply/med-patch"), 3);
    assert_eq!(
        quantity(&runtime, "armor/impact-vest"),
        0,
        "armor pickup is consumed into Rust-owned vitality instead of duplicating authority"
    );
    assert_eq!(runtime.session().health(PLAYER).unwrap().armor, 100);
    let armor_receipts = armor_receipts.unwrap();
    assert_eq!(armor_receipts.len(), 2);
    assert!(matches!(
        armor_receipts[0].action,
        InventoryAction::Grant { quantity: 1, .. }
    ));
    assert_eq!(
        armor_receipts[0]
            .after
            .stacks
            .iter()
            .find(|stack| stack.item.as_str() == "armor/impact-vest")
            .map(|stack| stack.quantity),
        Some(1)
    );
    assert!(matches!(
        armor_receipts[1].action,
        InventoryAction::Consume { quantity: 1, .. }
    ));
    assert!(armor_receipts[1]
        .after
        .stacks
        .iter()
        .all(|stack| stack.item.as_str() != "armor/impact-vest"));
    assert_eq!(quantity(&runtime, "key/maintenance-pass"), 1);
    let encoded = encode_game_snapshot(&runtime).unwrap();
    let reopened = decode_game_snapshot(&encoded).unwrap();
    assert_eq!(encode_game_snapshot(&reopened).unwrap(), encoded);
    assert_eq!(quantity(&reopened, "ammo/scatter-shell"), 14);
    assert_eq!(reopened.session().health(PLAYER).unwrap().armor, 100);
    assert!(reopened
        .session()
        .pickups()
        .filter(|pickup| (22..33).contains(&pickup.entity.raw()))
        .all(|pickup| matches!(pickup.state, PickupState::Collected { .. })));

    let restarted = GameRuntime::from_stored_project(PROJECT).unwrap();
    assert!(restarted
        .session()
        .pickups()
        .all(|pickup| if pickup.entity.raw() >= 33 {
            pickup.state == PickupState::Dormant
        } else {
            pickup.state == PickupState::Available
        }));
    assert_eq!(quantity(&restarted, "ammo/scatter-shell"), 0);
    assert_eq!(restarted.session().health(PLAYER).unwrap().armor, 0);
}

#[test]
fn weapon_pickup_grants_starter_ammunition_atomically_and_duplicates_grant_neither() {
    let mut runtime = with_overlap(
        GameRuntime::from_stored_project(PROJECT).unwrap(),
        EntityId::new(23),
    );
    let receipt = runtime
        .collect_pickup(PLAYER, EntityId::new(23), 1, 1)
        .unwrap();
    assert_eq!(receipt.inventory.len(), 2);
    assert_eq!(quantity(&runtime, "weapon/breach-scattergun"), 1);
    assert_eq!(quantity(&runtime, "ammo/scatter-shell"), 8);

    let mut duplicate = GameRuntime::from_stored_project(PROJECT).unwrap();
    duplicate
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 1,
                action: InventoryAction::Grant {
                    item: ItemDefinitionId::parse("weapon/breach-scattergun").unwrap(),
                    quantity: 1,
                },
            },
        )
        .unwrap();
    let mut duplicate = with_overlap(duplicate, EntityId::new(23));
    let before = encode_game_snapshot(&duplicate).unwrap();
    assert!(matches!(
        duplicate
            .collect_pickup(PLAYER, EntityId::new(23), 1, 1)
            .unwrap_err(),
        RuntimeError::Pickup(PickupRejection::Inventory(
            InventoryRejection::QuantityOverflow { .. }
        ))
    ));
    assert_eq!(encode_game_snapshot(&duplicate).unwrap(), before);
    assert_eq!(quantity(&duplicate, "ammo/scatter-shell"), 0);
    assert_eq!(
        duplicate.session().pickup(EntityId::new(23)).unwrap().state,
        PickupState::Available
    );
}

#[test]
fn schema_eleven_rejects_future_pickup_state_but_migrates_when_fields_are_absent() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    snapshot["schemaVersion"] = 11.into();
    support::strip_future_gameplay_mechanics_state(&mut snapshot);
    assert!(matches!(
        decode_game_snapshot(&snapshot.to_string()).unwrap_err(),
        loading_bay_game::GameSnapshotError::FuturePickupStateInLegacySnapshot
    ));

    snapshot.as_object_mut().unwrap().remove("pickups");
    snapshot.as_object_mut().unwrap().remove("pickupTriggers");
    snapshot.as_object_mut().unwrap().remove("progression");
    snapshot.as_object_mut().unwrap().remove("enemyCombat");
    strip_snapshot_enemy_archetype_fields(&mut snapshot);
    strip_snapshot_vitality_fields(&mut snapshot);
    strip_snapshot_weapon_item_fields(&mut snapshot);
    let migrated = decode_game_snapshot(&snapshot.to_string()).unwrap();
    assert_eq!(migrated.session().pickups().len(), 0);
    assert_eq!(quantity(&migrated, "ammo/energy-cell"), 18);
}

#[test]
fn schema_twelve_project_rejects_future_pickups_and_migrates_without_inventing_them() {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    project["schemaVersion"] = 12.into();
    let error = decode_project_document(&project.to_string()).unwrap_err();
    assert_eq!(error.diagnostic().code, diagnostic_code::MIGRATION);

    for scene in project["scenes"].as_array_mut().unwrap() {
        scene["entities"]
            .as_array_mut()
            .unwrap()
            .retain(|entity| entity.get("pickup").is_none() && entity.get("hazard").is_none());
        for entity in scene["entities"].as_array_mut().unwrap() {
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
    }
    strip_project_weapon_item_fields(&mut project);
    let migrated = decode_project_document(&project.to_string()).unwrap();
    assert_eq!(migrated.source_schema_version, 12);
    assert!(migrated.was_migrated());
    let runtime =
        GameRuntime::from_stored_project(&encode_project_document(&migrated.project).unwrap())
            .unwrap();
    assert_eq!(runtime.session().pickups().len(), 0);
    assert_eq!(quantity(&runtime, "ammo/energy-cell"), 18);
}

fn project_with_energy_overflow() -> serde_json::Value {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    let overflow = project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == ENERGY_OVERFLOW.raw())
        .unwrap();
    overflow["pickup"]["item"] = "ammo/energy-cell".into();
    overflow["pickup"]["quantity"] = 171.into();
    project
}

fn strip_snapshot_enemy_archetype_fields(snapshot: &mut serde_json::Value) {
    snapshot.as_object_mut().unwrap().remove("enemyDrops");
    for encounter in snapshot["encounters"].as_array_mut().unwrap() {
        encounter
            .as_object_mut()
            .unwrap()
            .remove("activationRadius");
        encounter["state"] = "active".into();
    }
}

fn strip_snapshot_vitality_fields(snapshot: &mut serde_json::Value) {
    snapshot.as_object_mut().unwrap().remove("hazards");
    snapshot.as_object_mut().unwrap().remove("hazardTriggers");
    for health in snapshot["health"].as_array_mut().unwrap() {
        let health = health.as_object_mut().unwrap();
        health.remove("maxArmor");
        health.remove("armorAbsorptionPercent");
        health.remove("armor");
        health.remove("armorItem");
        health.remove("state");
    }
}

fn strip_snapshot_weapon_item_fields(snapshot: &mut serde_json::Value) {
    for definition in snapshot["itemDefinitions"].as_array_mut().unwrap() {
        if definition["kind"]["kind"] == "weapon" {
            let kind = definition["kind"].as_object_mut().unwrap();
            for field in [
                "attackMode",
                "damage",
                "maxDistance",
                "cooldownTicks",
                "ammunitionCost",
                "muzzleOffset",
                "presentation",
            ] {
                kind.remove(field);
            }
        }
    }
    for inventory in snapshot["inventories"].as_array_mut().unwrap() {
        inventory.as_object_mut().unwrap().remove("weaponSlots");
        inventory.as_object_mut().unwrap().remove("weaponCooldowns");
    }
    for controller in snapshot["playerControllers"].as_array_mut().unwrap() {
        controller["bindings"]
            .as_object_mut()
            .unwrap()
            .remove("selectWeapon");
    }
}

fn strip_project_weapon_item_fields(project: &mut serde_json::Value) {
    for definition in project["itemDefinitions"].as_array_mut().unwrap() {
        if definition["kind"]["kind"] == "weapon" {
            let kind = definition["kind"].as_object_mut().unwrap();
            for field in [
                "attackMode",
                "damage",
                "maxDistance",
                "cooldownTicks",
                "ammunitionCost",
                "muzzleOffset",
                "presentation",
            ] {
                kind.remove(field);
            }
        }
    }
    for scene in project["scenes"].as_array_mut().unwrap() {
        for entity in scene["entities"].as_array_mut().unwrap() {
            if let Some(inventory) = entity.get_mut("inventory") {
                inventory.as_object_mut().unwrap().remove("weaponSlots");
            }
            if let Some(controller) = entity.get_mut("playerController") {
                controller["bindings"]
                    .as_object_mut()
                    .unwrap()
                    .remove("selectWeapon");
            }
        }
    }
}

#[test]
fn pickup_trigger_quota_is_rejected_before_runtime_construction_without_panicking() {
    let item = ItemDefinitionId::parse("ammo/quota-probe").unwrap();
    let definitions = (0..=engine_spatial::MAX_TRIGGER_DEFINITIONS)
        .map(|index| {
            let entity = EntityId::new(index as u64 + 1);
            GameEntityDefinition::new(EntityDefinition::new(entity, format!("pickup-{index}")))
                .as_pickup(PickupConfig::new(item.clone(), 1))
        })
        .collect::<Vec<_>>();

    assert!(matches!(
        GameSession::from_definitions(definitions).unwrap_err(),
        GameEntityDefinitionError::TooManyPickups { count, limit }
            if count == engine_spatial::MAX_TRIGGER_DEFINITIONS + 1
                && limit == engine_spatial::MAX_TRIGGER_DEFINITIONS
    ));
}

#[test]
fn snapshot_pickup_trigger_quota_has_a_deterministic_typed_rejection() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    let template = snapshot["pickups"][0].clone();
    let pickups = snapshot["pickups"].as_array_mut().unwrap();
    while pickups.len() <= engine_spatial::MAX_TRIGGER_DEFINITIONS {
        pickups.push(template.clone());
    }

    assert!(matches!(
        decode_game_snapshot(&snapshot.to_string()).unwrap_err(),
        loading_bay_game::GameSnapshotError::TooManyPickups { count, limit }
            if count == engine_spatial::MAX_TRIGGER_DEFINITIONS + 1
                && limit == engine_spatial::MAX_TRIGGER_DEFINITIONS
    ));
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
