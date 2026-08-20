use std::collections::{BTreeMap, BTreeSet};

use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::core_time::{Tick, TickDelta};
use rusty_engine::engine_spatial::MAX_TRIGGER_DEFINITIONS;
use rusty_engine::entity_state::{EntityState, EntityView};

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
    pub(crate) doors: BTreeMap<EntityId, DoorComponent>,
    pub(crate) door_access: BTreeMap<EntityId, DoorAccessConfig>,
    pub(crate) switches: BTreeMap<EntityId, SwitchComponent>,
    pub(crate) floor_actions: BTreeMap<EntityId, FloorActionComponent>,
    pub(crate) lifts: BTreeMap<EntityId, LiftComponent>,
    pub(crate) controls: BTreeMap<EntityId, Vec<EntityId>>,
    pub(crate) loading_bay_interlocks: BTreeMap<EntityId, LoadingBayInterlockConfig>,
    pub(crate) enemies: BTreeMap<EntityId, EnemyComponent>,
    pub(crate) enemy_combat: BTreeMap<EntityId, EnemyCombatComponent>,
    pub(crate) enemy_drops: BTreeMap<EntityId, EnemyDropComponent>,
    pub(crate) health: BTreeMap<EntityId, HealthConfig>,
    pub(crate) explosive_props: BTreeMap<EntityId, ExplosivePropComponent>,
    pub(crate) hazards: BTreeMap<EntityId, HazardComponent>,
    pub(crate) encounters: BTreeMap<EntityId, EncounterComponent>,
    pub(crate) extraction_beacons: BTreeMap<EntityId, ExtractionBeaconComponent>,
    pub(crate) navigators: BTreeMap<EntityId, NavigationComponent>,
    pub(crate) player_controllers: BTreeMap<EntityId, PlayerControllerComponent>,
    pub(crate) item_definitions: BTreeMap<ItemDefinitionId, ItemDefinition>,
    pub(crate) inventories: BTreeMap<EntityId, InventoryRuntime>,
    pub(crate) mechanics: MechanicsRuntime,
    pub(crate) pickups: BTreeMap<EntityId, PickupComponent>,
    pub(crate) secret_regions: BTreeMap<EntityId, SecretRegionComponent>,
    pub(crate) level_exits: BTreeMap<EntityId, LevelExitComponent>,
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
    pub(crate) fn from_item_entity_and_gameplay_programs(
        item_definitions: impl IntoIterator<Item = ItemDefinition>,
        definitions: impl IntoIterator<Item = GameEntityDefinition>,
        program_catalogs: SessionProgramCatalogs,
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
        let mechanics = mechanics::build_runtime(&item_definitions)
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
        let (hidden_weapons, weapon_entities) =
            mechanics::allocate_weapon_entities(&definitions, &inventory_configs)
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
        entity_definitions.extend(hidden_weapons);
        let registry = mechanics::mechanics_registry()
            .map_err(|reason| GameEntityDefinitionError::Mechanics { reason })?;
        let mut entities =
            EntityState::from_definitions_with_registry(registry, entity_definitions)
                .map_err(GameEntityDefinitionError::EntityState)?;

        let mut doors = BTreeMap::new();
        let mut door_access = BTreeMap::new();
        let mut switches = BTreeMap::new();
        let mut floor_actions = BTreeMap::new();
        let mut lifts = BTreeMap::new();
        let mut controls = BTreeMap::new();
        let mut loading_bay_interlocks = BTreeMap::new();
        let mut enemies = BTreeMap::new();
        let mut enemy_combat = BTreeMap::new();
        let mut enemy_drops = BTreeMap::new();
        let mut health = BTreeMap::new();
        let mut explosive_props = BTreeMap::new();
        let mut hazards = BTreeMap::new();
        let mut encounters = BTreeMap::new();
        let mut extraction_beacons = BTreeMap::new();
        let mut navigators = BTreeMap::new();
        let mut inventories = BTreeMap::new();
        let mut pickups = BTreeMap::new();
        let mut secret_regions = BTreeMap::new();
        let mut level_exits = BTreeMap::new();

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
                doors.insert(
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
                door_access.insert(entity, config.clone());
            }
            if definition.switch_config.is_some() && !definition.switch {
                return Err(GameEntityDefinitionError::SwitchConfigWithoutSwitch { entity });
            }
            if definition.switch {
                let config = definition.switch_config.clone().unwrap_or_default();
                if !config.is_valid() {
                    return Err(GameEntityDefinitionError::InvalidSwitchConfig { entity });
                }
                switches.insert(entity, SwitchComponent::new(config));
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
                floor_actions.insert(
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
                lifts.insert(
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
                loading_bay_interlocks.insert(entity, config);
            }
            if definition.enemy {
                let view = entities.view(entity).expect("definition created entity");
                if view.collision.is_none() {
                    return Err(GameEntityDefinitionError::EnemyMissingCollision { entity });
                }
                if view.renderable.is_none() {
                    return Err(GameEntityDefinitionError::EnemyMissingRenderable { entity });
                }
                enemies.insert(
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
                enemy_combat.insert(
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
                enemy_drops.insert(
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
                if !config.is_valid() {
                    return Err(GameEntityDefinitionError::InvalidHealthConfig { entity });
                }
                mechanics::attach_health(&mut entities, entity, config)
                    .map_err(|reason| GameEntityDefinitionError::Mechanics { reason })?;
                health.insert(entity, config);
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
                explosive_props.insert(
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
                hazards.insert(
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
                navigators.insert(
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
                debug_assert!(player_controllers.contains_key(&entity));
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
                pickups.insert(
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
                secret_regions.insert(
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
                level_exits.insert(
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
                encounters.insert(
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
                extraction_beacons.insert(
                    entity,
                    ExtractionBeaconComponent {
                        config,
                        state: ExtractionBeaconState::Standby,
                    },
                );
            }
        }

        let mut target_owners = BTreeMap::new();
        for (action, component) in &floor_actions {
            let target_platform = component.config.target_platform;
            validate_walk_trigger_target(
                &entities,
                &floor_actions,
                &lifts,
                &mut target_owners,
                *action,
                target_platform,
                true,
            )?;
        }
        for (lift, component) in &lifts {
            let target_platform = component.config.target_platform;
            validate_walk_trigger_target(
                &entities,
                &floor_actions,
                &lifts,
                &mut target_owners,
                *lift,
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
                if !doors.contains_key(target) {
                    return Err(GameEntityDefinitionError::ControlTargetIsNotDoor {
                        switch: *switch,
                        target: *target,
                    });
                }
            }
        }
        for (switch, interlock) in &loading_bay_interlocks {
            for target in [interlock.close_door, interlock.open_door] {
                if !doors.contains_key(&target) {
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
        for (switch, interlock) in &loading_bay_interlocks {
            let component = switches
                .get_mut(switch)
                .expect("Loading Bay interlock switch was admitted");
            component
                .config
                .push_effect_if_missing(SwitchEffect::CloseDoor(interlock.close_door));
            component
                .config
                .push_effect_if_missing(SwitchEffect::OpenDoor(interlock.open_door));
        }
        for (switch, targets) in &controls {
            let component = switches
                .get_mut(switch)
                .expect("control switch was admitted");
            for target in targets {
                component
                    .config
                    .push_effect_if_missing(SwitchEffect::OpenDoor(*target));
            }
        }
        for (switch, component) in &switches {
            if !component.config.is_valid() {
                return Err(GameEntityDefinitionError::InvalidSwitchConfig { entity: *switch });
            }
        }
        for (switch, component) in &switches {
            let mut effects = BTreeSet::new();
            for effect in &component.config.effects {
                if !effects.insert(effect) {
                    return Err(GameEntityDefinitionError::DuplicateSwitchEffect {
                        switch: *switch,
                        effect: effect.clone(),
                    });
                }
                let target = effect.door();
                if !entities.contains(target) {
                    return Err(GameEntityDefinitionError::UnknownSwitchEffectTarget {
                        switch: *switch,
                        target,
                    });
                }
                if !doors.contains_key(&target) {
                    return Err(GameEntityDefinitionError::SwitchEffectTargetIsNotDoor {
                        switch: *switch,
                        target,
                    });
                }
            }
        }

        let mut encounter_by_enemy = BTreeMap::new();
        for (encounter, component) in &encounters {
            if let Some(exit) = component.config.exit {
                if !entities.contains(exit) {
                    return Err(GameEntityDefinitionError::UnknownEncounterExit {
                        encounter: *encounter,
                        exit,
                    });
                }
                if !doors.contains_key(&exit) {
                    return Err(GameEntityDefinitionError::EncounterExitIsNotDoor {
                        encounter: *encounter,
                        exit,
                    });
                }
            }
            for member in &component.config.members {
                if !entities.contains(*member) {
                    return Err(GameEntityDefinitionError::UnknownEncounterMember {
                        encounter: *encounter,
                        member: *member,
                    });
                }
                if !enemies.contains_key(member) {
                    return Err(GameEntityDefinitionError::EncounterMemberIsNotEnemy {
                        encounter: *encounter,
                        member: *member,
                    });
                }
                if let Some(first) = encounter_by_enemy.insert(*member, *encounter) {
                    return Err(GameEntityDefinitionError::EnemyInMultipleEncounters {
                        enemy: *member,
                        first,
                        second: *encounter,
                    });
                }
            }
        }

        let mut enemy_by_drop_pickup = BTreeMap::new();
        for (enemy, drop) in &enemy_drops {
            if !entities.contains(drop.config.pickup) {
                return Err(GameEntityDefinitionError::UnknownEnemyDropPickup {
                    enemy: *enemy,
                    pickup: drop.config.pickup,
                });
            }
            let Some(pickup) = pickups.get_mut(&drop.config.pickup) else {
                return Err(GameEntityDefinitionError::EnemyDropTargetIsNotPickup {
                    enemy: *enemy,
                    pickup: drop.config.pickup,
                });
            };
            if let Some(first) = enemy_by_drop_pickup.insert(drop.config.pickup, *enemy) {
                return Err(GameEntityDefinitionError::PickupUsedByMultipleEnemyDrops {
                    pickup: drop.config.pickup,
                    first,
                    second: *enemy,
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
                    enemy: *enemy,
                    pickup: drop.config.pickup,
                });
            }
            pickup.state = PickupState::Dormant;
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
            doors,
            door_access,
            switches,
            floor_actions,
            lifts,
            controls,
            loading_bay_interlocks,
            enemies,
            enemy_combat,
            enemy_drops,
            health,
            explosive_props,
            hazards,
            encounters,
            extraction_beacons,
            navigators,
            player_controllers,
            item_definitions,
            inventories,
            mechanics,
            pickups,
            secret_regions,
            level_exits,
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
            self.pickups
                .iter()
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
        enemy_program_readout(
            &self.enemy_attack_programs,
            &self.enemy_defeat_programs,
            self.enemy_combat
                .iter()
                .map(|(entity, component)| (entity.raw(), component.config.attack_program.clone())),
            self.enemy_combat
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
        let component = self.doors.get(&entity)?;
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
            config: self.door_access.get(&entity)?.clone(),
        })
    }

    pub fn door_accesses(&self) -> impl ExactSizeIterator<Item = DoorAccessView> + '_ {
        self.door_access
            .iter()
            .map(|(door, config)| DoorAccessView {
                door: *door,
                config: config.clone(),
            })
    }

    pub fn switch(&self, entity: EntityId) -> Option<SwitchView> {
        let component = self.switches.get(&entity)?;
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
        self.switches.iter().map(|(entity, component)| SwitchView {
            entity: *entity,
            config: component.config.clone(),
            activation_count: component.activation_count,
            available: switch_is_available(self, *entity),
            controls_targets: self.controls.get(entity).cloned().unwrap_or_default(),
            entity_view: self
                .entities
                .view(*entity)
                .expect("admitted switch remains viewable"),
        })
    }

    pub fn floor_action(&self, entity: EntityId) -> Option<FloorActionView> {
        let component = self.floor_actions.get(&entity)?;
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
        self.floor_actions
            .iter()
            .map(|(entity, component)| FloorActionView {
                entity: *entity,
                config: component.config.clone(),
                state: component.state,
                motion_elapsed: component.motion_elapsed,
                entity_view: self
                    .entities
                    .view(*entity)
                    .expect("admitted floor action remains viewable"),
                target_platform_view: self
                    .entities
                    .view(component.config.target_platform)
                    .expect("admitted floor action target remains viewable"),
            })
    }

    pub fn lift(&self, entity: EntityId) -> Option<LiftView> {
        let component = self.lifts.get(&entity)?;
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
        self.lifts.iter().map(|(entity, component)| LiftView {
            entity: *entity,
            config: component.config.clone(),
            state: component.state,
            motion_elapsed: component.motion_elapsed,
            wait_elapsed: component.wait_elapsed,
            entity_view: self
                .entities
                .view(*entity)
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
            config: *self.loading_bay_interlocks.get(&entity)?,
            entity_view: self.entities.view(entity).ok()?,
        })
    }

    pub fn loading_bay_interlocks(
        &self,
    ) -> impl ExactSizeIterator<Item = LoadingBayInterlockView> + '_ {
        self.loading_bay_interlocks
            .iter()
            .map(|(switch, config)| LoadingBayInterlockView {
                switch: *switch,
                config: *config,
                entity_view: self
                    .entities
                    .view(*switch)
                    .expect("admitted Loading Bay interlock remains viewable"),
            })
    }

    pub fn enemy(&self, entity: EntityId) -> Option<EnemyView> {
        let component = self.enemies.get(&entity)?;
        Some(EnemyView {
            entity,
            state: component.state,
            entity_view: self.entities.view(entity).ok()?,
        })
    }

    pub fn enemy_combat(&self, entity: EntityId) -> Option<EnemyCombatView> {
        let component = self.enemy_combat.get(&entity)?;
        Some(EnemyCombatView {
            entity,
            config: component.config.clone(),
            state: component.state.clone(),
        })
    }

    pub fn enemy_combatants(&self) -> impl ExactSizeIterator<Item = EnemyCombatView> + '_ {
        self.enemy_combat
            .iter()
            .map(|(entity, component)| EnemyCombatView {
                entity: *entity,
                config: component.config.clone(),
                state: component.state.clone(),
            })
    }

    pub fn enemy_drop(&self, enemy: EntityId) -> Option<EnemyDropView> {
        let component = self.enemy_drops.get(&enemy)?;
        Some(EnemyDropView {
            enemy,
            pickup: component.config.pickup,
            state: component.state,
        })
    }

    pub fn health(&self, entity: EntityId) -> Option<HealthView> {
        let config = *self.health.get(&entity)?;
        let tracks = self
            .entities
            .component::<rusty_engine::gameplay_mechanics::TracksComponent>(entity)
            .ok()??;
        let current =
            u32::try_from(tracks.current(&crate::mechanics::health_track())?.get()).ok()?;
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

    pub fn explosive_prop(&self, entity: EntityId) -> Option<ExplosivePropView> {
        let component = self.explosive_props.get(&entity)?;
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
        if self.enemies.contains_key(&entity) {
            self.enemies
                .get(&entity)
                .is_some_and(|enemy| enemy.state == EnemyState::Alive)
                && EncounterService::enemy_is_active(self, entity)
        } else {
            self.explosive_props.contains_key(&entity)
        }
    }

    pub fn hazard(&self, entity: EntityId) -> Option<HazardView> {
        let component = self.hazards.get(&entity)?;
        Some(HazardView {
            entity,
            config: component.config,
            ready_at_tick: component.ready_at_tick,
        })
    }

    pub fn hazards(&self) -> impl ExactSizeIterator<Item = HazardView> + '_ {
        self.hazards.iter().map(|(entity, component)| HazardView {
            entity: *entity,
            config: component.config,
            ready_at_tick: component.ready_at_tick,
        })
    }

    pub fn encounter(&self, entity: EntityId) -> Option<EncounterView> {
        let component = self.encounters.get(&entity)?;
        Some(EncounterView {
            entity,
            members: component.config.members.clone(),
            exit: component.config.exit,
            activation_radius: component.config.activation_radius,
            state: component.state,
        })
    }

    pub fn navigation(&self, entity: EntityId) -> Option<NavigationView> {
        let component = self.navigators.get(&entity)?;
        Some(NavigationView {
            entity,
            config: component.config,
            state: component.state,
            entity_view: self.entities.view(entity).ok()?,
        })
    }

    pub fn extraction_beacon(&self, entity: EntityId) -> Option<ExtractionBeaconView> {
        let component = self.extraction_beacons.get(&entity)?;
        Some(ExtractionBeaconView {
            entity,
            config: component.config,
            state: component.state,
            entity_view: self.entities.view(entity).ok()?,
        })
    }

    pub fn player_controller(&self, entity: EntityId) -> Option<PlayerControllerView> {
        let component = self.player_controllers.get(&entity)?;
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
        if let Some(controller) = self.player_controllers.get(&entity) {
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
        self.pickups
            .get(&entity)
            .map(|component| pickup_view(entity, component))
    }

    pub fn pickups(&self) -> impl ExactSizeIterator<Item = PickupView> + '_ {
        self.pickups
            .iter()
            .map(|(entity, component)| pickup_view(*entity, component))
    }

    pub fn secret_region(&self, entity: EntityId) -> Option<SecretRegionView> {
        let component = self.secret_regions.get(&entity)?;
        Some(SecretRegionView {
            entity,
            config: component.config.clone(),
            state: component.state.clone(),
            entity_view: self.entities.view(entity).ok()?,
        })
    }

    pub fn secret_regions(&self) -> impl ExactSizeIterator<Item = SecretRegionView> + '_ {
        self.secret_regions
            .iter()
            .map(|(entity, component)| SecretRegionView {
                entity: *entity,
                config: component.config.clone(),
                state: component.state.clone(),
                entity_view: self
                    .entities
                    .view(*entity)
                    .expect("admitted secret region remains viewable"),
            })
    }

    pub fn level_exit(&self, entity: EntityId) -> Option<LevelExitView> {
        let component = self.level_exits.get(&entity)?;
        Some(LevelExitView {
            entity,
            config: component.config.clone(),
            state: component.state,
            entity_view: self.entities.view(entity).ok()?,
        })
    }

    pub fn level_exits(&self) -> impl ExactSizeIterator<Item = LevelExitView> + '_ {
        self.level_exits
            .iter()
            .map(|(entity, component)| LevelExitView {
                entity: *entity,
                config: component.config.clone(),
                state: component.state,
                entity_view: self
                    .entities
                    .view(*entity)
                    .expect("admitted level exit remains viewable"),
            })
    }

    pub fn level_complete(&self) -> bool {
        self.level_exits
            .values()
            .any(|component| matches!(component.state, LevelExitState::Completed { .. }))
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
    floor_actions: &BTreeMap<EntityId, FloorActionComponent>,
    lifts: &BTreeMap<EntityId, LiftComponent>,
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
    if floor_actions.contains_key(&target_platform) || lifts.contains_key(&target_platform) {
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
