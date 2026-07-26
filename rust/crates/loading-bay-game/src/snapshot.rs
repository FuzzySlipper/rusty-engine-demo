use std::collections::{BTreeMap, BTreeSet, VecDeque};

use core_ids::EntityId;
use core_math::Vec3;
use core_time::{Tick, TickDelta};
use engine_spatial::{
    GeneratedRoomConfig, MaterialVoxel, TriggerVolumeSnapshot, TriggerVolumeSystem,
    VoxelCollisionScene, VoxelSourceRevision, GENERATED_ROOM_VERSION,
};
use entity_state::{EntityLifecycle, EntityState, EntityStateSnapshot};
use serde::{Deserialize, Serialize};

use crate::combat::{EnemyComponent, EnemyState};
use crate::door::{DoorComponent, DoorConfig, DoorState};
use crate::encounter::{
    EncounterComponent, EncounterConfig, EncounterState, MAX_ENCOUNTER_ACTIVATION_RADIUS,
};
use crate::enemy_combat::{
    EnemyAttackConfig, EnemyAttackKind, EnemyCombatComponent, EnemyCombatConfig,
    EnemyCombatPosture, EnemyCombatState, EnemyPerceptionConfig,
};
use crate::enemy_drop::{EnemyDropComponent, EnemyDropConfig, EnemyDropState};
use crate::extraction_beacon::{
    ExtractionBeaconComponent, ExtractionBeaconConfig, ExtractionBeaconState,
};
use crate::hazard::{
    HazardComponent, HazardConfig, HAZARD_TRIGGER_SCOPE, MAX_HAZARD_COOLDOWN_TICKS,
};
use crate::interaction::SwitchComponent;
use crate::inventory::{
    admit_item_definitions, inventory_from_config, InventoryAdmissionError, InventoryConfig,
    InventoryStack, ItemDefinition, ItemDefinitionId, ItemKind, WeaponAttackMode, WeaponDefinition,
};
use crate::navigation::{
    NavigationComponent, NavigationConfig, NavigationState, MAX_NAVIGATION_QUERY_BUDGET,
    MAX_NAVIGATION_SPEED_UNITS_PER_SECOND,
};
use crate::pickup::{
    PickupCollectionCause, PickupComponent, PickupConfig, PickupState, PICKUP_TRIGGER_SCOPE,
};
use crate::player::{
    PlayerControllerComponent, PlayerControllerConfig, PlayerControllerState, PlayerInputBindings,
};
use crate::progression::{
    DoorAccessConfig, LevelExitComponent, LevelExitConfig, LevelExitState,
    LoadingBayInterlockConfig, RequiredKeyPolicy, SecretRegionComponent, SecretRegionConfig,
    SecretRegionState, SECRET_TRIGGER_SCOPE,
};
use crate::runtime::GameRuntime;
use crate::scheduler::{ScheduledIntent, ScheduledIntentKind, Scheduler};
use crate::session::GameSession;
use crate::vitality::{HealthComponent, HealthConfig, VitalityState};

pub const GAME_SNAPSHOT_SCHEMA_VERSION: u32 = 18;
const INVENTORY_WEAPON_SNAPSHOT_SCHEMA_VERSION: u32 = 13;
const VITALITY_SNAPSHOT_SCHEMA_VERSION: u32 = 14;
const PROGRESSION_SNAPSHOT_SCHEMA_VERSION: u32 = 16;
const ENEMY_COMBAT_SNAPSHOT_SCHEMA_VERSION: u32 = 17;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GameSnapshot {
    pub schema_version: u32,
    pub tick: u64,
    pub entities: EntityStateSnapshot,
    #[serde(default)]
    pub item_definitions: Vec<ItemDefinitionSnapshot>,
    pub voxel_collision: Option<VoxelCollisionSnapshot>,
    pub doors: Vec<DoorSnapshot>,
    pub switches: Vec<SwitchSnapshot>,
    pub extraction_beacons: Vec<ExtractionBeaconSnapshot>,
    pub controls: Vec<ControlsSnapshot>,
    pub enemies: Vec<EnemySnapshot>,
    #[serde(default)]
    pub enemy_combat: Vec<EnemyCombatSnapshot>,
    #[serde(default)]
    pub enemy_drops: Vec<EnemyDropSnapshot>,
    pub health: Vec<HealthSnapshot>,
    #[serde(default)]
    pub hazards: Vec<HazardSnapshot>,
    pub encounters: Vec<EncounterSnapshot>,
    pub navigations: Vec<NavigationSnapshot>,
    pub player_controllers: Vec<PlayerControllerSnapshot>,
    #[serde(default)]
    pub inventories: Vec<InventorySnapshot>,
    #[serde(default)]
    pub pickups: Vec<PickupSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pickup_triggers: Option<TriggerVolumeSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hazard_triggers: Option<TriggerVolumeSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weapons: Vec<WeaponSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progression: Option<ProgressionSnapshot>,
    pub scheduled: Vec<ScheduledSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VoxelCollisionSnapshot {
    pub voxel_size: f64,
    pub chunk_size: u32,
    pub source_revision: u64,
    pub authority_hash: u64,
    pub material_voxels: Vec<MaterialVoxelSnapshot>,
    pub generated_room: Option<GeneratedRoomSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ItemDefinitionSnapshot {
    pub id: String,
    pub max_quantity: u32,
    pub kind: SnapshotItemKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SnapshotItemKind {
    Weapon {
        ammunition: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attack_mode: Option<SnapshotWeaponAttackMode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pellet_count: Option<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spread_degrees: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        damage: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_distance: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cooldown_ticks: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ammunition_cost: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        muzzle_offset: Option<[f32; 3]>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        presentation: Option<String>,
    },
    Ammunition,
    AccessKey,
    HealthSupply {
        restore_health: u32,
    },
    Armor {
        protection: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotWeaponAttackMode {
    Hitscan,
    Spread,
    Automatic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InventorySnapshot {
    pub owner: u64,
    pub capacity_slots: usize,
    pub stacks: Vec<InventoryStackSnapshot>,
    pub equipped_weapon: Option<String>,
    #[serde(default)]
    pub weapon_slots: Vec<String>,
    #[serde(default)]
    pub weapon_cooldowns: Vec<WeaponCooldownSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WeaponCooldownSnapshot {
    pub item: String,
    pub ready_at_tick: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InventoryStackSnapshot {
    pub item: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PickupSnapshot {
    pub entity: u64,
    pub item: String,
    pub quantity: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starter_ammunition: Option<InventoryStackSnapshot>,
    pub state: SnapshotPickupState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SnapshotPickupState {
    Dormant,
    Available,
    Collected {
        actor: u64,
        collected_at_tick: u64,
        cause: SnapshotPickupCollectionCause,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SnapshotPickupCollectionCause {
    Overlap {
        trigger_revision: u64,
    },
    Interaction {
        connection_generation: u64,
        command_sequence: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MaterialVoxelSnapshot {
    pub address: [i64; 3],
    pub material_slot: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GeneratedRoomSnapshot {
    pub generator_version: u32,
    pub seed: u64,
    pub width: u32,
    pub height: u32,
    pub length: u32,
    pub output_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DoorSnapshot {
    pub entity: u64,
    pub state: SnapshotDoorState,
    pub closed_translation: [f32; 3],
    pub open_translation: [f32; 3],
    pub auto_close_after_ticks: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotDoorState {
    Closed,
    Open,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SwitchSnapshot {
    pub entity: u64,
    pub activation_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExtractionBeaconSnapshot {
    pub entity: u64,
    pub activation_radius: f32,
    pub state: SnapshotExtractionBeaconState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SnapshotExtractionBeaconState {
    Standby,
    Active { actor: u64, activated_at_tick: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ControlsSnapshot {
    pub switch: u64,
    pub targets: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EnemySnapshot {
    pub entity: u64,
    pub state: SnapshotEnemyState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotEnemyState {
    Alive,
    Defeated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EnemyCombatSnapshot {
    pub entity: u64,
    pub sight_range: f32,
    pub hearing_range: f32,
    pub attack_kind: SnapshotEnemyAttackKind,
    pub damage: u32,
    pub range: f32,
    pub cooldown_ticks: u64,
    pub origin_offset: [f32; 3],
    pub presentation: String,
    pub posture: SnapshotEnemyCombatPosture,
    pub ready_at_tick: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_known_target_position: Option<[f32; 3]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotEnemyAttackKind {
    Melee,
    RangedHitscan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotEnemyCombatPosture {
    Sleeping,
    Alert,
    Pursuing,
    Attacking,
    Dead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EnemyDropSnapshot {
    pub enemy: u64,
    pub pickup: u64,
    pub state: SnapshotEnemyDropState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotEnemyDropState {
    Armed,
    Materialized,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HealthSnapshot {
    pub entity: u64,
    pub current: u32,
    pub max: u32,
    pub hitbox_half_extents: [f32; 3],
    #[serde(default)]
    pub max_armor: u32,
    #[serde(default)]
    pub armor_absorption_percent: u8,
    #[serde(default)]
    pub armor: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armor_item: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<SnapshotVitalityState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotVitalityState {
    Alive,
    Dead,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HazardSnapshot {
    pub entity: u64,
    pub damage: u32,
    pub cooldown_ticks: u64,
    pub ready_at_tick: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EncounterSnapshot {
    pub entity: u64,
    pub state: SnapshotEncounterState,
    pub members: Vec<u64>,
    pub exit: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activation_radius: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotEncounterState {
    Dormant,
    Active,
    Cleared,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct NavigationSnapshot {
    pub entity: u64,
    pub state: SnapshotNavigationState,
    pub goal: [f32; 3],
    pub speed_units_per_second: f32,
    pub max_visited: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotNavigationState {
    Following,
    Arrived,
    Blocked,
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PlayerControllerSnapshot {
    pub entity: u64,
    pub move_speed_units_per_second: f32,
    pub move_step_seconds: f32,
    pub look_degrees_per_unit: f32,
    pub initial_yaw_degrees: f32,
    pub initial_pitch_degrees: f32,
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    pub bindings: PlayerInputBindingsSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PlayerInputBindingsSnapshot {
    pub move_forward: String,
    pub move_backward: String,
    pub move_left: String,
    pub move_right: String,
    pub mouse_look: String,
    pub primary_fire: String,
    #[serde(default)]
    pub select_weapon: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WeaponSnapshot {
    pub entity: u64,
    pub damage: u32,
    pub max_distance: f32,
    pub cooldown_ticks: u64,
    pub ammo_capacity: u32,
    pub muzzle_offset: [f32; 3],
    pub ammo_remaining: u32,
    pub ready_at_tick: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ScheduledSnapshot {
    pub due_tick: u64,
    pub kind: ScheduledSnapshotKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ScheduledSnapshotKind {
    CloseDoor { door: u64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProgressionSnapshot {
    pub door_access: Vec<DoorAccessSnapshot>,
    pub loading_bay_interlocks: Vec<LoadingBayInterlockSnapshot>,
    pub secret_regions: Vec<SecretRegionSnapshot>,
    pub level_exits: Vec<LevelExitSnapshot>,
    pub secret_triggers: TriggerVolumeSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DoorAccessSnapshot {
    pub door: u64,
    pub required_key: String,
    pub key_policy: SnapshotRequiredKeyPolicy,
    pub activation_radius: f32,
    pub denied_presentation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotRequiredKeyPolicy {
    Retain,
    Consume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LoadingBayInterlockSnapshot {
    pub switch: u64,
    pub close_door: u64,
    pub open_door: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SecretRegionSnapshot {
    pub entity: u64,
    pub presentation: String,
    pub state: SnapshotSecretRegionState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SnapshotSecretRegionState {
    Undiscovered,
    Discovered { actor: u64, discovered_at_tick: u64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LevelExitSnapshot {
    pub entity: u64,
    pub activation_radius: f32,
    pub presentation: String,
    pub state: SnapshotLevelExitState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SnapshotLevelExitState {
    Available,
    Completed { actor: u64, completed_at_tick: u64 },
}

#[derive(Debug)]
pub enum GameSnapshotError {
    Encode(serde_json::Error),
    Decode(serde_json::Error),
    UnsupportedSchema {
        actual: u32,
    },
    EntityState(entity_state::EntityStateSnapshotError),
    CollisionScene(engine_spatial::CollisionSceneError),
    AmbiguousVoxelSnapshot,
    GeneratedRoomRevisionMismatch {
        actual: u64,
    },
    UnsupportedGeneratedRoomVersion {
        actual: u32,
    },
    GeneratedRoomHashMismatch {
        expected: u64,
        actual: u64,
    },
    VoxelAuthorityHashMismatch {
        expected: u64,
        actual: u64,
    },
    DuplicateDoor {
        entity: u64,
    },
    DuplicateSwitch {
        entity: u64,
    },
    DuplicateExtractionBeacon {
        entity: u64,
    },
    DuplicateEnemy {
        entity: u64,
    },
    DuplicateEnemyCombat {
        entity: u64,
    },
    DuplicateHealth {
        entity: u64,
    },
    DuplicateEncounter {
        entity: u64,
    },
    DuplicateNavigation {
        entity: u64,
    },
    DuplicatePlayerController {
        entity: u64,
    },
    DuplicateWeapon {
        entity: u64,
    },
    InvalidItemDefinitionId {
        value: String,
    },
    Inventory(InventoryAdmissionError),
    FutureInventoryStateInLegacySnapshot,
    TriggerVolume(engine_spatial::TriggerVolumeError),
    DuplicateInventory {
        owner: u64,
    },
    UnknownInventoryEntity {
        owner: u64,
    },
    DuplicatePickup {
        entity: u64,
    },
    TooManyPickups {
        count: usize,
        limit: usize,
    },
    UnknownPickupEntity {
        entity: u64,
    },
    InvalidPickup {
        entity: u64,
    },
    PickupCollectionFromFuture {
        entity: u64,
        collected_at_tick: u64,
        snapshot_tick: u64,
    },
    InvalidPickupTriggerDefinitions,
    InvalidHazardTriggerDefinitions,
    FuturePickupStateInLegacySnapshot,
    FutureWeaponStateInLegacySnapshot,
    FutureVitalityStateInLegacySnapshot,
    FutureProgressionStateInLegacySnapshot,
    FutureEnemyCombatStateInLegacySnapshot,
    FutureEnemyArchetypeStateInLegacySnapshot,
    InvalidProgressionState,
    InvalidSecretTriggerDefinitions,
    InvalidHazardConfig {
        entity: u64,
    },
    DuplicateHazard {
        entity: u64,
    },
    UnknownHazardEntity {
        entity: u64,
    },
    MissingHazardCapability {
        entity: u64,
    },
    InvalidWeaponCooldown {
        owner: u64,
        item: String,
    },
    UnknownDoorEntity {
        entity: u64,
    },
    UnknownSwitchEntity {
        entity: u64,
    },
    UnknownExtractionBeaconEntity {
        entity: u64,
    },
    UnknownExtractionBeaconActor {
        beacon: u64,
        actor: u64,
    },
    UnknownEnemyEntity {
        entity: u64,
    },
    UnknownEnemyCombatEntity {
        entity: u64,
    },
    UnknownHealthEntity {
        entity: u64,
    },
    UnknownEncounterEntity {
        entity: u64,
    },
    UnknownNavigationEntity {
        entity: u64,
    },
    UnknownPlayerControllerEntity {
        entity: u64,
    },
    UnknownWeaponEntity {
        entity: u64,
    },
    UnknownControlTarget {
        switch: u64,
        target: u64,
    },
    UnknownEncounterMember {
        encounter: u64,
        member: u64,
    },
    UnknownEncounterExit {
        encounter: u64,
        exit: u64,
    },
    MissingDoorCapability {
        entity: u64,
    },
    MissingExtractionBeaconCapability {
        entity: u64,
    },
    MissingEnemyCapability {
        entity: u64,
    },
    MissingEnemyCombatCapability {
        entity: u64,
    },
    MissingHealthCapability {
        entity: u64,
    },
    MissingNavigationCapability {
        entity: u64,
    },
    MissingPlayerControllerCapability {
        entity: u64,
    },
    MissingWeaponCapability {
        entity: u64,
    },
    NavigationMissingCollisionScene {
        entity: u64,
    },
    PlayerControllerMissingCollisionScene {
        entity: u64,
    },
    InvalidNavigationConfig {
        entity: u64,
    },
    InvalidPlayerControllerConfig {
        entity: u64,
    },
    InvalidHealthConfig {
        entity: u64,
    },
    InvalidWeaponConfig {
        entity: u64,
    },
    InvalidExtractionBeaconConfig {
        entity: u64,
    },
    ExtractionBeaconActivationFromFuture {
        entity: u64,
        activated_at_tick: u64,
        snapshot_tick: u64,
    },
    EnemyHealthStateMismatch {
        entity: u64,
    },
    InvalidEnemyCombatState {
        entity: u64,
    },
    DuplicateEnemyDrop {
        enemy: u64,
    },
    DuplicateEnemyDropPickup {
        pickup: u64,
    },
    InvalidEnemyDropState {
        enemy: u64,
        pickup: u64,
    },
    DormantPickupMissingEnemyDrop {
        pickup: u64,
    },
    InvalidEncounterActivation {
        encounter: u64,
    },
    DuplicateEncounterMember {
        encounter: u64,
        member: u64,
    },
    EnemyInMultipleEncounters {
        enemy: u64,
        first: u64,
        second: u64,
    },
    DuplicateSchedule {
        door: u64,
    },
}

impl std::fmt::Display for GameSnapshotError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for GameSnapshotError {}

impl GameRuntime {
    pub fn snapshot(&self) -> GameSnapshot {
        let has_progression = !self.session.door_access.is_empty()
            || !self.session.loading_bay_interlocks.is_empty()
            || !self.session.secret_regions.is_empty()
            || !self.session.level_exits.is_empty();
        GameSnapshot {
            schema_version: GAME_SNAPSHOT_SCHEMA_VERSION,
            tick: self.tick.raw(),
            entities: self.session.entities.snapshot(),
            item_definitions: self
                .session
                .item_definitions
                .values()
                .map(|definition| ItemDefinitionSnapshot {
                    id: definition.id.as_str().to_string(),
                    max_quantity: definition.max_quantity,
                    kind: match &definition.kind {
                        ItemKind::Weapon(weapon) => SnapshotItemKind::Weapon {
                            ammunition: weapon.ammunition.as_str().to_string(),
                            attack_mode: Some(match weapon.attack_mode {
                                WeaponAttackMode::Hitscan => SnapshotWeaponAttackMode::Hitscan,
                                WeaponAttackMode::Spread { .. } => SnapshotWeaponAttackMode::Spread,
                                WeaponAttackMode::Automatic => SnapshotWeaponAttackMode::Automatic,
                            }),
                            pellet_count: match weapon.attack_mode {
                                WeaponAttackMode::Spread { pellet_count, .. } => Some(pellet_count),
                                WeaponAttackMode::Hitscan | WeaponAttackMode::Automatic => None,
                            },
                            spread_degrees: match weapon.attack_mode {
                                WeaponAttackMode::Spread { spread_degrees, .. } => {
                                    Some(spread_degrees)
                                }
                                WeaponAttackMode::Hitscan | WeaponAttackMode::Automatic => None,
                            },
                            damage: Some(weapon.damage),
                            max_distance: Some(weapon.max_distance),
                            cooldown_ticks: Some(weapon.cooldown_ticks),
                            ammunition_cost: Some(weapon.ammunition_cost),
                            muzzle_offset: Some(weapon.muzzle_offset.to_array()),
                            presentation: Some(weapon.presentation.clone()),
                        },
                        ItemKind::Ammunition => SnapshotItemKind::Ammunition,
                        ItemKind::AccessKey => SnapshotItemKind::AccessKey,
                        ItemKind::HealthSupply { restore_health } => {
                            SnapshotItemKind::HealthSupply {
                                restore_health: *restore_health,
                            }
                        }
                        ItemKind::Armor { protection } => SnapshotItemKind::Armor {
                            protection: *protection,
                        },
                    },
                })
                .collect(),
            voxel_collision: self
                .collision_scene
                .as_ref()
                .map(|scene| VoxelCollisionSnapshot {
                    voxel_size: scene.voxel_size(),
                    chunk_size: scene.chunk_size(),
                    source_revision: scene.source_revision().raw(),
                    authority_hash: scene.authority_hash(),
                    material_voxels: if scene.generated_room().is_some() {
                        Vec::new()
                    } else {
                        scene
                            .material_voxels()
                            .iter()
                            .map(|voxel| MaterialVoxelSnapshot {
                                address: voxel.address,
                                material_slot: voxel.material_slot,
                            })
                            .collect()
                    },
                    generated_room: scene.generated_room().map(|(config, record)| {
                        GeneratedRoomSnapshot {
                            generator_version: record.generator_version,
                            seed: config.seed,
                            width: config.width,
                            height: config.height,
                            length: config.length,
                            output_hash: record.output_hash,
                        }
                    }),
                }),
            doors: self
                .session
                .doors
                .iter()
                .map(|(entity, component)| DoorSnapshot {
                    entity: entity.raw(),
                    state: match component.state {
                        DoorState::Closed => SnapshotDoorState::Closed,
                        DoorState::Open => SnapshotDoorState::Open,
                    },
                    closed_translation: component.config.closed_translation.to_array(),
                    open_translation: component.config.open_translation.to_array(),
                    auto_close_after_ticks: component.config.auto_close_after.map(TickDelta::raw),
                })
                .collect(),
            switches: self
                .session
                .switches
                .iter()
                .map(|(entity, component)| SwitchSnapshot {
                    entity: entity.raw(),
                    activation_count: component.activation_count,
                })
                .collect(),
            extraction_beacons: self
                .session
                .extraction_beacons
                .iter()
                .map(|(entity, component)| ExtractionBeaconSnapshot {
                    entity: entity.raw(),
                    activation_radius: component.config.activation_radius,
                    state: match component.state {
                        ExtractionBeaconState::Standby => SnapshotExtractionBeaconState::Standby,
                        ExtractionBeaconState::Active {
                            actor,
                            activated_at,
                        } => SnapshotExtractionBeaconState::Active {
                            actor: actor.raw(),
                            activated_at_tick: activated_at.raw(),
                        },
                    },
                })
                .collect(),
            controls: self
                .session
                .controls
                .iter()
                .map(|(switch, targets)| ControlsSnapshot {
                    switch: switch.raw(),
                    targets: targets.iter().map(|target| target.raw()).collect(),
                })
                .collect(),
            enemies: self
                .session
                .enemies
                .iter()
                .map(|(entity, component)| EnemySnapshot {
                    entity: entity.raw(),
                    state: match component.state {
                        EnemyState::Alive => SnapshotEnemyState::Alive,
                        EnemyState::Defeated => SnapshotEnemyState::Defeated,
                    },
                })
                .collect(),
            enemy_combat: self
                .session
                .enemy_combat
                .iter()
                .map(|(entity, component)| EnemyCombatSnapshot {
                    entity: entity.raw(),
                    sight_range: component.config.perception.sight_range,
                    hearing_range: component.config.perception.hearing_range,
                    attack_kind: match component.config.attack.kind {
                        EnemyAttackKind::Melee => SnapshotEnemyAttackKind::Melee,
                        EnemyAttackKind::RangedHitscan => SnapshotEnemyAttackKind::RangedHitscan,
                    },
                    damage: component.config.attack.damage,
                    range: component.config.attack.range,
                    cooldown_ticks: component.config.attack.cooldown_ticks,
                    origin_offset: component.config.attack.origin_offset.to_array(),
                    presentation: component.config.attack.presentation.clone(),
                    posture: match component.state.posture {
                        EnemyCombatPosture::Sleeping => SnapshotEnemyCombatPosture::Sleeping,
                        EnemyCombatPosture::Alert => SnapshotEnemyCombatPosture::Alert,
                        EnemyCombatPosture::Pursuing => SnapshotEnemyCombatPosture::Pursuing,
                        EnemyCombatPosture::Attacking => SnapshotEnemyCombatPosture::Attacking,
                        EnemyCombatPosture::Dead => SnapshotEnemyCombatPosture::Dead,
                    },
                    ready_at_tick: component.state.ready_at_tick.raw(),
                    last_known_target_position: component
                        .state
                        .last_known_target_position
                        .map(Vec3::to_array),
                })
                .collect(),
            enemy_drops: self
                .session
                .enemy_drops
                .iter()
                .map(|(enemy, component)| EnemyDropSnapshot {
                    enemy: enemy.raw(),
                    pickup: component.config.pickup.raw(),
                    state: match component.state {
                        EnemyDropState::Armed => SnapshotEnemyDropState::Armed,
                        EnemyDropState::Materialized => SnapshotEnemyDropState::Materialized,
                    },
                })
                .collect(),
            health: self
                .session
                .health
                .iter()
                .map(|(entity, component)| HealthSnapshot {
                    entity: entity.raw(),
                    current: component.current,
                    max: component.config.max,
                    hitbox_half_extents: component.config.hitbox_half_extents.to_array(),
                    max_armor: component.config.max_armor,
                    armor_absorption_percent: component.config.armor_absorption_percent,
                    armor: component.armor,
                    armor_item: component
                        .armor_item
                        .as_ref()
                        .map(|item| item.as_str().to_owned()),
                    state: Some(match component.state {
                        VitalityState::Alive => SnapshotVitalityState::Alive,
                        VitalityState::Dead => SnapshotVitalityState::Dead,
                    }),
                })
                .collect(),
            hazards: self
                .session
                .hazards
                .iter()
                .map(|(entity, component)| HazardSnapshot {
                    entity: entity.raw(),
                    damage: component.config.damage,
                    cooldown_ticks: component.config.cooldown_ticks,
                    ready_at_tick: component.ready_at_tick.raw(),
                })
                .collect(),
            encounters: self
                .session
                .encounters
                .iter()
                .map(|(entity, component)| EncounterSnapshot {
                    entity: entity.raw(),
                    state: match component.state {
                        EncounterState::Dormant => SnapshotEncounterState::Dormant,
                        EncounterState::Active => SnapshotEncounterState::Active,
                        EncounterState::Cleared => SnapshotEncounterState::Cleared,
                    },
                    members: component
                        .config
                        .members
                        .iter()
                        .map(|member| member.raw())
                        .collect(),
                    exit: component.config.exit.raw(),
                    activation_radius: component.config.activation_radius,
                })
                .collect(),
            navigations: self
                .session
                .navigators
                .iter()
                .map(|(entity, component)| NavigationSnapshot {
                    entity: entity.raw(),
                    state: match component.state {
                        NavigationState::Following => SnapshotNavigationState::Following,
                        NavigationState::Arrived => SnapshotNavigationState::Arrived,
                        NavigationState::Blocked => SnapshotNavigationState::Blocked,
                        NavigationState::Unreachable => SnapshotNavigationState::Unreachable,
                    },
                    goal: component.config.goal.to_array(),
                    speed_units_per_second: component.config.speed_units_per_second,
                    max_visited: component.config.max_visited,
                })
                .collect(),
            player_controllers: self
                .session
                .player_controllers
                .iter()
                .map(|(entity, component)| PlayerControllerSnapshot {
                    entity: entity.raw(),
                    move_speed_units_per_second: component.config.move_speed_units_per_second,
                    move_step_seconds: component.config.move_step_seconds,
                    look_degrees_per_unit: component.config.look_degrees_per_unit,
                    initial_yaw_degrees: component.config.initial_yaw_degrees,
                    initial_pitch_degrees: component.config.initial_pitch_degrees,
                    yaw_degrees: component.state.yaw_degrees,
                    pitch_degrees: component.state.pitch_degrees,
                    bindings: PlayerInputBindingsSnapshot {
                        move_forward: component.config.bindings.move_forward.clone(),
                        move_backward: component.config.bindings.move_backward.clone(),
                        move_left: component.config.bindings.move_left.clone(),
                        move_right: component.config.bindings.move_right.clone(),
                        mouse_look: component.config.bindings.mouse_look.clone(),
                        primary_fire: component.config.bindings.primary_fire.clone(),
                        select_weapon: component.config.bindings.select_weapon.clone(),
                    },
                })
                .collect(),
            inventories: self
                .session
                .inventories
                .iter()
                .map(|(owner, component)| InventorySnapshot {
                    owner: owner.raw(),
                    capacity_slots: component.capacity_slots,
                    stacks: component
                        .stacks
                        .iter()
                        .map(|stack| InventoryStackSnapshot {
                            item: stack.item.as_str().to_string(),
                            quantity: stack.quantity,
                        })
                        .collect(),
                    equipped_weapon: component
                        .equipped_weapon
                        .as_ref()
                        .map(|item| item.as_str().to_string()),
                    weapon_slots: component
                        .weapon_slots
                        .iter()
                        .map(|item| item.as_str().to_string())
                        .collect(),
                    weapon_cooldowns: component
                        .weapon_ready_at
                        .iter()
                        .map(|(item, ready_at_tick)| WeaponCooldownSnapshot {
                            item: item.as_str().to_string(),
                            ready_at_tick: ready_at_tick.raw(),
                        })
                        .collect(),
                })
                .collect(),
            pickups: self
                .session
                .pickups
                .iter()
                .map(|(entity, component)| PickupSnapshot {
                    entity: entity.raw(),
                    item: component.config.item.as_str().to_string(),
                    quantity: component.config.quantity,
                    starter_ammunition: component.config.starter_ammunition.as_ref().map(
                        |starter| InventoryStackSnapshot {
                            item: starter.item.as_str().to_string(),
                            quantity: starter.quantity,
                        },
                    ),
                    state: match &component.state {
                        PickupState::Dormant => SnapshotPickupState::Dormant,
                        PickupState::Available => SnapshotPickupState::Available,
                        PickupState::Collected {
                            actor,
                            collected_at_tick,
                            cause,
                        } => SnapshotPickupState::Collected {
                            actor: actor.raw(),
                            collected_at_tick: *collected_at_tick,
                            cause: match cause {
                                PickupCollectionCause::Overlap { trigger_revision } => {
                                    SnapshotPickupCollectionCause::Overlap {
                                        trigger_revision: *trigger_revision,
                                    }
                                }
                                PickupCollectionCause::Interaction {
                                    connection_generation,
                                    command_sequence,
                                } => SnapshotPickupCollectionCause::Interaction {
                                    connection_generation: *connection_generation,
                                    command_sequence: *command_sequence,
                                },
                            },
                        },
                    },
                })
                .collect(),
            pickup_triggers: Some(self.pickup_triggers.snapshot()),
            hazard_triggers: Some(self.hazard_triggers.snapshot()),
            weapons: Vec::new(),
            progression: has_progression.then(|| ProgressionSnapshot {
                door_access: self
                    .session
                    .door_access
                    .iter()
                    .map(|(door, config)| DoorAccessSnapshot {
                        door: door.raw(),
                        required_key: config.required_key.as_str().to_owned(),
                        key_policy: match config.key_policy {
                            RequiredKeyPolicy::Retain => SnapshotRequiredKeyPolicy::Retain,
                            RequiredKeyPolicy::Consume => SnapshotRequiredKeyPolicy::Consume,
                        },
                        activation_radius: config.activation_radius,
                        denied_presentation: config.denied_presentation.clone(),
                    })
                    .collect(),
                loading_bay_interlocks: self
                    .session
                    .loading_bay_interlocks
                    .iter()
                    .map(|(switch, config)| LoadingBayInterlockSnapshot {
                        switch: switch.raw(),
                        close_door: config.close_door.raw(),
                        open_door: config.open_door.raw(),
                    })
                    .collect(),
                secret_regions: self
                    .session
                    .secret_regions
                    .iter()
                    .map(|(entity, component)| SecretRegionSnapshot {
                        entity: entity.raw(),
                        presentation: component.config.presentation.clone(),
                        state: match component.state {
                            SecretRegionState::Undiscovered => {
                                SnapshotSecretRegionState::Undiscovered
                            }
                            SecretRegionState::Discovered {
                                actor,
                                discovered_at,
                            } => SnapshotSecretRegionState::Discovered {
                                actor: actor.raw(),
                                discovered_at_tick: discovered_at.raw(),
                            },
                        },
                    })
                    .collect(),
                level_exits: self
                    .session
                    .level_exits
                    .iter()
                    .map(|(entity, component)| LevelExitSnapshot {
                        entity: entity.raw(),
                        activation_radius: component.config.activation_radius,
                        presentation: component.config.presentation.clone(),
                        state: match component.state {
                            LevelExitState::Available => SnapshotLevelExitState::Available,
                            LevelExitState::Completed {
                                actor,
                                completed_at,
                            } => SnapshotLevelExitState::Completed {
                                actor: actor.raw(),
                                completed_at_tick: completed_at.raw(),
                            },
                        },
                    })
                    .collect(),
                secret_triggers: self.secret_triggers.snapshot(),
            }),
            scheduled: self
                .scheduler
                .entries()
                .map(|entry| ScheduledSnapshot {
                    due_tick: entry.due.raw(),
                    kind: match entry.kind {
                        ScheduledIntentKind::CloseDoor { door } => {
                            ScheduledSnapshotKind::CloseDoor { door: door.raw() }
                        }
                    },
                })
                .collect(),
        }
    }

    pub fn from_snapshot(mut snapshot: GameSnapshot) -> Result<Self, GameSnapshotError> {
        if !(10..=GAME_SNAPSHOT_SCHEMA_VERSION).contains(&snapshot.schema_version) {
            return Err(GameSnapshotError::UnsupportedSchema {
                actual: snapshot.schema_version,
            });
        }
        let source_schema_version = snapshot.schema_version;
        if source_schema_version < 11
            && (!snapshot.item_definitions.is_empty() || !snapshot.inventories.is_empty())
        {
            return Err(GameSnapshotError::FutureInventoryStateInLegacySnapshot);
        }
        if source_schema_version < 12
            && (!snapshot.pickups.is_empty() || snapshot.pickup_triggers.is_some())
        {
            return Err(GameSnapshotError::FuturePickupStateInLegacySnapshot);
        }
        if source_schema_version < VITALITY_SNAPSHOT_SCHEMA_VERSION
            && (snapshot.health.iter().any(|health| {
                health.max_armor != 0
                    || health.armor_absorption_percent != 0
                    || health.armor != 0
                    || health.armor_item.is_some()
                    || health.state.is_some()
            }) || !snapshot.hazards.is_empty()
                || snapshot.hazard_triggers.is_some())
        {
            return Err(GameSnapshotError::FutureVitalityStateInLegacySnapshot);
        }
        if source_schema_version < INVENTORY_WEAPON_SNAPSHOT_SCHEMA_VERSION {
            if snapshot_has_inventory_weapon_fields(&snapshot) {
                return Err(GameSnapshotError::FutureWeaponStateInLegacySnapshot);
            }
            migrate_legacy_snapshot_weapon_authority(&mut snapshot)?;
        } else if !snapshot.weapons.is_empty() {
            return Err(GameSnapshotError::FutureWeaponStateInLegacySnapshot);
        }
        if source_schema_version < PROGRESSION_SNAPSHOT_SCHEMA_VERSION
            && snapshot_has_future_weapon_behavior_fields(&snapshot)
        {
            return Err(GameSnapshotError::FutureWeaponStateInLegacySnapshot);
        }
        if source_schema_version < PROGRESSION_SNAPSHOT_SCHEMA_VERSION
            && snapshot.progression.is_some()
        {
            return Err(GameSnapshotError::FutureProgressionStateInLegacySnapshot);
        }
        if source_schema_version < ENEMY_COMBAT_SNAPSHOT_SCHEMA_VERSION
            && !snapshot.enemy_combat.is_empty()
        {
            return Err(GameSnapshotError::FutureEnemyCombatStateInLegacySnapshot);
        }
        if source_schema_version < GAME_SNAPSHOT_SCHEMA_VERSION
            && (!snapshot.enemy_drops.is_empty()
                || snapshot.encounters.iter().any(|encounter| {
                    encounter.activation_radius.is_some()
                        || encounter.state == SnapshotEncounterState::Dormant
                })
                || snapshot
                    .pickups
                    .iter()
                    .any(|pickup| pickup.state == SnapshotPickupState::Dormant))
        {
            return Err(GameSnapshotError::FutureEnemyArchetypeStateInLegacySnapshot);
        }
        let progression_snapshot = if source_schema_version >= PROGRESSION_SNAPSHOT_SCHEMA_VERSION {
            snapshot.progression.take()
        } else {
            None
        };
        let collision_scene = snapshot
            .voxel_collision
            .map(|scene| match scene.generated_room {
                Some(generated) => {
                    if !scene.material_voxels.is_empty() {
                        return Err(GameSnapshotError::AmbiguousVoxelSnapshot);
                    }
                    if scene.source_revision != VoxelSourceRevision::INITIAL.raw() {
                        return Err(GameSnapshotError::GeneratedRoomRevisionMismatch {
                            actual: scene.source_revision,
                        });
                    }
                    if generated.generator_version != GENERATED_ROOM_VERSION {
                        return Err(GameSnapshotError::UnsupportedGeneratedRoomVersion {
                            actual: generated.generator_version,
                        });
                    }
                    let rebuilt = VoxelCollisionScene::from_generated_room(GeneratedRoomConfig {
                        seed: generated.seed,
                        voxel_size: scene.voxel_size,
                        chunk_size: scene.chunk_size,
                        width: generated.width,
                        height: generated.height,
                        length: generated.length,
                    })
                    .map_err(GameSnapshotError::CollisionScene)?;
                    let actual = rebuilt
                        .generated_room()
                        .expect("generated room constructor records provenance")
                        .1
                        .output_hash;
                    if actual != generated.output_hash {
                        return Err(GameSnapshotError::GeneratedRoomHashMismatch {
                            expected: generated.output_hash,
                            actual,
                        });
                    }
                    if rebuilt.authority_hash() != scene.authority_hash {
                        return Err(GameSnapshotError::VoxelAuthorityHashMismatch {
                            expected: scene.authority_hash,
                            actual: rebuilt.authority_hash(),
                        });
                    }
                    Ok(rebuilt)
                }
                None => {
                    let rebuilt = VoxelCollisionScene::from_material_voxels_at_revision(
                        scene.voxel_size,
                        scene.chunk_size,
                        scene
                            .material_voxels
                            .into_iter()
                            .map(|voxel| MaterialVoxel {
                                address: voxel.address,
                                material_slot: voxel.material_slot,
                            }),
                        VoxelSourceRevision::new(scene.source_revision),
                    )
                    .map_err(GameSnapshotError::CollisionScene)?;
                    if rebuilt.authority_hash() != scene.authority_hash {
                        return Err(GameSnapshotError::VoxelAuthorityHashMismatch {
                            expected: scene.authority_hash,
                            actual: rebuilt.authority_hash(),
                        });
                    }
                    Ok(rebuilt)
                }
            })
            .transpose()?;
        let entities = EntityState::from_snapshot(snapshot.entities)
            .map_err(GameSnapshotError::EntityState)?;
        let item_definitions = admit_item_definitions(
            snapshot
                .item_definitions
                .into_iter()
                .map(snapshot_item_definition)
                .collect::<Result<Vec<_>, _>>()?,
        )
        .map_err(GameSnapshotError::Inventory)?;
        let mut doors = BTreeMap::new();
        let mut door_ids = BTreeSet::new();
        for door in snapshot.doors {
            if !door_ids.insert(door.entity) {
                return Err(GameSnapshotError::DuplicateDoor {
                    entity: door.entity,
                });
            }
            let entity = EntityId::new(door.entity);
            let view = entities
                .view(entity)
                .map_err(|_| GameSnapshotError::UnknownDoorEntity {
                    entity: door.entity,
                })?;
            if view.transform.is_none() || view.collision.is_none() || view.renderable.is_none() {
                return Err(GameSnapshotError::MissingDoorCapability {
                    entity: door.entity,
                });
            }
            doors.insert(
                entity,
                DoorComponent {
                    config: DoorConfig {
                        closed_translation: array_vec3(door.closed_translation),
                        open_translation: array_vec3(door.open_translation),
                        auto_close_after: door.auto_close_after_ticks.map(TickDelta::new),
                    },
                    state: match door.state {
                        SnapshotDoorState::Closed => DoorState::Closed,
                        SnapshotDoorState::Open => DoorState::Open,
                    },
                },
            );
        }

        let mut switches = BTreeMap::new();
        let mut switch_ids = BTreeSet::new();
        for switch in snapshot.switches {
            if !switch_ids.insert(switch.entity) {
                return Err(GameSnapshotError::DuplicateSwitch {
                    entity: switch.entity,
                });
            }
            let entity = EntityId::new(switch.entity);
            if !entities.contains(entity) {
                return Err(GameSnapshotError::UnknownSwitchEntity {
                    entity: switch.entity,
                });
            }
            switches.insert(
                entity,
                SwitchComponent {
                    activation_count: switch.activation_count,
                },
            );
        }

        let mut extraction_beacons = BTreeMap::new();
        let mut extraction_beacon_ids = BTreeSet::new();
        for beacon in snapshot.extraction_beacons {
            if !extraction_beacon_ids.insert(beacon.entity) {
                return Err(GameSnapshotError::DuplicateExtractionBeacon {
                    entity: beacon.entity,
                });
            }
            let entity = EntityId::new(beacon.entity);
            let view = entities.view(entity).map_err(|_| {
                GameSnapshotError::UnknownExtractionBeaconEntity {
                    entity: beacon.entity,
                }
            })?;
            if view.transform.is_none() || view.renderable.is_none() {
                return Err(GameSnapshotError::MissingExtractionBeaconCapability {
                    entity: beacon.entity,
                });
            }
            let config = ExtractionBeaconConfig::new(beacon.activation_radius);
            if !config.is_valid() {
                return Err(GameSnapshotError::InvalidExtractionBeaconConfig {
                    entity: beacon.entity,
                });
            }
            let state = match beacon.state {
                SnapshotExtractionBeaconState::Standby => ExtractionBeaconState::Standby,
                SnapshotExtractionBeaconState::Active {
                    actor,
                    activated_at_tick,
                } => {
                    let actor_entity = EntityId::new(actor);
                    let actor_view = entities.view(actor_entity).map_err(|_| {
                        GameSnapshotError::UnknownExtractionBeaconActor {
                            beacon: beacon.entity,
                            actor,
                        }
                    })?;
                    if actor_view.transform.is_none() {
                        return Err(GameSnapshotError::UnknownExtractionBeaconActor {
                            beacon: beacon.entity,
                            actor,
                        });
                    }
                    if activated_at_tick > snapshot.tick {
                        return Err(GameSnapshotError::ExtractionBeaconActivationFromFuture {
                            entity: beacon.entity,
                            activated_at_tick,
                            snapshot_tick: snapshot.tick,
                        });
                    }
                    ExtractionBeaconState::Active {
                        actor: actor_entity,
                        activated_at: Tick::new(activated_at_tick),
                    }
                }
            };
            extraction_beacons.insert(entity, ExtractionBeaconComponent { config, state });
        }

        let mut controls = BTreeMap::new();
        for control in snapshot.controls {
            let switch = EntityId::new(control.switch);
            if !switches.contains_key(&switch) {
                return Err(GameSnapshotError::UnknownSwitchEntity {
                    entity: control.switch,
                });
            }
            let targets: Vec<EntityId> = control.targets.into_iter().map(EntityId::new).collect();
            for target in &targets {
                if !doors.contains_key(target) {
                    return Err(GameSnapshotError::UnknownControlTarget {
                        switch: control.switch,
                        target: target.raw(),
                    });
                }
            }
            controls.insert(switch, targets);
        }

        let (
            door_access,
            loading_bay_interlocks,
            secret_regions,
            level_exits,
            secret_trigger_snapshot,
        ) = if let Some(progression) = progression_snapshot {
            let mut door_access = BTreeMap::new();
            for access in progression.door_access {
                let door = EntityId::new(access.door);
                let required_key = parse_snapshot_item_id(access.required_key)?;
                let config = DoorAccessConfig {
                    required_key: required_key.clone(),
                    key_policy: match access.key_policy {
                        SnapshotRequiredKeyPolicy::Retain => RequiredKeyPolicy::Retain,
                        SnapshotRequiredKeyPolicy::Consume => RequiredKeyPolicy::Consume,
                    },
                    activation_radius: access.activation_radius,
                    denied_presentation: access.denied_presentation,
                };
                if !doors.contains_key(&door)
                    || !config.is_valid()
                    || item_definitions
                        .get(&required_key)
                        .is_none_or(|definition| !matches!(definition.kind, ItemKind::AccessKey))
                    || door_access.insert(door, config).is_some()
                {
                    return Err(GameSnapshotError::InvalidProgressionState);
                }
            }

            let mut loading_bay_interlocks = BTreeMap::new();
            for interlock in progression.loading_bay_interlocks {
                let switch = EntityId::new(interlock.switch);
                let config = LoadingBayInterlockConfig {
                    close_door: EntityId::new(interlock.close_door),
                    open_door: EntityId::new(interlock.open_door),
                };
                if !switches.contains_key(&switch)
                    || config.close_door == config.open_door
                    || !doors.contains_key(&config.close_door)
                    || !doors.contains_key(&config.open_door)
                    || loading_bay_interlocks.insert(switch, config).is_some()
                {
                    return Err(GameSnapshotError::InvalidProgressionState);
                }
            }

            let mut secret_regions = BTreeMap::new();
            for secret in progression.secret_regions {
                let entity = EntityId::new(secret.entity);
                let view = entities
                    .view(entity)
                    .map_err(|_| GameSnapshotError::InvalidProgressionState)?;
                let config = SecretRegionConfig {
                    presentation: secret.presentation,
                };
                if view.transform.is_none() || view.bounds.is_none() || !config.is_valid() {
                    return Err(GameSnapshotError::InvalidProgressionState);
                }
                let state = match secret.state {
                    SnapshotSecretRegionState::Undiscovered => SecretRegionState::Undiscovered,
                    SnapshotSecretRegionState::Discovered {
                        actor,
                        discovered_at_tick,
                    } => {
                        let actor = EntityId::new(actor);
                        if discovered_at_tick > snapshot.tick
                            || entities
                                .view(actor)
                                .ok()
                                .is_none_or(|view| view.transform.is_none())
                        {
                            return Err(GameSnapshotError::InvalidProgressionState);
                        }
                        SecretRegionState::Discovered {
                            actor,
                            discovered_at: Tick::new(discovered_at_tick),
                        }
                    }
                };
                if secret_regions
                    .insert(entity, SecretRegionComponent { config, state })
                    .is_some()
                {
                    return Err(GameSnapshotError::InvalidProgressionState);
                }
            }

            let mut level_exits = BTreeMap::new();
            for exit in progression.level_exits {
                let entity = EntityId::new(exit.entity);
                let view = entities
                    .view(entity)
                    .map_err(|_| GameSnapshotError::InvalidProgressionState)?;
                let config = LevelExitConfig {
                    activation_radius: exit.activation_radius,
                    presentation: exit.presentation,
                };
                if view.transform.is_none() || view.renderable.is_none() || !config.is_valid() {
                    return Err(GameSnapshotError::InvalidProgressionState);
                }
                let state = match exit.state {
                    SnapshotLevelExitState::Available => LevelExitState::Available,
                    SnapshotLevelExitState::Completed {
                        actor,
                        completed_at_tick,
                    } => {
                        let actor = EntityId::new(actor);
                        if completed_at_tick > snapshot.tick
                            || entities
                                .view(actor)
                                .ok()
                                .is_none_or(|view| view.transform.is_none())
                        {
                            return Err(GameSnapshotError::InvalidProgressionState);
                        }
                        LevelExitState::Completed {
                            actor,
                            completed_at: Tick::new(completed_at_tick),
                        }
                    }
                };
                if level_exits
                    .insert(entity, LevelExitComponent { config, state })
                    .is_some()
                {
                    return Err(GameSnapshotError::InvalidProgressionState);
                }
            }

            (
                door_access,
                loading_bay_interlocks,
                secret_regions,
                level_exits,
                Some(progression.secret_triggers),
            )
        } else {
            (
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeMap::new(),
                None,
            )
        };

        let mut enemies = BTreeMap::new();
        let mut enemy_ids = BTreeSet::new();
        for enemy in snapshot.enemies {
            if !enemy_ids.insert(enemy.entity) {
                return Err(GameSnapshotError::DuplicateEnemy {
                    entity: enemy.entity,
                });
            }
            let entity = EntityId::new(enemy.entity);
            let view =
                entities
                    .view(entity)
                    .map_err(|_| GameSnapshotError::UnknownEnemyEntity {
                        entity: enemy.entity,
                    })?;
            if view.collision.is_none() || view.renderable.is_none() {
                return Err(GameSnapshotError::MissingEnemyCapability {
                    entity: enemy.entity,
                });
            }
            enemies.insert(
                entity,
                EnemyComponent {
                    state: match enemy.state {
                        SnapshotEnemyState::Alive => EnemyState::Alive,
                        SnapshotEnemyState::Defeated => EnemyState::Defeated,
                    },
                },
            );
        }

        let mut health = BTreeMap::new();
        let mut health_ids = BTreeSet::new();
        for health_snapshot in snapshot.health {
            if !health_ids.insert(health_snapshot.entity) {
                return Err(GameSnapshotError::DuplicateHealth {
                    entity: health_snapshot.entity,
                });
            }
            let entity = EntityId::new(health_snapshot.entity);
            let view =
                entities
                    .view(entity)
                    .map_err(|_| GameSnapshotError::UnknownHealthEntity {
                        entity: health_snapshot.entity,
                    })?;
            if view.transform.is_none() || view.collision.is_none() {
                return Err(GameSnapshotError::MissingHealthCapability {
                    entity: health_snapshot.entity,
                });
            }
            let config = HealthConfig {
                max: health_snapshot.max,
                hitbox_half_extents: array_vec3(health_snapshot.hitbox_half_extents),
                max_armor: health_snapshot.max_armor,
                armor_absorption_percent: health_snapshot.armor_absorption_percent,
            };
            let state = match health_snapshot.state {
                Some(SnapshotVitalityState::Alive) => VitalityState::Alive,
                Some(SnapshotVitalityState::Dead) => VitalityState::Dead,
                None if health_snapshot.current == 0 => VitalityState::Dead,
                None => VitalityState::Alive,
            };
            let armor_item = health_snapshot
                .armor_item
                .map(parse_snapshot_item_id)
                .transpose()?;
            let armor_item_is_valid = match (&armor_item, health_snapshot.armor) {
                (None, 0) => true,
                (Some(item), armor) if armor > 0 => item_definitions
                    .get(item)
                    .is_some_and(|definition| matches!(definition.kind, ItemKind::Armor { .. })),
                _ => false,
            };
            if !config.is_valid()
                || health_snapshot.current > config.max
                || health_snapshot.armor > config.max_armor
                || !armor_item_is_valid
                || matches!(state, VitalityState::Alive) != (health_snapshot.current > 0)
            {
                return Err(GameSnapshotError::InvalidHealthConfig {
                    entity: health_snapshot.entity,
                });
            }
            health.insert(
                entity,
                HealthComponent {
                    config,
                    current: health_snapshot.current,
                    armor: health_snapshot.armor,
                    armor_item,
                    state,
                },
            );
        }
        for (entity, enemy) in &enemies {
            let Some(health) = health.get(entity) else {
                continue;
            };
            let consistent = match enemy.state {
                EnemyState::Alive => health.current > 0,
                EnemyState::Defeated => health.current == 0,
            };
            if !consistent {
                return Err(GameSnapshotError::EnemyHealthStateMismatch {
                    entity: entity.raw(),
                });
            }
        }

        let mut hazards = BTreeMap::new();
        let mut hazard_ids = BTreeSet::new();
        for hazard in snapshot.hazards {
            if !hazard_ids.insert(hazard.entity) {
                return Err(GameSnapshotError::DuplicateHazard {
                    entity: hazard.entity,
                });
            }
            let entity = EntityId::new(hazard.entity);
            let view =
                entities
                    .view(entity)
                    .map_err(|_| GameSnapshotError::UnknownHazardEntity {
                        entity: hazard.entity,
                    })?;
            if view.transform.is_none() || view.bounds.is_none() || view.renderable.is_none() {
                return Err(GameSnapshotError::MissingHazardCapability {
                    entity: hazard.entity,
                });
            }
            let config = HazardConfig {
                damage: hazard.damage,
                cooldown_ticks: hazard.cooldown_ticks,
            };
            if !config.is_valid()
                || hazard.cooldown_ticks > MAX_HAZARD_COOLDOWN_TICKS
                || hazard.ready_at_tick > snapshot.tick.saturating_add(hazard.cooldown_ticks)
            {
                return Err(GameSnapshotError::InvalidHazardConfig {
                    entity: hazard.entity,
                });
            }
            hazards.insert(
                entity,
                HazardComponent {
                    config,
                    ready_at_tick: Tick::new(hazard.ready_at_tick),
                },
            );
        }

        let mut navigators = BTreeMap::new();
        let mut navigation_ids = BTreeSet::new();
        for navigation in snapshot.navigations {
            if !navigation_ids.insert(navigation.entity) {
                return Err(GameSnapshotError::DuplicateNavigation {
                    entity: navigation.entity,
                });
            }
            let entity = EntityId::new(navigation.entity);
            let view =
                entities
                    .view(entity)
                    .map_err(|_| GameSnapshotError::UnknownNavigationEntity {
                        entity: navigation.entity,
                    })?;
            if !enemies.contains_key(&entity)
                || view.transform.is_none()
                || view.collision.is_none()
                || view.kinematic.is_none()
            {
                return Err(GameSnapshotError::MissingNavigationCapability {
                    entity: navigation.entity,
                });
            }
            if collision_scene.is_none() {
                return Err(GameSnapshotError::NavigationMissingCollisionScene {
                    entity: navigation.entity,
                });
            }
            let goal = array_vec3(navigation.goal);
            if !vec3_is_finite(goal)
                || !navigation.speed_units_per_second.is_finite()
                || navigation.speed_units_per_second <= 0.0
                || navigation.speed_units_per_second > MAX_NAVIGATION_SPEED_UNITS_PER_SECOND
                || !(1..=MAX_NAVIGATION_QUERY_BUDGET).contains(&navigation.max_visited)
            {
                return Err(GameSnapshotError::InvalidNavigationConfig {
                    entity: navigation.entity,
                });
            }
            navigators.insert(
                entity,
                NavigationComponent {
                    config: NavigationConfig {
                        goal,
                        speed_units_per_second: navigation.speed_units_per_second,
                        max_visited: navigation.max_visited,
                    },
                    state: match navigation.state {
                        SnapshotNavigationState::Following => NavigationState::Following,
                        SnapshotNavigationState::Arrived => NavigationState::Arrived,
                        SnapshotNavigationState::Blocked => NavigationState::Blocked,
                        SnapshotNavigationState::Unreachable => NavigationState::Unreachable,
                    },
                },
            );
        }

        let mut enemy_combat = BTreeMap::new();
        let mut enemy_combat_ids = BTreeSet::new();
        for combat in snapshot.enemy_combat {
            if !enemy_combat_ids.insert(combat.entity) {
                return Err(GameSnapshotError::DuplicateEnemyCombat {
                    entity: combat.entity,
                });
            }
            let entity = EntityId::new(combat.entity);
            entities
                .view(entity)
                .map_err(|_| GameSnapshotError::UnknownEnemyCombatEntity {
                    entity: combat.entity,
                })?;
            let Some(enemy) = enemies.get(&entity) else {
                return Err(GameSnapshotError::MissingEnemyCombatCapability {
                    entity: combat.entity,
                });
            };
            if !health.contains_key(&entity) || !navigators.contains_key(&entity) {
                return Err(GameSnapshotError::MissingEnemyCombatCapability {
                    entity: combat.entity,
                });
            }
            let config = EnemyCombatConfig {
                perception: EnemyPerceptionConfig {
                    sight_range: combat.sight_range,
                    hearing_range: combat.hearing_range,
                },
                attack: EnemyAttackConfig {
                    kind: match combat.attack_kind {
                        SnapshotEnemyAttackKind::Melee => EnemyAttackKind::Melee,
                        SnapshotEnemyAttackKind::RangedHitscan => EnemyAttackKind::RangedHitscan,
                    },
                    damage: combat.damage,
                    range: combat.range,
                    cooldown_ticks: combat.cooldown_ticks,
                    origin_offset: array_vec3(combat.origin_offset),
                    presentation: combat.presentation,
                },
            };
            let posture = match combat.posture {
                SnapshotEnemyCombatPosture::Sleeping => EnemyCombatPosture::Sleeping,
                SnapshotEnemyCombatPosture::Alert => EnemyCombatPosture::Alert,
                SnapshotEnemyCombatPosture::Pursuing => EnemyCombatPosture::Pursuing,
                SnapshotEnemyCombatPosture::Attacking => EnemyCombatPosture::Attacking,
                SnapshotEnemyCombatPosture::Dead => EnemyCombatPosture::Dead,
            };
            let last_known_target_position = combat.last_known_target_position.map(array_vec3);
            let position_state_valid = match posture {
                EnemyCombatPosture::Sleeping | EnemyCombatPosture::Dead => {
                    last_known_target_position.is_none()
                }
                EnemyCombatPosture::Pursuing | EnemyCombatPosture::Attacking => {
                    last_known_target_position.is_some()
                }
                EnemyCombatPosture::Alert => true,
            };
            let enemy_state_valid = matches!(
                (enemy.state, posture),
                (EnemyState::Defeated, EnemyCombatPosture::Dead)
                    | (
                        EnemyState::Alive,
                        EnemyCombatPosture::Sleeping
                            | EnemyCombatPosture::Alert
                            | EnemyCombatPosture::Pursuing
                            | EnemyCombatPosture::Attacking
                    )
            );
            if !config.is_valid()
                || !position_state_valid
                || !enemy_state_valid
                || last_known_target_position.is_some_and(|position| !vec3_is_finite(position))
                || combat.ready_at_tick > snapshot.tick.saturating_add(config.attack.cooldown_ticks)
            {
                return Err(GameSnapshotError::InvalidEnemyCombatState {
                    entity: combat.entity,
                });
            }
            enemy_combat.insert(
                entity,
                EnemyCombatComponent {
                    config,
                    state: EnemyCombatState {
                        posture,
                        ready_at_tick: Tick::new(combat.ready_at_tick),
                        last_known_target_position,
                    },
                },
            );
        }

        let mut player_controllers = BTreeMap::new();
        let mut player_controller_ids = BTreeSet::new();
        for controller in snapshot.player_controllers {
            if !player_controller_ids.insert(controller.entity) {
                return Err(GameSnapshotError::DuplicatePlayerController {
                    entity: controller.entity,
                });
            }
            let entity = EntityId::new(controller.entity);
            let view = entities.view(entity).map_err(|_| {
                GameSnapshotError::UnknownPlayerControllerEntity {
                    entity: controller.entity,
                }
            })?;
            if view.transform.is_none()
                || view.collision.is_none()
                || view.kinematic.is_none()
                || view.renderable.is_none()
            {
                return Err(GameSnapshotError::MissingPlayerControllerCapability {
                    entity: controller.entity,
                });
            }
            if collision_scene.is_none() {
                return Err(GameSnapshotError::PlayerControllerMissingCollisionScene {
                    entity: controller.entity,
                });
            }
            let config = PlayerControllerConfig {
                move_speed_units_per_second: controller.move_speed_units_per_second,
                move_step_seconds: controller.move_step_seconds,
                look_degrees_per_unit: controller.look_degrees_per_unit,
                initial_yaw_degrees: controller.initial_yaw_degrees,
                initial_pitch_degrees: controller.initial_pitch_degrees,
                bindings: PlayerInputBindings::new(
                    controller.bindings.move_forward,
                    controller.bindings.move_backward,
                    controller.bindings.move_left,
                    controller.bindings.move_right,
                    controller.bindings.mouse_look,
                    controller.bindings.primary_fire,
                    controller.bindings.select_weapon,
                ),
            };
            if !config.is_valid()
                || !controller.yaw_degrees.is_finite()
                || !controller.pitch_degrees.is_finite()
                || !(-89.0..=89.0).contains(&controller.pitch_degrees)
            {
                return Err(GameSnapshotError::InvalidPlayerControllerConfig {
                    entity: controller.entity,
                });
            }
            player_controllers.insert(
                entity,
                PlayerControllerComponent {
                    config,
                    state: PlayerControllerState {
                        yaw_degrees: controller.yaw_degrees,
                        pitch_degrees: controller.pitch_degrees,
                    },
                },
            );
        }

        let mut inventories = BTreeMap::new();
        for inventory in snapshot.inventories {
            let owner = EntityId::new(inventory.owner);
            if inventories.contains_key(&owner) {
                return Err(GameSnapshotError::DuplicateInventory {
                    owner: inventory.owner,
                });
            }
            if !entities.contains(owner) || !player_controllers.contains_key(&owner) {
                return Err(GameSnapshotError::UnknownInventoryEntity {
                    owner: inventory.owner,
                });
            }
            let weapon_slots = inventory
                .weapon_slots
                .into_iter()
                .map(parse_snapshot_item_id)
                .collect::<Result<Vec<_>, _>>()?;
            if player_controllers.get(&owner).is_none_or(|controller| {
                controller.config.bindings.select_weapon.len() != weapon_slots.len()
            }) {
                return Err(GameSnapshotError::InvalidPlayerControllerConfig {
                    entity: inventory.owner,
                });
            }
            let config = InventoryConfig::new(
                inventory.capacity_slots,
                inventory
                    .stacks
                    .into_iter()
                    .map(|stack| {
                        Ok(InventoryStack::new(
                            parse_snapshot_item_id(stack.item)?,
                            stack.quantity,
                        ))
                    })
                    .collect::<Result<Vec<_>, GameSnapshotError>>()?,
                inventory
                    .equipped_weapon
                    .map(parse_snapshot_item_id)
                    .transpose()?,
                weapon_slots.clone(),
            );
            let mut component = inventory_from_config(owner, &config, &item_definitions)
                .map_err(GameSnapshotError::Inventory)?;
            let mut cooldowns = BTreeMap::new();
            for cooldown in inventory.weapon_cooldowns {
                let raw_item = cooldown.item.clone();
                let item = parse_snapshot_item_id(cooldown.item)?;
                let latest_reachable =
                    item_definitions
                        .get(&item)
                        .and_then(|definition| match &definition.kind {
                            ItemKind::Weapon(weapon) => {
                                Some(snapshot.tick.saturating_add(weapon.cooldown_ticks))
                            }
                            _ => None,
                        });
                if !weapon_slots.contains(&item)
                    || latest_reachable.is_none_or(|latest| cooldown.ready_at_tick > latest)
                    || cooldowns
                        .insert(item, Tick::new(cooldown.ready_at_tick))
                        .is_some()
                {
                    return Err(GameSnapshotError::InvalidWeaponCooldown {
                        owner: inventory.owner,
                        item: raw_item,
                    });
                }
            }
            if cooldowns.len() != weapon_slots.len() {
                return Err(GameSnapshotError::InvalidWeaponCooldown {
                    owner: inventory.owner,
                    item: "missing-authored-slot".to_string(),
                });
            }
            component.weapon_ready_at = cooldowns;
            inventories.insert(owner, component);
        }

        if snapshot.pickups.len() > engine_spatial::MAX_TRIGGER_DEFINITIONS {
            return Err(GameSnapshotError::TooManyPickups {
                count: snapshot.pickups.len(),
                limit: engine_spatial::MAX_TRIGGER_DEFINITIONS,
            });
        }
        let mut pickups = BTreeMap::new();
        for pickup in snapshot.pickups {
            let entity = EntityId::new(pickup.entity);
            if pickups.contains_key(&entity) {
                return Err(GameSnapshotError::DuplicatePickup {
                    entity: pickup.entity,
                });
            }
            let view =
                entities
                    .view(entity)
                    .map_err(|_| GameSnapshotError::UnknownPickupEntity {
                        entity: pickup.entity,
                    })?;
            let item = parse_snapshot_item_id(pickup.item)?;
            let Some(definition) = item_definitions.get(&item) else {
                return Err(GameSnapshotError::InvalidPickup {
                    entity: pickup.entity,
                });
            };
            if pickup.quantity == 0 || pickup.quantity > definition.max_quantity {
                return Err(GameSnapshotError::InvalidPickup {
                    entity: pickup.entity,
                });
            }
            if let Some(starter) = &pickup.starter_ammunition {
                let ItemKind::Weapon(weapon) = &definition.kind else {
                    return Err(GameSnapshotError::InvalidPickup {
                        entity: pickup.entity,
                    });
                };
                let starter_item = ItemDefinitionId::parse(starter.item.clone()).map_err(|_| {
                    GameSnapshotError::InvalidPickup {
                        entity: pickup.entity,
                    }
                })?;
                if starter_item != weapon.ammunition
                    || starter.quantity == 0
                    || item_definitions
                        .get(&starter_item)
                        .is_none_or(|definition| {
                            !matches!(definition.kind, ItemKind::Ammunition)
                                || starter.quantity > definition.max_quantity
                        })
                {
                    return Err(GameSnapshotError::InvalidPickup {
                        entity: pickup.entity,
                    });
                }
            }
            let state = match pickup.state {
                SnapshotPickupState::Dormant => {
                    if view.lifecycle != EntityLifecycle::Active
                        || view.transform.is_none()
                        || view.bounds.is_none()
                        || view
                            .renderable
                            .as_ref()
                            .is_none_or(|renderable| renderable.visible)
                    {
                        return Err(GameSnapshotError::InvalidPickup {
                            entity: pickup.entity,
                        });
                    }
                    PickupState::Dormant
                }
                SnapshotPickupState::Available => {
                    if view.lifecycle != EntityLifecycle::Active
                        || view.transform.is_none()
                        || view.bounds.is_none()
                        || view.renderable.is_none()
                    {
                        return Err(GameSnapshotError::InvalidPickup {
                            entity: pickup.entity,
                        });
                    }
                    PickupState::Available
                }
                SnapshotPickupState::Collected {
                    actor,
                    collected_at_tick,
                    cause,
                } => {
                    if view.lifecycle != EntityLifecycle::Tombstoned
                        || !entities.contains(EntityId::new(actor))
                    {
                        return Err(GameSnapshotError::InvalidPickup {
                            entity: pickup.entity,
                        });
                    }
                    if collected_at_tick > snapshot.tick {
                        return Err(GameSnapshotError::PickupCollectionFromFuture {
                            entity: pickup.entity,
                            collected_at_tick,
                            snapshot_tick: snapshot.tick,
                        });
                    }
                    PickupState::Collected {
                        actor: EntityId::new(actor),
                        collected_at_tick,
                        cause: match cause {
                            SnapshotPickupCollectionCause::Overlap { trigger_revision } => {
                                PickupCollectionCause::Overlap { trigger_revision }
                            }
                            SnapshotPickupCollectionCause::Interaction {
                                connection_generation,
                                command_sequence,
                            } => PickupCollectionCause::Interaction {
                                connection_generation,
                                command_sequence,
                            },
                        },
                    }
                }
            };
            pickups.insert(
                entity,
                PickupComponent {
                    config: PickupConfig {
                        item,
                        quantity: pickup.quantity,
                        starter_ammunition: pickup
                            .starter_ammunition
                            .map(|starter| {
                                Ok(InventoryStack::new(
                                    parse_snapshot_item_id(starter.item)?,
                                    starter.quantity,
                                ))
                            })
                            .transpose()?,
                    },
                    state,
                },
            );
        }
        let pickup_triggers = if source_schema_version >= 12 {
            TriggerVolumeSystem::from_snapshot(
                snapshot
                    .pickup_triggers
                    .take()
                    .ok_or(GameSnapshotError::InvalidPickupTriggerDefinitions)?,
            )
            .map_err(GameSnapshotError::TriggerVolume)?
        } else {
            TriggerVolumeSystem::default()
        };
        let expected_trigger_entities = pickups.keys().copied().collect::<Vec<_>>();
        let actual_trigger_entities = pickup_triggers
            .definitions()
            .map(|definition| {
                let valid = definition.scope == PICKUP_TRIGGER_SCOPE
                    && definition.tags == ["pickup".to_string()]
                    && definition.geometry_source()
                        == engine_spatial::TriggerGeometrySource::EntityBounds;
                (definition.trigger_id(), valid)
            })
            .collect::<Vec<_>>();
        if actual_trigger_entities.len() != expected_trigger_entities.len()
            || actual_trigger_entities
                .iter()
                .zip(expected_trigger_entities)
                .any(|((actual, valid), expected)| !valid || *actual != expected)
            || pickup_triggers.active_overlaps().any(|pair| {
                !pickups
                    .get(&pair.trigger_id())
                    .is_some_and(|pickup| pickup.state == PickupState::Available)
                    || !entities.contains(pair.subject_id())
            })
        {
            return Err(GameSnapshotError::InvalidPickupTriggerDefinitions);
        }
        if pickups
            .len()
            .saturating_add(hazards.len())
            .saturating_add(secret_regions.len())
            > engine_spatial::MAX_TRIGGER_DEFINITIONS
        {
            return Err(GameSnapshotError::InvalidSecretTriggerDefinitions);
        }
        let hazard_triggers = if source_schema_version >= VITALITY_SNAPSHOT_SCHEMA_VERSION {
            TriggerVolumeSystem::from_snapshot(
                snapshot
                    .hazard_triggers
                    .take()
                    .ok_or(GameSnapshotError::InvalidHazardTriggerDefinitions)?,
            )
            .map_err(GameSnapshotError::TriggerVolume)?
        } else {
            TriggerVolumeSystem::default()
        };
        let expected_hazard_entities = hazards.keys().copied().collect::<Vec<_>>();
        let actual_hazard_entities = hazard_triggers
            .definitions()
            .map(|definition| {
                let valid = definition.scope == HAZARD_TRIGGER_SCOPE
                    && definition.tags == ["hazard".to_string()]
                    && definition.geometry_source()
                        == engine_spatial::TriggerGeometrySource::EntityBounds;
                (definition.trigger_id(), valid)
            })
            .collect::<Vec<_>>();
        if actual_hazard_entities.len() != expected_hazard_entities.len()
            || actual_hazard_entities
                .iter()
                .zip(expected_hazard_entities)
                .any(|((actual, valid), expected)| !valid || *actual != expected)
            || hazard_triggers.active_overlaps().any(|pair| {
                !hazards.contains_key(&pair.trigger_id()) || !entities.contains(pair.subject_id())
            })
        {
            return Err(GameSnapshotError::InvalidHazardTriggerDefinitions);
        }
        let secret_triggers = match secret_trigger_snapshot {
            Some(snapshot) => TriggerVolumeSystem::from_snapshot(snapshot)
                .map_err(GameSnapshotError::TriggerVolume)?,
            None => TriggerVolumeSystem::default(),
        };
        let expected_secret_entities = secret_regions.keys().copied().collect::<Vec<_>>();
        let actual_secret_entities = secret_triggers
            .definitions()
            .map(|definition| {
                let valid = definition.scope == SECRET_TRIGGER_SCOPE
                    && definition.tags == ["secret".to_string()]
                    && definition.geometry_source()
                        == engine_spatial::TriggerGeometrySource::EntityBounds;
                (definition.trigger_id(), valid)
            })
            .collect::<Vec<_>>();
        if actual_secret_entities.len() != expected_secret_entities.len()
            || actual_secret_entities
                .iter()
                .zip(expected_secret_entities)
                .any(|((actual, valid), expected)| !valid || *actual != expected)
            || secret_triggers.active_overlaps().any(|pair| {
                !secret_regions.contains_key(&pair.trigger_id())
                    || !entities.contains(pair.subject_id())
            })
        {
            return Err(GameSnapshotError::InvalidSecretTriggerDefinitions);
        }

        let mut enemy_drops = BTreeMap::new();
        let mut drop_pickups = BTreeSet::new();
        for drop in snapshot.enemy_drops {
            let enemy = EntityId::new(drop.enemy);
            let pickup = EntityId::new(drop.pickup);
            if enemy_drops.contains_key(&enemy) {
                return Err(GameSnapshotError::DuplicateEnemyDrop { enemy: drop.enemy });
            }
            if !drop_pickups.insert(pickup) {
                return Err(GameSnapshotError::DuplicateEnemyDropPickup {
                    pickup: drop.pickup,
                });
            }
            let Some(enemy_component) = enemies.get(&enemy) else {
                return Err(GameSnapshotError::InvalidEnemyDropState {
                    enemy: drop.enemy,
                    pickup: drop.pickup,
                });
            };
            let Some(pickup_component) = pickups.get(&pickup) else {
                return Err(GameSnapshotError::InvalidEnemyDropState {
                    enemy: drop.enemy,
                    pickup: drop.pickup,
                });
            };
            let state = match drop.state {
                SnapshotEnemyDropState::Armed
                    if enemy_component.state == EnemyState::Alive
                        && pickup_component.state == PickupState::Dormant =>
                {
                    EnemyDropState::Armed
                }
                SnapshotEnemyDropState::Materialized
                    if enemy_component.state == EnemyState::Defeated
                        && matches!(
                            pickup_component.state,
                            PickupState::Available | PickupState::Collected { .. }
                        ) =>
                {
                    EnemyDropState::Materialized
                }
                _ => {
                    return Err(GameSnapshotError::InvalidEnemyDropState {
                        enemy: drop.enemy,
                        pickup: drop.pickup,
                    });
                }
            };
            enemy_drops.insert(
                enemy,
                EnemyDropComponent {
                    config: EnemyDropConfig { pickup },
                    state,
                },
            );
        }
        if let Some((pickup, _)) = pickups.iter().find(|(pickup, component)| {
            component.state == PickupState::Dormant && !drop_pickups.contains(pickup)
        }) {
            return Err(GameSnapshotError::DormantPickupMissingEnemyDrop {
                pickup: pickup.raw(),
            });
        }

        let mut encounters = BTreeMap::new();
        let mut encounter_ids = BTreeSet::new();
        let mut encounter_by_enemy = BTreeMap::new();
        for encounter in snapshot.encounters {
            if !encounter_ids.insert(encounter.entity) {
                return Err(GameSnapshotError::DuplicateEncounter {
                    entity: encounter.entity,
                });
            }
            let encounter_entity = EntityId::new(encounter.entity);
            let encounter_view = entities.view(encounter_entity).map_err(|_| {
                GameSnapshotError::UnknownEncounterEntity {
                    entity: encounter.entity,
                }
            })?;
            if encounter.activation_radius.is_some_and(|radius| {
                !radius.is_finite()
                    || radius <= 0.0
                    || radius > MAX_ENCOUNTER_ACTIVATION_RADIUS
                    || encounter_view.transform.is_none()
            }) || (encounter.state == SnapshotEncounterState::Dormant
                && encounter.activation_radius.is_none())
            {
                return Err(GameSnapshotError::InvalidEncounterActivation {
                    encounter: encounter.entity,
                });
            }
            if !doors.contains_key(&EntityId::new(encounter.exit)) {
                return Err(GameSnapshotError::UnknownEncounterExit {
                    encounter: encounter.entity,
                    exit: encounter.exit,
                });
            }
            let mut unique = BTreeSet::new();
            let mut members = Vec::with_capacity(encounter.members.len());
            for member in encounter.members {
                if !unique.insert(member) {
                    return Err(GameSnapshotError::DuplicateEncounterMember {
                        encounter: encounter.entity,
                        member,
                    });
                }
                if !enemies.contains_key(&EntityId::new(member)) {
                    return Err(GameSnapshotError::UnknownEncounterMember {
                        encounter: encounter.entity,
                        member,
                    });
                }
                if let Some(first) = encounter_by_enemy.insert(member, encounter.entity) {
                    return Err(GameSnapshotError::EnemyInMultipleEncounters {
                        enemy: member,
                        first,
                        second: encounter.entity,
                    });
                }
                members.push(EntityId::new(member));
            }
            encounters.insert(
                encounter_entity,
                EncounterComponent {
                    config: EncounterConfig {
                        members,
                        exit: EntityId::new(encounter.exit),
                        activation_radius: encounter.activation_radius,
                    },
                    state: match encounter.state {
                        SnapshotEncounterState::Dormant => EncounterState::Dormant,
                        SnapshotEncounterState::Active => EncounterState::Active,
                        SnapshotEncounterState::Cleared => EncounterState::Cleared,
                    },
                },
            );
        }

        let mut scheduler = Scheduler::default();
        let mut scheduled_doors = BTreeSet::new();
        for entry in snapshot.scheduled {
            let kind = match entry.kind {
                ScheduledSnapshotKind::CloseDoor { door } => {
                    if !doors.contains_key(&EntityId::new(door)) {
                        return Err(GameSnapshotError::UnknownDoorEntity { entity: door });
                    }
                    if !scheduled_doors.insert(door) {
                        return Err(GameSnapshotError::DuplicateSchedule { door });
                    }
                    ScheduledIntentKind::CloseDoor {
                        door: EntityId::new(door),
                    }
                }
            };
            scheduler.schedule(ScheduledIntent {
                due: Tick::new(entry.due_tick),
                kind,
            });
        }

        Ok(Self {
            session: GameSession {
                entities,
                doors,
                door_access,
                switches,
                controls,
                loading_bay_interlocks,
                enemies,
                enemy_combat,
                enemy_drops,
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
            },
            tick: Tick::new(snapshot.tick),
            scheduler,
            events: VecDeque::new(),
            journal: Vec::new(),
            collision_scene,
            pickup_triggers,
            hazard_triggers,
            secret_triggers,
        })
    }
}

pub fn encode_game_snapshot(runtime: &GameRuntime) -> Result<String, GameSnapshotError> {
    serde_json::to_string_pretty(&runtime.snapshot()).map_err(GameSnapshotError::Encode)
}

pub fn decode_game_snapshot(input: &str) -> Result<GameRuntime, GameSnapshotError> {
    let snapshot: GameSnapshot = serde_json::from_str(input).map_err(GameSnapshotError::Decode)?;
    GameRuntime::from_snapshot(snapshot)
}

fn array_vec3(value: [f32; 3]) -> Vec3 {
    Vec3::new(value[0], value[1], value[2])
}

fn vec3_is_finite(value: Vec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}

fn snapshot_item_definition(
    snapshot: ItemDefinitionSnapshot,
) -> Result<ItemDefinition, GameSnapshotError> {
    let id = parse_snapshot_item_id(snapshot.id)?;
    let kind = match snapshot.kind {
        SnapshotItemKind::Weapon {
            ammunition,
            attack_mode,
            pellet_count,
            spread_degrees,
            damage,
            max_distance,
            cooldown_ticks,
            ammunition_cost,
            muzzle_offset,
            presentation,
        } => ItemKind::Weapon(WeaponDefinition {
            attack_mode: match attack_mode.ok_or_else(|| {
                GameSnapshotError::InvalidItemDefinitionId {
                    value: format!("{}:missing-attack-mode", id.as_str()),
                }
            })? {
                SnapshotWeaponAttackMode::Hitscan
                    if pellet_count.is_none() && spread_degrees.is_none() =>
                {
                    WeaponAttackMode::Hitscan
                }
                SnapshotWeaponAttackMode::Automatic
                    if pellet_count.is_none() && spread_degrees.is_none() =>
                {
                    WeaponAttackMode::Automatic
                }
                SnapshotWeaponAttackMode::Spread => WeaponAttackMode::Spread {
                    pellet_count: pellet_count.ok_or_else(|| {
                        GameSnapshotError::InvalidItemDefinitionId {
                            value: format!("{}:missing-pellet-count", id.as_str()),
                        }
                    })?,
                    spread_degrees: spread_degrees.ok_or_else(|| {
                        GameSnapshotError::InvalidItemDefinitionId {
                            value: format!("{}:missing-spread-degrees", id.as_str()),
                        }
                    })?,
                },
                _ => {
                    return Err(GameSnapshotError::InvalidItemDefinitionId {
                        value: format!("{}:incompatible-attack-mode-fields", id.as_str()),
                    });
                }
            },
            damage: damage.ok_or_else(|| GameSnapshotError::InvalidItemDefinitionId {
                value: format!("{}:missing-damage", id.as_str()),
            })?,
            max_distance: max_distance.ok_or_else(|| {
                GameSnapshotError::InvalidItemDefinitionId {
                    value: format!("{}:missing-range", id.as_str()),
                }
            })?,
            cooldown_ticks: cooldown_ticks.ok_or_else(|| {
                GameSnapshotError::InvalidItemDefinitionId {
                    value: format!("{}:missing-cadence", id.as_str()),
                }
            })?,
            ammunition: parse_snapshot_item_id(ammunition)?,
            ammunition_cost: ammunition_cost.ok_or_else(|| {
                GameSnapshotError::InvalidItemDefinitionId {
                    value: format!("{}:missing-ammunition-cost", id.as_str()),
                }
            })?,
            muzzle_offset: array_vec3(muzzle_offset.ok_or_else(|| {
                GameSnapshotError::InvalidItemDefinitionId {
                    value: format!("{}:missing-muzzle-offset", id.as_str()),
                }
            })?),
            presentation: presentation.ok_or_else(|| {
                GameSnapshotError::InvalidItemDefinitionId {
                    value: format!("{}:missing-presentation", id.as_str()),
                }
            })?,
        }),
        SnapshotItemKind::Ammunition => ItemKind::Ammunition,
        SnapshotItemKind::AccessKey => ItemKind::AccessKey,
        SnapshotItemKind::HealthSupply { restore_health } => {
            ItemKind::HealthSupply { restore_health }
        }
        SnapshotItemKind::Armor { protection } => ItemKind::Armor { protection },
    };
    Ok(ItemDefinition::new(id, kind, snapshot.max_quantity))
}

fn snapshot_has_future_weapon_behavior_fields(snapshot: &GameSnapshot) -> bool {
    snapshot.item_definitions.iter().any(|definition| {
        matches!(
            definition.kind,
            SnapshotItemKind::Weapon {
                attack_mode: Some(
                    SnapshotWeaponAttackMode::Spread | SnapshotWeaponAttackMode::Automatic
                ),
                ..
            } | SnapshotItemKind::Weapon {
                pellet_count: Some(_),
                ..
            } | SnapshotItemKind::Weapon {
                spread_degrees: Some(_),
                ..
            }
        )
    })
}

fn snapshot_has_inventory_weapon_fields(snapshot: &GameSnapshot) -> bool {
    snapshot.item_definitions.iter().any(|definition| {
        matches!(
            definition.kind,
            SnapshotItemKind::Weapon {
                attack_mode: Some(_),
                ..
            } | SnapshotItemKind::Weapon {
                damage: Some(_),
                ..
            } | SnapshotItemKind::Weapon {
                max_distance: Some(_),
                ..
            } | SnapshotItemKind::Weapon {
                cooldown_ticks: Some(_),
                ..
            } | SnapshotItemKind::Weapon {
                ammunition_cost: Some(_),
                ..
            } | SnapshotItemKind::Weapon {
                muzzle_offset: Some(_),
                ..
            } | SnapshotItemKind::Weapon {
                presentation: Some(_),
                ..
            }
        )
    }) || snapshot.inventories.iter().any(|inventory| {
        !inventory.weapon_slots.is_empty() || !inventory.weapon_cooldowns.is_empty()
    }) || snapshot
        .player_controllers
        .iter()
        .any(|controller| !controller.bindings.select_weapon.is_empty())
        || snapshot
            .pickups
            .iter()
            .any(|pickup| pickup.starter_ammunition.is_some())
}

fn migrate_legacy_snapshot_weapon_authority(
    snapshot: &mut GameSnapshot,
) -> Result<(), GameSnapshotError> {
    let legacy_weapons = std::mem::take(&mut snapshot.weapons);
    for legacy in &legacy_weapons {
        if !snapshot
            .player_controllers
            .iter()
            .any(|controller| controller.entity == legacy.entity)
        {
            return Err(GameSnapshotError::MissingWeaponCapability {
                entity: legacy.entity,
            });
        }
        if !snapshot
            .inventories
            .iter()
            .any(|inventory| inventory.owner == legacy.entity)
        {
            let weapon_id = format!("weapon/migrated-player-{}", legacy.entity);
            let ammunition_id = format!("ammo/migrated-player-{}", legacy.entity);
            snapshot.item_definitions.push(ItemDefinitionSnapshot {
                id: ammunition_id.clone(),
                max_quantity: legacy.ammo_capacity,
                kind: SnapshotItemKind::Ammunition,
            });
            snapshot.item_definitions.push(ItemDefinitionSnapshot {
                id: weapon_id.clone(),
                max_quantity: 1,
                kind: legacy_snapshot_weapon_kind(&weapon_id, ammunition_id.clone(), legacy),
            });
            let mut stacks = vec![InventoryStackSnapshot {
                item: weapon_id.clone(),
                quantity: 1,
            }];
            if legacy.ammo_remaining > 0 {
                stacks.push(InventoryStackSnapshot {
                    item: ammunition_id,
                    quantity: legacy.ammo_remaining,
                });
            }
            snapshot.inventories.push(InventorySnapshot {
                owner: legacy.entity,
                capacity_slots: 2,
                stacks,
                equipped_weapon: Some(weapon_id.clone()),
                weapon_slots: vec![weapon_id.clone()],
                weapon_cooldowns: vec![WeaponCooldownSnapshot {
                    item: weapon_id,
                    ready_at_tick: legacy.ready_at_tick,
                }],
            });
        } else {
            let equipped = snapshot
                .inventories
                .iter()
                .find(|inventory| inventory.owner == legacy.entity)
                .and_then(|inventory| inventory.equipped_weapon.clone())
                .ok_or(GameSnapshotError::InvalidWeaponConfig {
                    entity: legacy.entity,
                })?;
            let definition = snapshot
                .item_definitions
                .iter_mut()
                .find(|definition| definition.id == equipped)
                .ok_or(GameSnapshotError::InvalidWeaponConfig {
                    entity: legacy.entity,
                })?;
            let ammunition = match &definition.kind {
                SnapshotItemKind::Weapon { ammunition, .. } => ammunition.clone(),
                _ => {
                    return Err(GameSnapshotError::InvalidWeaponConfig {
                        entity: legacy.entity,
                    });
                }
            };
            definition.kind = legacy_snapshot_weapon_kind(&equipped, ammunition, legacy);
        }
    }

    for definition in &mut snapshot.item_definitions {
        let SnapshotItemKind::Weapon {
            attack_mode,
            pellet_count,
            spread_degrees,
            damage,
            max_distance,
            cooldown_ticks,
            ammunition_cost,
            muzzle_offset,
            presentation,
            ..
        } = &mut definition.kind
        else {
            continue;
        };
        *attack_mode = Some(SnapshotWeaponAttackMode::Hitscan);
        *pellet_count = None;
        *spread_degrees = None;
        *damage = Some(damage.unwrap_or(40));
        *max_distance = Some(max_distance.unwrap_or(20.0));
        *cooldown_ticks = Some(cooldown_ticks.unwrap_or(6));
        *ammunition_cost = Some(ammunition_cost.unwrap_or(1));
        *muzzle_offset = Some(muzzle_offset.unwrap_or([0.0, 0.0, 0.0]));
        *presentation = Some(
            presentation
                .clone()
                .unwrap_or_else(|| definition.id.clone()),
        );
    }
    let weapon_ids = snapshot
        .item_definitions
        .iter()
        .filter(|definition| matches!(definition.kind, SnapshotItemKind::Weapon { .. }))
        .map(|definition| definition.id.clone())
        .collect::<Vec<_>>();
    for inventory in &mut snapshot.inventories {
        if inventory.weapon_slots.is_empty() {
            inventory.weapon_slots = weapon_ids.clone();
        }
        if inventory.weapon_cooldowns.is_empty() {
            inventory.weapon_cooldowns = inventory
                .weapon_slots
                .iter()
                .map(|item| WeaponCooldownSnapshot {
                    item: item.clone(),
                    ready_at_tick: legacy_weapons
                        .iter()
                        .find(|legacy| {
                            legacy.entity == inventory.owner
                                && inventory.equipped_weapon.as_ref() == Some(item)
                        })
                        .map_or(0, |legacy| legacy.ready_at_tick),
                })
                .collect();
        }
        if let Some(controller) = snapshot
            .player_controllers
            .iter_mut()
            .find(|controller| controller.entity == inventory.owner)
        {
            controller.bindings.select_weapon = inventory
                .weapon_slots
                .iter()
                .enumerate()
                .map(|(index, _)| format!("Digit{}", index + 1))
                .collect();
        }
    }
    Ok(())
}

fn legacy_snapshot_weapon_kind(
    presentation: &str,
    ammunition: String,
    weapon: &WeaponSnapshot,
) -> SnapshotItemKind {
    SnapshotItemKind::Weapon {
        ammunition,
        attack_mode: Some(SnapshotWeaponAttackMode::Hitscan),
        pellet_count: None,
        spread_degrees: None,
        damage: Some(weapon.damage),
        max_distance: Some(weapon.max_distance),
        cooldown_ticks: Some(weapon.cooldown_ticks),
        ammunition_cost: Some(1),
        muzzle_offset: Some(weapon.muzzle_offset),
        presentation: Some(presentation.to_string()),
    }
}

fn parse_snapshot_item_id(value: String) -> Result<ItemDefinitionId, GameSnapshotError> {
    ItemDefinitionId::parse(value.clone())
        .map_err(|_| GameSnapshotError::InvalidItemDefinitionId { value })
}
