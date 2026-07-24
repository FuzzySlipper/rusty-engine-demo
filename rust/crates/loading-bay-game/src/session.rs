use std::collections::{BTreeMap, BTreeSet};

use core_ids::EntityId;
use core_math::Vec3;
use core_time::Tick;
use entity_state::{EntityState, EntityView};

use crate::combat::{
    EnemyComponent, EnemyState, EnemyView, HealthComponent, HealthView, WeaponComponent,
    WeaponState, WeaponView,
};
use crate::definition::{GameEntityDefinition, GameEntityDefinitionError};
use crate::door::{DoorComponent, DoorState, DoorView};
use crate::encounter::{EncounterComponent, EncounterState, EncounterView};
use crate::interaction::{SwitchComponent, SwitchView};
use crate::navigation::{
    NavigationComponent, NavigationState, NavigationView, MAX_NAVIGATION_QUERY_BUDGET,
    MAX_NAVIGATION_SPEED_UNITS_PER_SECOND,
};
use crate::player::{PlayerControllerComponent, PlayerControllerState, PlayerControllerView};

#[derive(Debug)]
pub struct GameSession {
    pub(crate) entities: EntityState,
    pub(crate) doors: BTreeMap<EntityId, DoorComponent>,
    pub(crate) switches: BTreeMap<EntityId, SwitchComponent>,
    pub(crate) controls: BTreeMap<EntityId, Vec<EntityId>>,
    pub(crate) enemies: BTreeMap<EntityId, EnemyComponent>,
    pub(crate) health: BTreeMap<EntityId, HealthComponent>,
    pub(crate) encounters: BTreeMap<EntityId, EncounterComponent>,
    pub(crate) navigators: BTreeMap<EntityId, NavigationComponent>,
    pub(crate) player_controllers: BTreeMap<EntityId, PlayerControllerComponent>,
    pub(crate) weapons: BTreeMap<EntityId, WeaponComponent>,
}

impl GameSession {
    pub fn from_definitions(
        definitions: impl IntoIterator<Item = GameEntityDefinition>,
    ) -> Result<Self, GameEntityDefinitionError> {
        let definitions: Vec<GameEntityDefinition> = definitions.into_iter().collect();
        let entities = EntityState::from_definitions(
            definitions
                .iter()
                .map(|definition| definition.entity.clone()),
        )
        .map_err(GameEntityDefinitionError::EntityState)?;

        let mut doors = BTreeMap::new();
        let mut switches = BTreeMap::new();
        let mut controls = BTreeMap::new();
        let mut enemies = BTreeMap::new();
        let mut health = BTreeMap::new();
        let mut encounters = BTreeMap::new();
        let mut navigators = BTreeMap::new();
        let mut player_controllers = BTreeMap::new();
        let mut weapons = BTreeMap::new();

        for definition in &definitions {
            let entity = definition.entity.id;
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
            if let Some(config) = definition.weapon {
                if definition.player_controller.is_none() {
                    return Err(GameEntityDefinitionError::WeaponWithoutPlayerController {
                        entity,
                    });
                }
                if !config.is_valid() {
                    return Err(GameEntityDefinitionError::InvalidWeaponConfig { entity });
                }
                weapons.insert(
                    entity,
                    WeaponComponent {
                        config,
                        state: WeaponState {
                            ammo_remaining: config.ammo_capacity,
                            ready_at_tick: Tick::ZERO,
                        },
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
                encounters.insert(
                    entity,
                    EncounterComponent {
                        config: config.clone(),
                        state: EncounterState::Active,
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
            switches,
            controls,
            enemies,
            health,
            encounters,
            navigators,
            player_controllers,
            weapons,
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

    pub fn switch(&self, entity: EntityId) -> Option<SwitchView> {
        let component = self.switches.get(&entity)?;
        Some(SwitchView {
            entity,
            activation_count: component.activation_count,
            controls_targets: self.controls.get(&entity).cloned().unwrap_or_default(),
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

    pub fn player_controller(&self, entity: EntityId) -> Option<PlayerControllerView> {
        let component = self.player_controllers.get(&entity)?;
        Some(PlayerControllerView {
            entity,
            config: component.config.clone(),
            state: component.state,
            entity_view: self.entities.view(entity).ok()?,
        })
    }

    pub fn weapon(&self, entity: EntityId) -> Option<WeaponView> {
        let component = self.weapons.get(&entity)?;
        Some(WeaponView {
            entity,
            config: component.config,
            state: component.state,
        })
    }
}

fn vec3_is_finite(value: Vec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}
