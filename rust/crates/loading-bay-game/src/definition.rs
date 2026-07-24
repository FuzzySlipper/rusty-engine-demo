use core_ids::EntityId;
use entity_state::EntityDefinition;

use crate::combat::{HealthConfig, WeaponConfig};
use crate::door::DoorConfig;
use crate::encounter::EncounterConfig;
use crate::navigation::NavigationConfig;
use crate::player::PlayerControllerConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct GameEntityDefinition {
    pub entity: EntityDefinition,
    pub door: Option<DoorConfig>,
    pub switch: bool,
    pub controls_targets: Vec<EntityId>,
    pub enemy: bool,
    pub health: Option<HealthConfig>,
    pub encounter: Option<EncounterConfig>,
    pub navigation: Option<NavigationConfig>,
    pub player_controller: Option<PlayerControllerConfig>,
    pub weapon: Option<WeaponConfig>,
}

impl GameEntityDefinition {
    pub fn new(entity: EntityDefinition) -> Self {
        Self {
            entity,
            door: None,
            switch: false,
            controls_targets: Vec::new(),
            enemy: false,
            health: None,
            encounter: None,
            navigation: None,
            player_controller: None,
            weapon: None,
        }
    }

    pub fn as_door(mut self, config: DoorConfig) -> Self {
        self.door = Some(config);
        self
    }

    pub fn as_switch(mut self) -> Self {
        self.switch = true;
        self
    }

    pub fn controls(mut self, targets: impl IntoIterator<Item = EntityId>) -> Self {
        self.controls_targets = targets.into_iter().collect();
        self
    }

    pub fn as_enemy(mut self) -> Self {
        self.enemy = true;
        self
    }

    pub fn with_health(mut self, config: HealthConfig) -> Self {
        self.health = Some(config);
        self
    }

    pub fn as_encounter(
        mut self,
        members: impl IntoIterator<Item = EntityId>,
        exit: EntityId,
    ) -> Self {
        self.encounter = Some(EncounterConfig {
            members: members.into_iter().collect(),
            exit,
        });
        self
    }

    pub fn with_navigation(mut self, config: NavigationConfig) -> Self {
        self.navigation = Some(config);
        self
    }

    pub fn with_player_controller(mut self, config: PlayerControllerConfig) -> Self {
        self.player_controller = Some(config);
        self
    }

    pub fn with_weapon(mut self, config: WeaponConfig) -> Self {
        self.weapon = Some(config);
        self
    }
}

#[derive(Debug)]
pub enum GameEntityDefinitionError {
    EntityState(entity_state::EntityDefinitionError),
    DuplicateControlTarget {
        switch: EntityId,
        target: EntityId,
    },
    ControlsWithoutSwitch {
        entity: EntityId,
    },
    UnknownControlTarget {
        switch: EntityId,
        target: EntityId,
    },
    ControlTargetIsNotDoor {
        switch: EntityId,
        target: EntityId,
    },
    DoorMissingTransform {
        entity: EntityId,
    },
    DoorMissingCollision {
        entity: EntityId,
    },
    DoorMissingRenderable {
        entity: EntityId,
    },
    EnemyMissingCollision {
        entity: EntityId,
    },
    EnemyMissingRenderable {
        entity: EntityId,
    },
    HealthMissingTransform {
        entity: EntityId,
    },
    HealthMissingCollision {
        entity: EntityId,
    },
    InvalidHealthConfig {
        entity: EntityId,
    },
    NavigationWithoutEnemy {
        entity: EntityId,
    },
    NavigationMissingTransform {
        entity: EntityId,
    },
    NavigationMissingCollision {
        entity: EntityId,
    },
    NavigationMissingKinematic {
        entity: EntityId,
    },
    InvalidNavigationGoal {
        entity: EntityId,
    },
    InvalidNavigationSpeed {
        entity: EntityId,
    },
    InvalidNavigationQueryBudget {
        entity: EntityId,
    },
    PlayerControllerMissingTransform {
        entity: EntityId,
    },
    PlayerControllerMissingCollision {
        entity: EntityId,
    },
    PlayerControllerMissingKinematic {
        entity: EntityId,
    },
    PlayerControllerMissingRenderable {
        entity: EntityId,
    },
    InvalidPlayerControllerConfig {
        entity: EntityId,
    },
    WeaponWithoutPlayerController {
        entity: EntityId,
    },
    InvalidWeaponConfig {
        entity: EntityId,
    },
    EmptyEncounter {
        encounter: EntityId,
    },
    DuplicateEncounterMember {
        encounter: EntityId,
        member: EntityId,
    },
    UnknownEncounterMember {
        encounter: EntityId,
        member: EntityId,
    },
    EncounterMemberIsNotEnemy {
        encounter: EntityId,
        member: EntityId,
    },
    UnknownEncounterExit {
        encounter: EntityId,
        exit: EntityId,
    },
    EncounterExitIsNotDoor {
        encounter: EntityId,
        exit: EntityId,
    },
    EnemyInMultipleEncounters {
        enemy: EntityId,
        first: EntityId,
        second: EntityId,
    },
}

impl std::fmt::Display for GameEntityDefinitionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for GameEntityDefinitionError {}
