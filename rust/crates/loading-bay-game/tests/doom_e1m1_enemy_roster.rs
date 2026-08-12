use std::collections::BTreeSet;

use loading_bay_game::{EnemyAttackKind, GameRuntime};
use serde_json::Value;

const PROJECT: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");
const INTERMEDIATE: &str = include_str!("../../../../content/doom-e1m1/e1m1.intermediate.json");

#[test]
fn canonical_e1m1_has_the_uv_roster_source_rotations_and_barrels() {
    let project: Value = serde_json::from_str(PROJECT).unwrap();
    let intermediate: Value = serde_json::from_str(INTERMEDIATE).unwrap();
    let entities = project["scenes"][0]["entities"].as_array().unwrap();
    let source_things = intermediate["level"]["things"].as_array().unwrap();

    let authored_enemies = entities
        .iter()
        .filter(|entity| entity["enemy"] == true)
        .collect::<Vec<_>>();
    let source_enemies = source_things
        .iter()
        .filter(|thing| {
            matches!(thing["type"].as_u64(), Some(9 | 3001 | 3004))
                && thing["options"]
                    .as_u64()
                    .is_some_and(|bits| bits & 4 != 0 && bits & 16 == 0)
        })
        .collect::<Vec<_>>();
    assert_eq!(authored_enemies.len(), 29);
    assert_eq!(authored_enemies.len(), source_enemies.len());
    for (authored, source) in authored_enemies.iter().zip(source_enemies) {
        let radians = -(source["angle"].as_f64().unwrap() * std::f64::consts::PI) / 180.0;
        let rotation = authored["rotation"]
            .as_array()
            .map(|rotation| [rotation[1].as_f64().unwrap(), rotation[3].as_f64().unwrap()])
            .unwrap_or([0.0, 1.0]);
        assert!((rotation[0] - (radians / 2.0).sin()).abs() < 1e-6);
        assert!((rotation[1] - (radians / 2.0).cos()).abs() < 1e-6);
    }

    let authored_barrels = entities
        .iter()
        .filter(|entity| !entity["explosiveProp"].is_null())
        .collect::<Vec<_>>();
    let source_barrels = source_things
        .iter()
        .filter(|thing| {
            thing["type"] == 2035 && thing["options"].as_u64().is_some_and(|bits| bits & 4 != 0)
        })
        .collect::<Vec<_>>();
    assert_eq!(authored_barrels.len(), 6);
    assert_eq!(authored_barrels.len(), source_barrels.len());
}

#[test]
fn admitted_roster_preserves_archetypes_drops_encounters_and_explosions() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let session = runtime.session();
    let project: Value = serde_json::from_str(PROJECT).unwrap();
    let authored_entities = project["scenes"][0]["entities"].as_array().unwrap();
    let mut counts = [0_usize; 3];

    for enemy in session.enemy_combatants() {
        let health = session.health(enemy.entity).unwrap();
        let drop = session.enemy_drop(enemy.entity);
        let name = session.entity(enemy.entity).unwrap().name;
        if name.starts_with("doom-shotgun-guy-") {
            counts[0] += 1;
            assert_eq!(
                (health.config.max, enemy.config.pain_duration_ticks),
                (30, 10)
            );
            assert_eq!(
                (
                    enemy.config.attack.kind,
                    enemy.config.attack.damage,
                    enemy.config.attack.cooldown_ticks
                ),
                (EnemyAttackKind::RangedHitscan, 15, 51)
            );
            let pickup = session.pickup(drop.unwrap().pickup).unwrap();
            assert_eq!(pickup.config.item.as_str(), "weapon/shotgun");
            assert_eq!(pickup.config.starter_ammunition.unwrap().quantity, 4);
        } else if name.starts_with("doom-imp-") {
            counts[1] += 1;
            assert_eq!(
                (health.config.max, enemy.config.pain_duration_ticks),
                (60, 7)
            );
            assert_eq!(
                (
                    enemy.config.attack.kind,
                    enemy.config.attack.damage,
                    enemy.config.attack.cooldown_ticks
                ),
                (EnemyAttackKind::Projectile, 12, 38)
            );
            assert!(enemy.config.attack.projectile.is_some());
            assert!(drop.is_none());
        } else if name.starts_with("doom-zombieman-") {
            counts[2] += 1;
            assert_eq!(
                (health.config.max, enemy.config.pain_duration_ticks),
                (20, 10)
            );
            assert_eq!(
                (
                    enemy.config.attack.kind,
                    enemy.config.attack.damage,
                    enemy.config.attack.cooldown_ticks
                ),
                (EnemyAttackKind::RangedHitscan, 9, 45)
            );
            let pickup = session.pickup(drop.unwrap().pickup).unwrap();
            assert_eq!(pickup.config.item.as_str(), "ammo/bullets");
            assert_eq!(pickup.config.quantity, 5);
        } else {
            panic!("unexpected E1M1 enemy {name}");
        }
    }
    assert_eq!(counts, [16, 4, 9]);

    let encounters = authored_entities
        .iter()
        .filter(|entity| {
            entity["name"]
                .as_str()
                .unwrap()
                .starts_with("doom-encounter-")
        })
        .map(|entity| {
            session
                .encounter(rusty_engine::core_ids::EntityId::new(
                    entity["id"].as_u64().unwrap(),
                ))
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(encounters.len(), 4);
    assert!(encounters.iter().all(|encounter| encounter.exit.is_none()));
    let members = encounters
        .iter()
        .flat_map(|encounter| encounter.members.iter().copied())
        .collect::<BTreeSet<_>>();
    assert_eq!(members.len(), 29);

    let barrels = authored_entities
        .iter()
        .filter(|entity| {
            entity["name"]
                .as_str()
                .unwrap()
                .starts_with("doom-explosive-barrel-")
        })
        .collect::<Vec<_>>();
    assert_eq!(barrels.len(), 6);
    for barrel in barrels {
        let entity = rusty_engine::core_ids::EntityId::new(barrel["id"].as_u64().unwrap());
        let prop = session.explosive_prop(entity).unwrap();
        assert_eq!((prop.config.damage, prop.config.radius), (128, 8.0));
        assert_eq!(session.health(entity).unwrap().config.max, 20);
    }
}
