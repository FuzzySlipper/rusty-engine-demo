use core_ids::EntityId;
use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, CombatFact, GameEvent, GameLoopFact, GameRuntime,
    InventoryAction, InventoryCommand, ItemDefinitionId, LoadingBayGameLoop, ResolvedAttackAction,
    RuntimeError, VitalityFact, WeaponAttackMode,
};
use serde_json::Value;

const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const PLAYER: EntityId = EntityId::new(1);
const FIRST_ENEMY: EntityId = EntityId::new(4);

#[test]
fn projectile_weapon_uses_engine_rigid_body_and_is_transient_across_save() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).expect("canonical project admits");
    let weapon = ItemDefinitionId::parse("weapon/kinetic-launcher").unwrap();
    runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 1,
                action: InventoryAction::SelectWeapon {
                    item: weapon.clone(),
                },
            },
        )
        .unwrap();

    let fired = runtime
        .attack(PLAYER, ResolvedAttackAction::Attack)
        .expect("projectile weapon fires");
    let projectile = fired
        .facts
        .iter()
        .find_map(|fact| match fact {
            CombatFact::ProjectileSpawned { entity, .. } => Some(*entity),
            _ => None,
        })
        .expect("fire produces a projectile fact");
    assert!(fired.facts.iter().any(|fact| matches!(
        fact,
        CombatFact::AttackFired {
            attack_mode: WeaponAttackMode::Projectile,
            ray_count: 1,
            ..
        }
    )));
    assert_eq!(
        runtime
            .session()
            .inventory(PLAYER)
            .unwrap()
            .stacks
            .iter()
            .find(|stack| stack.item == ItemDefinitionId::parse("ammo/kinetic-slug").unwrap())
            .unwrap()
            .quantity,
        7
    );
    let readout = runtime.readout();
    let projected = readout
        .projection
        .iter()
        .find(|node| node.entity == projectile)
        .expect("projectile remains in the authoritative projection");
    assert_eq!(projected.asset, "mesh/physics-projectile");
    let before = projected
        .transform
        .as_ref()
        .expect("projectile has a world transform")
        .translation;

    let stepped = runtime
        .run_projectile_phase(1.0 / 60.0)
        .expect("Engine rigid-body step succeeds");
    assert!(stepped.physics.is_some());
    let after = runtime
        .readout()
        .projection
        .iter()
        .find(|node| node.entity == projectile)
        .expect("projectile remains after one step")
        .transform
        .as_ref()
        .expect("projectile retains a world transform")
        .translation;
    assert_ne!(before, after);

    let snapshot = encode_game_snapshot(&runtime).unwrap();
    assert!(!snapshot.contains("physics projectile"));
    assert!(!snapshot.contains("mesh/physics-projectile"));
    let reopened = decode_game_snapshot(&snapshot).unwrap();
    assert!(reopened.session().weapon(PLAYER).is_some_and(|weapon| {
        weapon.item == ItemDefinitionId::parse("weapon/kinetic-launcher").unwrap()
    }));
    assert!(!reopened
        .readout()
        .projection
        .iter()
        .any(|node| node.entity == projectile));
}

#[test]
fn projectile_hits_canonical_floor_contact_and_removes_body() {
    let mut runtime = runtime_with_project(|project| {
        entity_mut(project, PLAYER.raw())["translation"] = serde_json::json!([7.5, 1.5, 10.5]);
        entity_mut(project, PLAYER.raw())["playerController"]["initialYawDegrees"] =
            serde_json::json!(180.0);
        entity_mut(project, PLAYER.raw())["playerController"]["initialPitchDegrees"] =
            serde_json::json!(60.0);
    });
    let projectile = fire_projectile(&mut runtime);

    let mut impact = None;
    for _ in 0..30 {
        let receipt = runtime
            .run_projectile_phase(1.0 / 60.0)
            .expect("canonical Engine rigid-body phase succeeds");
        if let Some(fact) = receipt.facts.iter().find(|fact| {
            matches!(
                fact,
                loading_bay_game::ProjectileFact::Impacted {
                    entity,
                    target: None,
                    ..
                } if *entity == projectile
            )
        }) {
            impact = Some(fact.clone());
            break;
        }
    }

    assert!(matches!(
        impact,
        Some(loading_bay_game::ProjectileFact::Impacted {
            entity,
            target: None,
            damage: 0,
            ..
        }) if entity == projectile
    ));
    assert!(!runtime
        .readout()
        .projection
        .iter()
        .any(|node| node.entity == projectile));
}

#[test]
fn projectile_hits_target_once_and_applies_damage_before_removal() {
    let runtime = runtime_with_project(|project| {
        entity_mut(project, PLAYER.raw())["translation"] = serde_json::json!([7.5, 1.5, 10.5]);
        entity_mut(project, PLAYER.raw())["playerController"]["initialYawDegrees"] =
            serde_json::json!(180.0);
        entity_mut(project, PLAYER.raw())["playerController"]["initialPitchDegrees"] =
            serde_json::json!(0.0);
    });
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).expect("game loop admits");
    let activation = game_loop
        .run_fixed_tick()
        .expect("encounter activation phase succeeds");
    assert!(activation.facts.iter().any(|fact| {
        matches!(
            fact,
            GameLoopFact::Event(GameEvent::EncounterActivated { encounter, .. })
                if *encounter == EntityId::new(2)
        )
    }));
    let mut runtime = game_loop.into_runtime();
    let projectile = fire_projectile(&mut runtime);
    let mut impact_receipt = None;

    for _ in 0..30 {
        let receipt = runtime
            .run_projectile_phase(1.0 / 60.0)
            .expect("canonical Engine rigid-body phase succeeds");
        if receipt.facts.iter().any(|fact| {
            matches!(
                fact,
                loading_bay_game::ProjectileFact::Impacted {
                    entity,
                    target: Some(FIRST_ENEMY),
                    ..
                } if *entity == projectile
            )
        }) {
            impact_receipt = Some(receipt);
            break;
        }
    }

    let receipt = impact_receipt.expect("projectile reaches the live target");
    assert_eq!(runtime.session().health(FIRST_ENEMY).unwrap().current, 55);
    assert_eq!(
        receipt
            .combat
            .iter()
            .filter(|fact| {
                matches!(
                    fact,
                    CombatFact::Vitality(VitalityFact::DamageApplied {
                        target: FIRST_ENEMY,
                        health_damage: 45,
                        ..
                    })
                )
            })
            .count(),
        1
    );
    assert!(!runtime
        .readout()
        .projection
        .iter()
        .any(|node| node.entity == projectile));

    let next = runtime
        .run_projectile_phase(1.0 / 60.0)
        .expect("empty projectile phase remains valid");
    assert!(next.facts.is_empty());
    assert_eq!(runtime.session().health(FIRST_ENEMY).unwrap().current, 55);
}

#[test]
fn projectile_expiry_removes_body_without_contact() {
    let mut runtime = runtime_with_project(|project| {
        entity_mut(project, PLAYER.raw())["translation"] = serde_json::json!([7.5, 1.5, 10.5]);
        entity_mut(project, PLAYER.raw())["playerController"]["initialYawDegrees"] =
            serde_json::json!(180.0);
        entity_mut(project, PLAYER.raw())["playerController"]["initialPitchDegrees"] =
            serde_json::json!(-60.0);
        item_kind_mut(project, "weapon/kinetic-launcher")["projectileLifetimeTicks"] =
            serde_json::json!(1);
    });
    let projectile = fire_projectile(&mut runtime);
    runtime.advance_by(1).expect("authoritative tick advances");

    let receipt = runtime
        .run_projectile_phase(1.0 / 60.0)
        .expect("expiry step succeeds");
    assert!(receipt.facts.iter().any(|fact| {
        matches!(
            fact,
            loading_bay_game::ProjectileFact::Expired { entity, .. } if *entity == projectile
        )
    }));
    assert!(!receipt.facts.iter().any(|fact| {
        matches!(
            fact,
            loading_bay_game::ProjectileFact::Impacted { entity, .. } if *entity == projectile
        )
    }));
    assert!(!runtime
        .readout()
        .projection
        .iter()
        .any(|node| node.entity == projectile));
}

#[test]
fn projectile_no_ammo_rejection_preserves_authoritative_state() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).expect("canonical project admits");
    select_launcher(&mut runtime);

    for shot in 0..8 {
        if shot > 0 {
            runtime.advance_by(18).expect("launcher cooldown advances");
        }
        runtime
            .attack(PLAYER, ResolvedAttackAction::Attack)
            .expect("authored kinetic ammunition fires");
    }
    runtime
        .advance_by(18)
        .expect("final launcher cooldown advances");
    let before_snapshot = encode_game_snapshot(&runtime).expect("snapshot encodes");
    let before_projection = runtime.readout().projection;
    let before_ammo = inventory_quantity(&runtime);

    let error = runtime
        .attack(PLAYER, ResolvedAttackAction::Attack)
        .expect_err("empty kinetic ammunition rejects");
    assert!(matches!(
        error,
        RuntimeError::CombatRejected {
            entity: PLAYER,
            reason: loading_bay_game::CombatRejectionReason::NoAmmo,
        }
    ));
    assert_eq!(inventory_quantity(&runtime), before_ammo);
    assert_eq!(encode_game_snapshot(&runtime).unwrap(), before_snapshot);
    assert_eq!(runtime.readout().projection, before_projection);
}

fn fire_projectile(runtime: &mut GameRuntime) -> EntityId {
    select_launcher(runtime);
    runtime
        .attack(PLAYER, ResolvedAttackAction::Attack)
        .expect("projectile weapon fires")
        .facts
        .into_iter()
        .find_map(|fact| match fact {
            CombatFact::ProjectileSpawned { entity, .. } => Some(entity),
            _ => None,
        })
        .expect("fire publishes projectile spawn")
}

fn select_launcher(runtime: &mut GameRuntime) {
    runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: runtime.tick().raw().saturating_add(1),
                action: InventoryAction::SelectWeapon {
                    item: ItemDefinitionId::parse("weapon/kinetic-launcher").unwrap(),
                },
            },
        )
        .expect("launcher selection succeeds");
}

fn runtime_with_project(mut edit: impl FnMut(&mut Value)) -> GameRuntime {
    let mut project: Value = serde_json::from_str(PROJECT).expect("canonical JSON parses");
    edit(&mut project);
    GameRuntime::from_stored_project(&project.to_string()).expect("edited canonical project admits")
}

fn entity_mut(project: &mut Value, entity: u64) -> &mut Value {
    project["scenes"][0]["entities"]
        .as_array_mut()
        .expect("scene entities are an array")
        .iter_mut()
        .find(|candidate| candidate["id"] == entity)
        .expect("entity exists")
}

fn item_kind_mut<'a>(project: &'a mut Value, item: &str) -> &'a mut Value {
    project["itemDefinitions"]
        .as_array_mut()
        .expect("item definitions are an array")
        .iter_mut()
        .find(|candidate| candidate["id"] == item)
        .map(|candidate| &mut candidate["kind"])
        .expect("item exists")
}

fn inventory_quantity(runtime: &GameRuntime) -> u32 {
    runtime
        .session()
        .inventory(PLAYER)
        .unwrap()
        .stacks
        .iter()
        .find(|stack| stack.item == ItemDefinitionId::parse("ammo/kinetic-slug").unwrap())
        .map_or(0, |stack| stack.quantity)
}
