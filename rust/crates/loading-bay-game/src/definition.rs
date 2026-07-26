use core_ids::EntityId;
use entity_state::EntityDefinition;

use crate::combat::WeaponConfig;
use crate::door::DoorConfig;
use crate::encounter::EncounterConfig;
use crate::extraction_beacon::ExtractionBeaconConfig;
use crate::hazard::HazardConfig;
use crate::inventory::{InventoryAdmissionError, InventoryConfig};
use crate::navigation::NavigationConfig;
use crate::pickup::PickupConfig;
use crate::player::PlayerControllerConfig;
use crate::progression::{
    DoorAccessConfig, LevelExitConfig, LoadingBayInterlockConfig, SecretRegionConfig,
};
use crate::vitality::HealthConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct GameEntityDefinition {
    pub entity: EntityDefinition,
    pub door: Option<DoorConfig>,
    pub door_access: Option<DoorAccessConfig>,
    pub switch: bool,
    pub controls_targets: Vec<EntityId>,
    pub loading_bay_interlock: Option<LoadingBayInterlockConfig>,
    pub enemy: bool,
    pub health: Option<HealthConfig>,
    pub hazard: Option<HazardConfig>,
    pub encounter: Option<EncounterConfig>,
    pub extraction_beacon: Option<ExtractionBeaconConfig>,
    pub navigation: Option<NavigationConfig>,
    pub player_controller: Option<PlayerControllerConfig>,
    pub inventory: Option<InventoryConfig>,
    pub pickup: Option<PickupConfig>,
    pub weapon: Option<WeaponConfig>,
    pub secret_region: Option<SecretRegionConfig>,
    pub level_exit: Option<LevelExitConfig>,
}

impl GameEntityDefinition {
    pub fn new(entity: EntityDefinition) -> Self {
        Self {
            entity,
            door: None,
            door_access: None,
            switch: false,
            controls_targets: Vec::new(),
            loading_bay_interlock: None,
            enemy: false,
            health: None,
            hazard: None,
            encounter: None,
            extraction_beacon: None,
            navigation: None,
            player_controller: None,
            inventory: None,
            pickup: None,
            weapon: None,
            secret_region: None,
            level_exit: None,
        }
    }

    pub fn as_door(mut self, config: DoorConfig) -> Self {
        self.door = Some(config);
        self
    }

    pub fn with_door_access(mut self, config: DoorAccessConfig) -> Self {
        self.door_access = Some(config);
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

    pub fn with_loading_bay_interlock(mut self, config: LoadingBayInterlockConfig) -> Self {
        self.loading_bay_interlock = Some(config);
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

    pub fn as_hazard(mut self, config: HazardConfig) -> Self {
        self.hazard = Some(config);
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

    pub fn with_extraction_beacon(mut self, config: ExtractionBeaconConfig) -> Self {
        self.extraction_beacon = Some(config);
        self
    }

    pub fn with_player_controller(mut self, config: PlayerControllerConfig) -> Self {
        self.player_controller = Some(config);
        self
    }

    pub fn with_inventory(mut self, config: InventoryConfig) -> Self {
        self.inventory = Some(config);
        self
    }

    pub fn as_pickup(mut self, config: PickupConfig) -> Self {
        self.pickup = Some(config);
        self
    }

    pub fn with_weapon(mut self, config: WeaponConfig) -> Self {
        self.weapon = Some(config);
        self
    }

    pub fn as_secret_region(mut self, config: SecretRegionConfig) -> Self {
        self.secret_region = Some(config);
        self
    }

    pub fn as_level_exit(mut self, config: LevelExitConfig) -> Self {
        self.level_exit = Some(config);
        self
    }
}

#[derive(Debug)]
pub enum GameEntityDefinitionError {
    EntityState(entity_state::EntityDefinitionError),
    Inventory(InventoryAdmissionError),
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
    DoorAccessWithoutDoor {
        entity: EntityId,
    },
    InvalidDoorAccessConfig {
        entity: EntityId,
    },
    DoorAccessKeyMissingDefinition {
        entity: EntityId,
    },
    DoorAccessKeyNotAccessKey {
        entity: EntityId,
    },
    LoadingBayInterlockWithoutSwitch {
        entity: EntityId,
    },
    InvalidLoadingBayInterlock {
        switch: EntityId,
        target: EntityId,
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
    HazardMissingTransform {
        entity: EntityId,
    },
    HazardMissingBounds {
        entity: EntityId,
    },
    HazardMissingRenderable {
        entity: EntityId,
    },
    InvalidHazardConfig {
        entity: EntityId,
    },
    HazardConflictsWithGameplayOwner {
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
    WeaponBindingSlotMismatch {
        entity: EntityId,
        binding_count: usize,
        slot_count: usize,
    },
    PickupMissingTransform {
        entity: EntityId,
    },
    PickupMissingBounds {
        entity: EntityId,
    },
    PickupMissingRenderable {
        entity: EntityId,
    },
    PickupMissingItemDefinition {
        entity: EntityId,
    },
    InvalidPickupQuantity {
        entity: EntityId,
    },
    SecretRegionMissingTransform {
        entity: EntityId,
    },
    SecretRegionMissingBounds {
        entity: EntityId,
    },
    InvalidSecretRegionConfig {
        entity: EntityId,
    },
    LevelExitMissingTransform {
        entity: EntityId,
    },
    LevelExitMissingRenderable {
        entity: EntityId,
    },
    InvalidLevelExitConfig {
        entity: EntityId,
    },
    TooManyPickups {
        count: usize,
        limit: usize,
    },
    InvalidPickupStarterAmmunition {
        entity: EntityId,
    },
    PickupConflictsWithGameplayOwner {
        entity: EntityId,
    },
    WeaponWithoutPlayerController {
        entity: EntityId,
    },
    InvalidWeaponConfig {
        entity: EntityId,
    },
    LegacyEntityWeapon {
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
    ExtractionBeaconMissingTransform {
        entity: EntityId,
    },
    ExtractionBeaconMissingRenderable {
        entity: EntityId,
    },
    InvalidExtractionBeaconConfig {
        entity: EntityId,
    },
}

impl std::fmt::Display for GameEntityDefinitionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for GameEntityDefinitionError {}
