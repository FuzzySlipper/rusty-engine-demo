use loading_bay_game::{
    decode_game_snapshot, decode_project_document, encode_game_snapshot, DamageCommand,
    DamageService, DamageSource, GameRuntime, GameplayProgramOutcomeStatus, ItemDefinitionId,
    PickupDisposition, PickupRejection, PickupState, RuntimeError,
};
use rusty_engine::core_ids::EntityId;

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
fn automatic_health_pickup_uses_its_bound_program_and_consumes_the_grant() {
    let supply = ItemDefinitionId::parse("supply/medikit").unwrap();
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let pickup = available_pickup_for_item(&runtime, &supply);
    let mut runtime = with_overlap(runtime, pickup);
    DamageService::apply(
        runtime.session_mut(),
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: PLAYER,
            amount: 30,
        },
    )
    .unwrap();

    let receipt = runtime.collect_pickup(PLAYER, pickup, 5, 11).unwrap();

    assert_eq!(runtime.session().health(PLAYER).unwrap().current, 95);
    assert_eq!(inventory_quantity(&runtime, &supply), 0);
    let outcome = runtime
        .session()
        .gameplay_outcome()
        .expect("automatic health pickup records its selected program");
    assert_eq!(outcome.program_id, "pickup/automatic-health");
    assert_eq!(outcome.status, GameplayProgramOutcomeStatus::Applied);
    assert!(outcome
        .executed_operations
        .iter()
        .any(|operation| operation == "use-granted-health-supply"));
    assert_eq!(
        runtime.session().entity(pickup).unwrap().lifecycle,
        rusty_engine::entity_state::EntityLifecycle::Tombstoned
    );
    assert!(receipt
        .trigger_facts
        .iter()
        .all(|fact| fact.pair.trigger_id() != pickup));
    assert_eq!(
        runtime
            .collect_pickup(PLAYER, pickup, 5, 11)
            .unwrap()
            .disposition,
        PickupDisposition::Repeated
    );
}

#[test]
fn automatic_armor_pickup_uses_its_bound_program_and_consumes_the_grant() {
    let armor = ItemDefinitionId::parse("armor/blue").unwrap();
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let pickup = available_pickup_for_item(&runtime, &armor);
    let mut runtime = with_overlap(runtime, pickup);

    let receipt = runtime.collect_pickup(PLAYER, pickup, 6, 12).unwrap();

    assert_eq!(runtime.session().health(PLAYER).unwrap().armor, 200);
    assert_eq!(inventory_quantity(&runtime, &armor), 0);
    let outcome = runtime
        .session()
        .gameplay_outcome()
        .expect("automatic armor pickup records its selected program");
    assert_eq!(outcome.program_id, "pickup/automatic-armor");
    assert_eq!(outcome.status, GameplayProgramOutcomeStatus::Applied);
    assert!(outcome
        .executed_operations
        .iter()
        .any(|operation| operation == "apply-granted-armor"));
    assert_eq!(
        runtime.session().entity(pickup).unwrap().lifecycle,
        rusty_engine::entity_state::EntityLifecycle::Tombstoned
    );
    assert!(receipt
        .trigger_facts
        .iter()
        .all(|fact| fact.pair.trigger_id() != pickup));
    assert_eq!(
        runtime
            .collect_pickup(PLAYER, pickup, 6, 12)
            .unwrap()
            .disposition,
        PickupDisposition::Repeated
    );
}

#[test]
fn snapshot_pickup_trigger_quota_has_a_deterministic_typed_rejection() {
    // Pickup facts persist as durable components on current schemas. A payload
    // that tries to exceed the trigger quota by duplicating a pickup value is
    // rejected deterministically by the component restore before admission
    // counting can observe it; the quota itself is enforced at project
    // admission and for genuine pre-migration saves through the same lens.
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    let template = snapshot["entities"]["registeredComponents"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|component| component["typeId"] == "loading-bay.pickup")
        .expect("pickup facts persist as durable components")["values"]
        .as_array_mut()
        .unwrap()[0]
        .clone();

    let components = snapshot["entities"]["registeredComponents"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|component| component["typeId"] == "loading-bay.pickup")
        .unwrap()["values"]
        .as_array_mut()
        .unwrap();
    components.push(template);

    let error = decode_game_snapshot(&snapshot.to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("DuplicateEntityValue"), "{error}");
}

#[test]
fn pickup_program_bindings_reject_missing_wrong_family_and_incompatible_profiles() {
    let mut missing: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    pickup_mut(&mut missing, 0)
        .as_object_mut()
        .unwrap()
        .remove("program");
    assert!(matches!(
        GameRuntime::from_stored_project(&missing.to_string()),
        Err(RuntimeError::StoredProject(_))
    ));

    let mut wrong_family: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    pickup_mut(&mut wrong_family, 0)["program"] = "item/health-supply".into();
    let RuntimeError::StoredProject(error) =
        GameRuntime::from_stored_project(&wrong_family.to_string()).unwrap_err()
    else {
        panic!("wrong pickup program family did not fail admission");
    };
    assert!(error.diagnostic().path.ends_with("pickup.program"));
    assert!(error
        .diagnostic()
        .message
        .contains("wrong-family pickup program"));

    let mut incompatible: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    let index = pickup_index_for_item(&incompatible, "ammo/bullets");
    pickup_mut(&mut incompatible, index)["program"] = "pickup/automatic-health".into();
    let RuntimeError::StoredProject(error) =
        GameRuntime::from_stored_project(&incompatible.to_string()).unwrap_err()
    else {
        panic!("incompatible pickup program did not fail admission");
    };
    assert!(error.diagnostic().path.ends_with("pickup.program"));
    assert!(error.diagnostic().message.contains("incompatible"));
}

#[test]
fn canonical_pickup_readout_has_all_seventy_eight_bindings() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let readout = runtime.session().pickup_programs();
    assert_eq!(readout.programs.len(), 4);
    assert_eq!(readout.bindings.len(), 78);
    assert!(readout
        .bindings
        .iter()
        .all(|binding| !binding.program_id.is_empty()));
    assert_eq!(
        readout
            .bindings
            .iter()
            .map(|binding| binding.pickup)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        78
    );
}

#[test]
fn authored_pickup_program_variant_changes_rust_inventory_outcome() {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    let program = project["pickupPrograms"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|program| program["id"] == "pickup/ammunition")
        .unwrap();
    program["program"] = serde_json::json!({
        "kind": "sequence",
        "steps": [{ "kind": "operation", "operation": "consumePickup" }]
    });
    let ammunition = ItemDefinitionId::parse("ammo/bullets").unwrap();
    let project_source = project.to_string();
    let runtime = GameRuntime::from_stored_project(&project_source).unwrap();
    let pickup = available_pickup_for_item(&runtime, &ammunition);
    let mut runtime = with_overlap_for_project(runtime, pickup, &project_source);
    let before = inventory_quantity(&runtime, &ammunition);

    runtime.collect_pickup(PLAYER, pickup, 10, 1).unwrap();

    assert_eq!(inventory_quantity(&runtime, &ammunition), before);
    assert_eq!(
        runtime.session().gameplay_outcome().unwrap().program_id,
        "pickup/ammunition"
    );
    assert_eq!(
        runtime
            .session()
            .gameplay_outcome()
            .unwrap()
            .executed_operations,
        ["consume-pickup"]
    );
}

#[test]
fn weapon_starter_program_grants_first_shotgun_then_only_drop_starter_ammunition() {
    let shotgun = ItemDefinitionId::parse("weapon/shotgun").unwrap();
    let shells = ItemDefinitionId::parse("ammo/shells").unwrap();
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let first_pickup = available_pickup_for_item(&runtime, &shotgun);
    let mut runtime = with_overlap(runtime, first_pickup);

    let first_receipt = runtime.collect_pickup(PLAYER, first_pickup, 11, 1).unwrap();

    assert_eq!(inventory_quantity(&runtime, &shotgun), 1);
    assert_eq!(inventory_quantity(&runtime, &shells), 8);
    assert!(matches!(
        first_receipt.inventory.as_slice(),
        [
            loading_bay_game::InventoryReceipt {
                action: loading_bay_game::InventoryAction::Grant { item, quantity: 1 },
                ..
            },
            loading_bay_game::InventoryReceipt {
                action: loading_bay_game::InventoryAction::Grant { item: ammunition, quantity: 8 },
                ..
            }
        ] if *item == shotgun && *ammunition == shells
    ));
    assert_eq!(
        runtime
            .session()
            .gameplay_outcome()
            .unwrap()
            .executed_operations,
        [
            "grant-picked-item",
            "grant-starter-ammunition",
            "consume-pickup"
        ]
    );

    let enemy = runtime
        .session()
        .enemy_combatants()
        .find(|enemy| {
            runtime
                .session()
                .enemy_drop(enemy.entity)
                .is_some_and(|drop| {
                    runtime
                        .session()
                        .pickup(drop.pickup)
                        .is_some_and(|pickup| pickup.config.item == shotgun)
                })
        })
        .expect("E1M1 has a shotgun enemy drop")
        .entity;
    let drop = runtime.session().enemy_drop(enemy).unwrap().pickup;
    DamageService::apply(
        runtime.session_mut(),
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: enemy,
            amount: 1_000,
        },
    )
    .unwrap();
    let mut runtime = with_overlap(runtime, drop);

    let repeat_receipt = runtime.collect_pickup(PLAYER, drop, 11, 2).unwrap();

    assert_eq!(inventory_quantity(&runtime, &shotgun), 1);
    assert_eq!(inventory_quantity(&runtime, &shells), 12);
    assert!(matches!(
        repeat_receipt.inventory.as_slice(),
        [loading_bay_game::InventoryReceipt {
            action: loading_bay_game::InventoryAction::Grant { item, quantity: 4 },
            ..
        }] if *item == shells
    ));
    assert_eq!(
        runtime
            .session()
            .gameplay_outcome()
            .unwrap()
            .executed_operations,
        ["grant-starter-ammunition", "consume-pickup"]
    );
    assert_eq!(
        runtime.session().pickup(drop).unwrap().state,
        PickupState::Collected {
            actor: PLAYER,
            collected_at_tick: 0,
            cause: loading_bay_game::PickupCollectionCause::Interaction {
                connection_generation: 11,
                command_sequence: 2,
            },
        }
    );
}

#[test]
fn later_pickup_operation_failure_rolls_back_inventory_vitality_lifecycle_and_trigger_state() {
    let supply = ItemDefinitionId::parse("supply/medikit").unwrap();
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let pickup = available_pickup_for_item(&runtime, &supply);
    let mut runtime = with_overlap(runtime, pickup);
    let before_inventory = inventory_quantity(&runtime, &supply);
    let before_health = runtime.session().health(PLAYER).unwrap();
    let before_triggers: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();

    assert!(matches!(
        runtime.collect_pickup(PLAYER, pickup, 10, 2),
        Err(RuntimeError::Pickup(PickupRejection::Vitality(_)))
    ));

    assert_eq!(inventory_quantity(&runtime, &supply), before_inventory);
    assert_eq!(runtime.session().health(PLAYER).unwrap(), before_health);
    assert_eq!(
        runtime.session().pickup(pickup).unwrap().state,
        PickupState::Available
    );
    assert_eq!(
        runtime.session().entity(pickup).unwrap().lifecycle,
        rusty_engine::entity_state::EntityLifecycle::Active
    );
    let after_triggers: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    assert_eq!(
        after_triggers["pickupTriggers"],
        before_triggers["pickupTriggers"]
    );
}

#[test]
fn distinct_stack_capacity_rejects_before_standard_pickup_grant_and_publishes_nothing() {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == PLAYER.raw())
        .unwrap()["inventory"]["capacitySlots"] = serde_json::json!(3);
    let project_source = project.to_string();
    let runtime = GameRuntime::from_stored_project(&project_source).unwrap();
    let supply = ItemDefinitionId::parse("supply/medikit").unwrap();
    let pickup = available_pickup_for_item(&runtime, &supply);
    let mut runtime = with_overlap_for_project(runtime, pickup, &project_source);
    let before = encode_game_snapshot(&runtime).unwrap();

    assert!(matches!(
        runtime.collect_pickup(PLAYER, pickup, 13, 1),
        Err(RuntimeError::Pickup(PickupRejection::Inventory(
            loading_bay_game::InventoryRejection::InventoryFull { capacity_slots: 3 }
        )))
    ));

    assert_eq!(encode_game_snapshot(&runtime).unwrap(), before);
    assert_eq!(
        runtime.session().pickup(pickup).unwrap().state,
        PickupState::Available
    );
}

#[test]
fn dormant_drop_runs_its_pickup_program_only_after_materialization_and_collection() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let enemy = enemy_with_drop();
    let drop = runtime.session().enemy_drop(enemy).unwrap().pickup;
    assert_eq!(
        runtime.session().pickup(drop).unwrap().state,
        PickupState::Dormant
    );
    assert!(matches!(
        runtime.collect_pickup(PLAYER, drop, 10, 3),
        Err(RuntimeError::Pickup(
            PickupRejection::NotMaterialized { .. }
        ))
    ));
    assert!(runtime.session().gameplay_outcome().is_none());

    DamageService::apply(
        runtime.session_mut(),
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: enemy,
            amount: 1_000,
        },
    )
    .unwrap();
    assert_eq!(
        runtime.session().pickup(drop).unwrap().state,
        PickupState::Available
    );
    let mut runtime = with_overlap(runtime, drop);
    runtime.collect_pickup(PLAYER, drop, 10, 4).unwrap();
    assert_eq!(
        runtime.session().gameplay_outcome().unwrap().program_id,
        "pickup/ammunition"
    );
}

fn available_pickup(runtime: &GameRuntime) -> EntityId {
    runtime
        .session()
        .pickups()
        .find(|pickup| pickup.state == PickupState::Available)
        .map(|pickup| pickup.entity)
        .expect("E1M1 must include an available pickup")
}

fn pickup_mut(project: &mut serde_json::Value, ordinal: usize) -> &mut serde_json::Value {
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .filter(|entity| entity.get("pickup").is_some())
        .nth(ordinal)
        .unwrap()
        .get_mut("pickup")
        .unwrap()
}

fn pickup_index_for_item(project: &serde_json::Value, item: &str) -> usize {
    project["scenes"][0]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entity| entity.get("pickup").is_some())
        .position(|entity| {
            entity["pickup"]["item"] == item && entity["pickup"]["program"] == "pickup/ammunition"
        })
        .unwrap()
}

fn enemy_with_drop() -> EntityId {
    let project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    EntityId::new(
        project["scenes"][0]["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entity| entity.get("defeatDrop").is_some())
            .and_then(|entity| entity["id"].as_u64())
            .unwrap(),
    )
}

fn available_pickup_for_item(runtime: &GameRuntime, item: &ItemDefinitionId) -> EntityId {
    runtime
        .session()
        .pickups()
        .find(|pickup| pickup.config.item == *item && pickup.state == PickupState::Available)
        .map(|pickup| pickup.entity)
        .expect("E1M1 must include the requested available pickup")
}

fn inventory_quantity(runtime: &GameRuntime, item: &ItemDefinitionId) -> u32 {
    runtime
        .session()
        .inventory(PLAYER)
        .unwrap()
        .stacks
        .into_iter()
        .find(|stack| stack.item == *item)
        .map_or(0, |stack| stack.quantity)
}

fn with_overlap(runtime: GameRuntime, pickup: EntityId) -> GameRuntime {
    with_overlap_for_project(runtime, pickup, PROJECT)
}

fn with_overlap_for_project(
    runtime: GameRuntime,
    pickup: EntityId,
    project_source: &str,
) -> GameRuntime {
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    snapshot["pickupTriggers"]["revision"] = snapshot["pickupTriggers"]["revision"]
        .as_u64()
        .unwrap()
        .saturating_add(1)
        .into();
    snapshot["pickupTriggers"]["activeOverlaps"] =
        serde_json::json!([{ "trigger": pickup.raw(), "subject": PLAYER.raw() }]);
    let mut restored = decode_game_snapshot(&snapshot.to_string()).unwrap();
    let authored = decode_project_document(project_source)
        .expect("decode E1M1 authored program bindings")
        .project;
    restored
        .reattach_authored_gameplay_programs(&authored)
        .expect("reattach admitted gameplay programs after snapshot restore");
    restored
}
