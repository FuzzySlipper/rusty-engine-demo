use core_ids::EntityId;
use core_math::Vec3;
use core_time::TickDelta;
use engine_spatial::{GeneratedRoomConfig, VoxelCollisionScene};
use entity_state::{EntityDefinition, MAX_ABS_TRANSLATION};
use serde::Deserialize;

use crate::combat::{HealthConfig, WeaponConfig};
use crate::definition::{GameEntityDefinition, GameEntityDefinitionError};
use crate::door::DoorConfig;
use crate::navigation::NavigationConfig;
use crate::player::{PlayerControllerConfig, PlayerInputBindings};
use crate::session::GameSession;

pub const PROJECT_CONTENT_SCHEMA_VERSION: u32 = 6;

#[derive(Debug)]
pub struct AdmittedProject {
    pub session: GameSession,
    pub collision_scene: Option<VoxelCollisionScene>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ProjectContent {
    schema_version: u32,
    entities: Vec<AuthoredEntityDefinition>,
    voxel_collision: Option<AuthoredVoxelCollision>,
    generated_voxel_environment: Option<AuthoredGeneratedVoxelEnvironment>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AuthoredEntityDefinition {
    id: u64,
    name: String,
    translation: Option<[f32; 3]>,
    collision: Option<AuthoredCollision>,
    renderable: Option<AuthoredRenderable>,
    door: Option<AuthoredDoor>,
    switch: Option<AuthoredSwitch>,
    #[serde(default)]
    enemy: bool,
    health: Option<AuthoredHealth>,
    encounter: Option<AuthoredEncounter>,
    kinematic: Option<AuthoredKinematic>,
    navigation: Option<AuthoredNavigation>,
    player_controller: Option<AuthoredPlayerController>,
    weapon: Option<AuthoredWeapon>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AuthoredVoxelCollision {
    voxel_size: f64,
    chunk_size: u32,
    solid_voxels: Vec<[i64; 3]>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AuthoredGeneratedVoxelEnvironment {
    seed: u64,
    voxel_size: f64,
    chunk_size: u32,
    width: u32,
    height: u32,
    length: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AuthoredCollision {
    enabled: bool,
    static_collider: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AuthoredRenderable {
    asset: String,
    visible: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AuthoredDoor {
    open_translation: [f32; 3],
    auto_close_after_ticks: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AuthoredSwitch {
    controls: Vec<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AuthoredEncounter {
    members: Vec<u64>,
    exit: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AuthoredHealth {
    max: u32,
    hitbox_half_extents: [f32; 3],
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AuthoredKinematic {
    half_extents: [f32; 3],
    velocity: [f32; 3],
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AuthoredNavigation {
    goal: [f32; 3],
    speed_units_per_second: f32,
    max_visited: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AuthoredPlayerController {
    move_speed_units_per_second: f32,
    move_step_seconds: f32,
    look_degrees_per_unit: f32,
    initial_yaw_degrees: f32,
    initial_pitch_degrees: f32,
    bindings: AuthoredPlayerInputBindings,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AuthoredWeapon {
    damage: u32,
    max_distance: f32,
    cooldown_ticks: u64,
    ammo_capacity: u32,
    muzzle_offset: [f32; 3],
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AuthoredPlayerInputBindings {
    move_forward: String,
    move_backward: String,
    move_left: String,
    move_right: String,
    mouse_look: String,
    primary_fire: String,
}

#[derive(Debug)]
pub enum ProjectContentError {
    Decode(serde_json::Error),
    UnsupportedSchema { actual: u32 },
    DoorMissingInitialTranslation { entity: EntityId },
    InvalidDoorOpenTranslation { entity: EntityId },
    InvalidAutoCloseTicks { entity: EntityId },
    KinematicMissingCollisionScene { entity: EntityId },
    NavigationMissingCollisionScene { entity: EntityId },
    AmbiguousVoxelEnvironment,
    CollisionScene(engine_spatial::CollisionSceneError),
    Definition(GameEntityDefinitionError),
}

impl std::fmt::Display for ProjectContentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ProjectContentError {}

pub fn decode_project_content(input: &str) -> Result<AdmittedProject, ProjectContentError> {
    let content: ProjectContent =
        serde_json::from_str(input).map_err(ProjectContentError::Decode)?;
    if content.schema_version != PROJECT_CONTENT_SCHEMA_VERSION {
        return Err(ProjectContentError::UnsupportedSchema {
            actual: content.schema_version,
        });
    }

    let definitions = content
        .entities
        .into_iter()
        .map(authored_definition)
        .collect::<Result<Vec<_>, _>>()?;
    let session =
        GameSession::from_definitions(definitions).map_err(ProjectContentError::Definition)?;
    let collision_scene = match (content.voxel_collision, content.generated_voxel_environment) {
        (Some(_), Some(_)) => return Err(ProjectContentError::AmbiguousVoxelEnvironment),
        (Some(authored), None) => Some(
            VoxelCollisionScene::from_solid_voxels(
                authored.voxel_size,
                authored.chunk_size,
                authored.solid_voxels,
            )
            .map_err(ProjectContentError::CollisionScene)?,
        ),
        (None, Some(authored)) => Some(
            VoxelCollisionScene::from_generated_room(GeneratedRoomConfig {
                seed: authored.seed,
                voxel_size: authored.voxel_size,
                chunk_size: authored.chunk_size,
                width: authored.width,
                height: authored.height,
                length: authored.length,
            })
            .map_err(ProjectContentError::CollisionScene)?,
        ),
        (None, None) => None,
    };
    if let Some(entity) = session
        .entities()
        .kinematic_bodies()
        .next()
        .map(|body| body.entity)
        .filter(|_| collision_scene.is_none())
    {
        return Err(ProjectContentError::KinematicMissingCollisionScene { entity });
    }
    if let Some(entity) = session
        .navigators
        .keys()
        .next()
        .copied()
        .filter(|_| collision_scene.is_none())
    {
        return Err(ProjectContentError::NavigationMissingCollisionScene { entity });
    }
    Ok(AdmittedProject {
        session,
        collision_scene,
    })
}

fn authored_definition(
    authored: AuthoredEntityDefinition,
) -> Result<GameEntityDefinition, ProjectContentError> {
    let entity = EntityId::new(authored.id);
    let initial_translation = authored.translation.map(array_vec3);
    let mut entity_definition = EntityDefinition::new(entity, authored.name);
    if let Some(translation) = initial_translation {
        entity_definition = entity_definition.with_transform(translation);
    }
    if let Some(collision) = authored.collision {
        entity_definition =
            entity_definition.with_collision(collision.enabled, collision.static_collider);
    }
    if let Some(renderable) = authored.renderable {
        entity_definition = entity_definition.with_renderable(renderable.asset, renderable.visible);
    }
    if let Some(kinematic) = authored.kinematic {
        entity_definition = entity_definition.with_kinematic(
            array_vec3(kinematic.half_extents),
            array_vec3(kinematic.velocity),
        );
    }

    let mut definition = GameEntityDefinition::new(entity_definition);
    if let Some(door) = authored.door {
        let Some(closed_translation) = initial_translation else {
            return Err(ProjectContentError::DoorMissingInitialTranslation { entity });
        };
        let open_translation = array_vec3(door.open_translation);
        if !translation_is_valid(open_translation) {
            return Err(ProjectContentError::InvalidDoorOpenTranslation { entity });
        }
        let auto_close_after = match door.auto_close_after_ticks {
            Some(0) => return Err(ProjectContentError::InvalidAutoCloseTicks { entity }),
            Some(ticks) => Some(TickDelta::new(ticks)),
            None => None,
        };
        definition = definition.as_door(DoorConfig::new(
            closed_translation,
            open_translation,
            auto_close_after,
        ));
    }
    if let Some(switch) = authored.switch {
        definition = definition
            .as_switch()
            .controls(switch.controls.into_iter().map(EntityId::new));
    }
    if authored.enemy {
        definition = definition.as_enemy();
    }
    if let Some(health) = authored.health {
        definition = definition.with_health(HealthConfig {
            max: health.max,
            hitbox_half_extents: array_vec3(health.hitbox_half_extents),
        });
    }
    if let Some(encounter) = authored.encounter {
        definition = definition.as_encounter(
            encounter.members.into_iter().map(EntityId::new),
            EntityId::new(encounter.exit),
        );
    }
    if let Some(navigation) = authored.navigation {
        definition = definition.with_navigation(NavigationConfig {
            goal: array_vec3(navigation.goal),
            speed_units_per_second: navigation.speed_units_per_second,
            max_visited: navigation.max_visited,
        });
    }
    if let Some(controller) = authored.player_controller {
        definition = definition.with_player_controller(PlayerControllerConfig {
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
            ),
        });
    }
    if let Some(weapon) = authored.weapon {
        definition = definition.with_weapon(WeaponConfig {
            damage: weapon.damage,
            max_distance: weapon.max_distance,
            cooldown_ticks: weapon.cooldown_ticks,
            ammo_capacity: weapon.ammo_capacity,
            muzzle_offset: array_vec3(weapon.muzzle_offset),
        });
    }
    Ok(definition)
}

fn array_vec3(value: [f32; 3]) -> Vec3 {
    Vec3::new(value[0], value[1], value[2])
}

fn translation_is_valid(value: Vec3) -> bool {
    value.x.is_finite()
        && value.y.is_finite()
        && value.z.is_finite()
        && value.x.abs() <= MAX_ABS_TRANSLATION
        && value.y.abs() <= MAX_ABS_TRANSLATION
        && value.z.abs() <= MAX_ABS_TRANSLATION
}
