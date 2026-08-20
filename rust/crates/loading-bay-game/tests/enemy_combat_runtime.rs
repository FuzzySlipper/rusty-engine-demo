use loading_bay_game::{
    decode_game_snapshot, diagnostic_code, encode_game_snapshot, EnemyAttackKind, EnemyCombatFact,
    EnemyCombatPosture, GameLoopFact, GameRuntime, LoadingBayGameLoop, RuntimeError,
};
use rusty_engine::core_ids::EntityId;

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
    let combat = snapshot["enemyCombat"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|combat| combat["entity"] == enemy.raw())
        .unwrap();
    let cooldown = combat["cooldownTicks"].as_u64().unwrap();
    combat["readyAtTick"] = (tick + cooldown + 1).into();

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
