use loading_bay_game::{
    decode_game_snapshot, diagnostic_code, encode_game_snapshot, EnemyAttackKind, EnemyCombatFact,
    EnemyCombatPosture, EnemyDropState, GameLoopFact, GameRuntime, LoadingBayGameLoop,
    RuntimeError,
};
use rusty_engine::core_ids::EntityId;
use std::collections::BTreeSet;

const PROJECT: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");
const PLAYER: EntityId = EntityId::new(1);

#[test]
fn e1m1_authors_projectile_and_hitscan_enemies_as_sleeping_runtime_authority() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let projectile = enemy_with_attack_kind("projectile");
    let hitscan = enemy_with_attack_kind("rangedHitscan");

    assert_eq!(
        runtime
            .session()
            .enemy_combat(projectile)
            .unwrap()
            .config
            .attack
            .kind,
        EnemyAttackKind::Projectile
    );
    assert_eq!(
        runtime
            .session()
            .enemy_combat(hitscan)
            .unwrap()
            .config
            .attack
            .kind,
        EnemyAttackKind::RangedHitscan
    );
    assert_eq!(
        runtime
            .session()
            .enemy_combat(projectile)
            .unwrap()
            .config
            .attack_program,
        "enemy-attack/projectile"
    );
    assert_eq!(
        runtime
            .session()
            .enemy_combat(hitscan)
            .unwrap()
            .config
            .defeat_program,
        "enemy-defeat/with-drop"
    );
    let programs = runtime.session().enemy_programs();
    assert_eq!(programs.attack.programs.len(), 2);
    assert_eq!(programs.defeat.programs.len(), 2);
    assert_eq!(programs.attack.bindings.len(), 29);
    assert_eq!(programs.defeat.bindings.len(), 29);
    assert!(programs
        .attack
        .bindings
        .iter()
        .any(|binding| binding.enemy == projectile.raw()));
    for enemy in [projectile, hitscan] {
        let combat = runtime.session().enemy_combat(enemy).unwrap();
        assert_eq!(combat.state.posture, EnemyCombatPosture::Sleeping);
        assert!(combat.config.attack.damage > 0);
        assert!(combat.config.attack.cooldown_ticks > 0);
        assert!(runtime.session().health(enemy).is_some());
        assert!(runtime.session().enemy(enemy).is_some());
    }
}

#[test]
fn enemy_program_bindings_reject_missing_and_wrong_family_ids() {
    let enemy = enemy_with_attack_kind("rangedHitscan");
    let mut wrong_family: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    entity_mut(&mut wrong_family, enemy)["enemyCombat"]["attackProgram"] =
        "enemy-defeat/with-drop".into();

    let RuntimeError::StoredProject(error) =
        GameRuntime::from_stored_project(&wrong_family.to_string()).unwrap_err()
    else {
        panic!("wrong family id did not fail admission");
    };
    assert_eq!(error.diagnostic().code, diagnostic_code::INVALID_VALUE);
    assert_eq!(
        error.diagnostic().path,
        format!(
            "scenes[0].entities[{}].enemyCombat.attackProgram",
            entity_index(enemy)
        )
    );

    let mut missing: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    entity_mut(&mut missing, enemy)["enemyCombat"]
        .as_object_mut()
        .unwrap()
        .remove("defeatProgram");
    assert!(GameRuntime::from_stored_project(&missing.to_string()).is_err());
}

#[test]
fn canonical_hitscan_and_projectile_enemy_programs_preserve_attack_and_cooldown() {
    for kind in ["rangedHitscan", "projectile"] {
        let (mut runtime, enemy) = runtime_with_immediately_attacking_enemy(kind);
        let first = runtime.run_enemy_attack_phase(PLAYER).unwrap();
        assert!(first.facts.iter().any(|fact| matches!(
            fact,
            EnemyCombatFact::AttackFired { enemy: observed, .. } if *observed == enemy
        )));
        if kind == "projectile" {
            assert!(first.facts.iter().any(|fact| matches!(
                fact,
                EnemyCombatFact::ProjectileSpawned { enemy: observed, .. } if *observed == enemy
            )));
        } else {
            assert!(first.facts.iter().any(|fact| matches!(
                fact,
                EnemyCombatFact::AttackHit { enemy: observed, .. } if *observed == enemy
            )));
        }
        let ready_at = runtime
            .session()
            .enemy_combat(enemy)
            .unwrap()
            .state
            .ready_at_tick;
        assert!(ready_at.raw() > 0, "the authored cooldown must be applied");
        let repeated = runtime.run_enemy_attack_phase(PLAYER).unwrap();
        assert!(!repeated.facts.iter().any(|fact| matches!(
            fact,
            EnemyCombatFact::AttackFired { enemy: observed, .. } if *observed == enemy
        )));
    }
}

#[test]
fn authored_projectile_program_variant_can_omit_spawn_without_rust_changes() {
    let (mut project, enemy) = project_with_isolated_enemy("projectile");
    let program = project["enemyAttackPrograms"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|program| program["id"] == "enemy-attack/projectile")
        .unwrap();
    program["program"] = serde_json::json!({
        "kind": "sequence",
        "steps": [
            { "kind": "operation", "operation": "recordEnemyAttack" },
            { "kind": "operation", "operation": "setEnemyCooldown" }
        ]
    });
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    ready_enemy_to_attack(&mut runtime, enemy);

    let receipt = runtime.run_enemy_attack_phase(PLAYER).unwrap();
    assert!(receipt.facts.iter().any(|fact| matches!(
        fact,
        EnemyCombatFact::AttackFired { enemy: observed, .. } if *observed == enemy
    )));
    assert!(!receipt.facts.iter().any(|fact| matches!(
        fact,
        EnemyCombatFact::ProjectileSpawned { enemy: observed, .. } if *observed == enemy
    )));
    assert!(
        runtime
            .session()
            .enemy_combat(enemy)
            .unwrap()
            .state
            .ready_at_tick
            .raw()
            > 0
    );
}

#[test]
fn failed_later_enemy_program_operation_cannot_leak_a_staged_projectile() {
    let (mut project, enemy) = project_with_isolated_enemy("projectile");
    let program = project["enemyAttackPrograms"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|program| program["id"] == "enemy-attack/projectile")
        .unwrap();
    program["program"] = serde_json::json!({
        "kind": "sequence",
        "steps": [
            { "kind": "operation", "operation": "recordEnemyAttack" },
            { "kind": "operation", "operation": "spawnEnemyProjectile" },
            { "kind": "operation", "operation": "spawnEnemyProjectile" },
            { "kind": "operation", "operation": "setEnemyCooldown" }
        ]
    });
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    ready_enemy_to_attack(&mut runtime, enemy);

    assert!(matches!(
        runtime.run_enemy_attack_phase(PLAYER),
        Err(RuntimeError::CombatResolutionFailed { .. })
    ));
    assert!(runtime
        .session()
        .entities()
        .entities()
        .all(|entity| entity.name != "enemy projectile"));
    assert_eq!(
        runtime
            .session()
            .enemy_combat(enemy)
            .unwrap()
            .state
            .ready_at_tick
            .raw(),
        0
    );
}

#[test]
fn failed_later_enemy_rolls_back_the_whole_attack_phase_and_projectile_service() {
    let (mut project, [first, rejecting]) = project_with_two_projectile_enemies();
    project["enemyAttackPrograms"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": "enemy-attack/projectile-reject",
            "program": {
                "kind": "sequence",
                "steps": [
                    { "kind": "operation", "operation": "recordEnemyAttack" },
                    { "kind": "operation", "operation": "spawnEnemyProjectile" },
                    { "kind": "operation", "operation": "spawnEnemyProjectile" },
                    { "kind": "operation", "operation": "setEnemyCooldown" }
                ]
            }
        }));
    entity_mut(&mut project, rejecting)["enemyCombat"]["attackProgram"] =
        "enemy-attack/projectile-reject".into();
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    ready_enemies_to_attack(&mut runtime, [first, rejecting]);
    let before = encode_game_snapshot(&runtime).unwrap();
    let admitted = runtime
        .session()
        .entities()
        .entities()
        .map(|entity| entity.id)
        .collect::<BTreeSet<_>>();
    let reserved =
        serde_json::from_str::<serde_json::Value>(&encode_game_snapshot(&runtime).unwrap())
            .unwrap()["inventories"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|inventory| inventory["weaponEntities"].as_array().unwrap())
            .map(|mapping| EntityId::new(mapping["entity"].as_u64().unwrap()))
            .collect::<BTreeSet<_>>();
    let first_unreserved = (1..)
        .map(EntityId::new)
        .find(|entity| !admitted.contains(entity) && !reserved.contains(entity))
        .unwrap();
    let first_absent = (1..)
        .map(EntityId::new)
        .find(|entity| !admitted.contains(entity))
        .unwrap();
    assert!(
        reserved.contains(&first_absent),
        "E1M1's first ordinary free ID is a reserved-absent weapon slot"
    );

    assert!(matches!(
        runtime.run_enemy_attack_phase(PLAYER),
        Err(RuntimeError::CombatResolutionFailed { .. })
    ));
    assert_eq!(encode_game_snapshot(&runtime).unwrap(), before);
    assert!(runtime
        .session()
        .entities()
        .entities()
        .all(|entity| entity.name != "enemy projectile"));

    entity_mut(&mut project, rejecting)["enemyCombat"]["attackProgram"] =
        "enemy-attack/projectile".into();
    let repaired = loading_bay_game::decode_project_document(&project.to_string())
        .unwrap()
        .project;
    runtime
        .reattach_authored_gameplay_programs(&repaired)
        .unwrap();
    let receipt = runtime.run_enemy_attack_phase(PLAYER).unwrap();
    assert!(receipt.facts.iter().any(|fact| matches!(
        fact,
        EnemyCombatFact::ProjectileSpawned { enemy, projectile, .. }
            if *enemy == first && *projectile == first_unreserved
    )));
}

#[test]
fn defeat_program_controls_bound_drop_through_central_damage_service() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let enemy = enemy_with_defeat_program("enemy-defeat/with-drop");
    let drop = runtime.session().enemy_drop(enemy).unwrap();
    assert_eq!(drop.state, EnemyDropState::Armed);
    assert_eq!(
        runtime.session().pickup(drop.pickup).unwrap().state,
        loading_bay_game::PickupState::Dormant
    );

    let receipt = loading_bay_game::DamageService::apply(
        runtime.session_mut(),
        loading_bay_game::DamageCommand {
            source: loading_bay_game::DamageSource::Direct { actor: PLAYER },
            target: enemy,
            amount: 1_000,
        },
    )
    .unwrap();

    assert!(receipt.facts.iter().any(|fact| matches!(
        fact,
        loading_bay_game::VitalityFact::EnemyDefeatProgramRecorded { enemy: observed, program_id }
            if *observed == enemy && program_id == "enemy-defeat/with-drop"
    )));
    assert_eq!(
        runtime.session().enemy_drop(enemy).unwrap().state,
        EnemyDropState::Materialized
    );
    assert_eq!(
        runtime.session().pickup(drop.pickup).unwrap().state,
        loading_bay_game::PickupState::Available
    );

    let mut no_drop_variant: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    let no_drop_enemy = enemy_with_defeat_program("enemy-defeat/with-drop");
    entity_mut(&mut no_drop_variant, no_drop_enemy)["enemyCombat"]["defeatProgram"] =
        "enemy-defeat/without-drop".into();
    let mut runtime = GameRuntime::from_stored_project(&no_drop_variant.to_string()).unwrap();
    let bound_drop = runtime.session().enemy_drop(no_drop_enemy).unwrap();
    assert_eq!(bound_drop.state, EnemyDropState::Armed);
    assert_eq!(
        runtime.session().pickup(bound_drop.pickup).unwrap().state,
        loading_bay_game::PickupState::Dormant
    );
    let receipt = loading_bay_game::DamageService::apply(
        runtime.session_mut(),
        loading_bay_game::DamageCommand {
            source: loading_bay_game::DamageSource::Direct { actor: PLAYER },
            target: no_drop_enemy,
            amount: 1_000,
        },
    )
    .unwrap();
    assert!(receipt.enemy_drops.is_empty());
    assert!(receipt.facts.iter().any(|fact| matches!(
        fact,
        loading_bay_game::VitalityFact::EnemyDefeatProgramRecorded { enemy: observed, program_id }
            if *observed == no_drop_enemy && program_id == "enemy-defeat/without-drop"
    )));
    assert_eq!(
        runtime.session().enemy_drop(no_drop_enemy).unwrap().state,
        EnemyDropState::Armed
    );
    assert_eq!(
        runtime.session().pickup(bound_drop.pickup).unwrap().state,
        loading_bay_game::PickupState::Dormant
    );
}

#[test]
fn defeated_e1m1_enemy_stays_dead_and_never_reawakens() {
    let enemy = enemy_with_attack_kind("rangedHitscan");
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    runtime.defeat_enemy(PLAYER, enemy).unwrap();
    let defeated_at = runtime
        .session()
        .enemy(enemy)
        .unwrap()
        .entity_view
        .transform
        .unwrap()
        .translation;
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();

    for _ in 0..5 {
        let receipt = game_loop.run_fixed_tick().unwrap();
        assert!(!receipt.facts.iter().any(|fact| matches!(
            fact,
            GameLoopFact::EnemyCombat(EnemyCombatFact::Alerted { enemy: observed, .. })
                if *observed == enemy
        )));
    }
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .enemy_combat(enemy)
            .unwrap()
            .state
            .posture,
        EnemyCombatPosture::Dead
    );
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .enemy(enemy)
            .unwrap()
            .entity_view
            .transform
            .unwrap()
            .translation,
        defeated_at
    );
}

#[test]
fn invalid_e1m1_enemy_composition_fails_at_the_authored_component() {
    let enemy = enemy_with_attack_kind("rangedHitscan");
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    entity_mut(&mut project, enemy)["enemyCombat"]["attack"]["damage"] = 0.into();

    let RuntimeError::StoredProject(error) =
        GameRuntime::from_stored_project(&project.to_string()).unwrap_err()
    else {
        panic!("enemy combat composition did not fail through project admission");
    };
    assert_eq!(error.diagnostic().code, diagnostic_code::INVALID_COMPONENT);
    assert_eq!(
        error.diagnostic().path,
        format!("scenes[0].entities[{}].enemyCombat", entity_index(enemy))
    );
}

#[test]
fn snapshot_rejects_enemy_cooldown_beyond_authored_cadence() {
    let enemy = enemy_with_attack_kind("rangedHitscan");
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    let tick = snapshot["tick"].as_u64().unwrap();
    let combat = snapshot["entities"]["registeredComponents"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|component| component["typeId"] == "loading-bay.enemy-combat")
        .expect("enemy combat facts persist as durable components")["values"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|value| value["entity"] == enemy.raw())
        .unwrap();
    let cooldown = combat["value"]["attack"]["cooldownTicks"].as_u64().unwrap();
    combat["value"]["readyAtTick"] = (tick + cooldown + 1).into();

    assert!(matches!(
        decode_game_snapshot(&snapshot.to_string()),
        Err(loading_bay_game::GameSnapshotError::InvalidEnemyCombatState { entity })
            if entity == enemy.raw()
    ));
}

fn enemy_with_attack_kind(kind: &str) -> EntityId {
    let project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    let id = project["scenes"][0]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["enemyCombat"]["attack"]["kind"] == kind)
        .and_then(|entity| entity["id"].as_u64())
        .unwrap_or_else(|| panic!("E1M1 must author a {kind} enemy"));
    EntityId::new(id)
}

fn entity_index(id: EntityId) -> usize {
    let project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    project["scenes"][0]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .position(|entity| entity["id"] == id.raw())
        .unwrap()
}

fn entity_mut(project: &mut serde_json::Value, id: EntityId) -> &mut serde_json::Value {
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == id.raw())
        .unwrap()
}

fn enemy_with_defeat_program(program: &str) -> EntityId {
    let project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    let id = project["scenes"][0]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["enemyCombat"]["defeatProgram"] == program)
        .and_then(|entity| entity["id"].as_u64())
        .unwrap_or_else(|| panic!("E1M1 must author defeat program {program}"));
    EntityId::new(id)
}

fn runtime_with_immediately_attacking_enemy(kind: &str) -> (GameRuntime, EntityId) {
    let (project, enemy) = project_with_isolated_enemy(kind);
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    ready_enemy_to_attack(&mut runtime, enemy);
    (runtime, enemy)
}

fn project_with_isolated_enemy(kind: &str) -> (serde_json::Value, EntityId) {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    let player_position = project["scenes"][0]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["id"] == PLAYER.raw())
        .unwrap()["translation"]
        .clone();
    let enemy = enemy_with_attack_kind_in_project(&project, kind);
    for entity in project["scenes"][0]["entities"].as_array_mut().unwrap() {
        entity.as_object_mut().unwrap().remove("encounter");
        if entity["id"] == enemy.raw() {
            entity["translation"] = player_position.clone();
            entity["enemyCombat"]["sightRange"] = 100_000.into();
            entity["enemyCombat"]["hearingRange"] = 0.into();
        } else if entity.get("enemyCombat").is_some() {
            entity["enemyCombat"]["sightRange"] = 0.01.into();
            entity["enemyCombat"]["hearingRange"] = 0.into();
        }
    }
    (project, enemy)
}

fn project_with_two_projectile_enemies() -> (serde_json::Value, [EntityId; 2]) {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    let player_position = project["scenes"][0]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["id"] == PLAYER.raw())
        .unwrap()["translation"]
        .clone();
    let mut enemies = project["scenes"][0]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entity| entity["enemyCombat"]["attack"]["kind"] == "projectile")
        .filter_map(|entity| entity["id"].as_u64())
        .map(EntityId::new)
        .collect::<Vec<_>>();
    enemies.sort();
    let [first, rejecting, ..] = enemies.as_slice() else {
        panic!("E1M1 must author at least two projectile enemies");
    };
    for entity in project["scenes"][0]["entities"].as_array_mut().unwrap() {
        entity.as_object_mut().unwrap().remove("encounter");
        if [*first, *rejecting].contains(&EntityId::new(entity["id"].as_u64().unwrap())) {
            entity["translation"] = player_position.clone();
            entity["enemyCombat"]["sightRange"] = 100_000.into();
            entity["enemyCombat"]["hearingRange"] = 0.into();
        } else if entity.get("enemyCombat").is_some() {
            entity["enemyCombat"]["sightRange"] = 0.01.into();
            entity["enemyCombat"]["hearingRange"] = 0.into();
        }
    }
    (project, [*first, *rejecting])
}

fn enemy_with_attack_kind_in_project(project: &serde_json::Value, kind: &str) -> EntityId {
    let id = project["scenes"][0]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["enemyCombat"]["attack"]["kind"] == kind)
        .and_then(|entity| entity["id"].as_u64())
        .unwrap_or_else(|| panic!("E1M1 must author a {kind} enemy"));
    EntityId::new(id)
}

fn ready_enemy_to_attack(runtime: &mut GameRuntime, enemy: EntityId) {
    ready_enemies_to_attack(runtime, [enemy]);
}

fn ready_enemies_to_attack(runtime: &mut GameRuntime, enemies: impl IntoIterator<Item = EntityId>) {
    runtime
        .run_enemy_intent_and_motion_phase(PLAYER, 1.0 / 60.0)
        .unwrap();
    runtime
        .run_enemy_intent_and_motion_phase(PLAYER, 1.0 / 60.0)
        .unwrap();
    for enemy in enemies {
        assert_eq!(
            runtime.session().enemy_combat(enemy).unwrap().state.posture,
            EnemyCombatPosture::Attacking
        );
    }
}
