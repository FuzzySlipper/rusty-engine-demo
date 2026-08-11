use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, CombatFact, DamageSource, ExplosivePropFact,
    ExplosivePropState, GameRuntime, ResolvedAttackAction, VitalityFact,
};
use rusty_engine::core_ids::EntityId;
use serde_json::{json, Value};

const PLAYER: EntityId = EntityId::new(1);
const FIRST_PROP: EntityId = EntityId::new(2);
const NEAR_TARGET: EntityId = EntityId::new(3);
const FAR_TARGET: EntityId = EntityId::new(4);
const SECOND_PROP: EntityId = EntityId::new(5);

#[test]
fn lethal_player_hit_triggers_and_resolves_the_prop_once() {
    let mut runtime = runtime(project(100, 4.0, &[]));

    let hit = trigger_prop(&mut runtime);

    assert!(hit.facts.iter().any(|fact| matches!(
        fact,
        CombatFact::AttackHit {
            target: FIRST_PROP,
            damage: 100,
            ..
        }
    )));
    assert!(hit.facts.iter().any(|fact| matches!(
        fact,
        CombatFact::Vitality(VitalityFact::Died {
            entity: FIRST_PROP,
            source: DamageSource::Weapon {
                attacker: PLAYER,
                ..
            },
        })
    )));
    assert!(hit.facts.iter().any(|fact| matches!(
        fact,
        CombatFact::ExplosiveProp(ExplosivePropFact::Triggered {
            prop: FIRST_PROP,
            source: DamageSource::Weapon {
                attacker: PLAYER,
                ..
            },
        })
    )));
    assert_eq!(runtime.session().health(FIRST_PROP).unwrap().current, 0);
    assert_eq!(
        runtime.session().explosive_prop(FIRST_PROP).unwrap().state,
        ExplosivePropState::Exploded
    );
    assert!(
        runtime
            .session()
            .explosive_prop(FIRST_PROP)
            .unwrap()
            .pending
    );

    let phase = runtime.run_explosive_prop_phase().unwrap();

    assert_eq!(
        phase.facts,
        vec![
            ExplosivePropFact::ExplosionStarted {
                prop: FIRST_PROP,
                damage: 100,
                radius: 4.0,
            },
            ExplosivePropFact::ExplosionResolved { prop: FIRST_PROP },
        ]
    );
    assert!(phase.damage.is_empty());
    assert!(phase.events.is_empty());
    assert!(
        !runtime
            .session()
            .explosive_prop(FIRST_PROP)
            .unwrap()
            .pending
    );
}

#[test]
fn explosion_damage_falls_off_with_distance() {
    let mut authored = project(100, 10.0, &[]);
    add_entity(&mut authored, health_target(NEAR_TARGET, 5.5, 100));
    add_entity(&mut authored, health_target(FAR_TARGET, 8.5, 100));
    let mut runtime = runtime(authored);

    trigger_prop(&mut runtime);
    let phase = runtime.run_explosive_prop_phase().unwrap();

    assert_eq!(runtime.session().health(NEAR_TARGET).unwrap().current, 20);
    assert_eq!(runtime.session().health(FAR_TARGET).unwrap().current, 50);
    assert!(phase.facts.iter().any(|fact| matches!(
        fact,
        ExplosivePropFact::TargetDamaged {
            prop: FIRST_PROP,
            target: NEAR_TARGET,
            damage: 80,
            distance,
        } if (*distance - 2.0).abs() < f32::EPSILON
    )));
    assert!(phase.facts.iter().any(|fact| matches!(
        fact,
        ExplosivePropFact::TargetDamaged {
            prop: FIRST_PROP,
            target: FAR_TARGET,
            damage: 50,
            distance,
        } if (*distance - 5.0).abs() < f32::EPSILON
    )));
}

#[test]
fn voxel_geometry_occludes_a_target_behind_the_prop() {
    let mut authored = project(100, 10.0, &[[5, 0, 0]]);
    add_entity(&mut authored, health_target(NEAR_TARGET, 6.5, 100));
    let mut runtime = runtime(authored);

    trigger_prop(&mut runtime);
    let phase = runtime.run_explosive_prop_phase().unwrap();

    assert_eq!(runtime.session().health(NEAR_TARGET).unwrap().current, 100);
    assert!(phase.facts.iter().any(|fact| matches!(
        fact,
        ExplosivePropFact::TargetOccluded {
            prop: FIRST_PROP,
            target: NEAR_TARGET,
        }
    )));
    assert!(!phase.facts.iter().any(|fact| matches!(
        fact,
        ExplosivePropFact::TargetDamaged {
            target: NEAR_TARGET,
            ..
        }
    )));
}

#[test]
fn chained_props_resolve_each_explosion_exactly_once() {
    let mut authored = project(200, 10.0, &[]);
    add_entity(&mut authored, explosive_prop(SECOND_PROP, 5.5, 200, 10.0));
    let mut runtime = runtime(authored);

    let hit = trigger_prop(&mut runtime);
    let phase = runtime.run_explosive_prop_phase().unwrap();

    assert_eq!(
        hit.facts
            .iter()
            .filter(|fact| matches!(
                fact,
                CombatFact::ExplosiveProp(ExplosivePropFact::Triggered {
                    prop: FIRST_PROP,
                    ..
                })
            ))
            .count(),
        1
    );
    assert_eq!(
        phase
            .facts
            .iter()
            .filter(|fact| matches!(
                fact,
                ExplosivePropFact::Triggered {
                    prop: SECOND_PROP,
                    ..
                }
            ))
            .count(),
        1
    );
    for prop in [FIRST_PROP, SECOND_PROP] {
        assert_eq!(
            phase
                .facts
                .iter()
                .filter(|fact| matches!(
                    fact,
                    ExplosivePropFact::ExplosionStarted { prop: candidate, .. }
                        if *candidate == prop
                ))
                .count(),
            1
        );
        assert_eq!(
            phase
                .facts
                .iter()
                .filter(|fact| matches!(
                    fact,
                    ExplosivePropFact::ExplosionResolved { prop: candidate }
                        if *candidate == prop
                ))
                .count(),
            1
        );
        let view = runtime.session().explosive_prop(prop).unwrap();
        assert_eq!(view.state, ExplosivePropState::Exploded);
        assert!(!view.pending);
    }
    assert_eq!(runtime.session().health(SECOND_PROP).unwrap().current, 0);

    let repeat = runtime.run_explosive_prop_phase().unwrap();
    assert!(repeat.facts.is_empty());
    assert!(repeat.damage.is_empty());
    assert!(repeat.events.is_empty());
}

#[test]
fn snapshot_reopen_preserves_a_resolved_prop_and_does_not_retrigger_it() {
    let mut authored = project(100, 10.0, &[]);
    add_entity(&mut authored, health_target(NEAR_TARGET, 5.5, 100));
    let mut runtime = runtime(authored);
    trigger_prop(&mut runtime);
    runtime.run_explosive_prop_phase().unwrap();
    let snapshot = encode_game_snapshot(&runtime).unwrap();

    let mut reopened = decode_game_snapshot(&snapshot).unwrap();

    assert_eq!(encode_game_snapshot(&reopened).unwrap(), snapshot);
    assert_eq!(
        reopened.session().explosive_prop(FIRST_PROP),
        runtime.session().explosive_prop(FIRST_PROP)
    );
    assert_eq!(reopened.session().health(FIRST_PROP).unwrap().current, 0);
    assert_eq!(reopened.session().health(NEAR_TARGET).unwrap().current, 20);
    assert_eq!(
        reopened.session().explosive_prop(FIRST_PROP).unwrap().state,
        ExplosivePropState::Exploded
    );
    assert!(
        !reopened
            .session()
            .explosive_prop(FIRST_PROP)
            .unwrap()
            .pending
    );

    let repeat = reopened.run_explosive_prop_phase().unwrap();
    assert!(repeat.facts.is_empty());
    assert!(repeat.damage.is_empty());
    assert!(repeat.events.is_empty());
}

fn trigger_prop(runtime: &mut GameRuntime) -> loading_bay_game::CombatReceipt {
    let receipt = runtime
        .attack(PLAYER, ResolvedAttackAction::Attack)
        .expect("player attack should hit the armed prop");
    assert!(receipt.facts.iter().any(|fact| matches!(
        fact,
        CombatFact::AttackHit {
            target: FIRST_PROP,
            ..
        }
    )));
    receipt
}

fn runtime(project: Value) -> GameRuntime {
    GameRuntime::from_stored_project(&project.to_string()).expect("admit explosive-prop project")
}

fn project(prop_damage: u32, prop_radius: f32, solid_voxels: &[[i64; 3]]) -> Value {
    json!({
        "schemaVersion": 6,
        "entities": [
            {
                "id": 1,
                "name": "player",
                "translation": [0.5, 0.5, 0.5],
                "collision": { "enabled": true, "staticCollider": false },
                "renderable": { "asset": "primitive/player-marker", "visible": true },
                "kinematic": { "halfExtents": [0.2, 0.2, 0.2], "velocity": [0, 0, 0] },
                "playerController": {
                    "moveSpeedUnitsPerSecond": 4,
                    "moveStepSeconds": 0.1,
                    "lookDegreesPerUnit": 12,
                    "initialYawDegrees": -90,
                    "initialPitchDegrees": 0,
                    "bindings": {
                        "moveForward": "KeyW",
                        "moveBackward": "KeyS",
                        "moveLeft": "KeyA",
                        "moveRight": "KeyD",
                        "mouseLook": "pointer",
                        "primaryFire": "Mouse0"
                    }
                },
                "weapon": {
                    "damage": 100,
                    "maxDistance": 20,
                    "cooldownTicks": 0,
                    "ammoCapacity": 8,
                    "muzzleOffset": [0, 0, 0]
                }
            },
            {
                "id": 2,
                "name": "first-prop",
                "translation": [3.5, 0.5, 0.5],
                "collision": { "enabled": true, "staticCollider": false },
                "renderable": { "asset": "primitive/explosive-prop", "visible": true },
                "health": { "max": 100, "hitboxHalfExtents": [0.4, 0.4, 0.4] },
                "explosiveProp": { "damage": prop_damage, "radius": prop_radius }
            }
        ],
        "voxelCollision": {
            "voxelSize": 1,
            "chunkSize": 16,
            "solidVoxels": solid_voxels
        }
    })
}

fn health_target(entity: EntityId, x: f32, max: u32) -> Value {
    json!({
        "id": entity.raw(),
        "name": format!("target-{}", entity.raw()),
        "translation": [x, 0.5, 0.5],
        "collision": { "enabled": false, "staticCollider": false },
        "health": { "max": max, "hitboxHalfExtents": [0.4, 0.4, 0.4] }
    })
}

fn explosive_prop(entity: EntityId, x: f32, damage: u32, radius: f32) -> Value {
    json!({
        "id": entity.raw(),
        "name": format!("prop-{}", entity.raw()),
        "translation": [x, 0.5, 0.5],
        "collision": { "enabled": true, "staticCollider": false },
        "renderable": { "asset": "primitive/explosive-prop", "visible": true },
        "health": { "max": 100, "hitboxHalfExtents": [0.4, 0.4, 0.4] },
        "explosiveProp": { "damage": damage, "radius": radius }
    })
}

fn add_entity(project: &mut Value, entity: Value) {
    project["entities"]
        .as_array_mut()
        .expect("project entities")
        .push(entity);
}
