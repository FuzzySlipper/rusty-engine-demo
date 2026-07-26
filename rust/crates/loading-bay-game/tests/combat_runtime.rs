use core_ids::EntityId;
use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, CombatFact, CombatMissReason,
    CombatRejectionReason, DoorState, EncounterState, EnemyState, GameEntityDefinitionError,
    GameEvent, GameRuntime, ItemDefinitionId, ProjectContentError, ResolvedAttackAction,
    RuntimeError, VitalityFact,
};
use serde_json::{json, Value};

const PLAYER: EntityId = EntityId::new(1);
const ENCOUNTER: EntityId = EntityId::new(2);
const EXIT: EntityId = EntityId::new(3);
const ENEMY: EntityId = EntityId::new(4);

#[test]
fn aimed_attack_hits_live_target_and_applies_typed_damage() {
    let mut runtime = runtime(combat_project(-90.0, 35, 0, vec![[7, 7, 7]]));

    let receipt = attack(&mut runtime);

    assert_eq!(runtime.session().health(ENEMY).unwrap().current, 65);
    assert!(receipt.events.is_empty());
    assert!(receipt.facts.iter().any(|fact| matches!(
        fact,
        CombatFact::AttackHit { target, .. } if *target == ENEMY
    )));
    assert!(receipt.facts.iter().any(|fact| matches!(
        fact,
        CombatFact::Vitality(VitalityFact::DamageApplied {
            target,
            health_damage: 35,
            health_before: 100,
            health_after: 65,
            ..
        }) if *target == ENEMY
    )));
}

#[test]
fn accepted_attack_can_miss_without_mutating_health() {
    let mut runtime = runtime(combat_project(0.0, 35, 0, vec![[7, 7, 7]]));

    let receipt = attack(&mut runtime);

    assert_eq!(runtime.session().health(ENEMY).unwrap().current, 100);
    assert!(receipt.facts.iter().any(|fact| matches!(
        fact,
        CombatFact::AttackMissed {
            reason: CombatMissReason::NoTarget,
            ..
        }
    )));
}

#[test]
fn canonical_voxel_geometry_occludes_a_target_behind_it() {
    let mut runtime = runtime(combat_project(-90.0, 35, 0, vec![[1, 0, 0]]));

    let receipt = attack(&mut runtime);

    assert_eq!(runtime.session().health(ENEMY).unwrap().current, 100);
    assert!(receipt.facts.iter().any(|fact| matches!(
        fact,
        CombatFact::AttackMissed {
            reason: CombatMissReason::WorldBlocked,
            ..
        }
    )));
}

#[test]
fn cooldown_rejects_atomically_until_authoritative_tick_is_ready() {
    let mut runtime = runtime(combat_project(-90.0, 30, 2, vec![[7, 7, 7]]));
    attack(&mut runtime);
    let health_after_first = runtime.session().health(ENEMY).unwrap().current;
    let weapon_after_first = runtime.session().weapon(PLAYER).unwrap();

    let error = runtime
        .attack(PLAYER, ResolvedAttackAction::Attack)
        .unwrap_err();

    assert!(matches!(
        error,
        RuntimeError::CombatRejected {
            entity: PLAYER,
            reason: CombatRejectionReason::Cooldown,
        }
    ));
    assert_eq!(
        runtime.session().health(ENEMY).unwrap().current,
        health_after_first
    );
    assert_eq!(
        runtime.session().weapon(PLAYER).unwrap(),
        weapon_after_first
    );

    runtime.advance_by(2).unwrap();
    attack(&mut runtime);
    assert_eq!(runtime.session().health(ENEMY).unwrap().current, 40);
}

#[test]
fn exhausted_authored_ammo_rejects_without_a_second_health_mutation() {
    let mut project = combat_project(-90.0, 30, 0, vec![[7, 7, 7]]);
    entity_mut(&mut project, PLAYER.raw())["weapon"]["ammoCapacity"] = json!(1);
    let mut runtime = runtime(project);
    attack(&mut runtime);

    let error = runtime
        .attack(PLAYER, ResolvedAttackAction::Attack)
        .unwrap_err();

    assert!(matches!(
        error,
        RuntimeError::CombatRejected {
            entity: PLAYER,
            reason: CombatRejectionReason::NoAmmo,
        }
    ));
    assert_eq!(runtime.session().health(ENEMY).unwrap().current, 70);
    assert_eq!(inventory_quantity(&runtime, "ammo/energy-cell"), 0);
}

#[test]
fn malformed_health_and_weapon_components_fail_during_admission() {
    let mut invalid_health = combat_project(-90.0, 30, 0, vec![[7, 7, 7]]);
    entity_mut(&mut invalid_health, ENEMY.raw())["health"]["max"] = json!(0);
    assert!(matches!(
        GameRuntime::from_project_content(&invalid_health.to_string()),
        Err(RuntimeError::Content(ProjectContentError::Definition(
            GameEntityDefinitionError::InvalidHealthConfig { entity: ENEMY }
        )))
    ));

    let mut invalid_weapon = combat_project(-90.0, 30, 0, vec![[7, 7, 7]]);
    entity_mut(&mut invalid_weapon, PLAYER.raw())["weapon"]["damage"] = json!(0);
    assert!(matches!(
        GameRuntime::from_project_content(&invalid_weapon.to_string()),
        Err(RuntimeError::Content(ProjectContentError::Definition(
            GameEntityDefinitionError::InvalidWeaponConfig { entity: PLAYER }
        )))
    ));
}

#[test]
fn repeated_lethal_attacks_emit_defeat_and_encounter_consequences_once() {
    let mut runtime = runtime(combat_project(-90.0, 100, 0, vec![[7, 7, 7]]));

    let lethal = attack(&mut runtime);
    let repeated = attack(&mut runtime);

    assert_eq!(
        runtime.session().enemy(ENEMY).unwrap().state,
        EnemyState::Defeated
    );
    assert_eq!(runtime.session().health(ENEMY).unwrap().current, 0);
    assert_eq!(
        runtime.session().encounter(ENCOUNTER).unwrap().state,
        EncounterState::Cleared
    );
    assert_eq!(runtime.session().door(EXIT).unwrap().state, DoorState::Open);
    assert!(lethal
        .facts
        .iter()
        .any(|fact| matches!(fact, CombatFact::EnemyDefeated { enemy, .. } if *enemy == ENEMY)));
    assert_eq!(
        lethal
            .events
            .iter()
            .filter(|event| matches!(event, GameEvent::EnemyDefeated { .. }))
            .count(),
        1
    );
    assert!(repeated.events.is_empty());
    assert!(repeated.facts.iter().any(|fact| matches!(
        fact,
        CombatFact::AttackMissed {
            reason: CombatMissReason::NoTarget,
            ..
        }
    )));
    assert_eq!(
        runtime
            .readout()
            .journal
            .iter()
            .filter(|entry| matches!(entry.event, GameEvent::EnemyDefeated { .. }))
            .count(),
        1
    );
}

#[test]
fn attack_queries_the_moving_enemys_current_transform() {
    let mut project = combat_project(-90.0, 30, 0, vec![[7, 7, 7]]);
    let enemy = entity_mut(&mut project, ENEMY.raw());
    enemy["kinematic"] = json!({
        "halfExtents": [0.25, 0.25, 0.25],
        "velocity": [0, 0, 0]
    });
    enemy["navigation"] = json!({
        "goal": [5.5, 0.5, 0.5],
        "speedUnitsPerSecond": 2,
        "maxVisited": 512
    });
    let mut runtime = runtime(project);
    let before = enemy_position(&runtime);

    runtime.run_navigation_phase(0.1).unwrap();
    let after = enemy_position(&runtime);
    let receipt = attack(&mut runtime);

    assert!(after.x > before.x);
    assert!(receipt
        .facts
        .iter()
        .any(|fact| matches!(fact, CombatFact::AttackHit { target, .. } if *target == ENEMY)));
    assert_eq!(runtime.session().health(ENEMY).unwrap().current, 70);
}

#[test]
fn save_reopen_preserves_partial_health_and_weapon_eligibility_then_clears_encounter() {
    let mut uninterrupted = runtime(combat_project(-90.0, 60, 2, vec![[7, 7, 7]]));
    attack(&mut uninterrupted);
    let encoded = encode_game_snapshot(&uninterrupted).unwrap();
    let mut reopened = decode_game_snapshot(&encoded).unwrap();

    assert_eq!(
        uninterrupted.session().health(ENEMY),
        reopened.session().health(ENEMY)
    );
    assert_eq!(
        uninterrupted.session().weapon(PLAYER),
        reopened.session().weapon(PLAYER)
    );
    assert!(matches!(
        reopened.attack(PLAYER, ResolvedAttackAction::Attack),
        Err(RuntimeError::CombatRejected {
            reason: CombatRejectionReason::Cooldown,
            ..
        })
    ));

    uninterrupted.advance_by(2).unwrap();
    reopened.advance_by(2).unwrap();
    let expected = attack(&mut uninterrupted);
    let actual = attack(&mut reopened);

    assert_eq!(expected.facts, actual.facts);
    assert_eq!(expected.events, actual.events);
    assert_eq!(reopened.session().health(ENEMY).unwrap().current, 0);
    assert_eq!(
        reopened.session().enemy(ENEMY).unwrap().state,
        EnemyState::Defeated
    );
    assert_eq!(
        reopened.session().encounter(ENCOUNTER).unwrap().state,
        EncounterState::Cleared
    );
    assert_eq!(
        reopened.session().door(EXIT).unwrap().state,
        DoorState::Open
    );
}

fn attack(runtime: &mut GameRuntime) -> loading_bay_game::CombatReceipt {
    runtime
        .attack(PLAYER, ResolvedAttackAction::Attack)
        .expect("accepted attack")
}

fn runtime(project: Value) -> GameRuntime {
    GameRuntime::from_project_content(&project.to_string()).expect("admit combat project")
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

fn combat_project(
    initial_yaw_degrees: f32,
    damage: u32,
    cooldown_ticks: u64,
    solid_voxels: Vec<[i64; 3]>,
) -> Value {
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
                    "initialYawDegrees": initial_yaw_degrees,
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
                    "damage": damage,
                    "maxDistance": 10,
                    "cooldownTicks": cooldown_ticks,
                    "ammoCapacity": 8,
                    "muzzleOffset": [0, 0, 0]
                }
            },
            {
                "id": 2,
                "name": "encounter",
                "encounter": { "members": [4], "exit": 3 }
            },
            {
                "id": 3,
                "name": "exit",
                "translation": [6.5, 0.5, 0.5],
                "collision": { "enabled": true, "staticCollider": true },
                "renderable": { "asset": "mesh/security-door", "visible": true },
                "door": { "openTranslation": [6.5, 3.5, 0.5], "autoCloseAfterTicks": null }
            },
            {
                "id": 4,
                "name": "sentry",
                "translation": [3.5, 0.5, 0.5],
                "collision": { "enabled": true, "staticCollider": false },
                "renderable": { "asset": "mesh/security-sentry", "visible": true },
                "enemy": true,
                "health": { "max": 100, "hitboxHalfExtents": [0.4, 0.4, 0.4] }
            }
        ],
        "voxelCollision": {
            "voxelSize": 1,
            "chunkSize": 8,
            "solidVoxels": solid_voxels
        }
    })
}

fn enemy_position(runtime: &GameRuntime) -> core_math::Vec3 {
    runtime
        .session()
        .enemy(ENEMY)
        .unwrap()
        .entity_view
        .transform
        .unwrap()
        .translation
}

fn entity_mut(project: &mut Value, id: u64) -> &mut Value {
    project["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == id)
        .unwrap()
}
