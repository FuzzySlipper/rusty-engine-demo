use loading_bay_game::{
    CombatFact, GameRuntime, GameplayProgramOutcomeStatus, InventoryAction, InventoryCommand,
    InventoryService, ItemDefinitionId, ResolvedAttackAction,
};
use rusty_engine::core_ids::EntityId;
use serde_json::{json, Value};

const PLAYER: EntityId = EntityId::new(1);
const E1M1: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");

#[test]
fn current_e1m1_weapon_programs_preserve_fist_pistol_and_shotgun_costs() {
    let mut runtime = GameRuntime::from_stored_project(E1M1).unwrap();
    let pistol = ItemDefinitionId::parse("weapon/pistol").unwrap();
    let shotgun = ItemDefinitionId::parse("weapon/shotgun").unwrap();
    let shells = ItemDefinitionId::parse("ammo/shells").unwrap();

    let bullets_before = inventory_quantity(&runtime, "ammo/bullets");
    let pistol_receipt = attack(&mut runtime);
    assert!(pistol_receipt.facts.iter().any(|fact| matches!(
        fact,
        CombatFact::AttackFired { weapon, ray_count: 1, .. } if *weapon == pistol
    )));
    assert_eq!(
        inventory_quantity(&runtime, "ammo/bullets"),
        bullets_before - 1
    );
    let outcome = runtime
        .session()
        .gameplay_outcome()
        .expect("pistol attack records its selected program");
    assert_eq!(outcome.program_id, "weapon/hitscan-ammunition");
    assert_eq!(outcome.status, GameplayProgramOutcomeStatus::Applied);
    assert!(outcome
        .executed_operations
        .iter()
        .any(|operation| operation == "consume-ammo"));

    runtime.advance_by(24).unwrap();
    InventoryService::apply(
        runtime.session_mut(),
        PLAYER,
        InventoryCommand {
            sequence: 2,
            action: InventoryAction::Grant {
                item: shotgun.clone(),
                quantity: 1,
            },
        },
    )
    .unwrap();
    InventoryService::apply(
        runtime.session_mut(),
        PLAYER,
        InventoryCommand {
            sequence: 3,
            action: InventoryAction::Grant {
                item: shells.clone(),
                quantity: 2,
            },
        },
    )
    .unwrap();
    InventoryService::select_weapon_slot(runtime.session_mut(), PLAYER, 1).unwrap();
    let shells_before = inventory_quantity(&runtime, "ammo/shells");
    let shotgun_receipt = attack(&mut runtime);
    assert!(shotgun_receipt.facts.iter().any(|fact| matches!(
        fact,
        CombatFact::AttackFired { weapon, ray_count: 7, .. } if *weapon == shotgun
    )));
    assert_eq!(
        inventory_quantity(&runtime, "ammo/shells"),
        shells_before - 1
    );

    runtime.advance_by(63).unwrap();
    InventoryService::select_weapon_slot(runtime.session_mut(), PLAYER, 2).unwrap();
    let bullets_after_pistol = inventory_quantity(&runtime, "ammo/bullets");
    let fist_receipt = attack(&mut runtime);
    assert!(fist_receipt.facts.iter().any(|fact| matches!(
        fact,
        CombatFact::AttackFired { ray_count: 1, ammo_before, ammo_after, .. }
            if *ammo_before == *ammo_after
    )));
    assert_eq!(
        inventory_quantity(&runtime, "ammo/bullets"),
        bullets_after_pistol
    );
}

#[test]
fn authored_weapon_program_variant_changes_the_rust_inventory_outcome() {
    let mut project: Value = serde_json::from_str(E1M1).unwrap();
    let program = project["gameplayPrograms"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|program| program["id"] == "weapon/hitscan-ammunition")
        .unwrap();
    program["program"] = json!({
        "kind": "sequence",
        "steps": [
            { "kind": "operation", "operation": "recordFired" },
            {
                "kind": "when",
                "predicate": "impactIsHit",
                "thenProgram": { "kind": "operation", "operation": "applyHit" },
                "otherwiseProgram": { "kind": "operation", "operation": "applyMiss" }
            },
            { "kind": "operation", "operation": "setCooldown" }
        ]
    });
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let before = inventory_quantity(&runtime, "ammo/bullets");

    attack(&mut runtime);

    assert_eq!(inventory_quantity(&runtime, "ammo/bullets"), before);
    let outcome = runtime
        .session()
        .gameplay_outcome()
        .expect("variant attack records its selected program");
    assert_eq!(outcome.program_id, "weapon/hitscan-ammunition");
    assert_eq!(outcome.status, GameplayProgramOutcomeStatus::Applied);
    assert!(!outcome
        .executed_operations
        .iter()
        .any(|operation| operation == "consume-ammo"));
}

#[test]
fn current_schema_rejects_retired_inventory_literals() {
    let mut project: Value = serde_json::from_str(E1M1).unwrap();
    let player = project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == PLAYER.raw())
        .unwrap();
    player["inventory"]["startingStacks"] = json!([
        { "item": "weapon/pistol", "quantity": 1 }
    ]);

    let error = GameRuntime::from_stored_project(&project.to_string()).unwrap_err();
    let loading_bay_game::RuntimeError::StoredProject(error) = error else {
        panic!("current project decoder must reject retired inventory literals");
    };
    assert_eq!(
        error.diagnostic().code,
        loading_bay_game::diagnostic_code::DECODE
    );
    assert_eq!(
        error.diagnostic().path,
        "scenes[0].entities[0].inventory.startingStacks"
    );
}

#[test]
fn current_project_admission_limits_player_weapons_to_hitscan_and_spread() {
    let mut project: Value = serde_json::from_str(E1M1).unwrap();
    let item_definitions = project["itemDefinitions"].as_array().unwrap();
    let modes = item_definitions
        .iter()
        .filter(|item| item["kind"]["kind"] == "weapon")
        .map(|item| {
            item["kind"]["attackMode"]
                .as_str()
                .expect("current E1M1 weapon mode")
        })
        .collect::<Vec<_>>();
    assert_eq!(modes, ["hitscan", "hitscan", "spread"]);

    let pistol = project["itemDefinitions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["id"] == "weapon/pistol")
        .unwrap();
    pistol["kind"]["attackMode"] = "projectile".into();

    let error = GameRuntime::from_stored_project(&project.to_string()).unwrap_err();
    let loading_bay_game::RuntimeError::StoredProject(error) = error else {
        panic!("current project decoder must reject retired player projectile weapons");
    };
    assert_eq!(
        error.diagnostic().code,
        loading_bay_game::diagnostic_code::DECODE
    );
    assert_eq!(error.diagnostic().path, "itemDefinitions[9].kind");
    assert!(error
        .diagnostic()
        .message
        .contains("unknown variant `projectile`"));
}

fn attack(runtime: &mut GameRuntime) -> loading_bay_game::CombatReceipt {
    runtime
        .attack(PLAYER, ResolvedAttackAction::Attack)
        .expect("accepted attack")
}

fn inventory_quantity(runtime: &GameRuntime, item: &str) -> u32 {
    let item = ItemDefinitionId::parse(item).unwrap();
    runtime
        .session()
        .inventory(PLAYER)
        .unwrap()
        .stacks
        .into_iter()
        .find(|stack| stack.item == item)
        .map_or(0, |stack| stack.quantity)
}
