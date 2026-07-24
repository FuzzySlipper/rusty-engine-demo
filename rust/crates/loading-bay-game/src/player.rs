use std::collections::BTreeSet;

use core_ids::EntityId;
use core_math::Vec3;
use engine_spatial::{
    KinematicMotionSystem, MotionFact, MotionPhaseReceipt, VoxelCollisionScene,
    MAX_MOTION_DELTA_SECONDS,
};
use entity_state::{EntityCommand, EntityCommandBatch, EntityView};
use serde::{Deserialize, Serialize};

use crate::runtime::RuntimeError;
use crate::session::GameSession;

pub const MAX_PLAYER_SPEED_UNITS_PER_SECOND: f32 = 1_000.0;
pub const MAX_PLAYER_LOOK_DEGREES_PER_UNIT: f32 = 180.0;
pub const MAX_INPUT_CONTROL_LENGTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerInputBindings {
    pub move_forward: String,
    pub move_backward: String,
    pub move_left: String,
    pub move_right: String,
    pub mouse_look: String,
    pub primary_fire: String,
}

impl PlayerInputBindings {
    pub fn new(
        move_forward: impl Into<String>,
        move_backward: impl Into<String>,
        move_left: impl Into<String>,
        move_right: impl Into<String>,
        mouse_look: impl Into<String>,
        primary_fire: impl Into<String>,
    ) -> Self {
        Self {
            move_forward: move_forward.into(),
            move_backward: move_backward.into(),
            move_left: move_left.into(),
            move_right: move_right.into(),
            mouse_look: mouse_look.into(),
            primary_fire: primary_fire.into(),
        }
    }

    pub(crate) fn is_valid(&self) -> bool {
        let controls = [
            self.move_forward.as_str(),
            self.move_backward.as_str(),
            self.move_left.as_str(),
            self.move_right.as_str(),
            self.mouse_look.as_str(),
            self.primary_fire.as_str(),
        ];
        if controls
            .iter()
            .any(|control| control.is_empty() || control.len() > MAX_INPUT_CONTROL_LENGTH)
        {
            return false;
        }
        controls
            .iter()
            .enumerate()
            .all(|(index, control)| !controls[..index].contains(control))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerControllerConfig {
    pub move_speed_units_per_second: f32,
    pub move_step_seconds: f32,
    pub look_degrees_per_unit: f32,
    pub initial_yaw_degrees: f32,
    pub initial_pitch_degrees: f32,
    pub bindings: PlayerInputBindings,
}

impl PlayerControllerConfig {
    pub(crate) fn is_valid(&self) -> bool {
        self.move_speed_units_per_second.is_finite()
            && self.move_speed_units_per_second > 0.0
            && self.move_speed_units_per_second <= MAX_PLAYER_SPEED_UNITS_PER_SECOND
            && self.move_step_seconds.is_finite()
            && self.move_step_seconds > 0.0
            && self.move_step_seconds <= MAX_MOTION_DELTA_SECONDS
            && self.look_degrees_per_unit.is_finite()
            && self.look_degrees_per_unit > 0.0
            && self.look_degrees_per_unit <= MAX_PLAYER_LOOK_DEGREES_PER_UNIT
            && self.initial_yaw_degrees.is_finite()
            && self.initial_pitch_degrees.is_finite()
            && (-89.0..=89.0).contains(&self.initial_pitch_degrees)
            && self.bindings.is_valid()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerControllerState {
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerControllerComponent {
    pub config: PlayerControllerConfig,
    pub state: PlayerControllerState,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ResolvedPlayerAction {
    Move { forward: f32, right: f32 },
    Look { yaw_delta: f32, pitch_delta: f32 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerControlFact {
    Moved {
        entity: EntityId,
        before: Vec3,
        after: Vec3,
    },
    Blocked {
        entity: EntityId,
        attempted_velocity: Vec3,
    },
    LookChanged {
        entity: EntityId,
        before_yaw_degrees: f32,
        after_yaw_degrees: f32,
        before_pitch_degrees: f32,
        after_pitch_degrees: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerControlReceipt {
    pub action: ResolvedPlayerAction,
    pub facts: Vec<PlayerControlFact>,
    pub motion: Option<MotionPhaseReceipt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerControllerView {
    pub entity: EntityId,
    pub config: PlayerControllerConfig,
    pub state: PlayerControllerState,
    pub entity_view: EntityView,
}

pub(crate) struct PlayerControllerService;

impl PlayerControllerService {
    pub(crate) fn apply(
        session: &mut GameSession,
        scene: &VoxelCollisionScene,
        player: EntityId,
        action: ResolvedPlayerAction,
    ) -> Result<PlayerControlReceipt, RuntimeError> {
        if !player_action_is_valid(action) {
            return Err(RuntimeError::InvalidPlayerAction { action });
        }
        let Some(component) = session.player_controllers.get(&player).cloned() else {
            return Err(RuntimeError::UnknownPlayerController { player });
        };
        match action {
            ResolvedPlayerAction::Look {
                yaw_delta,
                pitch_delta,
            } => {
                let before = component.state;
                let controller = session
                    .player_controllers
                    .get_mut(&player)
                    .expect("player controller validated above");
                controller.state.yaw_degrees = normalize_yaw(
                    before.yaw_degrees + yaw_delta * component.config.look_degrees_per_unit,
                );
                controller.state.pitch_degrees = (before.pitch_degrees
                    + pitch_delta * component.config.look_degrees_per_unit)
                    .clamp(-89.0, 89.0);
                Ok(PlayerControlReceipt {
                    action,
                    facts: vec![PlayerControlFact::LookChanged {
                        entity: player,
                        before_yaw_degrees: before.yaw_degrees,
                        after_yaw_degrees: controller.state.yaw_degrees,
                        before_pitch_degrees: before.pitch_degrees,
                        after_pitch_degrees: controller.state.pitch_degrees,
                    }],
                    motion: None,
                })
            }
            ResolvedPlayerAction::Move { forward, right } => {
                let input_length = (forward * forward + right * right).sqrt();
                if input_length == 0.0 {
                    return Ok(PlayerControlReceipt {
                        action,
                        facts: Vec::new(),
                        motion: None,
                    });
                }
                let scale = 1.0 / input_length.max(1.0);
                let yaw = component.state.yaw_degrees.to_radians();
                let forward_basis = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
                let right_basis = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
                let velocity = (forward_basis * (forward * scale) + right_basis * (right * scale))
                    * component.config.move_speed_units_per_second;
                session
                    .entities
                    .apply_batch(EntityCommandBatch::new([
                        EntityCommand::SetKinematicVelocity {
                            entity: player,
                            velocity,
                        },
                    ]))
                    .map_err(RuntimeError::EntityBatch)?;
                let selected = BTreeSet::from([player]);
                let motion_result = KinematicMotionSystem::run_selected(
                    &mut session.entities,
                    scene,
                    component.config.move_step_seconds,
                    &selected,
                );
                session
                    .entities
                    .apply_batch(EntityCommandBatch::new([
                        EntityCommand::SetKinematicVelocity {
                            entity: player,
                            velocity: Vec3::ZERO,
                        },
                    ]))
                    .map_err(RuntimeError::EntityBatch)?;
                let motion = motion_result.map_err(RuntimeError::Motion)?;
                let facts = motion
                    .facts
                    .iter()
                    .filter_map(|fact| match fact {
                        MotionFact::Moved {
                            entity,
                            before,
                            after,
                        } if *entity == player => Some(PlayerControlFact::Moved {
                            entity: *entity,
                            before: *before,
                            after: *after,
                        }),
                        MotionFact::Blocked { entity, .. } if *entity == player => {
                            Some(PlayerControlFact::Blocked {
                                entity: *entity,
                                attempted_velocity: velocity,
                            })
                        }
                        MotionFact::Moved { .. } | MotionFact::Blocked { .. } => None,
                    })
                    .collect();
                Ok(PlayerControlReceipt {
                    action,
                    facts,
                    motion: Some(motion),
                })
            }
        }
    }
}

fn player_action_is_valid(action: ResolvedPlayerAction) -> bool {
    match action {
        ResolvedPlayerAction::Move { forward, right } => {
            forward.is_finite()
                && right.is_finite()
                && (-1.0..=1.0).contains(&forward)
                && (-1.0..=1.0).contains(&right)
        }
        ResolvedPlayerAction::Look {
            yaw_delta,
            pitch_delta,
        } => {
            yaw_delta.is_finite()
                && pitch_delta.is_finite()
                && (-1.0..=1.0).contains(&yaw_delta)
                && (-1.0..=1.0).contains(&pitch_delta)
        }
    }
}

fn normalize_yaw(yaw_degrees: f32) -> f32 {
    (yaw_degrees + 180.0).rem_euclid(360.0) - 180.0
}
