//! Downstream inert gameplay facts stored in the Engine typed component store.
//!
//! Every fact here is an entity-attached value that previously lived in a parallel
//! `BTreeMap<EntityId, _>` side table on [`crate::session::GameSession`]. The data is
//! unchanged; only its storage moved into the `EntityState` component store so that
//! registration, exact slot revisions, destroy cleanup, and durable snapshot round-trips
//! are Engine mechanisms.
//!
//! Classification of former side tables:
//!
//! - **Entity facts (this module):** doors, switches, floor actions, lifts, enemies,
//!   enemy combat, enemy drops, explosive props, hazards, encounters, extraction
//!   beacons, navigation, player controllers, pickups, secret regions, level exits,
//!   door access policy, Loading Bay interlocks, and vitality hitbox configuration.
//!   All are registered as durable components because product saves persist them.
//! - **Retained service-owned state (documented in `session.rs`):** `controls`
//!   (switch-to-target relationship index with no Engine generic-relationship
//!   equivalent), `inventories` (`InventoryRuntime` scheduling/capacity cache over
//!   already-Engine-stored inventory components), item/program catalogs, and family
//!   program bindings (catalog-adjacent configuration of the closed program grammar).

use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::core_time::{Tick, TickDelta};
use rusty_engine::engine_spatial::FirstPersonLookState;
use rusty_engine::entity_state::{
    ComponentCodec, ComponentRegistration, ComponentRegistry, ComponentTypeId, EntityComponent,
};

use crate::combat::{EnemyComponent, EnemyState};
use crate::door::{DoorComponent, DoorConfig, DoorState};
use crate::encounter::{EncounterComponent, EncounterConfig, EncounterState};
use crate::enemy_combat::{
    EnemyAttackConfig, EnemyAttackKind, EnemyCombatComponent, EnemyCombatConfig,
    EnemyCombatPosture, EnemyCombatState, EnemyPerceptionConfig,
};
use crate::enemy_drop::{EnemyDropComponent, EnemyDropConfig, EnemyDropState};
use crate::explosive_prop::{ExplosivePropComponent, ExplosivePropConfig, ExplosivePropState};
use crate::extraction_beacon::{
    ExtractionBeaconComponent, ExtractionBeaconConfig, ExtractionBeaconState,
};
use crate::floor_action::{FloorActionComponent, FloorActionConfig, FloorActionState};
use crate::hazard::{HazardComponent, HazardConfig};
use crate::interaction::{SwitchComponent, SwitchConfig, SwitchEffect};
use crate::inventory::{InventoryStack, ItemDefinitionId, ProjectileDefinition};
use crate::lift::{LiftComponent, LiftConfig, LiftState};
use crate::navigation::{NavigationComponent, NavigationConfig, NavigationState};
use crate::pickup::{PickupCollectionCause, PickupComponent, PickupConfig, PickupState};
use crate::player::{
    PlayerControllerComponent, PlayerControllerConfig, PlayerInputBindings, PlayerTraversalConfig,
};
use crate::progression::{
    DoorAccessConfig, LevelExitComponent, LevelExitConfig, LevelExitState,
    LoadingBayInterlockConfig, RequiredKeyPolicy, SecretRegionComponent, SecretRegionConfig,
    SecretRegionState,
};
use crate::vitality::HealthConfig;

pub const DOOR_COMPONENT_TYPE_ID: &str = "loading-bay.door";
pub const SWITCH_COMPONENT_TYPE_ID: &str = "loading-bay.switch";
pub const FLOOR_ACTION_COMPONENT_TYPE_ID: &str = "loading-bay.floor-action";
pub const LIFT_COMPONENT_TYPE_ID: &str = "loading-bay.lift";
pub const ENEMY_COMPONENT_TYPE_ID: &str = "loading-bay.enemy";
pub const ENEMY_COMBAT_COMPONENT_TYPE_ID: &str = "loading-bay.enemy-combat";
pub const ENEMY_DROP_COMPONENT_TYPE_ID: &str = "loading-bay.enemy-drop";
pub const EXPLOSIVE_PROP_COMPONENT_TYPE_ID: &str = "loading-bay.explosive-prop";
pub const HAZARD_COMPONENT_TYPE_ID: &str = "loading-bay.hazard";
pub const ENCOUNTER_COMPONENT_TYPE_ID: &str = "loading-bay.encounter";
pub const EXTRACTION_BEACON_COMPONENT_TYPE_ID: &str = "loading-bay.extraction-beacon";
pub const NAVIGATION_COMPONENT_TYPE_ID: &str = "loading-bay.navigation";
pub const PLAYER_CONTROLLER_COMPONENT_TYPE_ID: &str = "loading-bay.player-controller";
pub const PICKUP_COMPONENT_TYPE_ID: &str = "loading-bay.pickup";
pub const SECRET_REGION_COMPONENT_TYPE_ID: &str = "loading-bay.secret-region";
pub const LEVEL_EXIT_COMPONENT_TYPE_ID: &str = "loading-bay.level-exit";
pub const DOOR_ACCESS_COMPONENT_TYPE_ID: &str = "loading-bay.door-access";
pub const INTERLOCK_COMPONENT_TYPE_ID: &str = "loading-bay.interlock";
pub const HEALTH_CONFIG_COMPONENT_TYPE_ID: &str = "loading-bay.health-config";

const DOOR_CODEC_ID: &str = "loading-bay.door.json";
const SWITCH_CODEC_ID: &str = "loading-bay.switch.json";
const FLOOR_ACTION_CODEC_ID: &str = "loading-bay.floor-action.json";
const LIFT_CODEC_ID: &str = "loading-bay.lift.json";
const ENEMY_CODEC_ID: &str = "loading-bay.enemy.json";
const ENEMY_COMBAT_CODEC_ID: &str = "loading-bay.enemy-combat.json";
const ENEMY_DROP_CODEC_ID: &str = "loading-bay.enemy-drop.json";
const EXPLOSIVE_PROP_CODEC_ID: &str = "loading-bay.explosive-prop.json";
const HAZARD_CODEC_ID: &str = "loading-bay.hazard.json";
const ENCOUNTER_CODEC_ID: &str = "loading-bay.encounter.json";
const EXTRACTION_BEACON_CODEC_ID: &str = "loading-bay.extraction-beacon.json";
const NAVIGATION_CODEC_ID: &str = "loading-bay.navigation.json";
const PLAYER_CONTROLLER_CODEC_ID: &str = "loading-bay.player-controller.json";
const PICKUP_CODEC_ID: &str = "loading-bay.pickup.json";
const SECRET_REGION_CODEC_ID: &str = "loading-bay.secret-region.json";
const LEVEL_EXIT_CODEC_ID: &str = "loading-bay.level-exit.json";
const DOOR_ACCESS_CODEC_ID: &str = "loading-bay.door-access.json";
const INTERLOCK_CODEC_ID: &str = "loading-bay.interlock.json";
const HEALTH_CONFIG_CODEC_ID: &str = "loading-bay.health-config.json";

const CODEC_VERSION: u32 = 1;

fn parse_type_id(value: &'static str) -> ComponentTypeId {
    ComponentTypeId::parse(value).expect("downstream fact component identity is valid")
}

type FactValidator<T> = Option<fn(&T) -> Result<(), String>>;

fn durable_registration<T: EntityComponent>(
    type_id: &'static str,
    codec_id: &'static str,
    encode: fn(&T) -> serde_json::Value,
    decode: fn(serde_json::Value) -> Result<T, String>,
    validator: FactValidator<T>,
) -> Result<ComponentRegistration<T>, String> {
    let codec = ComponentCodec::new(codec_id, CODEC_VERSION, encode, decode)
        .map_err(|error| error.to_string())?;
    let mut registration = ComponentRegistration::durable(parse_type_id(type_id), codec);
    if let Some(validator) = validator {
        registration = registration.with_validator(validator);
    }
    Ok(registration)
}

fn vec3_value(value: Vec3) -> [f32; 3] {
    value.to_array()
}

fn vec3_value_from(value: [f32; 3]) -> Result<Vec3, String> {
    let vec = Vec3::new(value[0], value[1], value[2]);
    let finite = vec.x.is_finite() && vec.y.is_finite() && vec.z.is_finite();
    finite
        .then_some(vec)
        .ok_or_else(|| "vector is not finite".to_string())
}

fn tick_value(value: Tick) -> u64 {
    value.raw()
}

fn tick_value_from(value: u64) -> Tick {
    Tick::new(value)
}

fn tick_delta_value(value: TickDelta) -> u64 {
    value.raw()
}

fn tick_delta_value_from(value: u64) -> TickDelta {
    TickDelta::new(value)
}

fn entity_value(value: EntityId) -> u64 {
    value.raw()
}

fn entity_value_from(value: u64) -> EntityId {
    EntityId::new(value)
}

fn item_value(value: &ItemDefinitionId) -> String {
    value.as_str().to_string()
}

fn item_value_from(value: String) -> Result<ItemDefinitionId, String> {
    ItemDefinitionId::parse(value).map_err(|error| error.to_string())
}

// ---------------------------------------------------------------------------
// Door
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct DoorFactValue {
    closed_translation: [f32; 3],
    open_translation: [f32; 3],
    auto_close_after_ticks: Option<u64>,
    motion_duration_ticks: u64,
    state: DoorStateValue,
    motion_elapsed_ticks: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum DoorStateValue {
    Closed,
    Opening,
    Open,
    Closing,
}

impl From<DoorState> for DoorStateValue {
    fn from(value: DoorState) -> Self {
        match value {
            DoorState::Closed => Self::Closed,
            DoorState::Opening => Self::Opening,
            DoorState::Open => Self::Open,
            DoorState::Closing => Self::Closing,
        }
    }
}

impl From<DoorStateValue> for DoorState {
    fn from(value: DoorStateValue) -> Self {
        match value {
            DoorStateValue::Closed => Self::Closed,
            DoorStateValue::Opening => Self::Opening,
            DoorStateValue::Open => Self::Open,
            DoorStateValue::Closing => Self::Closing,
        }
    }
}

fn encode_door(value: &DoorComponent) -> serde_json::Value {
    serde_json::to_value(DoorFactValue {
        closed_translation: vec3_value(value.config.closed_translation),
        open_translation: vec3_value(value.config.open_translation),
        auto_close_after_ticks: value.config.auto_close_after.map(tick_delta_value),
        motion_duration_ticks: tick_delta_value(value.config.motion_duration),
        state: value.state.into(),
        motion_elapsed_ticks: tick_delta_value(value.motion_elapsed),
    })
    .expect("door fact serialization cannot fail")
}

fn decode_door(value: serde_json::Value) -> Result<DoorComponent, String> {
    let value: DoorFactValue = serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(DoorComponent {
        config: DoorConfig {
            closed_translation: vec3_value_from(value.closed_translation)?,
            open_translation: vec3_value_from(value.open_translation)?,
            auto_close_after: value.auto_close_after_ticks.map(tick_delta_value_from),
            motion_duration: tick_delta_value_from(value.motion_duration_ticks),
        },
        state: value.state.into(),
        motion_elapsed: tick_delta_value_from(value.motion_elapsed_ticks),
    })
}

// ---------------------------------------------------------------------------
// Switch
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SwitchFactValue {
    activation_radius: f32,
    prompt: String,
    unavailable_presentation: String,
    repeatable: bool,
    effects: Vec<SwitchEffectValue>,
    activation_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum SwitchEffectValue {
    OpenDoor { door: u64 },
    CloseDoor { door: u64 },
}

impl From<&SwitchEffect> for SwitchEffectValue {
    fn from(value: &SwitchEffect) -> Self {
        match *value {
            SwitchEffect::OpenDoor(door) => Self::OpenDoor {
                door: entity_value(door),
            },
            SwitchEffect::CloseDoor(door) => Self::CloseDoor {
                door: entity_value(door),
            },
        }
    }
}

impl From<SwitchEffectValue> for SwitchEffect {
    fn from(value: SwitchEffectValue) -> Self {
        match value {
            SwitchEffectValue::OpenDoor { door } => Self::OpenDoor(entity_value_from(door)),
            SwitchEffectValue::CloseDoor { door } => Self::CloseDoor(entity_value_from(door)),
        }
    }
}

fn encode_switch(value: &SwitchComponent) -> serde_json::Value {
    serde_json::to_value(SwitchFactValue {
        activation_radius: value.config.activation_radius,
        prompt: value.config.prompt.clone(),
        unavailable_presentation: value.config.unavailable_presentation.clone(),
        repeatable: value.config.repeatable,
        effects: value.config.effects.iter().map(Into::into).collect(),
        activation_count: value.activation_count,
    })
    .expect("switch fact serialization cannot fail")
}

fn decode_switch(value: serde_json::Value) -> Result<SwitchComponent, String> {
    let value: SwitchFactValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(SwitchComponent {
        config: SwitchConfig {
            activation_radius: value.activation_radius,
            prompt: value.prompt,
            unavailable_presentation: value.unavailable_presentation,
            repeatable: value.repeatable,
            effects: value.effects.into_iter().map(Into::into).collect(),
        },
        activation_count: value.activation_count,
    })
}

// ---------------------------------------------------------------------------
// Floor action
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FloorActionFactValue {
    target_platform: u64,
    upper_translation: [f32; 3],
    lowered_translation: [f32; 3],
    motion_duration_ticks: u64,
    prompt: String,
    presentation: String,
    source: String,
    state: FloorActionStateValue,
    motion_elapsed_ticks: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum FloorActionStateValue {
    Armed,
    Lowering,
    Lowered,
}

impl From<FloorActionState> for FloorActionStateValue {
    fn from(value: FloorActionState) -> Self {
        match value {
            FloorActionState::Armed => Self::Armed,
            FloorActionState::Lowering => Self::Lowering,
            FloorActionState::Lowered => Self::Lowered,
        }
    }
}

impl From<FloorActionStateValue> for FloorActionState {
    fn from(value: FloorActionStateValue) -> Self {
        match value {
            FloorActionStateValue::Armed => Self::Armed,
            FloorActionStateValue::Lowering => Self::Lowering,
            FloorActionStateValue::Lowered => Self::Lowered,
        }
    }
}

fn encode_floor_action(value: &FloorActionComponent) -> serde_json::Value {
    serde_json::to_value(FloorActionFactValue {
        target_platform: entity_value(value.config.target_platform),
        upper_translation: vec3_value(value.config.upper_translation),
        lowered_translation: vec3_value(value.config.lowered_translation),
        motion_duration_ticks: tick_delta_value(value.config.motion_duration),
        prompt: value.config.prompt.clone(),
        presentation: value.config.presentation.clone(),
        source: value.config.source.clone(),
        state: value.state.into(),
        motion_elapsed_ticks: tick_delta_value(value.motion_elapsed),
    })
    .expect("floor action fact serialization cannot fail")
}

fn decode_floor_action(value: serde_json::Value) -> Result<FloorActionComponent, String> {
    let value: FloorActionFactValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(FloorActionComponent {
        config: FloorActionConfig {
            target_platform: entity_value_from(value.target_platform),
            upper_translation: vec3_value_from(value.upper_translation)?,
            lowered_translation: vec3_value_from(value.lowered_translation)?,
            motion_duration: tick_delta_value_from(value.motion_duration_ticks),
            prompt: value.prompt,
            presentation: value.presentation,
            source: value.source,
        },
        state: value.state.into(),
        motion_elapsed: tick_delta_value_from(value.motion_elapsed_ticks),
    })
}

// ---------------------------------------------------------------------------
// Lift
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LiftFactValue {
    target_platform: u64,
    raised_translation: [f32; 3],
    lowered_translation: [f32; 3],
    motion_duration_ticks: u64,
    lowered_wait_ticks: u64,
    prompt: String,
    presentation: String,
    source: String,
    state: LiftStateValue,
    motion_elapsed_ticks: u64,
    wait_elapsed_ticks: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum LiftStateValue {
    Raised,
    Lowering,
    Waiting,
    Raising,
}

impl From<LiftState> for LiftStateValue {
    fn from(value: LiftState) -> Self {
        match value {
            LiftState::Raised => Self::Raised,
            LiftState::Lowering => Self::Lowering,
            LiftState::Waiting => Self::Waiting,
            LiftState::Raising => Self::Raising,
        }
    }
}

impl From<LiftStateValue> for LiftState {
    fn from(value: LiftStateValue) -> Self {
        match value {
            LiftStateValue::Raised => Self::Raised,
            LiftStateValue::Lowering => Self::Lowering,
            LiftStateValue::Waiting => Self::Waiting,
            LiftStateValue::Raising => Self::Raising,
        }
    }
}

fn encode_lift(value: &LiftComponent) -> serde_json::Value {
    serde_json::to_value(LiftFactValue {
        target_platform: entity_value(value.config.target_platform),
        raised_translation: vec3_value(value.config.raised_translation),
        lowered_translation: vec3_value(value.config.lowered_translation),
        motion_duration_ticks: tick_delta_value(value.config.motion_duration),
        lowered_wait_ticks: tick_delta_value(value.config.lowered_wait),
        prompt: value.config.prompt.clone(),
        presentation: value.config.presentation.clone(),
        source: value.config.source.clone(),
        state: value.state.into(),
        motion_elapsed_ticks: tick_delta_value(value.motion_elapsed),
        wait_elapsed_ticks: tick_delta_value(value.wait_elapsed),
    })
    .expect("lift fact serialization cannot fail")
}

fn decode_lift(value: serde_json::Value) -> Result<LiftComponent, String> {
    let value: LiftFactValue = serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(LiftComponent {
        config: LiftConfig {
            target_platform: entity_value_from(value.target_platform),
            raised_translation: vec3_value_from(value.raised_translation)?,
            lowered_translation: vec3_value_from(value.lowered_translation)?,
            motion_duration: tick_delta_value_from(value.motion_duration_ticks),
            lowered_wait: tick_delta_value_from(value.lowered_wait_ticks),
            prompt: value.prompt,
            presentation: value.presentation,
            source: value.source,
        },
        state: value.state.into(),
        motion_elapsed: tick_delta_value_from(value.motion_elapsed_ticks),
        wait_elapsed: tick_delta_value_from(value.wait_elapsed_ticks),
    })
}

// ---------------------------------------------------------------------------
// Enemy
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum EnemyStateValue {
    Alive,
    Defeated,
}

fn encode_enemy(value: &EnemyComponent) -> serde_json::Value {
    serde_json::to_value(match value.state {
        EnemyState::Alive => EnemyStateValue::Alive,
        EnemyState::Defeated => EnemyStateValue::Defeated,
    })
    .expect("enemy fact serialization cannot fail")
}

fn decode_enemy(value: serde_json::Value) -> Result<EnemyComponent, String> {
    let value: EnemyStateValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(EnemyComponent {
        state: match value {
            EnemyStateValue::Alive => EnemyState::Alive,
            EnemyStateValue::Defeated => EnemyState::Defeated,
        },
    })
}

// ---------------------------------------------------------------------------
// Enemy combat
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct EnemyCombatFactValue {
    sight_range: f32,
    hearing_range: f32,
    pain_duration_ticks: u64,
    attack_program: String,
    defeat_program: String,
    attack: EnemyAttackFactValue,
    posture: EnemyCombatPostureValue,
    ready_at_tick: u64,
    last_known_target_position: Option<[f32; 3]>,
    pain_ticks_remaining: u64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct EnemyAttackFactValue {
    kind: EnemyAttackKindValue,
    damage: u32,
    range: f32,
    cooldown_ticks: u64,
    origin_offset: [f32; 3],
    presentation: String,
    projectile: Option<ProjectileDefinitionFactValue>,
    projectile_visual_asset: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum EnemyAttackKindValue {
    Melee,
    RangedHitscan,
    Projectile,
}

impl From<EnemyAttackKind> for EnemyAttackKindValue {
    fn from(value: EnemyAttackKind) -> Self {
        match value {
            EnemyAttackKind::Melee => Self::Melee,
            EnemyAttackKind::RangedHitscan => Self::RangedHitscan,
            EnemyAttackKind::Projectile => Self::Projectile,
        }
    }
}

impl From<EnemyAttackKindValue> for EnemyAttackKind {
    fn from(value: EnemyAttackKindValue) -> Self {
        match value {
            EnemyAttackKindValue::Melee => Self::Melee,
            EnemyAttackKindValue::RangedHitscan => Self::RangedHitscan,
            EnemyAttackKindValue::Projectile => Self::Projectile,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ProjectileDefinitionFactValue {
    mass: f32,
    radius: f32,
    impulse: f32,
    gravity_scale: f32,
    lifetime_ticks: u64,
    restitution: f32,
}

impl From<ProjectileDefinition> for ProjectileDefinitionFactValue {
    fn from(value: ProjectileDefinition) -> Self {
        Self {
            mass: value.mass,
            radius: value.radius,
            impulse: value.impulse,
            gravity_scale: value.gravity_scale,
            lifetime_ticks: value.lifetime_ticks,
            restitution: value.restitution,
        }
    }
}

impl TryFrom<ProjectileDefinitionFactValue> for ProjectileDefinition {
    type Error = String;

    fn try_from(value: ProjectileDefinitionFactValue) -> Result<Self, Self::Error> {
        let candidate = ProjectileDefinition {
            mass: value.mass,
            radius: value.radius,
            impulse: value.impulse,
            gravity_scale: value.gravity_scale,
            lifetime_ticks: value.lifetime_ticks,
            restitution: value.restitution,
        };
        candidate
            .is_valid()
            .then_some(candidate)
            .ok_or_else(|| "projectile definition violates its validated limits".to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum EnemyCombatPostureValue {
    Sleeping,
    Alert,
    Pursuing,
    Attacking,
    Dead,
}

impl From<EnemyCombatPosture> for EnemyCombatPostureValue {
    fn from(value: EnemyCombatPosture) -> Self {
        match value {
            EnemyCombatPosture::Sleeping => Self::Sleeping,
            EnemyCombatPosture::Alert => Self::Alert,
            EnemyCombatPosture::Pursuing => Self::Pursuing,
            EnemyCombatPosture::Attacking => Self::Attacking,
            EnemyCombatPosture::Dead => Self::Dead,
        }
    }
}

impl From<EnemyCombatPostureValue> for EnemyCombatPosture {
    fn from(value: EnemyCombatPostureValue) -> Self {
        match value {
            EnemyCombatPostureValue::Sleeping => Self::Sleeping,
            EnemyCombatPostureValue::Alert => Self::Alert,
            EnemyCombatPostureValue::Pursuing => Self::Pursuing,
            EnemyCombatPostureValue::Attacking => Self::Attacking,
            EnemyCombatPostureValue::Dead => Self::Dead,
        }
    }
}

fn encode_enemy_combat(value: &EnemyCombatComponent) -> serde_json::Value {
    serde_json::to_value(EnemyCombatFactValue {
        sight_range: value.config.perception.sight_range,
        hearing_range: value.config.perception.hearing_range,
        pain_duration_ticks: value.config.pain_duration_ticks,
        attack_program: value.config.attack_program.clone(),
        defeat_program: value.config.defeat_program.clone(),
        attack: EnemyAttackFactValue {
            kind: value.config.attack.kind.into(),
            damage: value.config.attack.damage,
            range: value.config.attack.range,
            cooldown_ticks: value.config.attack.cooldown_ticks,
            origin_offset: vec3_value(value.config.attack.origin_offset),
            presentation: value.config.attack.presentation.clone(),
            projectile: value.config.attack.projectile.map(Into::into),
            projectile_visual_asset: value.config.attack.projectile_visual_asset.clone(),
        },
        posture: value.state.posture.into(),
        ready_at_tick: tick_value(value.state.ready_at_tick),
        last_known_target_position: value.state.last_known_target_position.map(vec3_value),
        pain_ticks_remaining: value.state.pain_ticks_remaining,
    })
    .expect("enemy combat fact serialization cannot fail")
}

fn decode_enemy_combat(value: serde_json::Value) -> Result<EnemyCombatComponent, String> {
    let value: EnemyCombatFactValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(EnemyCombatComponent {
        config: EnemyCombatConfig {
            perception: EnemyPerceptionConfig {
                sight_range: value.sight_range,
                hearing_range: value.hearing_range,
            },
            pain_duration_ticks: value.pain_duration_ticks,
            attack_program: value.attack_program,
            defeat_program: value.defeat_program,
            attack: EnemyAttackConfig {
                kind: value.attack.kind.into(),
                damage: value.attack.damage,
                range: value.attack.range,
                cooldown_ticks: value.attack.cooldown_ticks,
                origin_offset: vec3_value_from(value.attack.origin_offset)?,
                presentation: value.attack.presentation,
                projectile: value
                    .attack
                    .projectile
                    .map(ProjectileDefinition::try_from)
                    .transpose()?,
                projectile_visual_asset: value.attack.projectile_visual_asset,
            },
        },
        state: EnemyCombatState {
            posture: value.posture.into(),
            ready_at_tick: tick_value_from(value.ready_at_tick),
            last_known_target_position: value
                .last_known_target_position
                .map(vec3_value_from)
                .transpose()?,
            pain_ticks_remaining: value.pain_ticks_remaining,
        },
    })
}

// ---------------------------------------------------------------------------
// Enemy drop
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct EnemyDropFactValue {
    pickup: u64,
    state: EnemyDropStateValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum EnemyDropStateValue {
    Armed,
    Materialized,
}

fn encode_enemy_drop(value: &EnemyDropComponent) -> serde_json::Value {
    serde_json::to_value(EnemyDropFactValue {
        pickup: entity_value(value.config.pickup),
        state: match value.state {
            EnemyDropState::Armed => EnemyDropStateValue::Armed,
            EnemyDropState::Materialized => EnemyDropStateValue::Materialized,
        },
    })
    .expect("enemy drop fact serialization cannot fail")
}

fn decode_enemy_drop(value: serde_json::Value) -> Result<EnemyDropComponent, String> {
    let value: EnemyDropFactValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(EnemyDropComponent {
        config: EnemyDropConfig {
            pickup: entity_value_from(value.pickup),
        },
        state: match value.state {
            EnemyDropStateValue::Armed => EnemyDropState::Armed,
            EnemyDropStateValue::Materialized => EnemyDropState::Materialized,
        },
    })
}

// ---------------------------------------------------------------------------
// Explosive prop
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ExplosivePropFactValue {
    damage: u32,
    radius: f32,
    state: ExplosivePropStateValue,
    pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum ExplosivePropStateValue {
    Armed,
    Exploded,
}

fn encode_explosive_prop(value: &ExplosivePropComponent) -> serde_json::Value {
    serde_json::to_value(ExplosivePropFactValue {
        damage: value.config.damage,
        radius: value.config.radius,
        state: match value.state {
            ExplosivePropState::Armed => ExplosivePropStateValue::Armed,
            ExplosivePropState::Exploded => ExplosivePropStateValue::Exploded,
        },
        pending: value.pending,
    })
    .expect("explosive prop fact serialization cannot fail")
}

fn decode_explosive_prop(value: serde_json::Value) -> Result<ExplosivePropComponent, String> {
    let value: ExplosivePropFactValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(ExplosivePropComponent {
        config: ExplosivePropConfig {
            damage: value.damage,
            radius: value.radius,
        },
        state: match value.state {
            ExplosivePropStateValue::Armed => ExplosivePropState::Armed,
            ExplosivePropStateValue::Exploded => ExplosivePropState::Exploded,
        },
        pending: value.pending,
    })
}

// ---------------------------------------------------------------------------
// Hazard
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct HazardFactValue {
    damage: u32,
    cooldown_ticks: u64,
    ready_at_tick: u64,
}

fn encode_hazard(value: &HazardComponent) -> serde_json::Value {
    serde_json::to_value(HazardFactValue {
        damage: value.config.damage,
        cooldown_ticks: value.config.cooldown_ticks,
        ready_at_tick: tick_value(value.ready_at_tick),
    })
    .expect("hazard fact serialization cannot fail")
}

fn decode_hazard(value: serde_json::Value) -> Result<HazardComponent, String> {
    let value: HazardFactValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(HazardComponent {
        config: HazardConfig {
            damage: value.damage,
            cooldown_ticks: value.cooldown_ticks,
        },
        ready_at_tick: tick_value_from(value.ready_at_tick),
    })
}

// ---------------------------------------------------------------------------
// Encounter
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct EncounterFactValue {
    members: Vec<u64>,
    exit: Option<u64>,
    activation_radius: Option<f32>,
    state: EncounterStateValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum EncounterStateValue {
    Dormant,
    Active,
    Cleared,
}

impl From<EncounterState> for EncounterStateValue {
    fn from(value: EncounterState) -> Self {
        match value {
            EncounterState::Dormant => Self::Dormant,
            EncounterState::Active => Self::Active,
            EncounterState::Cleared => Self::Cleared,
        }
    }
}

impl From<EncounterStateValue> for EncounterState {
    fn from(value: EncounterStateValue) -> Self {
        match value {
            EncounterStateValue::Dormant => Self::Dormant,
            EncounterStateValue::Active => Self::Active,
            EncounterStateValue::Cleared => Self::Cleared,
        }
    }
}

fn encode_encounter(value: &EncounterComponent) -> serde_json::Value {
    serde_json::to_value(EncounterFactValue {
        members: value
            .config
            .members
            .iter()
            .map(|member| entity_value(*member))
            .collect(),
        exit: value.config.exit.map(entity_value),
        activation_radius: value.config.activation_radius,
        state: value.state.into(),
    })
    .expect("encounter fact serialization cannot fail")
}

fn decode_encounter(value: serde_json::Value) -> Result<EncounterComponent, String> {
    let value: EncounterFactValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(EncounterComponent {
        config: EncounterConfig {
            members: value.members.into_iter().map(entity_value_from).collect(),
            exit: value.exit.map(entity_value_from),
            activation_radius: value.activation_radius,
        },
        state: value.state.into(),
    })
}

// ---------------------------------------------------------------------------
// Extraction beacon
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ExtractionBeaconFactValue {
    activation_radius: f32,
    state: ExtractionBeaconStateValue,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ExtractionBeaconStateValue {
    Standby,
    Active { actor: u64, activated_at_tick: u64 },
}

fn encode_extraction_beacon(value: &ExtractionBeaconComponent) -> serde_json::Value {
    serde_json::to_value(ExtractionBeaconFactValue {
        activation_radius: value.config.activation_radius,
        state: match value.state {
            ExtractionBeaconState::Standby => ExtractionBeaconStateValue::Standby,
            ExtractionBeaconState::Active {
                actor,
                activated_at,
            } => ExtractionBeaconStateValue::Active {
                actor: entity_value(actor),
                activated_at_tick: tick_value(activated_at),
            },
        },
    })
    .expect("extraction beacon fact serialization cannot fail")
}

fn decode_extraction_beacon(value: serde_json::Value) -> Result<ExtractionBeaconComponent, String> {
    let value: ExtractionBeaconFactValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(ExtractionBeaconComponent {
        config: ExtractionBeaconConfig {
            activation_radius: value.activation_radius,
        },
        state: match value.state {
            ExtractionBeaconStateValue::Standby => ExtractionBeaconState::Standby,
            ExtractionBeaconStateValue::Active {
                actor,
                activated_at_tick,
            } => ExtractionBeaconState::Active {
                actor: entity_value_from(actor),
                activated_at: tick_value_from(activated_at_tick),
            },
        },
    })
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct NavigationFactValue {
    goal: [f32; 3],
    speed_units_per_second: f32,
    max_visited: usize,
    state: NavigationStateValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum NavigationStateValue {
    Following,
    Arrived,
    Blocked,
    Unreachable,
}

impl From<NavigationState> for NavigationStateValue {
    fn from(value: NavigationState) -> Self {
        match value {
            NavigationState::Following => Self::Following,
            NavigationState::Arrived => Self::Arrived,
            NavigationState::Blocked => Self::Blocked,
            NavigationState::Unreachable => Self::Unreachable,
        }
    }
}

impl From<NavigationStateValue> for NavigationState {
    fn from(value: NavigationStateValue) -> Self {
        match value {
            NavigationStateValue::Following => Self::Following,
            NavigationStateValue::Arrived => Self::Arrived,
            NavigationStateValue::Blocked => Self::Blocked,
            NavigationStateValue::Unreachable => Self::Unreachable,
        }
    }
}

fn encode_navigation(value: &NavigationComponent) -> serde_json::Value {
    serde_json::to_value(NavigationFactValue {
        goal: vec3_value(value.config.goal),
        speed_units_per_second: value.config.speed_units_per_second,
        max_visited: value.config.max_visited,
        state: value.state.into(),
    })
    .expect("navigation fact serialization cannot fail")
}

fn decode_navigation(value: serde_json::Value) -> Result<NavigationComponent, String> {
    let value: NavigationFactValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(NavigationComponent {
        config: NavigationConfig {
            goal: vec3_value_from(value.goal)?,
            speed_units_per_second: value.speed_units_per_second,
            max_visited: value.max_visited,
        },
        state: value.state.into(),
    })
}

// ---------------------------------------------------------------------------
// Player controller
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PlayerControllerFactValue {
    move_speed_units_per_second: f32,
    move_step_seconds: f32,
    look_degrees_per_unit: f32,
    initial_yaw_degrees: f32,
    initial_pitch_degrees: f32,
    traversal: PlayerTraversalFactValue,
    bindings: PlayerBindingsFactValue,
    canonical_standing_height: f32,
    canonical_crouched_height: f32,
    canonical_radius: f32,
    eye_offset_from_center: f32,
    yaw_degrees: f32,
    pitch_degrees: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PlayerTraversalFactValue {
    max_step_height: f32,
    gravity_units_per_second_squared: f32,
    jump_impulse_units_per_second: f32,
    ground_probe_distance: f32,
    eye_height: f32,
    manual_jump_enabled: bool,
    max_air_jumps: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PlayerBindingsFactValue {
    move_forward: String,
    move_backward: String,
    move_left: String,
    move_right: String,
    mouse_look: String,
    primary_fire: String,
    jump: Option<String>,
    select_weapon: Vec<String>,
}

fn encode_player_controller(value: &PlayerControllerComponent) -> serde_json::Value {
    serde_json::to_value(PlayerControllerFactValue {
        move_speed_units_per_second: value.config.move_speed_units_per_second,
        move_step_seconds: value.config.move_step_seconds,
        look_degrees_per_unit: value.config.look_degrees_per_unit,
        initial_yaw_degrees: value.config.initial_yaw_degrees,
        initial_pitch_degrees: value.config.initial_pitch_degrees,
        traversal: PlayerTraversalFactValue {
            max_step_height: value.config.traversal.max_step_height,
            gravity_units_per_second_squared: value
                .config
                .traversal
                .gravity_units_per_second_squared,
            jump_impulse_units_per_second: value.config.traversal.jump_impulse_units_per_second,
            ground_probe_distance: value.config.traversal.ground_probe_distance,
            eye_height: value.config.traversal.eye_height,
            manual_jump_enabled: value.config.traversal.manual_jump_enabled,
            max_air_jumps: value.config.traversal.max_air_jumps,
        },
        bindings: PlayerBindingsFactValue {
            move_forward: value.config.bindings.move_forward.clone(),
            move_backward: value.config.bindings.move_backward.clone(),
            move_left: value.config.bindings.move_left.clone(),
            move_right: value.config.bindings.move_right.clone(),
            mouse_look: value.config.bindings.mouse_look.clone(),
            primary_fire: value.config.bindings.primary_fire.clone(),
            jump: value.config.bindings.jump.clone(),
            select_weapon: value.config.bindings.select_weapon.clone(),
        },
        canonical_standing_height: value.engine.shape.standing_height,
        canonical_crouched_height: value.engine.shape.crouched_height,
        canonical_radius: value.engine.shape.radius,
        eye_offset_from_center: value.eye_offset_from_center,
        yaw_degrees: value.look_state.yaw_radians.to_degrees(),
        pitch_degrees: value.look_state.pitch_radians.to_degrees(),
    })
    .expect("player controller fact serialization cannot fail")
}

fn decode_player_controller(value: serde_json::Value) -> Result<PlayerControllerComponent, String> {
    let value: PlayerControllerFactValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    if !(-89.0..=89.0).contains(&value.pitch_degrees) {
        return Err("player pitch exceeds its validated range".to_string());
    }
    let mut bindings = PlayerInputBindings::new(
        value.bindings.move_forward,
        value.bindings.move_backward,
        value.bindings.move_left,
        value.bindings.move_right,
        value.bindings.mouse_look,
        value.bindings.primary_fire,
        value.bindings.select_weapon,
    );
    bindings.jump = value.bindings.jump;
    let config = PlayerControllerConfig {
        move_speed_units_per_second: value.move_speed_units_per_second,
        move_step_seconds: value.move_step_seconds,
        look_degrees_per_unit: value.look_degrees_per_unit,
        initial_yaw_degrees: value.initial_yaw_degrees,
        initial_pitch_degrees: value.initial_pitch_degrees,
        traversal: PlayerTraversalConfig {
            max_step_height: value.traversal.max_step_height,
            gravity_units_per_second_squared: value.traversal.gravity_units_per_second_squared,
            jump_impulse_units_per_second: value.traversal.jump_impulse_units_per_second,
            ground_probe_distance: value.traversal.ground_probe_distance,
            eye_height: value.traversal.eye_height,
            manual_jump_enabled: value.traversal.manual_jump_enabled,
            max_air_jumps: value.traversal.max_air_jumps,
        },
        bindings,
    };
    if !config.is_valid() {
        return Err("invalid player controller configuration".to_string());
    }
    PlayerControllerComponent::restore(
        config,
        FirstPersonLookState {
            yaw_radians: value.yaw_degrees.to_radians(),
            pitch_radians: value.pitch_degrees.to_radians(),
        },
        value.canonical_standing_height,
        value.canonical_crouched_height,
        value.canonical_radius,
        value.eye_offset_from_center,
    )
    .map_err(|error| error.to_string())
}

// ---------------------------------------------------------------------------
// Pickup
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PickupFactValue {
    item: String,
    quantity: u32,
    program: String,
    starter_ammunition: Option<PickupStarterFactValue>,
    state: PickupStateValue,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PickupStarterFactValue {
    item: String,
    quantity: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum PickupStateValue {
    Dormant,
    Available,
    Collected {
        actor: u64,
        collected_at_tick: u64,
        cause: PickupCauseFactValue,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "cause", rename_all = "camelCase")]
enum PickupCauseFactValue {
    Overlap {
        trigger_revision: u64,
    },
    Interaction {
        connection_generation: u64,
        command_sequence: u64,
    },
}

impl From<&PickupCollectionCause> for PickupCauseFactValue {
    fn from(value: &PickupCollectionCause) -> Self {
        match *value {
            PickupCollectionCause::Overlap { trigger_revision } => {
                Self::Overlap { trigger_revision }
            }
            PickupCollectionCause::Interaction {
                connection_generation,
                command_sequence,
            } => Self::Interaction {
                connection_generation,
                command_sequence,
            },
        }
    }
}

impl From<PickupCauseFactValue> for PickupCollectionCause {
    fn from(value: PickupCauseFactValue) -> Self {
        match value {
            PickupCauseFactValue::Overlap { trigger_revision } => {
                Self::Overlap { trigger_revision }
            }
            PickupCauseFactValue::Interaction {
                connection_generation,
                command_sequence,
            } => Self::Interaction {
                connection_generation,
                command_sequence,
            },
        }
    }
}

fn encode_pickup(value: &PickupComponent) -> serde_json::Value {
    serde_json::to_value(PickupFactValue {
        item: item_value(&value.config.item),
        quantity: value.config.quantity,
        program: value.config.program.clone(),
        starter_ammunition: value.config.starter_ammunition.as_ref().map(|stack| {
            PickupStarterFactValue {
                item: item_value(&stack.item),
                quantity: stack.quantity,
            }
        }),
        state: match &value.state {
            PickupState::Dormant => PickupStateValue::Dormant,
            PickupState::Available => PickupStateValue::Available,
            PickupState::Collected {
                actor,
                collected_at_tick,
                cause,
            } => PickupStateValue::Collected {
                actor: entity_value(*actor),
                collected_at_tick: *collected_at_tick,
                cause: cause.into(),
            },
        },
    })
    .expect("pickup fact serialization cannot fail")
}

fn decode_pickup(value: serde_json::Value) -> Result<PickupComponent, String> {
    let value: PickupFactValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(PickupComponent {
        config: PickupConfig {
            item: item_value_from(value.item)?,
            quantity: value.quantity,
            program: value.program,
            starter_ammunition: value
                .starter_ammunition
                .map(|starter| {
                    Ok::<InventoryStack, String>(InventoryStack {
                        item: item_value_from(starter.item)?,
                        quantity: starter.quantity,
                    })
                })
                .transpose()?,
        },
        state: match value.state {
            PickupStateValue::Dormant => PickupState::Dormant,
            PickupStateValue::Available => PickupState::Available,
            PickupStateValue::Collected {
                actor,
                collected_at_tick,
                cause,
            } => PickupState::Collected {
                actor: entity_value_from(actor),
                collected_at_tick,
                cause: cause.into(),
            },
        },
    })
}

// ---------------------------------------------------------------------------
// Secret region / level exit / door access / interlock / health configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SecretRegionFactValue {
    presentation: String,
    state: SecretRegionStateValue,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum SecretRegionStateValue {
    Undiscovered,
    Discovered { actor: u64, discovered_at_tick: u64 },
}

fn encode_secret_region(value: &SecretRegionComponent) -> serde_json::Value {
    serde_json::to_value(SecretRegionFactValue {
        presentation: value.config.presentation.clone(),
        state: match value.state {
            SecretRegionState::Undiscovered => SecretRegionStateValue::Undiscovered,
            SecretRegionState::Discovered {
                actor,
                discovered_at,
            } => SecretRegionStateValue::Discovered {
                actor: entity_value(actor),
                discovered_at_tick: tick_value(discovered_at),
            },
        },
    })
    .expect("secret region fact serialization cannot fail")
}

fn decode_secret_region(value: serde_json::Value) -> Result<SecretRegionComponent, String> {
    let value: SecretRegionFactValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(SecretRegionComponent {
        config: SecretRegionConfig {
            presentation: value.presentation,
        },
        state: match value.state {
            SecretRegionStateValue::Undiscovered => SecretRegionState::Undiscovered,
            SecretRegionStateValue::Discovered {
                actor,
                discovered_at_tick,
            } => SecretRegionState::Discovered {
                actor: entity_value_from(actor),
                discovered_at: tick_value_from(discovered_at_tick),
            },
        },
    })
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LevelExitFactValue {
    activation_radius: f32,
    presentation: String,
    state: LevelExitStateValue,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum LevelExitStateValue {
    Available,
    Completed { actor: u64, completed_at_tick: u64 },
}

fn encode_level_exit(value: &LevelExitComponent) -> serde_json::Value {
    serde_json::to_value(LevelExitFactValue {
        activation_radius: value.config.activation_radius,
        presentation: value.config.presentation.clone(),
        state: match value.state {
            LevelExitState::Available => LevelExitStateValue::Available,
            LevelExitState::Completed {
                actor,
                completed_at,
            } => LevelExitStateValue::Completed {
                actor: entity_value(actor),
                completed_at_tick: tick_value(completed_at),
            },
        },
    })
    .expect("level exit fact serialization cannot fail")
}

fn decode_level_exit(value: serde_json::Value) -> Result<LevelExitComponent, String> {
    let value: LevelExitFactValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(LevelExitComponent {
        config: LevelExitConfig {
            activation_radius: value.activation_radius,
            presentation: value.presentation,
        },
        state: match value.state {
            LevelExitStateValue::Available => LevelExitState::Available,
            LevelExitStateValue::Completed {
                actor,
                completed_at_tick,
            } => LevelExitState::Completed {
                actor: entity_value_from(actor),
                completed_at: tick_value_from(completed_at_tick),
            },
        },
    })
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct DoorAccessFactValue {
    required_key: String,
    key_policy: RequiredKeyPolicyValue,
    activation_radius: f32,
    denied_presentation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum RequiredKeyPolicyValue {
    Retain,
    Consume,
}

fn encode_door_access(value: &DoorAccessConfig) -> serde_json::Value {
    serde_json::to_value(DoorAccessFactValue {
        required_key: item_value(&value.required_key),
        key_policy: match value.key_policy {
            RequiredKeyPolicy::Retain => RequiredKeyPolicyValue::Retain,
            RequiredKeyPolicy::Consume => RequiredKeyPolicyValue::Consume,
        },
        activation_radius: value.activation_radius,
        denied_presentation: value.denied_presentation.clone(),
    })
    .expect("door access fact serialization cannot fail")
}

fn decode_door_access(value: serde_json::Value) -> Result<DoorAccessConfig, String> {
    let value: DoorAccessFactValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(DoorAccessConfig {
        required_key: item_value_from(value.required_key)?,
        key_policy: match value.key_policy {
            RequiredKeyPolicyValue::Retain => RequiredKeyPolicy::Retain,
            RequiredKeyPolicyValue::Consume => RequiredKeyPolicy::Consume,
        },
        activation_radius: value.activation_radius,
        denied_presentation: value.denied_presentation,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct InterlockFactValue {
    close_door: u64,
    open_door: u64,
}

fn encode_interlock(value: &LoadingBayInterlockConfig) -> serde_json::Value {
    serde_json::to_value(InterlockFactValue {
        close_door: entity_value(value.close_door),
        open_door: entity_value(value.open_door),
    })
    .expect("interlock fact serialization cannot fail")
}

fn decode_interlock(value: serde_json::Value) -> Result<LoadingBayInterlockConfig, String> {
    let value: InterlockFactValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(LoadingBayInterlockConfig {
        close_door: entity_value_from(value.close_door),
        open_door: entity_value_from(value.open_door),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct HealthConfigFactValue {
    max: u32,
    starting: u32,
    hitbox_half_extents: [f32; 3],
    max_armor: u32,
    armor_absorption_percent: u8,
}

fn encode_health_config(value: &HealthConfig) -> serde_json::Value {
    serde_json::to_value(HealthConfigFactValue {
        max: value.max,
        starting: value.starting,
        hitbox_half_extents: vec3_value(value.hitbox_half_extents),
        max_armor: value.max_armor,
        armor_absorption_percent: value.armor_absorption_percent,
    })
    .expect("health configuration fact serialization cannot fail")
}

fn decode_health_config(value: serde_json::Value) -> Result<HealthConfig, String> {
    let value: HealthConfigFactValue =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(HealthConfig {
        max: value.max,
        starting: value.starting,
        hitbox_half_extents: vec3_value_from(value.hitbox_half_extents)?,
        max_armor: value.max_armor,
        armor_absorption_percent: value.armor_absorption_percent,
    })
}

// ---------------------------------------------------------------------------
// EntityComponent opt-ins
//
// These types stay plain inert data; the marker trait only lets the Engine
// store hold them after an explicit registration below.
// ---------------------------------------------------------------------------

impl EntityComponent for DoorComponent {}
impl EntityComponent for SwitchComponent {}
impl EntityComponent for FloorActionComponent {}
impl EntityComponent for LiftComponent {}
impl EntityComponent for EnemyComponent {}
impl EntityComponent for EnemyCombatComponent {}
impl EntityComponent for EnemyDropComponent {}
impl EntityComponent for ExplosivePropComponent {}
impl EntityComponent for HazardComponent {}
impl EntityComponent for EncounterComponent {}
impl EntityComponent for ExtractionBeaconComponent {}
impl EntityComponent for NavigationComponent {}
impl EntityComponent for PlayerControllerComponent {}
impl EntityComponent for PickupComponent {}
impl EntityComponent for SecretRegionComponent {}
impl EntityComponent for LevelExitComponent {}
impl EntityComponent for DoorAccessConfig {}
impl EntityComponent for LoadingBayInterlockConfig {}
impl EntityComponent for HealthConfig {}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Registers every downstream gameplay fact on top of the standard mechanics registry.
///
/// The same registry must back session construction and snapshot restoration so durable
/// codec signatures always agree.
pub(crate) fn gameplay_fact_registry() -> Result<ComponentRegistry, String> {
    let mut registry = crate::mechanics::mechanics_registry()?;
    register_fact_components(&mut registry)?;
    Ok(registry)
}

pub(crate) fn register_fact_components(registry: &mut ComponentRegistry) -> Result<(), String> {
    registry
        .register(durable_registration(
            DOOR_COMPONENT_TYPE_ID,
            DOOR_CODEC_ID,
            encode_door,
            decode_door,
            Some(|value| {
                value
                    .config
                    .is_valid()
                    .then_some(())
                    .ok_or_else(|| "invalid door motion duration".to_string())
            }),
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            SWITCH_COMPONENT_TYPE_ID,
            SWITCH_CODEC_ID,
            encode_switch,
            decode_switch,
            Some(|value| {
                value
                    .config
                    .is_valid()
                    .then_some(())
                    .ok_or_else(|| "invalid switch configuration".to_string())
            }),
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            FLOOR_ACTION_COMPONENT_TYPE_ID,
            FLOOR_ACTION_CODEC_ID,
            encode_floor_action,
            decode_floor_action,
            Some(|value| {
                value
                    .config
                    .is_valid()
                    .then_some(())
                    .ok_or_else(|| "invalid floor action configuration".to_string())
            }),
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            LIFT_COMPONENT_TYPE_ID,
            LIFT_CODEC_ID,
            encode_lift,
            decode_lift,
            Some(|value| {
                value
                    .config
                    .is_valid()
                    .then_some(())
                    .ok_or_else(|| "invalid lift configuration".to_string())
            }),
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            ENEMY_COMPONENT_TYPE_ID,
            ENEMY_CODEC_ID,
            encode_enemy,
            decode_enemy,
            None,
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            ENEMY_COMBAT_COMPONENT_TYPE_ID,
            ENEMY_COMBAT_CODEC_ID,
            encode_enemy_combat,
            decode_enemy_combat,
            Some(|value| {
                value
                    .config
                    .is_valid()
                    .then_some(())
                    .ok_or_else(|| "invalid enemy combat configuration".to_string())
            }),
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            ENEMY_DROP_COMPONENT_TYPE_ID,
            ENEMY_DROP_CODEC_ID,
            encode_enemy_drop,
            decode_enemy_drop,
            None,
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            EXPLOSIVE_PROP_COMPONENT_TYPE_ID,
            EXPLOSIVE_PROP_CODEC_ID,
            encode_explosive_prop,
            decode_explosive_prop,
            Some(|value| {
                value
                    .config
                    .is_valid()
                    .then_some(())
                    .ok_or_else(|| "invalid explosive prop configuration".to_string())
            }),
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            HAZARD_COMPONENT_TYPE_ID,
            HAZARD_CODEC_ID,
            encode_hazard,
            decode_hazard,
            Some(|value| {
                value
                    .config
                    .is_valid()
                    .then_some(())
                    .ok_or_else(|| "invalid hazard configuration".to_string())
            }),
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            ENCOUNTER_COMPONENT_TYPE_ID,
            ENCOUNTER_CODEC_ID,
            encode_encounter,
            decode_encounter,
            None,
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            EXTRACTION_BEACON_COMPONENT_TYPE_ID,
            EXTRACTION_BEACON_CODEC_ID,
            encode_extraction_beacon,
            decode_extraction_beacon,
            Some(|value| {
                value
                    .config
                    .is_valid()
                    .then_some(())
                    .ok_or_else(|| "invalid extraction beacon configuration".to_string())
            }),
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            NAVIGATION_COMPONENT_TYPE_ID,
            NAVIGATION_CODEC_ID,
            encode_navigation,
            decode_navigation,
            None,
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            PLAYER_CONTROLLER_COMPONENT_TYPE_ID,
            PLAYER_CONTROLLER_CODEC_ID,
            encode_player_controller,
            decode_player_controller,
            None,
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            PICKUP_COMPONENT_TYPE_ID,
            PICKUP_CODEC_ID,
            encode_pickup,
            decode_pickup,
            None,
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            SECRET_REGION_COMPONENT_TYPE_ID,
            SECRET_REGION_CODEC_ID,
            encode_secret_region,
            decode_secret_region,
            Some(|value| {
                value
                    .config
                    .is_valid()
                    .then_some(())
                    .ok_or_else(|| "invalid secret region configuration".to_string())
            }),
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            LEVEL_EXIT_COMPONENT_TYPE_ID,
            LEVEL_EXIT_CODEC_ID,
            encode_level_exit,
            decode_level_exit,
            Some(|value| {
                value
                    .config
                    .is_valid()
                    .then_some(())
                    .ok_or_else(|| "invalid level exit configuration".to_string())
            }),
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            DOOR_ACCESS_COMPONENT_TYPE_ID,
            DOOR_ACCESS_CODEC_ID,
            encode_door_access,
            decode_door_access,
            Some(|value| {
                value
                    .is_valid()
                    .then_some(())
                    .ok_or_else(|| "invalid door access configuration".to_string())
            }),
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            INTERLOCK_COMPONENT_TYPE_ID,
            INTERLOCK_CODEC_ID,
            encode_interlock,
            decode_interlock,
            None,
        )?)
        .map_err(|error| error.to_string())?;
    registry
        .register(durable_registration(
            HEALTH_CONFIG_COMPONENT_TYPE_ID,
            HEALTH_CONFIG_CODEC_ID,
            encode_health_config,
            decode_health_config,
            None,
        )?)
        .map_err(|error| error.to_string())?;
    Ok(())
}
