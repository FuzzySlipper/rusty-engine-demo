use loading_bay_game::{
    decode_game_snapshot, encode_game_snapshot, DamageCommand, DamageDisposition, DamageService,
    DamageSource, GameEvent, GameRuntime, InventoryAction, InventoryCommand, InventoryService,
    ItemDefinitionId, VitalityFact, VitalityState,
};
use rusty_engine::core_ids::EntityId;

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
