mod support;

use loading_bay_game::{
    decode_game_snapshot, diagnostic_code, encode_game_snapshot, EncounterState, EnemyAttackKind,
    EnemyDropState, EnemyState, GameEvent, GameLoopFact, GameRuntime, LoadingBayGameLoop,
    PickupFact, PickupState, RuntimeError, MAX_EVENT_WAVE,
};
use rusty_engine::core_ids::EntityId;

const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const PLAYER: EntityId = EntityId::new(1);
const ENCOUNTER: EntityId = EntityId::new(2);
const MELEE: EntityId = EntityId::new(4);
const RANGED: EntityId = EntityId::new(5);
const MELEE_DROP: EntityId = EntityId::new(33);
const RANGED_DROP: EntityId = EntityId::new(34);

#[test]
fn authored_archetypes_stay_dormant_until_the_bounded_encounter_activates() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let encounter = runtime.session().encounter(ENCOUNTER).unwrap();
    let melee = runtime.session().enemy_combat(MELEE).unwrap();
    let ranged = runtime.session().enemy_combat(RANGED).unwrap();

    assert_eq!(encounter.state, EncounterState::Dormant);
    assert_eq!(encounter.activation_radius, Some(6.0));
    assert_eq!(melee.config.attack.kind, EnemyAttackKind::Melee);
    assert_eq!(ranged.config.attack.kind, EnemyAttackKind::RangedHitscan);
    assert_eq!(
        runtime
            .session()
            .entity(MELEE)
            .unwrap()
            .kinematic
            .unwrap()
            .half_extents
            .to_array(),
        [0.45, 0.25, 0.45]
    );
    assert_eq!(
        runtime
            .session()
            .entity(RANGED)
            .unwrap()
            .kinematic
            .unwrap()
            .half_extents
            .to_array(),
        [0.3, 0.5, 0.3]
    );
    assert_eq!(
        runtime
            .session()
            .enemy(MELEE)
            .unwrap()
            .entity_view
            .renderable
            .unwrap()
            .asset
            .as_str(),
        "mesh-animation/bay-rusher"
    );
    assert_eq!(
        runtime
            .session()
            .enemy(RANGED)
            .unwrap()
            .entity_view
            .renderable
            .unwrap()
            .asset
            .as_str(),
        "mesh-animation/arc-warden"
    );
    assert_eq!(
        runtime.session().enemy_drop(MELEE).unwrap().state,
        EnemyDropState::Armed
    );
    assert_eq!(
        runtime.session().enemy_drop(RANGED).unwrap().state,
        EnemyDropState::Armed
    );
    assert_eq!(
        runtime.session().pickup(MELEE_DROP).unwrap().state,
        PickupState::Dormant
    );
    assert_eq!(
        runtime.session().pickup(RANGED_DROP).unwrap().state,
        PickupState::Dormant
    );

    let mut outside = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();
    let receipt = outside.run_fixed_tick().unwrap();
    assert!(!receipt.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::EnemyCombat(_) | GameLoopFact::Event(GameEvent::EncounterActivated { .. })
    )));
    assert_eq!(
        outside
            .runtime()
            .session()
            .encounter(ENCOUNTER)
            .unwrap()
            .state,
        EncounterState::Dormant
    );

    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    entity_mut(&mut project, PLAYER)["translation"] = serde_json::json!([7.5, 1.5, 8.5]);
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut inside = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();
    let receipt = inside.run_fixed_tick().unwrap();
    assert!(receipt
        .facts
        .contains(&GameLoopFact::Event(GameEvent::EncounterActivated {
            encounter: ENCOUNTER,
            player: PLAYER,
        })));
    assert_eq!(
        inside
            .runtime()
            .session()
            .encounter(ENCOUNTER)
            .unwrap()
            .state,
        EncounterState::Active
    );
}

#[test]
fn lethal_damage_materializes_one_ordinary_drop_and_snapshots_every_state() {
    let mut project = single_melee_project();
    entity_mut(&mut project, MELEE)["translation"] = serde_json::json!([1.5, 1.5, 2.5]);
    entity_mut(&mut project, MELEE)["health"]["max"] = 60.into();
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();

    let armed = decode_game_snapshot(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    assert_eq!(
        armed.session().enemy_drop(MELEE).unwrap().state,
        EnemyDropState::Armed
    );
    assert_eq!(
        armed.session().pickup(MELEE_DROP).unwrap().state,
        PickupState::Dormant
    );

    // This is a drop-lifecycle proof, so use the bounded direct-defeat seam;
    // directional player hitscan targeting is covered by its own combat tests.
    let defeated = runtime.defeat_enemy(PLAYER, MELEE).unwrap();
    assert!(defeated.events.iter().any(|event| matches!(
        event,
        GameEvent::EnemyDefeated {
            enemy: MELEE,
            actor: PLAYER,
            ..
        }
    )));
    assert_eq!(
        runtime.session().enemy(MELEE).unwrap().state,
        EnemyState::Defeated
    );
    assert_eq!(
        runtime.session().enemy_drop(MELEE).unwrap().state,
        EnemyDropState::Materialized
    );
    assert_eq!(
        runtime.session().pickup(MELEE_DROP).unwrap().state,
        PickupState::Available
    );
    let drop_entity = runtime.session().entity(MELEE_DROP).unwrap();
    assert_eq!(
        drop_entity.transform.unwrap().translation.to_array(),
        [1.5, 1.5, 2.5]
    );
    assert!(drop_entity.renderable.unwrap().visible);

    let revision = runtime.readout().entity_revision;
    let repeated = runtime.defeat_enemy(PLAYER, MELEE).unwrap();
    assert!(repeated.events.is_empty());
    assert_eq!(runtime.readout().entity_revision, revision);
    assert_eq!(
        runtime.session().enemy_drop(MELEE).unwrap().state,
        EnemyDropState::Materialized
    );

    let reopened = decode_game_snapshot(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    assert_eq!(
        reopened.session().enemy_drop(MELEE).unwrap().state,
        EnemyDropState::Materialized
    );
    assert_eq!(
        reopened.session().pickup(MELEE_DROP).unwrap().state,
        PickupState::Available
    );

    let mut game_loop = LoadingBayGameLoop::new(reopened, PLAYER).unwrap();
    let collected = game_loop.run_fixed_tick().unwrap();
    assert!(collected.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::Pickup(PickupFact::Collected {
            pickup: MELEE_DROP,
            actor: PLAYER,
            ..
        })
    )));
    assert!(matches!(
        game_loop
            .runtime()
            .session()
            .pickup(MELEE_DROP)
            .unwrap()
            .state,
        PickupState::Collected {
            actor: PLAYER,
            collected_at_tick: _,
            cause: _,
        }
    ));
    assert_eq!(
        game_loop
            .runtime()
            .session()
            .inventory(PLAYER)
            .unwrap()
            .stacks
            .iter()
            .find(|stack| stack.item.as_str() == "supply/med-patch")
            .unwrap()
            .quantity,
        2
    );
    let reopened =
        decode_game_snapshot(&encode_game_snapshot(game_loop.runtime()).unwrap()).unwrap();
    assert_eq!(
        reopened.session().enemy_drop(MELEE).unwrap().state,
        EnemyDropState::Materialized
    );
    assert!(matches!(
        reopened.session().pickup(MELEE_DROP).unwrap().state,
        PickupState::Collected {
            actor: PLAYER,
            collected_at_tick: _,
            cause: _,
        }
    ));

    let reset = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    assert_eq!(
        reset.session().enemy_drop(MELEE).unwrap().state,
        EnemyDropState::Armed
    );
    assert_eq!(
        reset.session().pickup(MELEE_DROP).unwrap().state,
        PickupState::Dormant
    );
}

#[test]
fn legacy_snapshot_admission_fails_closed_for_archetype_state() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut future: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    future["schemaVersion"] = 17.into();
    support::strip_future_gameplay_mechanics_state(&mut future);
    assert!(matches!(
        decode_game_snapshot(&future.to_string()),
        Err(loading_bay_game::GameSnapshotError::FutureEnemyArchetypeStateInLegacySnapshot)
    ));

    let legacy_project = single_melee_project_without_archetype_fields();
    let runtime = GameRuntime::from_stored_project(&legacy_project.to_string()).unwrap();
    let mut legacy: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    legacy["schemaVersion"] = 17.into();
    support::strip_future_gameplay_mechanics_state(&mut legacy);
    assert!(decode_game_snapshot(&legacy.to_string()).is_ok());
}

#[test]
fn invalid_drop_relationships_fail_before_runtime_publication() {
    let mut duplicate: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    entity_mut(&mut duplicate, RANGED)["defeatDrop"]["pickup"] = MELEE_DROP.raw().into();
    let mut visible: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    entity_mut(&mut visible, MELEE_DROP)["renderable"]["visible"] = true.into();

    for project in [duplicate, visible] {
        let RuntimeError::StoredProject(error) =
            GameRuntime::from_stored_project(&project.to_string()).unwrap_err()
        else {
            panic!("invalid defeat-drop relationship reached a live runtime");
        };
        assert_eq!(
            error.diagnostic().code,
            diagnostic_code::INVALID_RELATIONSHIP
        );
        assert!(error.diagnostic().path.ends_with("defeatDrop.pickup"));
    }
}

#[test]
fn snapshot_rejects_a_dormant_pickup_without_its_drop_authority() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    snapshot["enemyDrops"] = serde_json::json!([]);

    assert!(matches!(
        decode_game_snapshot(&snapshot.to_string()),
        Err(loading_bay_game::GameSnapshotError::DormantPickupMissingEnemyDrop {
            pickup
        }) if pickup == MELEE_DROP.raw()
    ));
}

#[test]
fn encounter_activation_delivers_the_bounded_wave_exactly_once() {
    let project = project_with_overlapping_encounters(MAX_EVENT_WAVE);
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();

    let first = game_loop.run_fixed_tick().unwrap();
    let activated = first
        .facts
        .iter()
        .filter_map(|fact| match fact {
            GameLoopFact::Event(GameEvent::EncounterActivated { encounter, player })
                if *player == PLAYER =>
            {
                Some(*encounter)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(activated.len(), MAX_EVENT_WAVE);
    assert_eq!(
        activated
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        MAX_EVENT_WAVE
    );
    assert!(activated.iter().all(|encounter| {
        game_loop
            .runtime()
            .session()
            .encounter(*encounter)
            .is_some_and(|component| component.state == EncounterState::Active)
    }));

    let second = game_loop.run_fixed_tick().unwrap();
    assert!(!second.facts.iter().any(|fact| matches!(
        fact,
        GameLoopFact::Event(GameEvent::EncounterActivated { .. })
    )));
}

#[test]
fn encounter_activation_overflow_preserves_all_encounters_and_the_journal() {
    let project = project_with_overlapping_encounters(MAX_EVENT_WAVE + 1);
    let runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let encounter_ids = (0..=MAX_EVENT_WAVE)
        .map(|index| EntityId::new(10_000 + index as u64))
        .collect::<Vec<_>>();
    let journal_before = runtime.readout().journal;
    let mut game_loop = LoadingBayGameLoop::new(runtime, PLAYER).unwrap();

    assert!(matches!(
        game_loop.run_fixed_tick(),
        Err(RuntimeError::EventWaveLimit {
            limit: MAX_EVENT_WAVE
        })
    ));
    assert!(encounter_ids.iter().all(|encounter| {
        game_loop
            .runtime()
            .session()
            .encounter(*encounter)
            .is_some_and(|component| component.state == EncounterState::Dormant)
    }));
    assert_eq!(game_loop.runtime().readout().journal, journal_before);
}

fn project_with_overlapping_encounters(count: usize) -> serde_json::Value {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    entity_mut(&mut project, PLAYER)["translation"] = serde_json::json!([1.5, 1.5, 3.5]);
    let encounter = entity_mut(&mut project, ENCOUNTER).clone();
    let enemy = entity_mut(&mut project, MELEE).clone();
    let entities = project["scenes"][0]["entities"].as_array_mut().unwrap();
    entities.retain(|entity| entity["encounter"].is_null());
    for index in 0..count {
        let enemy_id = 20_000 + index as u64;
        let mut member = enemy.clone();
        member["id"] = enemy_id.into();
        member["name"] = format!("bounded-member-{index}").into();
        member["translation"] = serde_json::json!([20_000.0 + index as f64, 1.5, 20_000.0]);
        let member = member.as_object_mut().unwrap();
        for component in ["defeatDrop", "enemyCombat", "navigation"] {
            member.remove(component);
        }
        entities.push(serde_json::Value::Object(member.clone()));

        let mut instance = encounter.clone();
        instance["id"] = (10_000 + index as u64).into();
        instance["name"] = format!("bounded-encounter-{index}").into();
        instance["translation"] = serde_json::json!([1.5, 1.5, 3.5]);
        instance["encounter"]["members"] = serde_json::json!([enemy_id]);
        entities.push(instance);
    }
    project
}

fn single_melee_project() -> serde_json::Value {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).unwrap();
    const CAMPAIGN_ONLY: [u64; 11] = [40, 41, 42, 50, 51, 52, 53, 54, 60, 61, 62];
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .retain(|entity| {
            let id = entity["id"].as_u64().unwrap();
            id != RANGED.raw()
                && id != RANGED_DROP.raw()
                && !CAMPAIGN_ONLY.contains(&id)
                && !(63..=65).contains(&id)
        });
    entity_mut(&mut project, PLAYER)["translation"] = serde_json::json!([1.5, 1.5, 2.5]);
    let encounter = entity_mut(&mut project, ENCOUNTER);
    encounter["encounter"]["members"] = serde_json::json!([MELEE.raw()]);
    encounter["encounter"]["activationRadius"] = serde_json::Value::Null;
    project
}

fn single_melee_project_without_archetype_fields() -> serde_json::Value {
    let mut project = single_melee_project();
    entity_mut(&mut project, MELEE)
        .as_object_mut()
        .unwrap()
        .remove("defeatDrop");
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .retain(|entity| entity["id"] != MELEE_DROP.raw());
    project
}

fn entity_mut(project: &mut serde_json::Value, id: EntityId) -> &mut serde_json::Value {
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == id.raw())
        .unwrap()
}
