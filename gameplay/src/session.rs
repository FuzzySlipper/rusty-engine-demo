use std::collections::{BTreeMap, BTreeSet};

use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::core_time::{Tick, TickDelta};
use rusty_engine::engine_spatial::MAX_TRIGGER_DEFINITIONS;
use rusty_engine::entity_state::{
    EntityAuthoringService, EntityComponent, EntityState, EntityView,
};

use crate::combat::{EnemyComponent, EnemyState, EnemyView, WeaponState, WeaponView};
use crate::definition::{GameEntityDefinition, GameEntityDefinitionError};
use crate::door::{DoorComponent, DoorState, DoorView, DEFAULT_DOOR_MOTION_DURATION_TICKS};
use crate::encounter::{
    EncounterComponent, EncounterService, EncounterState, EncounterView,
    MAX_ENCOUNTER_ACTIVATION_RADIUS,
};
use crate::encounter_program::{EncounterProgramCatalog, EncounterProgramReadout};
use crate::enemy_combat::{
    EnemyCombatComponent, EnemyCombatPosture, EnemyCombatState, EnemyCombatView,
};
use crate::enemy_drop::{EnemyDropComponent, EnemyDropState, EnemyDropView};
use crate::enemy_program::{
    attack_program_operations, defeat_program_activates_bound_drop, enemy_program_readout,
    EnemyAttackOperation, EnemyAttackProgramCatalog, EnemyDefeatProgramCatalog,
    EnemyProgramReadout,
};
use crate::explosive_prop::{ExplosivePropComponent, ExplosivePropState, ExplosivePropView};
use crate::explosive_prop_program::{ExplosivePropProgramCatalog, ExplosivePropProgramReadout};
use crate::extraction_beacon::{
    ExtractionBeaconComponent, ExtractionBeaconState, ExtractionBeaconView,
};
use crate::floor_action::{FloorActionComponent, FloorActionState, FloorActionView};
use crate::floor_action_program::{FloorActionProgramCatalog, FloorActionProgramReadout};
use crate::gameplay_program::{
    GameplayProgramCatalog, GameplayProgramOutcome, GameplayProgramReadout,
};
use crate::hazard::{HazardComponent, HazardView};
use crate::hazard_program::{HazardProgramCatalog, HazardProgramReadout};
use crate::interaction::{switch_is_available, SwitchComponent, SwitchEffect, SwitchView};
use crate::inventory::{
    admit_item_definitions, inventory_from_config, inventory_view, InventoryView, ItemDefinition,
    ItemDefinitionId, ItemDefinitionView,
};
use crate::level_exit_program::{LevelExitProgramCatalog, LevelExitProgramReadout};
use crate::lift::{LiftComponent, LiftState, LiftView};
use crate::lift_program::{LiftProgramCatalog, LiftProgramReadout};
use crate::mechanics::{self, InventoryRuntime, MechanicsRuntime};
use crate::navigation::{
    NavigationComponent, NavigationState, NavigationView, MAX_NAVIGATION_QUERY_BUDGET,
    MAX_NAVIGATION_SPEED_UNITS_PER_SECOND,
};
use crate::pickup::{pickup_view, PickupComponent, PickupState, PickupView};
use crate::pickup_program::{PickupProgramCatalog, PickupProgramReadout};
use crate::player::{PlayerControllerComponent, PlayerControllerView};
use crate::player_program::{PlayerSetupProgramCatalog, PlayerSetupProgramReadout};
use crate::progression::{
    DoorAccessConfig, DoorAccessView, LevelExitComponent, LevelExitState, LevelExitView,
    LoadingBayInterlockConfig, LoadingBayInterlockView, SecretRegionComponent, SecretRegionState,
    SecretRegionView,
};
use crate::secret_program::{SecretProgramCatalog, SecretProgramReadout};
use crate::switch_program::{SwitchProgramCatalog, SwitchProgramReadout};
use crate::vitality::{HealthConfig, HealthView, VitalityState};

#[derive(Debug, Clone)]
pub struct GameSession {
    pub(crate) entities: EntityState,
    /// Switch-to-controlled-target relationships. Retained downstream because the
    /// Engine relationship model has no generic named-relationship kind; this is a
    /// product policy index, not an inert entity fact.
    pub(crate) controls: BTreeMap<EntityId, Vec<EntityId>>,
    pub(crate) item_definitions: BTreeMap<ItemDefinitionId, ItemDefinition>,
    /// Service-owned scheduling and capacity cache over inventory facts that already
    /// live in Engine components; not a second durable store.
    pub(crate) inventories: BTreeMap<EntityId, InventoryRuntime>,
    /// Collection receipts for pickups whose entities the Engine destroy path
    /// retired. A collected pickup is a tombstoned entity, so its fact no longer
    /// exists in the component store; this downstream ledger keeps the
    /// product-visible collection consequence observable.
    pub(crate) collected_pickups: BTreeMap<EntityId, PickupComponent>,
    pub(crate) mechanics: MechanicsRuntime,
    pub(crate) gameplay_programs: GameplayProgramCatalog,
    pub(crate) pickup_programs: PickupProgramCatalog,
    pub(crate) player_setup_programs: PlayerSetupProgramCatalog,
    pub(crate) player_setup_bindings: BTreeMap<EntityId, String>,
    pub(crate) enemy_attack_programs: EnemyAttackProgramCatalog,
    pub(crate) enemy_defeat_programs: EnemyDefeatProgramCatalog,
    pub(crate) hazard_programs: HazardProgramCatalog,
    pub(crate) hazard_program_bindings: BTreeMap<EntityId, String>,
    pub(crate) encounter_programs: EncounterProgramCatalog,
    pub(crate) encounter_program_bindings: BTreeMap<EntityId, String>,
    pub(crate) explosive_prop_programs: ExplosivePropProgramCatalog,
    pub(crate) explosive_prop_program_bindings: BTreeMap<EntityId, String>,
    pub(crate) switch_programs: SwitchProgramCatalog,
    pub(crate) switch_program_bindings: BTreeMap<EntityId, String>,
    pub(crate) floor_action_programs: FloorActionProgramCatalog,
    pub(crate) floor_action_program_bindings: BTreeMap<EntityId, String>,
    pub(crate) lift_programs: LiftProgramCatalog,
    pub(crate) lift_program_bindings: BTreeMap<EntityId, String>,
    pub(crate) secret_programs: SecretProgramCatalog,
    pub(crate) secret_program_bindings: BTreeMap<EntityId, String>,
    pub(crate) level_exit_programs: LevelExitProgramCatalog,
    pub(crate) level_exit_program_bindings: BTreeMap<EntityId, String>,
    /// One latest-value product readout; intentionally absent in a new session
    /// and never persisted as a history or replay spine.
    pub(crate) gameplay_outcome: Option<GameplayProgramOutcome>,
}

/// The already-compiled closed catalogs attached to one independent session.
/// This groups fixed family fields only; it is not a dynamic registry.
#[derive(Debug, Clone, Default)]
pub(crate) struct SessionProgramCatalogs {
    pub(crate) gameplay: GameplayProgramCatalog,
    pub(crate) pickup: PickupProgramCatalog,
    pub(crate) player_setup: PlayerSetupProgramCatalog,
    pub(crate) player_setup_bindings: BTreeMap<EntityId, String>,
    pub(crate) enemy_attack: EnemyAttackProgramCatalog,
    pub(crate) enemy_defeat: EnemyDefeatProgramCatalog,
    pub(crate) hazard: HazardProgramCatalog,
    pub(crate) hazard_bindings: BTreeMap<EntityId, String>,
    pub(crate) encounter: EncounterProgramCatalog,
    pub(crate) encounter_bindings: BTreeMap<EntityId, String>,
    pub(crate) explosive_prop: ExplosivePropProgramCatalog,
    pub(crate) explosive_prop_bindings: BTreeMap<EntityId, String>,
    pub(crate) switch: SwitchProgramCatalog,
    pub(crate) switch_bindings: BTreeMap<EntityId, String>,
    pub(crate) floor_action: FloorActionProgramCatalog,
    pub(crate) floor_action_bindings: BTreeMap<EntityId, String>,
    pub(crate) lift: LiftProgramCatalog,
    pub(crate) lift_bindings: BTreeMap<EntityId, String>,
    pub(crate) secret: SecretProgramCatalog,
    pub(crate) secret_bindings: BTreeMap<EntityId, String>,
    pub(crate) level_exit: LevelExitProgramCatalog,
    pub(crate) level_exit_bindings: BTreeMap<EntityId, String>,
}

impl GameSession {
    /// Product-reserved weapon identities are not always admitted Engine entities. Live product
    /// allocators must still treat them as occupied so a transient entity can never be mistaken
    /// later for a reserved unique weapon.
    pub(crate) fn is_reserved_weapon_entity(&self, entity: EntityId) -> bool {
        self.inventories.values().any(|inventory| {
            inventory
                .weapon_entities
                .values()
                .any(|reserved| *reserved == entity)
        })
    }

    pub(crate) fn from_item_entity_and_gameplay_programs(
        item_definitions: impl IntoIterator<Item = ItemDefinition>,
        definitions: impl IntoIterator<Item = GameEntityDefinition>,
        program_catalogs: SessionProgramCatalogs,
        vitality_policy: crate::DoomVitalityPolicy,
    ) -> Result<Self, GameEntityDefinitionError> {
        let SessionProgramCatalogs {
            gameplay: gameplay_programs,
            pickup: pickup_programs,
            player_setup: player_setup_programs,
            player_setup_bindings,
            enemy_attack: enemy_attack_programs,
            enemy_defeat: enemy_defeat_programs,
            hazard: hazard_programs,
            hazard_bindings: hazard_program_bindings,
            encounter: encounter_programs,
            encounter_bindings: encounter_program_bindings,
            explosive_prop: explosive_prop_programs,
            explosive_prop_bindings: explosive_prop_program_bindings,
            switch: switch_programs,
            switch_bindings: switch_program_bindings,
            floor_action: floor_action_programs,
            floor_action_bindings: floor_action_program_bindings,
            lift: lift_programs,
            lift_bindings: lift_program_bindings,
            secret: secret_programs,
            secret_bindings: secret_program_bindings,
            level_exit: level_exit_programs,
            level_exit_bindings: level_exit_program_bindings,
        } = program_catalogs;
        let definitions: Vec<GameEntityDefinition> = definitions.into_iter().collect();
        let trigger_count = definitions
            .iter()
            .filter(|definition| {
                definition.pickup.is_some()
                    || definition.hazard.is_some()
                    || definition.secret_region.is_some()
                    || definition.floor_action.is_some()
                    || definition.lift.is_some()
            })
            .count();
        if trigger_count > MAX_TRIGGER_DEFINITIONS {
            return Err(GameEntityDefinitionError::TooManyPickups {
                count: trigger_count,
                limit: MAX_TRIGGER_DEFINITIONS,
            });
        }
        let item_definitions = admit_item_definitions(item_definitions)
            .map_err(GameEntityDefinitionError::Inventory)?;
        let destructible_integrity_maximum = mechanics::destructible_integrity_capacity(
            definitions
                .iter()
                .filter(|definition| definition.explosive_prop.is_some())
                .filter_map(|definition| definition.health.map(|config| config.max)),
        );
        let mechanics = mechanics::build_runtime(
            &item_definitions,
            vitality_policy,
            destructible_integrity_maximum,
        )
        .map_err(|reason| GameEntityDefinitionError::Mechanics { reason })?;
        let inventory_configs = definitions
            .iter()
            .filter_map(|definition| {
                definition
                    .inventory
                    .as_ref()
                    .map(|config| (definition.entity.id, config.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        for (owner, config) in &inventory_configs {
            inventory_from_config(*owner, config, &item_definitions)
                .map_err(GameEntityDefinitionError::Inventory)?;
        }
        let weapon_entities = mechanics::reserve_weapon_entities(&definitions, &inventory_configs)
            .map_err(|reason| GameEntityDefinitionError::Mechanics { reason })?;
        let mut entity_definitions = definitions
            .iter()
            .map(|definition| definition.entity.clone())
            .collect::<Vec<_>>();
        for entity in &mut entity_definitions {
            if entity.bounds.is_none() {
                if let Some(kinematic) = entity.kinematic {
                    entity.bounds = Some(rusty_engine::entity_state::BoundsComponent {
                        min: kinematic.half_extents * -1.0,
                        max: kinematic.half_extents,
                    });
                }
            }
        }
        let mut player_controllers = BTreeMap::new();
        for (definition, entity_definition) in definitions.iter().zip(entity_definitions.iter_mut())
        {
            if let Some(config) = &definition.player_controller {
                if !config.is_valid() {
                    return Err(GameEntityDefinitionError::InvalidPlayerControllerConfig {
                        entity: definition.entity.id,
                    });
                }
                player_controllers.insert(
                    definition.entity.id,
                    PlayerControllerComponent::admit(config.clone(), entity_definition)?,
                );
            }
        }
        let registry = crate::facts::gameplay_fact_registry()
            .map_err(|reason| GameEntityDefinitionError::Mechanics { reason })?;
        let mut entities =
            EntityState::from_definitions_with_registry(registry, entity_definitions)
                .map_err(GameEntityDefinitionError::EntityState)?;

        for (entity, component) in player_controllers {
            attach(&mut entities, entity, component);
        }

        let mut controls: BTreeMap<EntityId, Vec<EntityId>> = BTreeMap::new();
        let mut inventories = BTreeMap::new();

        for definition in &definitions {
            let entity = definition.entity.id;
            if let (Some(controller), Some(inventory)) =
                (&definition.player_controller, &definition.inventory)
            {
                if controller.bindings.select_weapon.len() != inventory.weapon_slots.len() {
                    return Err(GameEntityDefinitionError::WeaponBindingSlotMismatch {
                        entity,
                        binding_count: controller.bindings.select_weapon.len(),
                        slot_count: inventory.weapon_slots.len(),
                    });
                }
            }
            if let Some(config) = definition.door {
                let view = entities.view(entity).expect("definition created entity");
                if view.transform.is_none() {
                    return Err(GameEntityDefinitionError::DoorMissingTransform { entity });
                }
                let Some(collision) = view.collision else {
                    return Err(GameEntityDefinitionError::DoorMissingCollision { entity });
                };
                if collision.static_collider
                    && config.motion_duration.raw() != DEFAULT_DOOR_MOTION_DURATION_TICKS
                {
                    return Err(GameEntityDefinitionError::DoorMustBeMovable { entity });
                }
                if view.renderable.is_none() {
                    return Err(GameEntityDefinitionError::DoorMissingRenderable { entity });
                }
                attach(
                    &mut entities,
                    entity,
                    DoorComponent {
                        config,
                        state: DoorState::Closed,
                        motion_elapsed: TickDelta::ZERO,
                    },
                );
            }
            if let Some(config) = &definition.door_access {
                if definition.door.is_none() {
                    return Err(GameEntityDefinitionError::DoorAccessWithoutDoor { entity });
                }
                if !config.is_valid() {
                    return Err(GameEntityDefinitionError::InvalidDoorAccessConfig { entity });
                }
                let Some(item) = item_definitions.get(&config.required_key) else {
                    return Err(GameEntityDefinitionError::DoorAccessKeyMissingDefinition {
                        entity,
                    });
                };
                if !matches!(item.kind, crate::inventory::ItemKind::AccessKey) {
                    return Err(GameEntityDefinitionError::DoorAccessKeyNotAccessKey { entity });
                }
                attach(&mut entities, entity, config.clone());
            }
            if definition.switch_config.is_some() && !definition.switch {
                return Err(GameEntityDefinitionError::SwitchConfigWithoutSwitch { entity });
            }
            if definition.switch {
                let config = definition.switch_config.clone().unwrap_or_default();
                if !config.is_valid() {
                    return Err(GameEntityDefinitionError::InvalidSwitchConfig { entity });
                }
                attach(&mut entities, entity, SwitchComponent::new(config));
            }
            if let Some(config) = &definition.floor_action {
                let view = entities.view(entity).expect("definition created entity");
                if view.transform.is_none() {
                    return Err(GameEntityDefinitionError::FloorActionMissingTransform { entity });
                }
                if view.bounds.is_none() {
                    return Err(GameEntityDefinitionError::FloorActionMissingBounds { entity });
                }
                if !config.is_valid() {
                    return Err(GameEntityDefinitionError::InvalidFloorActionConfig { entity });
                }
                if definition.lift.is_some() || conflicts_with_walk_trigger(definition) {
                    return Err(
                        GameEntityDefinitionError::FloorActionConflictsWithGameplayOwner { entity },
                    );
                }
                attach(
                    &mut entities,
                    entity,
                    FloorActionComponent {
                        config: config.clone(),
                        state: FloorActionState::Armed,
                        motion_elapsed: TickDelta::ZERO,
                    },
                );
            }
            if let Some(config) = &definition.lift {
                let view = entities.view(entity).expect("definition created entity");
                if view.transform.is_none() {
                    return Err(GameEntityDefinitionError::LiftMissingTransform { entity });
                }
                if view.bounds.is_none() {
                    return Err(GameEntityDefinitionError::LiftMissingBounds { entity });
                }
                if !config.is_valid() {
                    return Err(GameEntityDefinitionError::InvalidLiftConfig { entity });
                }
                if definition.floor_action.is_some() || conflicts_with_walk_trigger(definition) {
                    return Err(GameEntityDefinitionError::LiftConflictsWithGameplayOwner {
                        entity,
                    });
                }
                attach(
                    &mut entities,
                    entity,
                    LiftComponent {
                        config: config.clone(),
                        state: LiftState::Raised,
                        motion_elapsed: TickDelta::ZERO,
                        wait_elapsed: TickDelta::ZERO,
                    },
                );
            }
            if !definition.controls_targets.is_empty() {
                if !definition.switch {
                    return Err(GameEntityDefinitionError::ControlsWithoutSwitch { entity });
                }
                let mut unique = BTreeSet::new();
                for target in &definition.controls_targets {
                    if !unique.insert(*target) {
                        return Err(GameEntityDefinitionError::DuplicateControlTarget {
                            switch: entity,
                            target: *target,
                        });
                    }
                }
                controls.insert(entity, definition.controls_targets.clone());
            }
            if let Some(config) = definition.loading_bay_interlock {
                if !definition.switch {
                    return Err(
                        GameEntityDefinitionError::LoadingBayInterlockWithoutSwitch { entity },
                    );
                }
                attach(&mut entities, entity, config);
            }
            if definition.enemy {
                let view = entities.view(entity).expect("definition created entity");
                if view.collision.is_none() {
                    return Err(GameEntityDefinitionError::EnemyMissingCollision { entity });
                }
                if view.renderable.is_none() {
                    return Err(GameEntityDefinitionError::EnemyMissingRenderable { entity });
                }
                attach(
                    &mut entities,
                    entity,
                    EnemyComponent {
                        state: EnemyState::Alive,
                    },
                );
            }
            if let Some(config) = &definition.enemy_combat {
                if !definition.enemy {
                    return Err(GameEntityDefinitionError::EnemyCombatWithoutEnemy { entity });
                }
                let view = entities.view(entity).expect("definition created entity");
                if view.transform.is_none() {
                    return Err(GameEntityDefinitionError::EnemyCombatMissingTransform { entity });
                }
                if definition.health.is_none() {
                    return Err(GameEntityDefinitionError::EnemyCombatMissingHealth { entity });
                }
                if definition.navigation.is_none() {
                    return Err(GameEntityDefinitionError::EnemyCombatMissingNavigation { entity });
                }
                if !config.is_valid() {
                    return Err(GameEntityDefinitionError::InvalidEnemyCombatConfig { entity });
                }
                let attack_program = enemy_attack_programs
                    .get(&config.attack_program)
                    .ok_or(GameEntityDefinitionError::MissingEnemyAttackProgram { entity })?;
                let defeat_program = enemy_defeat_programs
                    .get(&config.defeat_program)
                    .ok_or(GameEntityDefinitionError::MissingEnemyDefeatProgram { entity })?;
                let attack_operations = attack_program_operations(attack_program);
                let has_projectile_spawn =
                    attack_operations.contains(&EnemyAttackOperation::SpawnEnemyProjectile);
                let has_hitscan_impact = attack_operations.iter().any(|operation| {
                    matches!(
                        operation,
                        EnemyAttackOperation::ApplyEnemyHit | EnemyAttackOperation::ApplyEnemyMiss
                    )
                });
                if matches!(config.attack.kind, crate::EnemyAttackKind::Projectile)
                    && has_hitscan_impact
                    || !matches!(config.attack.kind, crate::EnemyAttackKind::Projectile)
                        && has_projectile_spawn
                {
                    return Err(GameEntityDefinitionError::EnemyAttackProgramIncompatible {
                        entity,
                    });
                }
                if defeat_program_activates_bound_drop(defeat_program)
                    && definition.enemy_drop.is_none()
                {
                    return Err(GameEntityDefinitionError::EnemyDefeatProgramRequiresDrop {
                        entity,
                    });
                }
                attach(
                    &mut entities,
                    entity,
                    EnemyCombatComponent {
                        config: config.clone(),
                        state: EnemyCombatState {
                            posture: EnemyCombatPosture::Sleeping,
                            ready_at_tick: Tick::ZERO,
                            last_known_target_position: None,
                            pain_ticks_remaining: 0,
                        },
                    },
                );
            }
            if let Some(config) = definition.enemy_drop {
                if !definition.enemy {
                    return Err(GameEntityDefinitionError::EnemyDropWithoutEnemy { entity });
                }
                attach(
                    &mut entities,
                    entity,
                    EnemyDropComponent {
                        config,
                        state: EnemyDropState::Armed,
                    },
                );
            }
            if let Some(config) = definition.health {
                let view = entities.view(entity).expect("definition created entity");
                if view.transform.is_none() {
                    return Err(GameEntityDefinitionError::HealthMissingTransform { entity });
                }
                if view.collision.is_none() {
                    return Err(GameEntityDefinitionError::HealthMissingCollision { entity });
                }
                if !config.is_valid(vitality_policy) {
                    return Err(GameEntityDefinitionError::InvalidHealthConfig { entity });
                }
                let preset = if definition.explosive_prop.is_some() {
                    mechanics::VitalityPreset::DestructibleObject
                } else {
                    mechanics::VitalityPreset::ActionActor
                };
                mechanics::attach_health(&mut entities, entity, config, preset, vitality_policy)
                    .map_err(|reason| GameEntityDefinitionError::Mechanics { reason })?;
                attach(&mut entities, entity, config);
            }
            if let Some(config) = definition.explosive_prop {
                if definition.enemy {
                    return Err(GameEntityDefinitionError::ExplosivePropOnEnemy { entity });
                }
                let view = entities.view(entity).expect("definition created entity");
                if view.transform.is_none() {
                    return Err(GameEntityDefinitionError::ExplosivePropMissingTransform {
                        entity,
                    });
                }
                if view.collision.is_none() {
                    return Err(GameEntityDefinitionError::ExplosivePropMissingCollision {
                        entity,
                    });
                }
                if view.renderable.is_none() {
                    return Err(GameEntityDefinitionError::ExplosivePropMissingRenderable {
                        entity,
                    });
                }
                if definition.health.is_none() {
                    return Err(GameEntityDefinitionError::ExplosivePropMissingHealth { entity });
                }
                if !config.is_valid() {
                    return Err(GameEntityDefinitionError::InvalidExplosivePropConfig { entity });
                }
                attach(
                    &mut entities,
                    entity,
                    ExplosivePropComponent {
                        config,
                        state: ExplosivePropState::Armed,
                        pending: false,
                    },
                );
            }
            if let Some(config) = definition.hazard {
                let view = entities.view(entity).expect("definition created entity");
                if view.transform.is_none() {
                    return Err(GameEntityDefinitionError::HazardMissingTransform { entity });
                }
                if view.bounds.is_none() {
                    return Err(GameEntityDefinitionError::HazardMissingBounds { entity });
                }
                if view.renderable.is_none() {
                    return Err(GameEntityDefinitionError::HazardMissingRenderable { entity });
                }
                if !config.is_valid() {
                    return Err(GameEntityDefinitionError::InvalidHazardConfig { entity });
                }
                if definition.door.is_some()
                    || definition.switch
                    || definition.enemy
                    || definition.health.is_some()
                    || definition.encounter.is_some()
                    || definition.extraction_beacon.is_some()
                    || definition.navigation.is_some()
                    || definition.player_controller.is_some()
                    || definition.inventory.is_some()
                    || definition.pickup.is_some()
                {
                    return Err(
                        GameEntityDefinitionError::HazardConflictsWithGameplayOwner { entity },
                    );
                }
                attach(
                    &mut entities,
                    entity,
                    HazardComponent {
                        config,
                        ready_at_tick: Tick::ZERO,
                    },
                );
            }
            if let Some(config) = definition.navigation {
                if !definition.enemy {
                    return Err(GameEntityDefinitionError::NavigationWithoutEnemy { entity });
                }
                let view = entities.view(entity).expect("definition created entity");
                if view.transform.is_none() {
                    return Err(GameEntityDefinitionError::NavigationMissingTransform { entity });
                }
                if view.collision.is_none() {
                    return Err(GameEntityDefinitionError::NavigationMissingCollision { entity });
                }
                if view.kinematic.is_none() {
                    return Err(GameEntityDefinitionError::NavigationMissingKinematic { entity });
                }
                if !vec3_is_finite(config.goal) {
                    return Err(GameEntityDefinitionError::InvalidNavigationGoal { entity });
                }
                if !config.speed_units_per_second.is_finite()
                    || !(0.0..=MAX_NAVIGATION_SPEED_UNITS_PER_SECOND)
                        .contains(&config.speed_units_per_second)
                    || config.speed_units_per_second == 0.0
                {
                    return Err(GameEntityDefinitionError::InvalidNavigationSpeed { entity });
                }
                if !(1..=MAX_NAVIGATION_QUERY_BUDGET).contains(&config.max_visited) {
                    return Err(GameEntityDefinitionError::InvalidNavigationQueryBudget { entity });
                }
                attach(
                    &mut entities,
                    entity,
                    NavigationComponent {
                        config,
                        state: NavigationState::Following,
                    },
                );
            }
            if definition.player_controller.is_some() {
                let view = entities.view(entity).expect("definition created entity");
                if view.transform.is_none() {
                    return Err(
                        GameEntityDefinitionError::PlayerControllerMissingTransform { entity },
                    );
                }
                if view.collision.is_none() {
                    return Err(
                        GameEntityDefinitionError::PlayerControllerMissingCollision { entity },
                    );
                }
                if view.character_motion.is_none() {
                    return Err(
                        GameEntityDefinitionError::PlayerControllerMissingKinematic { entity },
                    );
                }
                if view.renderable.is_none() {
                    return Err(
                        GameEntityDefinitionError::PlayerControllerMissingRenderable { entity },
                    );
                }
                debug_assert!(entities
                    .has_component::<PlayerControllerComponent>(entity)
                    .expect("downstream fact component is registered"));
            }
            if let Some(config) = &definition.inventory {
                if definition.player_controller.is_none() {
                    return Err(GameEntityDefinitionError::Inventory(
                        crate::inventory::InventoryAdmissionError::InventoryWithoutPlayerController {
                            owner: entity,
                        },
                    ));
                }
                let runtime = mechanics::attach_inventory(
                    &mut entities,
                    entity,
                    config,
                    weapon_entities
                        .get(&entity)
                        .expect("weapon entities allocated for every inventory"),
                )
                .map_err(|reason| GameEntityDefinitionError::Mechanics { reason })?;
                for stack in config
                    .starting_stacks
                    .iter()
                    .filter(|stack| runtime.weapon_entities.contains_key(&stack.item))
                {
                    let weapon = runtime.weapon_entities[&stack.item];
                    let receipt = mechanics::materialize_weapon(
                        &mut entities,
                        &mechanics.catalog,
                        entity,
                        &stack.item,
                        weapon,
                    )
                    .map_err(|reason| GameEntityDefinitionError::Mechanics { reason })?;
                    debug_assert_eq!(receipt.entity, weapon);
                    debug_assert_eq!(receipt.container, entity);
                    debug_assert_eq!(receipt.containment_after, Some(entity));
                }
                if let Some(item) = &config.initially_equipped_weapon {
                    let weapon = runtime.weapon_entities.get(item).copied().ok_or_else(|| {
                        GameEntityDefinitionError::Mechanics {
                            reason: format!("missing reserved weapon entity for {item}"),
                        }
                    })?;
                    crate::inventory::equip_initial_weapon(
                        &mut entities,
                        &mechanics.catalog,
                        entity,
                        weapon,
                    )
                    .map_err(|error| GameEntityDefinitionError::Mechanics {
                        reason: format!("initial weapon equipment rejected: {error:?}"),
                    })?;
                }
                inventories.insert(entity, runtime);
            }
            if let Some(config) = &definition.pickup {
                let view = entities.view(entity).expect("definition created entity");
                if view.transform.is_none() {
                    return Err(GameEntityDefinitionError::PickupMissingTransform { entity });
                }
                if view.bounds.is_none() {
                    return Err(GameEntityDefinitionError::PickupMissingBounds { entity });
                }
                if view.renderable.is_none() {
                    return Err(GameEntityDefinitionError::PickupMissingRenderable { entity });
                }
                if definition.door.is_some()
                    || definition.switch
                    || definition.enemy
                    || definition.health.is_some()
                    || definition.hazard.is_some()
                    || definition.encounter.is_some()
                    || definition.extraction_beacon.is_some()
                    || definition.navigation.is_some()
                    || definition.player_controller.is_some()
                    || definition.inventory.is_some()
                {
                    return Err(
                        GameEntityDefinitionError::PickupConflictsWithGameplayOwner { entity },
                    );
                }
                let Some(item) = item_definitions.get(&config.item) else {
                    return Err(GameEntityDefinitionError::PickupMissingItemDefinition { entity });
                };
                if config.quantity == 0 || config.quantity > item.max_quantity {
                    return Err(GameEntityDefinitionError::InvalidPickupQuantity { entity });
                }
                if let Some(starter) = &config.starter_ammunition {
                    let crate::inventory::ItemKind::Weapon(weapon) = &item.kind else {
                        return Err(GameEntityDefinitionError::InvalidPickupStarterAmmunition {
                            entity,
                        });
                    };
                    let valid = starter.item == weapon.ammunition
                        && starter.quantity > 0
                        && item_definitions
                            .get(&starter.item)
                            .is_some_and(|definition| {
                                matches!(definition.kind, crate::inventory::ItemKind::Ammunition)
                                    && starter.quantity <= definition.max_quantity
                            });
                    if !valid {
                        return Err(GameEntityDefinitionError::InvalidPickupStarterAmmunition {
                            entity,
                        });
                    }
                }
                attach(
                    &mut entities,
                    entity,
                    PickupComponent {
                        config: config.clone(),
                        state: PickupState::Available,
                    },
                );
            }
            if let Some(config) = &definition.secret_region {
                let view = entities.view(entity).expect("definition created entity");
                if view.transform.is_none() {
                    return Err(GameEntityDefinitionError::SecretRegionMissingTransform { entity });
                }
                if view.bounds.is_none() {
                    return Err(GameEntityDefinitionError::SecretRegionMissingBounds { entity });
                }
                if !config.is_valid() {
                    return Err(GameEntityDefinitionError::InvalidSecretRegionConfig { entity });
                }
                attach(
                    &mut entities,
                    entity,
                    SecretRegionComponent {
                        config: config.clone(),
                        state: SecretRegionState::Undiscovered,
                    },
                );
            }
            if let Some(config) = &definition.level_exit {
                let view = entities.view(entity).expect("definition created entity");
                if view.transform.is_none() {
                    return Err(GameEntityDefinitionError::LevelExitMissingTransform { entity });
                }
                if view.renderable.is_none() {
                    return Err(GameEntityDefinitionError::LevelExitMissingRenderable { entity });
                }
                if !config.is_valid() {
                    return Err(GameEntityDefinitionError::InvalidLevelExitConfig { entity });
                }
                attach(
                    &mut entities,
                    entity,
                    LevelExitComponent {
                        config: config.clone(),
                        state: LevelExitState::Available,
                    },
                );
            }
            if let Some(config) = &definition.encounter {
                if config.members.is_empty() {
                    return Err(GameEntityDefinitionError::EmptyEncounter { encounter: entity });
                }
                let mut unique = BTreeSet::new();
                for member in &config.members {
                    if !unique.insert(*member) {
                        return Err(GameEntityDefinitionError::DuplicateEncounterMember {
                            encounter: entity,
                            member: *member,
                        });
                    }
                }
                if let Some(radius) = config.activation_radius {
                    if !radius.is_finite()
                        || radius <= 0.0
                        || radius > MAX_ENCOUNTER_ACTIVATION_RADIUS
                    {
                        return Err(
                            GameEntityDefinitionError::InvalidEncounterActivationRadius {
                                encounter: entity,
                            },
                        );
                    }
                    if entities
                        .view(entity)
                        .expect("encounter definition created entity")
                        .transform
                        .is_none()
                    {
                        return Err(
                            GameEntityDefinitionError::EncounterActivationMissingTransform {
                                encounter: entity,
                            },
                        );
                    }
                }
                attach(
                    &mut entities,
                    entity,
                    EncounterComponent {
                        config: config.clone(),
                        state: if config.activation_radius.is_some() {
                            EncounterState::Dormant
                        } else {
                            EncounterState::Active
                        },
                    },
                );
            }
            if let Some(config) = definition.extraction_beacon {
                let view = entities.view(entity).expect("definition created entity");
                if view.transform.is_none() {
                    return Err(
                        GameEntityDefinitionError::ExtractionBeaconMissingTransform { entity },
                    );
                }
                if view.renderable.is_none() {
                    return Err(
                        GameEntityDefinitionError::ExtractionBeaconMissingRenderable { entity },
                    );
                }
                if !config.is_valid() {
                    return Err(GameEntityDefinitionError::InvalidExtractionBeaconConfig {
                        entity,
                    });
                }
                attach(
                    &mut entities,
                    entity,
                    ExtractionBeaconComponent {
                        config,
                        state: ExtractionBeaconState::Standby,
                    },
                );
            }
        }

        let mut target_owners = BTreeMap::new();
        for (action, component) in facts_of::<FloorActionComponent>(&entities) {
            let target_platform = component.config.target_platform;
            validate_walk_trigger_target(
                &entities,
                &mut target_owners,
                action,
                target_platform,
                true,
            )?;
        }
        for (lift, component) in facts_of::<LiftComponent>(&entities) {
            let target_platform = component.config.target_platform;
            validate_walk_trigger_target(
                &entities,
                &mut target_owners,
                lift,
                target_platform,
                false,
            )?;
        }

        for (switch, targets) in &controls {
            for target in targets {
                if !entities.contains(*target) {
                    return Err(GameEntityDefinitionError::UnknownControlTarget {
                        switch: *switch,
                        target: *target,
                    });
                }
                if !entities
                    .has_component::<DoorComponent>(*target)
                    .expect("downstream fact component is registered")
                {
                    return Err(GameEntityDefinitionError::ControlTargetIsNotDoor {
                        switch: *switch,
                        target: *target,
                    });
                }
            }
        }
        let interlocks = facts_of::<LoadingBayInterlockConfig>(&entities);
        for (switch, interlock) in &interlocks {
            for target in [interlock.close_door, interlock.open_door] {
                if !entities
                    .has_component::<DoorComponent>(target)
                    .expect("downstream fact component is registered")
                {
                    return Err(GameEntityDefinitionError::InvalidLoadingBayInterlock {
                        switch: *switch,
                        target,
                    });
                }
            }
            if interlock.close_door == interlock.open_door {
                return Err(GameEntityDefinitionError::InvalidLoadingBayInterlock {
                    switch: *switch,
                    target: interlock.open_door,
                });
            }
        }
        for (switch, interlock) in &interlocks {
            let Some(mut component) = fact_of::<SwitchComponent>(&entities, *switch) else {
                return Err(
                    GameEntityDefinitionError::LoadingBayInterlockWithoutSwitch { entity: *switch },
                );
            };
            component
                .config
                .push_effect_if_missing(SwitchEffect::CloseDoor(interlock.close_door));
            component
                .config
                .push_effect_if_missing(SwitchEffect::OpenDoor(interlock.open_door));
            store(&mut entities, *switch, component);
        }
        for (switch, targets) in &controls {
            let Some(mut component) = fact_of::<SwitchComponent>(&entities, *switch) else {
                return Err(GameEntityDefinitionError::ControlsWithoutSwitch { entity: *switch });
            };
            for target in targets {
                component
                    .config
                    .push_effect_if_missing(SwitchEffect::OpenDoor(*target));
            }
            store(&mut entities, *switch, component);
        }
        for (switch, component) in facts_of::<SwitchComponent>(&entities) {
            if !component.config.is_valid() {
                return Err(GameEntityDefinitionError::InvalidSwitchConfig { entity: switch });
            }
        }
        for (switch, component) in facts_of::<SwitchComponent>(&entities) {
            let mut effects = BTreeSet::new();
            for effect in &component.config.effects {
                if !effects.insert(effect) {
                    return Err(GameEntityDefinitionError::DuplicateSwitchEffect {
                        switch,
                        effect: effect.clone(),
                    });
                }
                let target = effect.door();
                if !entities.contains(target) {
                    return Err(GameEntityDefinitionError::UnknownSwitchEffectTarget {
                        switch,
                        target,
                    });
                }
                if !entities
                    .has_component::<DoorComponent>(target)
                    .expect("downstream fact component is registered")
                {
                    return Err(GameEntityDefinitionError::SwitchEffectTargetIsNotDoor {
                        switch,
                        target,
                    });
                }
            }
        }

        let mut encounter_by_enemy = BTreeMap::new();
        for (encounter, component) in facts_of::<EncounterComponent>(&entities) {
            if let Some(exit) = component.config.exit {
                if !entities.contains(exit) {
                    return Err(GameEntityDefinitionError::UnknownEncounterExit {
                        encounter,
                        exit,
                    });
                }
                if !entities
                    .has_component::<DoorComponent>(exit)
                    .expect("downstream fact component is registered")
                {
                    return Err(GameEntityDefinitionError::EncounterExitIsNotDoor {
                        encounter,
                        exit,
                    });
                }
            }
            for member in &component.config.members {
                if !entities.contains(*member) {
                    return Err(GameEntityDefinitionError::UnknownEncounterMember {
                        encounter,
                        member: *member,
                    });
                }
                if !entities
                    .has_component::<EnemyComponent>(*member)
                    .expect("downstream fact component is registered")
                {
                    return Err(GameEntityDefinitionError::EncounterMemberIsNotEnemy {
                        encounter,
                        member: *member,
                    });
                }
                if let Some(first) = encounter_by_enemy.insert(*member, encounter) {
                    return Err(GameEntityDefinitionError::EnemyInMultipleEncounters {
                        enemy: *member,
                        first,
                        second: encounter,
                    });
                }
            }
        }

        let mut enemy_by_drop_pickup = BTreeMap::new();
        for (enemy, drop) in facts_of::<EnemyDropComponent>(&entities) {
            if !entities.contains(drop.config.pickup) {
                return Err(GameEntityDefinitionError::UnknownEnemyDropPickup {
                    enemy,
                    pickup: drop.config.pickup,
                });
            }
            let Some(mut pickup) = fact_of::<PickupComponent>(&entities, drop.config.pickup) else {
                return Err(GameEntityDefinitionError::EnemyDropTargetIsNotPickup {
                    enemy,
                    pickup: drop.config.pickup,
                });
            };
            if let Some(first) = enemy_by_drop_pickup.insert(drop.config.pickup, enemy) {
                return Err(GameEntityDefinitionError::PickupUsedByMultipleEnemyDrops {
                    pickup: drop.config.pickup,
                    first,
                    second: enemy,
                });
            }
            if entities
                .view(drop.config.pickup)
                .expect("drop pickup definition created entity")
                .renderable
                .as_ref()
                .is_some_and(|renderable| renderable.visible)
            {
                return Err(GameEntityDefinitionError::EnemyDropPickupVisibleAtStart {
                    enemy,
                    pickup: drop.config.pickup,
                });
            }
            pickup.state = PickupState::Dormant;
            store(&mut entities, drop.config.pickup, pickup);
        }
        rusty_engine::gameplay_mechanics::validate_state_against_catalog(
            &entities,
            &mechanics.catalog,
        )
        .map_err(|error| GameEntityDefinitionError::Mechanics {
            reason: error.to_string(),
        })?;

        Ok(Self {
            entities,
            controls,
            item_definitions,
            inventories,
            mechanics,
            collected_pickups: BTreeMap::new(),
            gameplay_programs,
            pickup_programs,
            player_setup_programs,
            player_setup_bindings,
            enemy_attack_programs,
            enemy_defeat_programs,
            hazard_programs,
            hazard_program_bindings,
            encounter_programs,
            encounter_program_bindings,
            explosive_prop_programs,
            explosive_prop_program_bindings,
            switch_programs,
            switch_program_bindings,
            floor_action_programs,
            floor_action_program_bindings,
            lift_programs,
            lift_program_bindings,
            secret_programs,
            secret_program_bindings,
            level_exit_programs,
            level_exit_program_bindings,
            gameplay_outcome: None,
        })
    }

    pub fn entities(&self) -> &EntityState {
        &self.entities
    }

    // -- Downstream fact storage helpers ------------------------------------
    //
    // All entity facts live in the Engine typed component store (see
    // `crate::facts`). These helpers keep call sites readable while routing
    // every mutation through Engine authoring boundaries with exact slot
    // revisions. Registration failures are programmer errors and panic with
    // context; absence is a normal outcome callers handle.

    pub(crate) fn fact<T: EntityComponent + Clone>(&self, entity: EntityId) -> Option<T> {
        self.entities
            .component::<T>(entity)
            .expect("downstream fact component is registered")
            .cloned()
    }

    pub(crate) fn has_fact<T: EntityComponent>(&self, entity: EntityId) -> bool {
        self.entities
            .has_component::<T>(entity)
            .expect("downstream fact component is registered")
    }

    /// Collected snapshot of one fact family; safe to iterate while mutating.
    pub(crate) fn facts<T: EntityComponent + Clone>(&self) -> Vec<(EntityId, T)> {
        self.entities
            .components::<T>()
            .expect("downstream fact component is registered")
            .map(|(entity, value)| (entity, value.clone()))
            .collect()
    }

    pub(crate) fn fact_count<T: EntityComponent>(&self) -> usize {
        self.entities
            .components::<T>()
            .expect("downstream fact component is registered")
            .len()
    }

    pub(crate) fn store_fact<T: EntityComponent + PartialEq>(
        &mut self,
        entity: EntityId,
        value: T,
    ) {
        debug_assert!(self.has_fact::<T>(entity), "stored fact must exist");
        let revision = self
            .entities
            .component_revision::<T>(entity)
            .expect("downstream fact component is registered");
        EntityAuthoringService
            .replace_component(&mut self.entities, revision, entity, value)
            .expect("existing gameplay fact always replaces");
    }

    pub(crate) fn update_fact<T: EntityComponent + PartialEq>(
        &mut self,
        entity: EntityId,
        update: impl FnOnce(&mut T),
    ) {
        let mut value = self.fact::<T>(entity).expect("updated fact must exist");
        update(&mut value);
        self.store_fact(entity, value);
    }

    /// Read-only compiled program catalog and item selection bindings for
    /// product adapters. The catalog has already passed admission bounds.
    pub fn gameplay_programs(&self) -> GameplayProgramReadout {
        self.gameplay_programs
            .readout(self.item_definitions.values().filter_map(|definition| {
                definition
                    .program
                    .as_ref()
                    .map(|program_id| (definition.id.as_str().to_owned(), program_id.clone()))
            }))
    }

    /// Read-only pickup-family catalog and all authored pickup bindings.
    pub fn pickup_programs(&self) -> PickupProgramReadout {
        self.pickup_programs.readout(
            self.facts::<PickupComponent>()
                .into_iter()
                .map(|(entity, pickup)| (entity.raw(), pickup.config.program.clone())),
        )
    }

    /// Read-only player initialization catalog and explicit player bindings.
    /// Setup never re-executes for a running or restored session.
    pub fn player_setup_programs(&self) -> PlayerSetupProgramReadout {
        self.player_setup_programs.readout(
            self.player_setup_bindings
                .iter()
                .map(|(player, program_id)| (player.raw(), program_id.clone())),
        )
    }

    /// Read-only family-specific enemy program catalogs and per-enemy bindings.
    pub fn enemy_programs(&self) -> EnemyProgramReadout {
        let combatants = self.facts::<EnemyCombatComponent>();
        enemy_program_readout(
            &self.enemy_attack_programs,
            &self.enemy_defeat_programs,
            combatants
                .iter()
                .map(|(entity, component)| (entity.raw(), component.config.attack_program.clone())),
            combatants
                .iter()
                .map(|(entity, component)| (entity.raw(), component.config.defeat_program.clone())),
        )
    }

    /// Read-only hazard catalog and placed-trigger bindings for product tooling.
    pub fn hazard_programs(&self) -> HazardProgramReadout {
        self.hazard_programs.readout(
            self.hazard_program_bindings
                .iter()
                .map(|(hazard, program_id)| (hazard.raw(), program_id.clone())),
        )
    }

    /// Read-only encounter lifecycle catalog and explicit encounter bindings.
    pub fn encounter_programs(&self) -> EncounterProgramReadout {
        self.encounter_programs.readout(
            self.encounter_program_bindings
                .iter()
                .map(|(encounter, program_id)| (encounter.raw(), program_id.clone())),
        )
    }

    /// Read-only explosive-prop catalog and placed-prop bindings for product tooling.
    pub fn explosive_prop_programs(&self) -> ExplosivePropProgramReadout {
        self.explosive_prop_programs.readout(
            self.explosive_prop_program_bindings
                .iter()
                .map(|(prop, program_id)| (prop.raw(), program_id.clone())),
        )
    }

    /// Read-only switch interaction program catalog and explicit switch bindings.
    pub fn switch_programs(&self) -> SwitchProgramReadout {
        self.switch_programs.readout(
            self.switch_program_bindings
                .iter()
                .map(|(switch, program_id)| (switch.raw(), program_id.clone())),
        )
    }

    /// Read-only floor-action catalog and explicit placed-trigger bindings.
    pub fn floor_action_programs(&self) -> FloorActionProgramReadout {
        self.floor_action_programs.readout(
            self.floor_action_program_bindings
                .iter()
                .map(|(action, program_id)| (action.raw(), program_id.clone())),
        )
    }

    /// Read-only lift catalog and explicit placed-trigger bindings.
    pub fn lift_programs(&self) -> LiftProgramReadout {
        self.lift_programs.readout(
            self.lift_program_bindings
                .iter()
                .map(|(lift, program_id)| (lift.raw(), program_id.clone())),
        )
    }

    /// Read-only secret-discovery catalog and placed-region bindings.
    pub fn secret_programs(&self) -> SecretProgramReadout {
        self.secret_programs.readout(
            self.secret_program_bindings
                .iter()
                .map(|(secret, program_id)| (secret.raw(), program_id.clone())),
        )
    }

    /// Read-only level-exit completion catalog and placed-exit bindings.
    pub fn level_exit_programs(&self) -> LevelExitProgramReadout {
        self.level_exit_programs.readout(
            self.level_exit_program_bindings
                .iter()
                .map(|(exit, program_id)| (exit.raw(), program_id.clone())),
        )
    }

    /// At most one most-recent selected-program result. Session replacement
    /// constructs this field as `None`, so no old result crosses a boundary.
    pub fn gameplay_outcome(&self) -> Option<&GameplayProgramOutcome> {
        self.gameplay_outcome.as_ref()
    }

    pub(crate) fn record_gameplay_outcome(&mut self, outcome: GameplayProgramOutcome) {
        self.gameplay_outcome = Some(outcome);
    }

    pub(crate) fn clear_gameplay_outcome(&mut self) {
        self.gameplay_outcome = None;
    }

    pub fn entity(
        &self,
        entity: EntityId,
    ) -> Result<EntityView, rusty_engine::entity_state::ViewError> {
        self.entities.view(entity)
    }

    pub fn door(&self, entity: EntityId) -> Option<DoorView> {
        let component = self.fact::<DoorComponent>(entity)?;
        Some(DoorView {
            entity,
            config: component.config,
            state: component.state,
            motion_elapsed: component.motion_elapsed,
            entity_view: self.entities.view(entity).ok()?,
        })
    }

    pub fn door_access(&self, entity: EntityId) -> Option<DoorAccessView> {
        Some(DoorAccessView {
            door: entity,
            config: self.fact::<DoorAccessConfig>(entity)?,
        })
    }

    pub fn door_accesses(&self) -> impl ExactSizeIterator<Item = DoorAccessView> + '_ {
        self.facts::<DoorAccessConfig>()
            .into_iter()
            .map(|(door, config)| DoorAccessView { door, config })
    }

    pub fn switch(&self, entity: EntityId) -> Option<SwitchView> {
        let component = self.fact::<SwitchComponent>(entity)?;
        Some(SwitchView {
            entity,
            config: component.config.clone(),
            activation_count: component.activation_count,
            available: switch_is_available(self, entity),
            controls_targets: self.controls.get(&entity).cloned().unwrap_or_default(),
            entity_view: self.entities.view(entity).ok()?,
        })
    }

    pub fn switches(&self) -> impl ExactSizeIterator<Item = SwitchView> + '_ {
        self.facts::<SwitchComponent>()
            .into_iter()
            .map(|(entity, component)| SwitchView {
                entity,
                config: component.config.clone(),
                activation_count: component.activation_count,
                available: switch_is_available(self, entity),
                controls_targets: self.controls.get(&entity).cloned().unwrap_or_default(),
                entity_view: self
                    .entities
                    .view(entity)
                    .expect("admitted switch remains viewable"),
            })
    }

    pub fn floor_action(&self, entity: EntityId) -> Option<FloorActionView> {
        let component = self.fact::<FloorActionComponent>(entity)?;
        Some(FloorActionView {
            entity,
            config: component.config.clone(),
            state: component.state,
            motion_elapsed: component.motion_elapsed,
            entity_view: self.entities.view(entity).ok()?,
            target_platform_view: self.entities.view(component.config.target_platform).ok()?,
        })
    }

    pub fn floor_actions(&self) -> impl ExactSizeIterator<Item = FloorActionView> + '_ {
        self.facts::<FloorActionComponent>()
            .into_iter()
            .map(|(entity, component)| FloorActionView {
                entity,
                config: component.config.clone(),
                state: component.state,
                motion_elapsed: component.motion_elapsed,
                entity_view: self
                    .entities
                    .view(entity)
                    .expect("admitted floor action remains viewable"),
                target_platform_view: self
                    .entities
                    .view(component.config.target_platform)
                    .expect("admitted floor action target remains viewable"),
            })
    }

    pub fn lift(&self, entity: EntityId) -> Option<LiftView> {
        let component = self.fact::<LiftComponent>(entity)?;
        Some(LiftView {
            entity,
            config: component.config.clone(),
            state: component.state,
            motion_elapsed: component.motion_elapsed,
            wait_elapsed: component.wait_elapsed,
            entity_view: self.entities.view(entity).ok()?,
            target_platform_view: self.entities.view(component.config.target_platform).ok()?,
        })
    }

    pub fn lifts(&self) -> impl ExactSizeIterator<Item = LiftView> + '_ {
        self.facts::<LiftComponent>()
            .into_iter()
            .map(|(entity, component)| LiftView {
                entity,
                config: component.config.clone(),
                state: component.state,
                motion_elapsed: component.motion_elapsed,
                wait_elapsed: component.wait_elapsed,
                entity_view: self
                    .entities
                    .view(entity)
                    .expect("admitted lift remains viewable"),
                target_platform_view: self
                    .entities
                    .view(component.config.target_platform)
                    .expect("admitted lift target remains viewable"),
            })
    }

    pub fn loading_bay_interlock(&self, entity: EntityId) -> Option<LoadingBayInterlockView> {
        Some(LoadingBayInterlockView {
            switch: entity,
            config: self.fact::<LoadingBayInterlockConfig>(entity)?,
            entity_view: self.entities.view(entity).ok()?,
        })
    }

    pub fn loading_bay_interlocks(
        &self,
    ) -> impl ExactSizeIterator<Item = LoadingBayInterlockView> + '_ {
        self.facts::<LoadingBayInterlockConfig>()
            .into_iter()
            .map(|(switch, config)| LoadingBayInterlockView {
                switch,
                config,
                entity_view: self
                    .entities
                    .view(switch)
                    .expect("admitted Loading Bay interlock remains viewable"),
            })
    }

    pub fn enemy(&self, entity: EntityId) -> Option<EnemyView> {
        let component = self.fact::<EnemyComponent>(entity)?;
        Some(EnemyView {
            entity,
            state: component.state,
            entity_view: self.entities.view(entity).ok()?,
        })
    }

    pub fn enemy_combat(&self, entity: EntityId) -> Option<EnemyCombatView> {
        let component = self.fact::<EnemyCombatComponent>(entity)?;
        Some(EnemyCombatView {
            entity,
            config: component.config.clone(),
            state: component.state.clone(),
        })
    }

    pub fn enemy_combatants(&self) -> impl ExactSizeIterator<Item = EnemyCombatView> + '_ {
        self.facts::<EnemyCombatComponent>()
            .into_iter()
            .map(|(entity, component)| EnemyCombatView {
                entity,
                config: component.config.clone(),
                state: component.state.clone(),
            })
    }

    pub fn enemy_drop(&self, enemy: EntityId) -> Option<EnemyDropView> {
        let component = self.fact::<EnemyDropComponent>(enemy)?;
        Some(EnemyDropView {
            enemy,
            pickup: component.config.pickup,
            state: component.state,
        })
    }

    pub fn health(&self, entity: EntityId) -> Option<HealthView> {
        let config = self.fact::<HealthConfig>(entity)?;
        let tracks = self
            .entities
            .component::<rusty_engine::gameplay_mechanics::TracksComponent>(entity)
            .ok()??;
        let preset = if self.has_fact::<ExplosivePropComponent>(entity) {
            crate::mechanics::VitalityPreset::DestructibleObject
        } else {
            crate::mechanics::VitalityPreset::ActionActor
        };
        let current = u32::try_from(
            tracks
                .current(&crate::mechanics::vitality_track(preset))?
                .get(),
        )
        .ok()?;
        let armor = u32::try_from(tracks.current(&crate::mechanics::armor_track())?.get()).ok()?;
        let armor_item = self
            .entities
            .component::<rusty_engine::gameplay_mechanics::ActiveEffectsComponent>(entity)
            .ok()??
            .effects()
            .iter()
            .find_map(|effect| {
                self.mechanics.armor.iter().find_map(|(item, binding)| {
                    (effect.definition() == &binding.effect).then(|| item.clone())
                })
            });
        Some(HealthView {
            entity,
            config,
            current,
            armor,
            armor_item,
            state: if current == 0 {
                VitalityState::Dead
            } else {
                VitalityState::Alive
            },
        })
    }

    /// Maps the public standard host DTO against the exact live component
    /// revision immediately before borrowed command dispatch.
    pub fn developer_map_track_set(
        &self,
        request: rusty_engine::developer_command_standard::HostTrackSetRequest,
    ) -> Result<rusty_engine::gameplay_mechanics::TrackSetRequest, String> {
        request
            .map_live(&self.entities)
            .map_err(|error| error.to_string())
    }

    /// Applies an admitted exact standard track request through its named
    /// Engine mechanics owner.
    pub fn developer_set_track(
        &mut self,
        request: rusty_engine::gameplay_mechanics::TrackSetRequest,
    ) -> Result<
        rusty_engine::gameplay_mechanics::TrackSetReceipt,
        rusty_engine::gameplay_mechanics::MechanicsError,
    > {
        rusty_engine::developer_command_standard::admin_set_track(
            &mut self.entities,
            &self.mechanics.catalog,
            request,
        )
    }

    pub fn developer_inspect_entity(
        &self,
        entity: EntityId,
    ) -> Option<rusty_engine::engine_inspector::EntityInspection> {
        rusty_engine::developer_command_standard::inspect_entity(&self.entities, entity)
    }

    pub fn developer_inspect_mechanics(
        &self,
        entity: EntityId,
    ) -> Result<
        rusty_engine::engine_inspector::MechanicsStructuralEntityInspection,
        rusty_engine::gameplay_mechanics::MechanicsError,
    > {
        rusty_engine::developer_command_standard::inspect_mechanics(
            &self.entities,
            &self.mechanics.catalog,
            entity,
        )
    }

    pub fn explosive_prop(&self, entity: EntityId) -> Option<ExplosivePropView> {
        let component = self.fact::<ExplosivePropComponent>(entity)?;
        Some(ExplosivePropView {
            entity,
            config: component.config,
            state: component.state,
            pending: component.pending,
        })
    }

    pub(crate) fn is_player_attack_target(&self, entity: EntityId) -> bool {
        let Some(health) = self.health(entity) else {
            return false;
        };
        if health.state != VitalityState::Alive {
            return false;
        }
        let Ok(view) = self.entities.view(entity) else {
            return false;
        };
        if !view.collision.is_some_and(|collision| collision.enabled)
            || !view.renderable.is_some_and(|renderable| renderable.visible)
        {
            return false;
        }
        if self.has_fact::<EnemyComponent>(entity) {
            self.fact::<EnemyComponent>(entity)
                .is_some_and(|enemy| enemy.state == EnemyState::Alive)
                && EncounterService::enemy_is_active(self, entity)
        } else {
            self.has_fact::<ExplosivePropComponent>(entity)
        }
    }

    pub fn hazard(&self, entity: EntityId) -> Option<HazardView> {
        let component = self.fact::<HazardComponent>(entity)?;
        Some(HazardView {
            entity,
            config: component.config,
            ready_at_tick: component.ready_at_tick,
        })
    }

    pub fn hazards(&self) -> impl ExactSizeIterator<Item = HazardView> + '_ {
        self.facts::<HazardComponent>()
            .into_iter()
            .map(|(entity, component)| HazardView {
                entity,
                config: component.config,
                ready_at_tick: component.ready_at_tick,
            })
    }

    pub fn encounter(&self, entity: EntityId) -> Option<EncounterView> {
        let component = self.fact::<EncounterComponent>(entity)?;
        Some(EncounterView {
            entity,
            members: component.config.members.clone(),
            exit: component.config.exit,
            activation_radius: component.config.activation_radius,
            state: component.state,
        })
    }

    pub fn navigation(&self, entity: EntityId) -> Option<NavigationView> {
        let component = self.fact::<NavigationComponent>(entity)?;
        Some(NavigationView {
            entity,
            config: component.config,
            state: component.state,
            entity_view: self.entities.view(entity).ok()?,
        })
    }

    pub fn extraction_beacon(&self, entity: EntityId) -> Option<ExtractionBeaconView> {
        let component = self.fact::<ExtractionBeaconComponent>(entity)?;
        Some(ExtractionBeaconView {
            entity,
            config: component.config,
            state: component.state,
            entity_view: self.entities.view(entity).ok()?,
        })
    }

    pub fn player_controller(&self, entity: EntityId) -> Option<PlayerControllerView> {
        let component = self.fact::<PlayerControllerComponent>(entity)?;
        let motion = self.entities.character_motion(entity)?;
        Some(PlayerControllerView {
            entity,
            config: component.config.clone(),
            state: component.state(motion),
            eye_offset_from_center: component.eye_offset_from_center,
            entity_view: self.entities.view(entity).ok()?,
        })
    }

    pub(crate) fn gameplay_translation(&self, entity: EntityId) -> Option<Vec3> {
        let mut translation = self.entities.transform(entity)?.translation;
        if let Some(controller) = self.fact::<PlayerControllerComponent>(entity) {
            translation.y +=
                controller.eye_offset_from_center - controller.config.traversal.eye_height;
        }
        Some(translation)
    }

    pub fn item_definition(&self, item: &ItemDefinitionId) -> Option<ItemDefinitionView> {
        let definition = self.item_definitions.get(item)?;
        Some(ItemDefinitionView {
            id: definition.id.clone(),
            kind: definition.kind.clone(),
            max_quantity: definition.max_quantity,
        })
    }

    pub fn item_definitions(&self) -> impl ExactSizeIterator<Item = ItemDefinitionView> + '_ {
        self.item_definitions
            .values()
            .map(|definition| ItemDefinitionView {
                id: definition.id.clone(),
                kind: definition.kind.clone(),
                max_quantity: definition.max_quantity,
            })
    }

    pub fn inventory(&self, owner: EntityId) -> Option<InventoryView> {
        inventory_view(self, owner).ok()
    }

    pub fn pickup(&self, entity: EntityId) -> Option<PickupView> {
        self.fact::<PickupComponent>(entity)
            .as_ref()
            .or_else(|| self.collected_pickups.get(&entity))
            .map(|component| pickup_view(entity, component))
    }

    pub fn pickups(&self) -> impl ExactSizeIterator<Item = PickupView> + '_ {
        let mut all_pickups = self.facts::<PickupComponent>();
        all_pickups.extend(
            self.collected_pickups
                .iter()
                .map(|(entity, component)| (*entity, component.clone())),
        );
        all_pickups
            .into_iter()
            .map(|(entity, component)| pickup_view(entity, &component))
    }

    pub fn secret_region(&self, entity: EntityId) -> Option<SecretRegionView> {
        let component = self.fact::<SecretRegionComponent>(entity)?;
        Some(SecretRegionView {
            entity,
            config: component.config.clone(),
            state: component.state.clone(),
            entity_view: self.entities.view(entity).ok()?,
        })
    }

    pub fn secret_regions(&self) -> impl ExactSizeIterator<Item = SecretRegionView> + '_ {
        self.facts::<SecretRegionComponent>()
            .into_iter()
            .map(|(entity, component)| SecretRegionView {
                entity,
                config: component.config.clone(),
                state: component.state.clone(),
                entity_view: self
                    .entities
                    .view(entity)
                    .expect("admitted secret region remains viewable"),
            })
    }

    pub fn level_exit(&self, entity: EntityId) -> Option<LevelExitView> {
        let component = self.fact::<LevelExitComponent>(entity)?;
        Some(LevelExitView {
            entity,
            config: component.config.clone(),
            state: component.state,
            entity_view: self.entities.view(entity).ok()?,
        })
    }

    pub fn level_exits(&self) -> impl ExactSizeIterator<Item = LevelExitView> + '_ {
        self.facts::<LevelExitComponent>()
            .into_iter()
            .map(|(entity, component)| LevelExitView {
                entity,
                config: component.config.clone(),
                state: component.state,
                entity_view: self
                    .entities
                    .view(entity)
                    .expect("admitted level exit remains viewable"),
            })
    }

    pub fn level_complete(&self) -> bool {
        self.facts::<LevelExitComponent>()
            .iter()
            .any(|(_, component)| matches!(component.state, LevelExitState::Completed { .. }))
    }

    pub fn weapon(&self, entity: EntityId) -> Option<WeaponView> {
        let inventory = self.inventories.get(&entity)?;
        let item = self.equipped_weapon(entity)?;
        let definition = self.item_definitions.get(&item)?;
        let crate::inventory::ItemKind::Weapon(weapon) = &definition.kind else {
            return None;
        };
        Some(WeaponView {
            owner: entity,
            item: item.clone(),
            definition: weapon.clone(),
            state: WeaponState {
                ready_at_tick: inventory
                    .weapon_ready_at
                    .get(&item)
                    .copied()
                    .unwrap_or(Tick::ZERO),
            },
        })
    }

    pub(crate) fn equipped_weapon(&self, owner: EntityId) -> Option<ItemDefinitionId> {
        let runtime = self.inventories.get(&owner)?;
        let equipment = self
            .entities
            .component::<rusty_engine::gameplay_mechanics::EquipmentComponent>(owner)
            .ok()??;
        let item_entity = equipment.assignment(&crate::mechanics::weapon_slot())?.item;
        runtime
            .weapon_entities
            .iter()
            .find_map(|(item, entity)| (*entity == item_entity).then(|| item.clone()))
    }
}

fn attach<T: EntityComponent>(entities: &mut EntityState, entity: EntityId, value: T) {
    let revision = entities
        .component_revision::<T>(entity)
        .expect("downstream fact component is registered");
    EntityAuthoringService
        .attach_component(entities, revision, entity, value)
        .expect("admitted gameplay fact attaches exactly once");
}

fn store<T: EntityComponent + PartialEq>(entities: &mut EntityState, entity: EntityId, value: T) {
    let revision = entities
        .component_revision::<T>(entity)
        .expect("downstream fact component is registered");
    EntityAuthoringService
        .replace_component(entities, revision, entity, value)
        .expect("existing gameplay fact always replaces");
}

fn fact_of<T: EntityComponent + Clone>(entities: &EntityState, entity: EntityId) -> Option<T> {
    entities
        .component::<T>(entity)
        .expect("downstream fact component is registered")
        .cloned()
}

fn facts_of<T: EntityComponent + Clone>(entities: &EntityState) -> Vec<(EntityId, T)> {
    entities
        .components::<T>()
        .expect("downstream fact component is registered")
        .map(|(entity, value)| (entity, value.clone()))
        .collect()
}

fn conflicts_with_walk_trigger(definition: &GameEntityDefinition) -> bool {
    definition.door.is_some()
        || definition.door_access.is_some()
        || definition.switch
        || !definition.controls_targets.is_empty()
        || definition.loading_bay_interlock.is_some()
        || definition.enemy
        || definition.enemy_combat.is_some()
        || definition.enemy_drop.is_some()
        || definition.health.is_some()
        || definition.hazard.is_some()
        || definition.encounter.is_some()
        || definition.extraction_beacon.is_some()
        || definition.navigation.is_some()
        || definition.player_controller.is_some()
        || definition.inventory.is_some()
        || definition.pickup.is_some()
        || definition.secret_region.is_some()
        || definition.level_exit.is_some()
}

fn validate_walk_trigger_target(
    entities: &EntityState,
    target_owners: &mut BTreeMap<EntityId, EntityId>,
    owner: EntityId,
    target_platform: EntityId,
    is_floor_action: bool,
) -> Result<(), GameEntityDefinitionError> {
    let unknown_target = || {
        if is_floor_action {
            GameEntityDefinitionError::UnknownFloorActionTarget {
                action: owner,
                target_platform,
            }
        } else {
            GameEntityDefinitionError::UnknownLiftTarget {
                lift: owner,
                target_platform,
            }
        }
    };
    let missing_transform = || {
        if is_floor_action {
            GameEntityDefinitionError::FloorActionTargetMissingTransform {
                action: owner,
                target_platform,
            }
        } else {
            GameEntityDefinitionError::LiftTargetMissingTransform {
                lift: owner,
                target_platform,
            }
        }
    };
    let missing_collision = || {
        if is_floor_action {
            GameEntityDefinitionError::FloorActionTargetMissingCollision {
                action: owner,
                target_platform,
            }
        } else {
            GameEntityDefinitionError::LiftTargetMissingCollision {
                lift: owner,
                target_platform,
            }
        }
    };
    let not_movable = || {
        if is_floor_action {
            GameEntityDefinitionError::FloorActionTargetMustBeMovable {
                action: owner,
                target_platform,
            }
        } else {
            GameEntityDefinitionError::LiftTargetMustBeMovable {
                lift: owner,
                target_platform,
            }
        }
    };

    if !entities.contains(target_platform) {
        return Err(unknown_target());
    }
    if entities
        .has_component::<FloorActionComponent>(target_platform)
        .expect("downstream fact component is registered")
        || entities
            .has_component::<LiftComponent>(target_platform)
            .expect("downstream fact component is registered")
    {
        return Err(GameEntityDefinitionError::DuplicateMovingPlatformTarget {
            target_platform,
            first_owner: target_platform,
            second_owner: owner,
        });
    }
    let view = entities
        .view(target_platform)
        .expect("target existence was checked");
    if view.transform.is_none() {
        return Err(missing_transform());
    }
    let Some(collision) = view.collision else {
        return Err(missing_collision());
    };
    if collision.static_collider {
        return Err(not_movable());
    }
    if let Some(first_owner) = target_owners.insert(target_platform, owner) {
        return Err(GameEntityDefinitionError::DuplicateMovingPlatformTarget {
            target_platform,
            first_owner,
            second_owner: owner,
        });
    }
    Ok(())
}

fn vec3_is_finite(value: Vec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}
