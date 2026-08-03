use core_ids::EntityId;
use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, CombatFact, GameRuntime, InventoryAction,
    InventoryCommand, ItemDefinitionId, ResolvedAttackAction, WeaponAttackMode,
};

const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const PLAYER: EntityId = EntityId::new(1);

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
