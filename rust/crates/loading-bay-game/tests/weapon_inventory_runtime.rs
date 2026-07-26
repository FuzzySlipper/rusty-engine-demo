use core_ids::EntityId;
use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, CombatFact, CombatRejectionReason, GameEvent,
    GameRuntime, GameSnapshotError, InventoryAction, InventoryCommand, ItemDefinitionId,
    ResolvedAttackAction, ResolvedPlayerAction, RuntimeError, WeaponAttackMode,
};

const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const PLAYER: EntityId = EntityId::new(1);
const ENEMY: EntityId = EntityId::new(4);

#[test]
fn equipped_item_definitions_own_cadence_damage_and_distinct_ammunition_pools() {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == ENEMY.raw())
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("navigation");
    let mut uninterrupted = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    aim_at(&mut uninterrupted, ENEMY);

    assert_eq!(
        weapon_mode(&uninterrupted, "weapon/arc-pistol"),
        WeaponAttackMode::Hitscan
    );
    assert_eq!(
        weapon_mode(&uninterrupted, "weapon/breach-scattergun"),
        WeaponAttackMode::Spread {
            pellet_count: 7,
            spread_degrees: 7.0,
        }
    );
    assert_eq!(
        weapon_mode(&uninterrupted, "weapon/rivet-carbine"),
        WeaponAttackMode::Automatic
    );

    apply(
        &mut uninterrupted,
        1,
        InventoryAction::Grant {
            item: item("weapon/rivet-carbine"),
            quantity: 1,
        },
    );
    apply(
        &mut uninterrupted,
        2,
        InventoryAction::SelectWeapon {
            item: item("weapon/rivet-carbine"),
        },
    );
    let automatic = uninterrupted
        .attack(PLAYER, ResolvedAttackAction::Attack)
        .unwrap();
    assert_eq!(quantity(&uninterrupted, "ammo/energy-cell"), 39);
    assert_eq!(uninterrupted.session().health(ENEMY).unwrap().current, 82);
    assert!(automatic.facts.iter().any(|fact| matches!(
        fact,
        CombatFact::AttackFired {
            attack_mode: WeaponAttackMode::Automatic,
            presentation,
            ray_count: 1,
            ..
        } if presentation == "rivet-carbine"
    )));

    apply(
        &mut uninterrupted,
        4,
        InventoryAction::SelectWeapon {
            item: item("weapon/arc-pistol"),
        },
    );
    let single = uninterrupted
        .attack(PLAYER, ResolvedAttackAction::Attack)
        .unwrap();
    assert_eq!(quantity(&uninterrupted, "ammo/energy-cell"), 38);
    assert_eq!(uninterrupted.session().health(ENEMY).unwrap().current, 22);
    assert!(single.facts.iter().any(|fact| matches!(
        fact,
        CombatFact::AttackFired {
            attack_mode: WeaponAttackMode::Hitscan,
            presentation,
            ray_count: 1,
            ..
        } if presentation == "arc-pistol"
    )));

    apply(
        &mut uninterrupted,
        6,
        InventoryAction::SelectWeapon {
            item: item("weapon/rivet-carbine"),
        },
    );
    let before_cooldown = encode_game_snapshot(&uninterrupted).unwrap();
    assert!(matches!(
        uninterrupted
            .attack(PLAYER, ResolvedAttackAction::Attack)
            .unwrap_err(),
        RuntimeError::CombatRejected {
            reason: CombatRejectionReason::Cooldown,
            ..
        }
    ));
    assert_eq!(
        encode_game_snapshot(&uninterrupted).unwrap(),
        before_cooldown
    );

    apply(
        &mut uninterrupted,
        7,
        InventoryAction::Grant {
            item: item("weapon/breach-scattergun"),
            quantity: 1,
        },
    );
    apply(
        &mut uninterrupted,
        8,
        InventoryAction::Grant {
            item: item("ammo/scatter-shell"),
            quantity: 4,
        },
    );
    apply(
        &mut uninterrupted,
        9,
        InventoryAction::SelectWeapon {
            item: item("weapon/breach-scattergun"),
        },
    );

    let snapshot = encode_game_snapshot(&uninterrupted).unwrap();
    let mut reopened = decode_game_snapshot(&snapshot).unwrap();
    assert_eq!(
        reopened.session().inventory(PLAYER),
        uninterrupted.session().inventory(PLAYER)
    );

    let expected_lethal = uninterrupted
        .attack(PLAYER, ResolvedAttackAction::Attack)
        .unwrap();
    let actual_lethal = reopened
        .attack(PLAYER, ResolvedAttackAction::Attack)
        .unwrap();
    assert_eq!(actual_lethal, expected_lethal);
    assert_eq!(quantity(&uninterrupted, "ammo/scatter-shell"), 3);
    assert_eq!(quantity(&uninterrupted, "ammo/energy-cell"), 38);
    assert!(expected_lethal.facts.iter().any(|fact| matches!(
        fact,
        CombatFact::AttackFired {
            attack_mode:
                WeaponAttackMode::Spread {
                    pellet_count,
                    spread_degrees,
                },
            presentation,
            ray_count: 7,
            ..
        } if *pellet_count == 7
            && *spread_degrees == 7.0
            && presentation == "breach-scattergun"
    )));
    let ray_directions = expected_lethal
        .facts
        .iter()
        .filter_map(|fact| match fact {
            CombatFact::AttackHit {
                ray_index,
                direction,
                ..
            }
            | CombatFact::AttackMissed {
                ray_index,
                direction,
                ..
            } => Some((*ray_index, *direction)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        ray_directions
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>(),
        (0..7).collect::<Vec<_>>()
    );
    assert!(ray_directions
        .iter()
        .skip(1)
        .any(|(_, direction)| *direction != ray_directions[0].1));
    assert_eq!(
        expected_lethal
            .events
            .iter()
            .filter(|event| matches!(event, GameEvent::EnemyDefeated { .. }))
            .count(),
        1
    );

    let before_rejected_cooldown = encode_game_snapshot(&uninterrupted).unwrap();
    assert!(matches!(
        uninterrupted
            .attack(PLAYER, ResolvedAttackAction::Attack)
            .unwrap_err(),
        RuntimeError::CombatRejected {
            reason: CombatRejectionReason::Cooldown,
            ..
        }
    ));
    assert_eq!(
        encode_game_snapshot(&uninterrupted).unwrap(),
        before_rejected_cooldown
    );

    uninterrupted.advance_by(36).unwrap();
    reopened.advance_by(36).unwrap();
    let expected_miss = uninterrupted
        .attack(PLAYER, ResolvedAttackAction::Attack)
        .unwrap();
    let actual_miss = reopened
        .attack(PLAYER, ResolvedAttackAction::Attack)
        .unwrap();
    assert_eq!(actual_miss, expected_miss);
    assert!(expected_miss
        .facts
        .iter()
        .all(|fact| !matches!(fact, CombatFact::EnemyDefeated { .. })));
    assert!(expected_miss.events.is_empty());
    assert_eq!(quantity(&uninterrupted, "ammo/scatter-shell"), 2);

    for expected_remaining in [1, 0] {
        uninterrupted.advance_by(36).unwrap();
        uninterrupted
            .attack(PLAYER, ResolvedAttackAction::Attack)
            .unwrap();
        assert_eq!(
            quantity(&uninterrupted, "ammo/scatter-shell"),
            expected_remaining
        );
    }
    uninterrupted.advance_by(36).unwrap();
    let before_no_ammo = encode_game_snapshot(&uninterrupted).unwrap();
    assert!(matches!(
        uninterrupted
            .attack(PLAYER, ResolvedAttackAction::Attack)
            .unwrap_err(),
        RuntimeError::CombatRejected {
            reason: CombatRejectionReason::NoAmmo,
            ..
        }
    ));
    assert_eq!(
        encode_game_snapshot(&uninterrupted).unwrap(),
        before_no_ammo
    );
}

#[test]
fn snapshot_rejects_weapon_cooldown_beyond_authored_cadence() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    let cooldown = snapshot["inventories"][0]["weaponCooldowns"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|cooldown| cooldown["item"] == "weapon/arc-pistol")
        .unwrap();
    cooldown["readyAtTick"] = 10_000.into();

    let error = decode_game_snapshot(&snapshot.to_string()).unwrap_err();

    assert!(matches!(
        error,
        GameSnapshotError::InvalidWeaponCooldown { owner: 1, ref item }
            if item == "weapon/arc-pistol"
    ));
}

#[test]
fn schema_fourteen_snapshot_rejects_future_spread_and_automatic_definitions() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    snapshot["schemaVersion"] = 14.into();

    assert!(matches!(
        decode_game_snapshot(&snapshot.to_string()).unwrap_err(),
        GameSnapshotError::FutureWeaponStateInLegacySnapshot
    ));
}

fn apply(runtime: &mut GameRuntime, sequence: u64, action: InventoryAction) {
    runtime
        .apply_inventory_command(PLAYER, InventoryCommand { sequence, action })
        .unwrap();
}

fn quantity(runtime: &GameRuntime, item_id: &str) -> u32 {
    let item = item(item_id);
    runtime
        .session()
        .inventory(PLAYER)
        .unwrap()
        .stacks
        .into_iter()
        .find(|stack| stack.item == item)
        .map_or(0, |stack| stack.quantity)
}

fn weapon_mode(runtime: &GameRuntime, item_id: &str) -> WeaponAttackMode {
    let definition = runtime.session().item_definition(&item(item_id)).unwrap();
    let loading_bay_game::ItemKind::Weapon(weapon) = definition.kind else {
        panic!("{item_id} is not a weapon");
    };
    weapon.attack_mode
}

fn item(value: &str) -> ItemDefinitionId {
    ItemDefinitionId::parse(value).unwrap()
}

fn aim_at(runtime: &mut GameRuntime, target: EntityId) {
    let player = runtime
        .session()
        .entity(PLAYER)
        .unwrap()
        .transform
        .unwrap()
        .translation;
    let target = runtime
        .session()
        .entity(target)
        .unwrap()
        .transform
        .unwrap()
        .translation;
    let offset_x = target.x - player.x;
    let offset_y = target.y - player.y;
    let offset_z = target.z - player.z;
    let desired_yaw = normalize_degrees((-offset_x).atan2(-offset_z).to_degrees());
    let desired_pitch = offset_y
        .atan2((offset_x * offset_x + offset_z * offset_z).sqrt())
        .to_degrees();

    for _ in 0..40 {
        let controller = runtime.session().player_controller(PLAYER).unwrap();
        let yaw_difference = normalize_degrees(desired_yaw - controller.state.yaw_degrees);
        let pitch_difference = desired_pitch - controller.state.pitch_degrees;
        if yaw_difference.abs() < 0.01 && pitch_difference.abs() < 0.01 {
            return;
        }
        runtime
            .apply_player_action(
                PLAYER,
                ResolvedPlayerAction::Look {
                    yaw_delta: (yaw_difference / controller.config.look_degrees_per_unit)
                        .clamp(-1.0, 1.0),
                    pitch_delta: (pitch_difference / controller.config.look_degrees_per_unit)
                        .clamp(-1.0, 1.0),
                },
            )
            .unwrap();
    }
    panic!("could not aim at target");
}

fn normalize_degrees(value: f32) -> f32 {
    (value + 180.0).rem_euclid(360.0) - 180.0
}
