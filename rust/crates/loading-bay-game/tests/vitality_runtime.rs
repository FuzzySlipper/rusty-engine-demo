use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, DamageCommand, DamageDisposition, DamageService,
    DamageSource, ExplosivePropError, ExplosivePropFact, GameEvent, GameRuntime,
    GameplayProgramOutcomeStatus, InventoryAction, InventoryCommand, InventoryService,
    ItemDefinitionId, RuntimeError, VitalityFact, VitalityState,
};
use rusty_engine::core_ids::EntityId;
use rusty_engine::gameplay_mechanics::{
    ActiveEffectInstance, ActiveEffectsComponent, EffectInstanceId, OperationId, SourceInstanceId,
    SourceInstanceIdentity,
};
use serde_json::{json, Value};

const PROJECT: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");
const PLAYER: EntityId = EntityId::new(1);

#[test]
fn e1m1_armor_absorbs_damage_and_lethal_damage_is_exactly_once() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut session = runtime.session().clone();
    let armor = ItemDefinitionId::parse("armor/green").unwrap();
    InventoryService::apply(
        &mut session,
        PLAYER,
        InventoryCommand {
            sequence: 1,
            action: InventoryAction::Grant {
                item: armor.clone(),
                quantity: 1,
            },
        },
    )
    .unwrap();
    DamageService::grant_armor(&mut session, PLAYER, armor).unwrap();

    let hit = DamageService::apply(
        &mut session,
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: PLAYER,
            amount: 30,
        },
    )
    .unwrap();
    assert_eq!(hit.disposition, DamageDisposition::Applied);
    assert!(hit.facts.iter().any(|fact| matches!(
        fact,
        VitalityFact::DamageApplied { armor_absorbed, .. } if *armor_absorbed > 0
    )));

    let lethal = DamageService::apply(
        &mut session,
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: PLAYER,
            amount: 1_000,
        },
    )
    .unwrap();
    assert_eq!(session.health(PLAYER).unwrap().state, VitalityState::Dead);
    assert_eq!(
        lethal
            .facts
            .iter()
            .filter(|fact| matches!(fact, VitalityFact::Died { entity, .. } if *entity == PLAYER))
            .count(),
        1
    );
    assert!(matches!(lethal.event, Some(GameEvent::PlayerDied { .. })));
}

#[test]
fn armor_replace_uses_the_standard_effect_path_and_survives_reopen_exactly() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut session = runtime.session().clone();
    let green = ItemDefinitionId::parse("armor/green").unwrap();
    let blue = ItemDefinitionId::parse("armor/blue").unwrap();
    let bonus = ItemDefinitionId::parse("armor/bonus").unwrap();

    grant_inventory(&mut session, 1, green.clone());
    DamageService::grant_armor(&mut session, PLAYER, green.clone()).unwrap();
    let green_effect = active_armor_effect(&session);

    grant_inventory(&mut session, 3, bonus.clone());
    DamageService::grant_armor(&mut session, PLAYER, bonus.clone()).unwrap();
    let preserved_effect = active_armor_effect(&session);
    assert_eq!(
        session.health(PLAYER).unwrap().armor_item.as_ref(),
        Some(&green)
    );
    assert_eq!(preserved_effect.definition(), green_effect.definition());
    assert_eq!(
        preserved_effect.provenance(),
        &SourceInstanceIdentity::Request {
            operation: OperationId::parse("grant-armor-1-4").unwrap(),
            instance: SourceInstanceId::parse("armor-effect").unwrap(),
        }
    );

    grant_inventory(&mut session, 5, blue.clone());
    DamageService::grant_armor(&mut session, PLAYER, blue).unwrap();
    let blue_effect = active_armor_effect(&session);
    assert_eq!(blue_effect.instance().as_str(), "armor");
    assert_ne!(blue_effect.definition(), green_effect.definition());
    assert_eq!(blue_effect.stacks(), 1);
    assert_eq!(
        blue_effect.provenance(),
        &SourceInstanceIdentity::Request {
            operation: OperationId::parse("grant-armor-1-6").unwrap(),
            instance: SourceInstanceId::parse("armor-effect").unwrap(),
        }
    );

    grant_inventory(&mut session, 7, bonus.clone());
    DamageService::grant_armor(&mut session, PLAYER, bonus).unwrap();
    assert_eq!(session.health(PLAYER).unwrap().armor, 200);
    assert_eq!(active_armor_effect(&session), blue_effect);

    let mut runtime = runtime;
    *runtime.session_mut() = session;
    let reopened = decode_game_snapshot(&encode_game_snapshot(&runtime).unwrap()).unwrap();
    assert_eq!(active_armor_effect(reopened.session()), blue_effect);
}

#[test]
fn reject_different_armor_transition_leaves_the_product_session_unchanged() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    project["itemDefinitions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["id"] == "armor/bonus")
        .unwrap()["kind"]["transition"] = json!("rejectDifferent");
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    let green = ItemDefinitionId::parse("armor/green").unwrap();
    let bonus = ItemDefinitionId::parse("armor/bonus").unwrap();

    grant_inventory(runtime.session_mut(), 1, green.clone());
    DamageService::grant_armor(runtime.session_mut(), PLAYER, green).unwrap();
    grant_inventory(runtime.session_mut(), 3, bonus.clone());
    let before = encode_game_snapshot(&runtime).unwrap();

    assert!(matches!(
        DamageService::grant_armor(runtime.session_mut(), PLAYER, bonus),
        Err(loading_bay_game::VitalityRejection::ArmorItemConflict { .. })
    ));
    assert_eq!(encode_game_snapshot(&runtime).unwrap(), before);
}

#[test]
fn e1m1_health_supply_restores_damage_after_rust_owned_inventory_grant() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut session = runtime.session().clone();
    let supply = ItemDefinitionId::parse("supply/medikit").unwrap();
    InventoryService::apply(
        &mut session,
        PLAYER,
        InventoryCommand {
            sequence: 1,
            action: InventoryAction::Grant {
                item: supply.clone(),
                quantity: 1,
            },
        },
    )
    .unwrap();
    DamageService::apply(
        &mut session,
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: PLAYER,
            amount: 20,
        },
    )
    .unwrap();
    let healed = DamageService::use_health_supply(&mut session, PLAYER, supply).unwrap();
    assert!(healed
        .facts
        .iter()
        .any(|fact| matches!(fact, VitalityFact::HealthRestored { .. })));
    assert_eq!(session.health(PLAYER).unwrap().current, 100);
}

#[test]
fn explicit_health_use_executes_the_bound_authored_program() {
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let supply = ItemDefinitionId::parse("supply/medikit").unwrap();
    runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 1,
                action: InventoryAction::Grant {
                    item: supply.clone(),
                    quantity: 1,
                },
            },
        )
        .unwrap();
    DamageService::apply(
        runtime.session_mut(),
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: PLAYER,
            amount: 20,
        },
    )
    .unwrap();

    let receipt = runtime.use_health_supply(PLAYER, supply).unwrap();

    assert!(receipt
        .facts
        .iter()
        .any(|fact| matches!(fact, VitalityFact::HealthRestored { amount: 20, .. })));
    assert_eq!(runtime.session().health(PLAYER).unwrap().current, 100);
    let outcome = runtime
        .session()
        .gameplay_outcome()
        .expect("explicit health use records its selected program");
    assert_eq!(outcome.program_id, "item/health-supply");
    assert_eq!(outcome.status, GameplayProgramOutcomeStatus::Applied);
    assert!(outcome
        .executed_operations
        .iter()
        .any(|operation| operation == "use-health-supply"));
}

#[test]
fn e1m1_dead_vitality_state_round_trips_exactly() {
    let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
    let mut session = runtime.session().clone();
    DamageService::apply(
        &mut session,
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: PLAYER,
            amount: 1_000,
        },
    )
    .unwrap();
    let mut runtime = runtime;
    *runtime.session_mut() = session;
    let encoded = encode_game_snapshot(&runtime).unwrap();
    let reopened = decode_game_snapshot(&encoded).unwrap();
    assert_eq!(
        reopened.session().health(PLAYER).unwrap().state,
        VitalityState::Dead
    );
    assert_eq!(encode_game_snapshot(&reopened).unwrap(), encoded);
}

#[test]
fn canonical_nukage_program_applies_damage_then_holds_its_authored_cooldown() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    let hazard_position = first_environment_position(&project, "hazard");
    set_player_translation(&mut project, hazard_position);
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();

    let first = runtime.run_hazard_phase(PLAYER).unwrap();
    assert_eq!(runtime.session().health(PLAYER).unwrap().current, 95);
    assert!(first.facts.iter().any(|fact| matches!(
        fact,
        loading_bay_game::HazardFact::Damage(VitalityFact::DamageApplied {
            source: DamageSource::Hazard { .. },
            target,
            health_damage: 5,
            ..
        }) if *target == PLAYER
    )));

    runtime.begin_fixed_tick();
    runtime.run_hazard_phase(PLAYER).unwrap();
    assert_eq!(runtime.session().health(PLAYER).unwrap().current, 95);
    assert!(runtime.session().gameplay_outcome().is_none());
}

#[test]
fn authored_hazard_program_without_cooldown_repeats_rust_damage_on_next_tick() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    let hazard_position = first_environment_position(&project, "hazard");
    set_player_translation(&mut project, hazard_position);
    project["hazardPrograms"][0]["program"] = json!({
        "kind": "when",
        "predicate": "playerOverlapping",
        "thenProgram": {
            "kind": "when",
            "predicate": "playerEligible",
            "thenProgram": {
                "kind": "when",
                "predicate": "cooldownReady",
                "thenProgram": { "kind": "operation", "operation": "applyHazardDamage" }
            }
        }
    });
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();

    runtime.run_hazard_phase(PLAYER).unwrap();
    runtime.begin_fixed_tick();
    runtime.run_hazard_phase(PLAYER).unwrap();

    assert_eq!(runtime.session().health(PLAYER).unwrap().current, 90);
    assert!(runtime.session().gameplay_outcome().is_none());
}

#[test]
fn environmental_program_bindings_reject_other_families_at_admission() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    set_environment_program(&mut project, "hazard", "explosive-prop/barrel");
    let error = GameRuntime::from_stored_project(&project.to_string()).unwrap_err();
    let RuntimeError::StoredProject(error) = error else {
        panic!("wrong-family hazard binding must fail project admission");
    };
    assert!(error.diagnostic().path.ends_with(".hazard.program"));
    assert!(error
        .diagnostic()
        .message
        .contains("wrong-family hazard program"));

    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    set_environment_program(&mut project, "explosiveProp", "hazard/nukage");
    let error = GameRuntime::from_stored_project(&project.to_string()).unwrap_err();
    let RuntimeError::StoredProject(error) = error else {
        panic!("wrong-family explosive prop binding must fail project admission");
    };
    assert!(error.diagnostic().path.ends_with(".explosiveProp.program"));
    assert!(error
        .diagnostic()
        .message
        .contains("wrong-family explosive-prop program"));
}

#[test]
fn canonical_barrel_program_preserves_rust_radial_damage_and_pending_chain_resolution() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    let (first_barrel, position) = first_environment_id_and_position(&project, "explosiveProp");
    let chained_barrel = second_environment_id(&project, "explosiveProp");
    set_entity_translation(&mut project, chained_barrel, position.clone());
    set_player_translation(&mut project, position);
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();

    DamageService::apply(
        runtime.session_mut(),
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: EntityId::new(first_barrel),
            amount: 20,
        },
    )
    .unwrap();
    let receipt = runtime.run_explosive_prop_phase().unwrap();

    assert!(receipt.facts.iter().any(|fact| matches!(
        fact,
        ExplosivePropFact::Triggered { prop, source: DamageSource::Explosion { source } }
            if prop.raw() == chained_barrel && source.raw() == first_barrel
    )));
    for prop in [first_barrel, chained_barrel] {
        assert!(receipt.facts.iter().any(|fact| matches!(
            fact,
            ExplosivePropFact::ExplosionResolved { prop: resolved } if resolved.raw() == prop
        )));
        assert!(
            !runtime
                .session()
                .explosive_prop(EntityId::new(prop))
                .unwrap()
                .pending
        );
    }
    assert!(receipt
        .damage
        .iter()
        .any(|damage| damage.facts.iter().any(|fact| matches!(
            fact,
            VitalityFact::DamageApplied {
                source: DamageSource::Explosion { source }, target, ..
            } if source.raw() == first_barrel && target.raw() == chained_barrel
        ))));
}

#[test]
fn resolve_only_explosive_variant_resolves_without_rust_radial_damage() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    let (barrel, position) = first_environment_id_and_position(&project, "explosiveProp");
    set_player_translation(&mut project, position);
    project["explosivePropPrograms"][0]["program"] = json!({
        "kind": "when",
        "predicate": "explosionPending",
        "thenProgram": { "kind": "operation", "operation": "resolveExplosion" }
    });
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    DamageService::apply(
        runtime.session_mut(),
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: EntityId::new(barrel),
            amount: 20,
        },
    )
    .unwrap();

    let receipt = runtime.run_explosive_prop_phase().unwrap();

    assert!(receipt.damage.is_empty());
    assert_eq!(runtime.session().health(PLAYER).unwrap().current, 100);
    assert!(
        !runtime
            .session()
            .explosive_prop(EntityId::new(barrel))
            .unwrap()
            .pending
    );
}

#[test]
fn explosive_source_order_rejection_and_unresolved_program_roll_back_the_phase_candidate() {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    let (barrel, position) = first_environment_id_and_position(&project, "explosiveProp");
    set_player_translation(&mut project, position);
    project["explosivePropPrograms"][0]["program"] = json!({
        "kind": "when",
        "predicate": "explosionPending",
        "thenProgram": {
            "kind": "sequence",
            "steps": [
                { "kind": "operation", "operation": "applyScaledDamage" },
                { "kind": "operation", "operation": "resolveExplosion" }
            ]
        }
    });
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    DamageService::apply(
        runtime.session_mut(),
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: EntityId::new(barrel),
            amount: 20,
        },
    )
    .unwrap();
    let error = runtime.run_explosive_prop_phase().unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::ExplosiveProp(ExplosivePropError::ApplyBeforeTargetSelection { prop }) if prop.raw() == barrel
    ));
    assert_eq!(runtime.session().health(PLAYER).unwrap().current, 100);
    assert!(
        runtime
            .session()
            .explosive_prop(EntityId::new(barrel))
            .unwrap()
            .pending
    );

    project["explosivePropPrograms"][0]["program"] = json!({
        "kind": "when",
        "predicate": "explosionPending",
        "thenProgram": {
            "kind": "sequence",
            "steps": [
                { "kind": "operation", "operation": "selectRadialTargets" },
                { "kind": "operation", "operation": "applyScaledDamage" }
            ]
        }
    });
    let mut runtime = GameRuntime::from_stored_project(&project.to_string()).unwrap();
    DamageService::apply(
        runtime.session_mut(),
        DamageCommand {
            source: DamageSource::Direct { actor: PLAYER },
            target: EntityId::new(barrel),
            amount: 20,
        },
    )
    .unwrap();
    let error = runtime.run_explosive_prop_phase().unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::ExplosiveProp(ExplosivePropError::ProgramLeftPending { prop, .. }) if prop.raw() == barrel
    ));
    assert_eq!(runtime.session().health(PLAYER).unwrap().current, 100);
    assert!(
        runtime
            .session()
            .explosive_prop(EntityId::new(barrel))
            .unwrap()
            .pending
    );
}

fn first_environment_position(project: &Value, component: &str) -> Value {
    first_environment_id_and_position(project, component).1
}

fn first_environment_id_and_position(project: &Value, component: &str) -> (u64, Value) {
    project["scenes"][0]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity.get(component).is_some())
        .map(|entity| {
            (
                entity["id"].as_u64().unwrap(),
                entity["translation"].clone(),
            )
        })
        .unwrap()
}

fn second_environment_id(project: &Value, component: &str) -> u64 {
    project["scenes"][0]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entity| entity.get(component).is_some())
        .nth(1)
        .unwrap()["id"]
        .as_u64()
        .unwrap()
}

fn set_player_translation(project: &mut Value, position: Value) {
    set_entity_translation(project, PLAYER.raw(), position);
}

fn set_entity_translation(project: &mut Value, entity_id: u64, position: Value) {
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"].as_u64() == Some(entity_id))
        .unwrap()["translation"] = position;
}

fn set_environment_program(project: &mut Value, component: &str, program: &str) {
    project["scenes"][0]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity.get(component).is_some())
        .unwrap()[component]["program"] = json!(program);
}

fn grant_inventory(
    session: &mut loading_bay_game::GameSession,
    sequence: u64,
    item: ItemDefinitionId,
) {
    InventoryService::apply(
        session,
        PLAYER,
        InventoryCommand {
            sequence,
            action: InventoryAction::Grant { item, quantity: 1 },
        },
    )
    .unwrap();
}

fn active_armor_effect(session: &loading_bay_game::GameSession) -> ActiveEffectInstance {
    let armor = EffectInstanceId::parse("armor").unwrap();
    session
        .entities()
        .component::<ActiveEffectsComponent>(PLAYER)
        .unwrap()
        .unwrap()
        .effects()
        .iter()
        .find(|effect| effect.instance() == &armor)
        .cloned()
        .expect("admitted armor effect remains active")
}
