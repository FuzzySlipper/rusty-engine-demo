use loading_bay_game::{
    CombatFact, CombatRejectionReason, GameLoopEdgeCommand, GameLoopEdgeCommandKind, GameLoopFact,
    GameRuntime, ItemDefinitionId, ItemKind, LoadingBayGameLoop, PickupState, PlayerInputCommand,
    PlayerInputIntent, WeaponAttackMode,
};
use rusty_engine::core_ids::EntityId;

const PROJECT: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");
const PLAYER: EntityId = EntityId::new(1);

#[test]
fn authored_e1m1_has_only_the_single_player_pistol_shotgun_and_ammunition_ledger() {
    let runtime = GameRuntime::from_stored_project(PROJECT).expect("admit E1M1");
    let session = runtime.session();
    let weapons = session
        .item_definitions()
        .filter_map(|definition| match definition.kind {
            ItemKind::Weapon(weapon) => Some((definition.id, weapon)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        weapons
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>(),
        ["weapon/fist", "weapon/pistol", "weapon/shotgun"]
    );
    assert_eq!(weapons[0].1.ammunition_cost, 0);
    assert_eq!(weapons[0].1.attack_mode, WeaponAttackMode::Hitscan);
    assert!(weapons[0].1.repeat_while_held);
    assert_eq!(weapons[0].1.damage, 2);
    assert_eq!(weapons[0].1.damage_rolls, 10);
    assert_eq!(weapons[0].1.max_distance, 4.0);
    assert_eq!(weapons[0].1.cooldown_ticks, 38);
    assert_eq!(weapons[1].1.ammunition.as_str(), "ammo/bullets");
    assert_eq!(weapons[1].1.attack_mode, WeaponAttackMode::Hitscan);
    assert!(weapons[1].1.repeat_while_held);
    assert_eq!(weapons[1].1.damage, 5);
    assert_eq!(weapons[1].1.damage_rolls, 3);
    assert_eq!(weapons[1].1.cooldown_ticks, 24);
    assert_eq!(weapons[2].1.ammunition.as_str(), "ammo/shells");
    assert_eq!(
        weapons[2].1.attack_mode,
        WeaponAttackMode::Spread {
            pellet_count: 7,
            spread_degrees: 5.625,
        }
    );
    assert!(weapons[2].1.repeat_while_held);
    assert_eq!(weapons[2].1.damage, 5);
    assert_eq!(weapons[2].1.damage_rolls, 3);
    assert_eq!(weapons[2].1.cooldown_ticks, 63);

    let inventory = session.inventory(PLAYER).expect("player inventory");
    assert_eq!(
        inventory
            .stacks
            .iter()
            .map(|stack| (stack.item.as_str(), stack.quantity))
            .collect::<Vec<_>>(),
        [
            ("weapon/fist", 1),
            ("weapon/pistol", 1),
            ("ammo/bullets", 50)
        ]
    );
    assert_eq!(
        inventory
            .weapon_slots
            .iter()
            .map(ItemDefinitionId::as_str)
            .collect::<Vec<_>>(),
        ["weapon/pistol", "weapon/shotgun", "weapon/fist"]
    );
    assert_eq!(
        session.weapon(PLAYER).unwrap().item.as_str(),
        "weapon/pistol"
    );

    let pickups = session.pickups().collect::<Vec<_>>();
    let quantities = |item: &str| {
        let mut quantities = pickups
            .iter()
            .filter(|pickup| {
                pickup.config.item.as_str() == item && pickup.state == PickupState::Available
            })
            .map(|pickup| pickup.config.quantity)
            .collect::<Vec<_>>();
        quantities.sort_unstable();
        quantities
    };
    assert_eq!(quantities("ammo/bullets"), [10, 10, 50]);
    assert_eq!(quantities("ammo/shells"), [4, 4, 20, 20, 20]);
    let shotgun = pickups
        .iter()
        .find(|pickup| {
            pickup.config.item.as_str() == "weapon/shotgun"
                && pickup.state == PickupState::Available
        })
        .expect("single-player shotgun pickup");
    assert_eq!(
        shotgun
            .config
            .starter_ammunition
            .as_ref()
            .map(|stack| (stack.item.as_str(), stack.quantity)),
        Some(("ammo/shells", 8))
    );
    assert!(!pickups.iter().any(|pickup| matches!(
        pickup.config.item.as_str(),
        "weapon/rivet-carbine" | "weapon/kinetic-launcher"
    )));
}

#[test]
fn authored_fist_is_selectable_and_damages_at_zero_ammunition_cost() {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    let entities = project["scenes"][0]["entities"].as_array_mut().unwrap();
    let target_id = EntityId::new(
        entities
            .iter()
            .find(|entity| entity["enemy"] == true)
            .unwrap()["id"]
            .as_u64()
            .unwrap(),
    );
    for entity in entities.iter_mut() {
        entity.as_object_mut().unwrap().remove("enemyCombat");
        entity.as_object_mut().unwrap().remove("navigation");
        entity.as_object_mut().unwrap().remove("encounter");
    }
    entities
        .iter_mut()
        .find(|entity| entity["id"] == target_id.raw())
        .unwrap()["translation"] = serde_json::json!([114.0, 9.25, 80.0]);

    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();
    let generation = game_loop.start_connection().connection_generation;
    game_loop
        .submit_edge_command(GameLoopEdgeCommand {
            connection_generation: generation,
            sequence: 1,
            command: GameLoopEdgeCommandKind::SelectWeaponSlot { slot: 2 },
        })
        .unwrap();
    game_loop.run_fixed_tick().unwrap();
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .weapon(PLAYER)
            .unwrap()
            .item
            .as_str(),
        "weapon/fist"
    );

    game_loop.submit_input(input(generation, 2, true)).unwrap();
    let tick = game_loop.run_fixed_tick().unwrap();
    assert!(
        tick.facts.iter().any(|fact| matches!(
            fact,
            GameLoopFact::Combat(CombatFact::AttackHit { target, .. })
                if *target == target_id
        )),
        "fist facts: {:#?}",
        tick.facts
    );
    assert!(
        game_loop
            .runtime()
            .session()
            .health(target_id)
            .unwrap()
            .current
            < 60
    );
    assert_eq!(quantity(&game_loop, "ammo/bullets"), 50);
}

#[test]
fn held_semantic_fire_obeys_e1m1_cadence_and_dry_fire_without_extra_mutation() {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    for entity in project["scenes"][0]["entities"].as_array_mut().unwrap() {
        entity.as_object_mut().unwrap().remove("enemyCombat");
        entity.as_object_mut().unwrap().remove("navigation");
    }
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();
    let generation = game_loop.start_connection().connection_generation;
    let mut fired = 0;
    for sequence in 1..=60 {
        game_loop
            .submit_input(input(generation, sequence, true))
            .unwrap();
        fired += game_loop
            .run_fixed_tick()
            .unwrap()
            .facts
            .iter()
            .filter(|fact| matches!(fact, GameLoopFact::Combat(CombatFact::AttackFired { weapon, .. }) if weapon.as_str() == "weapon/pistol"))
            .count();
    }
    assert_eq!(fired, 3);
    assert_eq!(quantity(&game_loop, "ammo/bullets"), 47);

    let mut dry_project = project;
    let player = dry_project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == PLAYER.raw())
        .unwrap();
    player["inventory"]["startingStacks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|stack| stack["item"] == "ammo/bullets")
        .unwrap()["quantity"] = 1.into();
    let mut dry = LoadingBayGameLoop::new(
        GameRuntime::from_stored_project(&dry_project.to_string()).unwrap(),
        PLAYER,
    )
    .unwrap();
    let generation = dry.start_connection().connection_generation;
    let mut fired = 0;
    let mut rejected = 0;
    for sequence in 1..=30 {
        dry.submit_input(input(generation, sequence, true)).unwrap();
        for fact in dry.run_fixed_tick().unwrap().facts {
            match fact {
                GameLoopFact::Combat(CombatFact::AttackFired { .. }) => fired += 1,
                GameLoopFact::CombatRejected {
                    reason: CombatRejectionReason::NoAmmo,
                    ..
                } => rejected += 1,
                _ => {}
            }
        }
    }
    assert_eq!(fired, 1);
    assert!(rejected > 0);
    assert_eq!(quantity(&dry, "ammo/bullets"), 0);
}

#[test]
fn overlapping_the_authored_shotgun_grants_shells_and_numeric_selection_equips_it() {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    let entities = project["scenes"][0]["entities"].as_array_mut().unwrap();
    let shotgun_position = entities
        .iter()
        .find(|entity| {
            entity["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("doom-pickup-2001-"))
        })
        .unwrap()["translation"]
        .clone();
    let player = entities
        .iter_mut()
        .find(|entity| entity["id"] == PLAYER.raw())
        .unwrap();
    player["translation"] = shotgun_position;
    for entity in entities {
        entity.as_object_mut().unwrap().remove("enemyCombat");
        entity.as_object_mut().unwrap().remove("navigation");
    }

    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();
    let generation = game_loop.start_connection().connection_generation;
    game_loop.run_fixed_tick().unwrap();
    assert_eq!(quantity(&game_loop, "weapon/shotgun"), 1);
    assert_eq!(quantity(&game_loop, "ammo/shells"), 8);

    game_loop
        .submit_edge_command(GameLoopEdgeCommand {
            connection_generation: generation,
            sequence: 1,
            command: GameLoopEdgeCommandKind::SelectWeaponSlot { slot: 1 },
        })
        .unwrap();
    game_loop.run_fixed_tick().unwrap();
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .weapon(PLAYER)
            .unwrap()
            .item
            .as_str(),
        "weapon/shotgun"
    );

    let mut fired = 0;
    for sequence in 2..=120 {
        game_loop
            .submit_input(input(generation, sequence, true))
            .unwrap();
        fired += game_loop
            .run_fixed_tick()
            .unwrap()
            .facts
            .iter()
            .filter(|fact| matches!(fact, GameLoopFact::Combat(CombatFact::AttackFired { weapon, ray_count: 7, .. }) if weapon.as_str() == "weapon/shotgun"))
            .count();
    }
    assert_eq!(fired, 2);
    assert_eq!(quantity(&game_loop, "ammo/shells"), 6);
}

fn input(generation: u64, sequence: u64, primary_fire_held: bool) -> PlayerInputCommand {
    PlayerInputCommand {
        connection_generation: generation,
        sequence,
        intent: PlayerInputIntent {
            movement: [0.0, 0.0],
            look_delta: [0.0, 0.0],
            primary_fire_held,
        },
    }
}

fn quantity(game_loop: &LoadingBayGameLoop, item: &str) -> u32 {
    game_loop
        .runtime()
        .session()
        .inventory(PLAYER)
        .unwrap()
        .stacks
        .iter()
        .find(|stack| stack.item.as_str() == item)
        .map_or(0, |stack| stack.quantity)
}
