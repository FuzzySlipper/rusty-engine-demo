use std::collections::{BTreeMap, BTreeSet, VecDeque};

use core_ids::EntityId;
use core_math::Vec3;
use core_time::{Tick, TickDelta};
use engine_spatial::{
    GeneratedRoomConfig, MaterialVoxel, VoxelCollisionScene, VoxelSourceRevision,
    GENERATED_ROOM_VERSION,
};
use entity_state::{EntityState, EntityStateSnapshot};
use serde::{Deserialize, Serialize};

use crate::combat::{
    EnemyComponent, EnemyState, HealthComponent, HealthConfig, WeaponComponent, WeaponConfig,
    WeaponState,
};
use crate::door::{DoorComponent, DoorConfig, DoorState};
use crate::encounter::{EncounterComponent, EncounterConfig, EncounterState};
use crate::interaction::SwitchComponent;
use crate::navigation::{
    NavigationComponent, NavigationConfig, NavigationState, MAX_NAVIGATION_QUERY_BUDGET,
    MAX_NAVIGATION_SPEED_UNITS_PER_SECOND,
};
use crate::player::{
    PlayerControllerComponent, PlayerControllerConfig, PlayerControllerState, PlayerInputBindings,
};
use crate::runtime::GameRuntime;
use crate::scheduler::{ScheduledIntent, ScheduledIntentKind, Scheduler};
use crate::session::GameSession;

pub const GAME_SNAPSHOT_SCHEMA_VERSION: u32 = 9;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GameSnapshot {
    pub schema_version: u32,
    pub tick: u64,
    pub entities: EntityStateSnapshot,
    pub voxel_collision: Option<VoxelCollisionSnapshot>,
    pub doors: Vec<DoorSnapshot>,
    pub switches: Vec<SwitchSnapshot>,
    pub controls: Vec<ControlsSnapshot>,
    pub enemies: Vec<EnemySnapshot>,
    pub health: Vec<HealthSnapshot>,
    pub encounters: Vec<EncounterSnapshot>,
    pub navigations: Vec<NavigationSnapshot>,
    pub player_controllers: Vec<PlayerControllerSnapshot>,
    pub weapons: Vec<WeaponSnapshot>,
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
pub struct HealthSnapshot {
    pub entity: u64,
    pub current: u32,
    pub max: u32,
    pub hitbox_half_extents: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EncounterSnapshot {
    pub entity: u64,
    pub state: SnapshotEncounterState,
    pub members: Vec<u64>,
    pub exit: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotEncounterState {
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

#[derive(Debug)]
pub enum GameSnapshotError {
    Encode(serde_json::Error),
    Decode(serde_json::Error),
    UnsupportedSchema { actual: u32 },
    EntityState(entity_state::EntityStateSnapshotError),
    CollisionScene(engine_spatial::CollisionSceneError),
    AmbiguousVoxelSnapshot,
    GeneratedRoomRevisionMismatch { actual: u64 },
    UnsupportedGeneratedRoomVersion { actual: u32 },
    GeneratedRoomHashMismatch { expected: u64, actual: u64 },
    VoxelAuthorityHashMismatch { expected: u64, actual: u64 },
    DuplicateDoor { entity: u64 },
    DuplicateSwitch { entity: u64 },
    DuplicateEnemy { entity: u64 },
    DuplicateHealth { entity: u64 },
    DuplicateEncounter { entity: u64 },
    DuplicateNavigation { entity: u64 },
    DuplicatePlayerController { entity: u64 },
    DuplicateWeapon { entity: u64 },
    UnknownDoorEntity { entity: u64 },
    UnknownSwitchEntity { entity: u64 },
    UnknownEnemyEntity { entity: u64 },
    UnknownHealthEntity { entity: u64 },
    UnknownEncounterEntity { entity: u64 },
    UnknownNavigationEntity { entity: u64 },
    UnknownPlayerControllerEntity { entity: u64 },
    UnknownWeaponEntity { entity: u64 },
    UnknownControlTarget { switch: u64, target: u64 },
    UnknownEncounterMember { encounter: u64, member: u64 },
    UnknownEncounterExit { encounter: u64, exit: u64 },
    MissingDoorCapability { entity: u64 },
    MissingEnemyCapability { entity: u64 },
    MissingHealthCapability { entity: u64 },
    MissingNavigationCapability { entity: u64 },
    MissingPlayerControllerCapability { entity: u64 },
    MissingWeaponCapability { entity: u64 },
    NavigationMissingCollisionScene { entity: u64 },
    PlayerControllerMissingCollisionScene { entity: u64 },
    InvalidNavigationConfig { entity: u64 },
    InvalidPlayerControllerConfig { entity: u64 },
    InvalidHealthConfig { entity: u64 },
    InvalidWeaponConfig { entity: u64 },
    EnemyHealthStateMismatch { entity: u64 },
    DuplicateEncounterMember { encounter: u64, member: u64 },
    EnemyInMultipleEncounters { enemy: u64, first: u64, second: u64 },
    DuplicateSchedule { door: u64 },
}

impl std::fmt::Display for GameSnapshotError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for GameSnapshotError {}

impl GameRuntime {
    pub fn snapshot(&self) -> GameSnapshot {
        GameSnapshot {
            schema_version: GAME_SNAPSHOT_SCHEMA_VERSION,
            tick: self.tick.raw(),
            entities: self.session.entities.snapshot(),
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
            health: self
                .session
                .health
                .iter()
                .map(|(entity, component)| HealthSnapshot {
                    entity: entity.raw(),
                    current: component.current,
                    max: component.config.max,
                    hitbox_half_extents: component.config.hitbox_half_extents.to_array(),
                })
                .collect(),
            encounters: self
                .session
                .encounters
                .iter()
                .map(|(entity, component)| EncounterSnapshot {
                    entity: entity.raw(),
                    state: match component.state {
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
                    },
                })
                .collect(),
            weapons: self
                .session
                .weapons
                .iter()
                .map(|(entity, component)| WeaponSnapshot {
                    entity: entity.raw(),
                    damage: component.config.damage,
                    max_distance: component.config.max_distance,
                    cooldown_ticks: component.config.cooldown_ticks,
                    ammo_capacity: component.config.ammo_capacity,
                    muzzle_offset: component.config.muzzle_offset.to_array(),
                    ammo_remaining: component.state.ammo_remaining,
                    ready_at_tick: component.state.ready_at_tick.raw(),
                })
                .collect(),
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

    pub fn from_snapshot(snapshot: GameSnapshot) -> Result<Self, GameSnapshotError> {
        if snapshot.schema_version != GAME_SNAPSHOT_SCHEMA_VERSION {
            return Err(GameSnapshotError::UnsupportedSchema {
                actual: snapshot.schema_version,
            });
        }
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
            };
            if !config.is_valid() || health_snapshot.current > config.max {
                return Err(GameSnapshotError::InvalidHealthConfig {
                    entity: health_snapshot.entity,
                });
            }
            health.insert(
                entity,
                HealthComponent {
                    config,
                    current: health_snapshot.current,
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

        let mut weapons = BTreeMap::new();
        let mut weapon_ids = BTreeSet::new();
        for weapon_snapshot in snapshot.weapons {
            if !weapon_ids.insert(weapon_snapshot.entity) {
                return Err(GameSnapshotError::DuplicateWeapon {
                    entity: weapon_snapshot.entity,
                });
            }
            let entity = EntityId::new(weapon_snapshot.entity);
            if !entities.contains(entity) {
                return Err(GameSnapshotError::UnknownWeaponEntity {
                    entity: weapon_snapshot.entity,
                });
            }
            if !player_controllers.contains_key(&entity) {
                return Err(GameSnapshotError::MissingWeaponCapability {
                    entity: weapon_snapshot.entity,
                });
            }
            let config = WeaponConfig {
                damage: weapon_snapshot.damage,
                max_distance: weapon_snapshot.max_distance,
                cooldown_ticks: weapon_snapshot.cooldown_ticks,
                ammo_capacity: weapon_snapshot.ammo_capacity,
                muzzle_offset: array_vec3(weapon_snapshot.muzzle_offset),
            };
            if !config.is_valid() || weapon_snapshot.ammo_remaining > config.ammo_capacity {
                return Err(GameSnapshotError::InvalidWeaponConfig {
                    entity: weapon_snapshot.entity,
                });
            }
            weapons.insert(
                entity,
                WeaponComponent {
                    config,
                    state: WeaponState {
                        ammo_remaining: weapon_snapshot.ammo_remaining,
                        ready_at_tick: Tick::new(weapon_snapshot.ready_at_tick),
                    },
                },
            );
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
            if !entities.contains(EntityId::new(encounter.entity)) {
                return Err(GameSnapshotError::UnknownEncounterEntity {
                    entity: encounter.entity,
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
                EntityId::new(encounter.entity),
                EncounterComponent {
                    config: EncounterConfig {
                        members,
                        exit: EntityId::new(encounter.exit),
                    },
                    state: match encounter.state {
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
                switches,
                controls,
                enemies,
                health,
                encounters,
                navigators,
                player_controllers,
                weapons,
            },
            tick: Tick::new(snapshot.tick),
            scheduler,
            events: VecDeque::new(),
            journal: Vec::new(),
            collision_scene,
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
