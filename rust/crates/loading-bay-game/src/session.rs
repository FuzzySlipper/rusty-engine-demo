use std::collections::{BTreeMap, BTreeSet};

use core_ids::EntityId;
use core_math::Vec3;
use core_time::Tick;
use engine_spatial::MAX_TRIGGER_DEFINITIONS;
use entity_state::{EntityState, EntityView};

use crate::combat::{EnemyComponent, EnemyState, EnemyView, WeaponState, WeaponView};
use crate::definition::{GameEntityDefinition, GameEntityDefinitionError};
use crate::door::{DoorComponent, DoorState, DoorView};
use crate::encounter::{EncounterComponent, EncounterState, EncounterView};
use crate::extraction_beacon::{
    ExtractionBeaconComponent, ExtractionBeaconState, ExtractionBeaconView,
};
use crate::hazard::{HazardComponent, HazardView};
use crate::interaction::{SwitchComponent, SwitchView};
use crate::inventory::{
    admit_item_definitions, inventory_from_config, inventory_view, InventoryComponent,
    InventoryView, ItemDefinition, ItemDefinitionId, ItemDefinitionView,
};
use crate::navigation::{
    NavigationComponent, NavigationState, NavigationView, MAX_NAVIGATION_QUERY_BUDGET,
    MAX_NAVIGATION_SPEED_UNITS_PER_SECOND,
};
use crate::pickup::{pickup_view, PickupComponent, PickupState, PickupView};
use crate::player::{PlayerControllerComponent, PlayerControllerState, PlayerControllerView};
use crate::progression::{
    DoorAccessConfig, DoorAccessView, LevelExitComponent, LevelExitState, LevelExitView,
    LoadingBayInterlockConfig, LoadingBayInterlockView, SecretRegionComponent, SecretRegionState,
    SecretRegionView,
};
use crate::vitality::{HealthComponent, HealthView, VitalityState};

#[derive(Debug, Clone)]
pub struct GameSession {
    pub(crate) entities: EntityState,
    pub(crate) doors: BTreeMap<EntityId, DoorComponent>,
    pub(crate) door_access: BTreeMap<EntityId, DoorAccessConfig>,
    pub(crate) switches: BTreeMap<EntityId, SwitchComponent>,
    pub(crate) controls: BTreeMap<EntityId, Vec<EntityId>>,
    pub(crate) loading_bay_interlocks: BTreeMap<EntityId, LoadingBayInterlockConfig>,
    pub(crate) enemies: BTreeMap<EntityId, EnemyComponent>,
    pub(crate) health: BTreeMap<EntityId, HealthComponent>,
    pub(crate) hazards: BTreeMap<EntityId, HazardComponent>,
    pub(crate) encounters: BTreeMap<EntityId, EncounterComponent>,
    pub(crate) extraction_beacons: BTreeMap<EntityId, ExtractionBeaconComponent>,
    pub(crate) navigators: BTreeMap<EntityId, NavigationComponent>,
    pub(crate) player_controllers: BTreeMap<EntityId, PlayerControllerComponent>,
    pub(crate) item_definitions: BTreeMap<ItemDefinitionId, ItemDefinition>,
    pub(crate) inventories: BTreeMap<EntityId, InventoryComponent>,
    pub(crate) pickups: BTreeMap<EntityId, PickupComponent>,
    pub(crate) secret_regions: BTreeMap<EntityId, SecretRegionComponent>,
    pub(crate) level_exits: BTreeMap<EntityId, LevelExitComponent>,
}

impl GameSession {
    pub fn from_definitions(
        definitions: impl IntoIterator<Item = GameEntityDefinition>,
    ) -> Result<Self, GameEntityDefinitionError> {
        Self::from_item_and_entity_definitions([], definitions)
    }

    pub fn from_item_and_entity_definitions(
        item_definitions: impl IntoIterator<Item = ItemDefinition>,
        definitions: impl IntoIterator<Item = GameEntityDefinition>,
    ) -> Result<Self, GameEntityDefinitionError> {
        let definitions: Vec<GameEntityDefinition> = definitions.into_iter().collect();
        let trigger_count = definitions
            .iter()
            .filter(|definition| {
                definition.pickup.is_some()
                    || definition.hazard.is_some()
                    || definition.secret_region.is_some()
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
        let entities = EntityState::from_definitions(
            definitions
                .iter()
                .map(|definition| definition.entity.clone()),
        )
        .map_err(GameEntityDefinitionError::EntityState)?;

        let mut doors = BTreeMap::new();
        let mut door_access = BTreeMap::new();
        let mut switches = BTreeMap::new();
        let mut controls = BTreeMap::new();
        let mut loading_bay_interlocks = BTreeMap::new();
        let mut enemies = BTreeMap::new();
        let mut health = BTreeMap::new();
        let mut hazards = BTreeMap::new();
        let mut encounters = BTreeMap::new();
        let mut extraction_beacons = BTreeMap::new();
        let mut navigators = BTreeMap::new();
        let mut player_controllers = BTreeMap::new();
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
                if view.collision.is_none() {
                    return Err(GameEntityDefinitionError::DoorMissingCollision { entity });
                }
                if view.renderable.is_none() {
                    return Err(GameEntityDefinitionError::DoorMissingRenderable { entity });
                }
                doors.insert(
                    entity,
                    DoorComponent {
                        config,
                        state: DoorState::Closed,
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
            if definition.switch {
                switches.insert(entity, SwitchComponent::default());
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
                health.insert(
                    entity,
                    HealthComponent {
                        config,
                        current: config.max,
                        armor: 0,
                        armor_item: None,
                        state: VitalityState::Alive,
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
                    || definition.weapon.is_some()
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
            if let Some(config) = &definition.player_controller {
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
                if view.kinematic.is_none() {
                    return Err(
                        GameEntityDefinitionError::PlayerControllerMissingKinematic { entity },
                    );
                }
                if view.renderable.is_none() {
                    return Err(
                        GameEntityDefinitionError::PlayerControllerMissingRenderable { entity },
                    );
                }
                if !config.is_valid() {
                    return Err(GameEntityDefinitionError::InvalidPlayerControllerConfig {
                        entity,
                    });
                }
                player_controllers.insert(
                    entity,
                    PlayerControllerComponent {
                        config: config.clone(),
                        state: PlayerControllerState {
                            yaw_degrees: config.initial_yaw_degrees,
                            pitch_degrees: config.initial_pitch_degrees,
                        },
                    },
                );
            }
            if let Some(config) = &definition.inventory {
                if definition.player_controller.is_none() {
                    return Err(GameEntityDefinitionError::Inventory(
                        crate::inventory::InventoryAdmissionError::InventoryWithoutPlayerController {
                            owner: entity,
                        },
                    ));
                }
                inventories.insert(
                    entity,
                    inventory_from_config(entity, config, &item_definitions)
                        .map_err(GameEntityDefinitionError::Inventory)?,
                );
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
                    || definition.weapon.is_some()
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
            if definition.weapon.is_some() {
                return Err(GameEntityDefinitionError::LegacyEntityWeapon { entity });
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
                encounters.insert(
                    entity,
                    EncounterComponent {
                        config: config.clone(),
                        state: EncounterState::Active,
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

        let mut encounter_by_enemy = BTreeMap::new();
        for (encounter, component) in &encounters {
            if !entities.contains(component.config.exit) {
                return Err(GameEntityDefinitionError::UnknownEncounterExit {
                    encounter: *encounter,
                    exit: component.config.exit,
                });
            }
            if !doors.contains_key(&component.config.exit) {
                return Err(GameEntityDefinitionError::EncounterExitIsNotDoor {
                    encounter: *encounter,
                    exit: component.config.exit,
                });
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

        Ok(Self {
            entities,
            doors,
            door_access,
            switches,
            controls,
            loading_bay_interlocks,
            enemies,
            health,
            hazards,
            encounters,
            extraction_beacons,
            navigators,
            player_controllers,
            item_definitions,
            inventories,
            pickups,
            secret_regions,
            level_exits,
        })
    }

    pub fn entities(&self) -> &EntityState {
        &self.entities
    }

    pub fn entity(&self, entity: EntityId) -> Result<EntityView, entity_state::ViewError> {
        self.entities.view(entity)
    }

    pub fn door(&self, entity: EntityId) -> Option<DoorView> {
        let component = self.doors.get(&entity)?;
        Some(DoorView {
            entity,
            config: component.config,
            state: component.state,
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
            activation_count: component.activation_count,
            controls_targets: self.controls.get(&entity).cloned().unwrap_or_default(),
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

    pub fn health(&self, entity: EntityId) -> Option<HealthView> {
        let component = self.health.get(&entity)?;
        Some(HealthView {
            entity,
            config: component.config,
            current: component.current,
            armor: component.armor,
            armor_item: component.armor_item.clone(),
            state: component.state,
        })
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
        Some(PlayerControllerView {
            entity,
            config: component.config.clone(),
            state: component.state,
            entity_view: self.entities.view(entity).ok()?,
        })
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
        self.inventories
            .get(&owner)
            .map(|component| inventory_view(owner, component))
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
        let item = inventory.equipped_weapon.clone()?;
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
}

fn vec3_is_finite(value: Vec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}
