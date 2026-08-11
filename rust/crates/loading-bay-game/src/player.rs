use std::collections::BTreeSet;

use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::engine_spatial::{
    KinematicMotionSystem, MotionAxis, MotionFact, MotionPhaseReceipt, VoxelCollisionScene,
    MAX_MOTION_DELTA_SECONDS,
};
use rusty_engine::entity_state::{EntityCommand, EntityCommandBatch, EntityView};
use serde::{Deserialize, Serialize};

use crate::runtime::RuntimeError;
use crate::session::GameSession;

pub const MAX_PLAYER_SPEED_UNITS_PER_SECOND: f32 = 1_000.0;
pub const MAX_PLAYER_LOOK_DEGREES_PER_UNIT: f32 = 180.0;
pub const MAX_INPUT_CONTROL_LENGTH: usize = 64;
pub const MAX_PLAYER_STEP_HEIGHT: f32 = 4.0;
pub const MAX_PLAYER_JUMP_IMPULSE: f32 = 100.0;
pub const MAX_PLAYER_GRAVITY: f32 = 200.0;
pub const MAX_GROUND_PROBE_DISTANCE: f32 = 1.0;
pub const MAX_PLAYER_EYE_HEIGHT: f32 = 10.0;
const STEP_SEARCH_SLICES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerInputBindings {
    pub move_forward: String,
    pub move_backward: String,
    pub move_left: String,
    pub move_right: String,
    pub mouse_look: String,
    pub primary_fire: String,
    pub jump: Option<String>,
    pub select_weapon: Vec<String>,
}

impl PlayerInputBindings {
    pub fn new(
        move_forward: impl Into<String>,
        move_backward: impl Into<String>,
        move_left: impl Into<String>,
        move_right: impl Into<String>,
        mouse_look: impl Into<String>,
        primary_fire: impl Into<String>,
        select_weapon: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            move_forward: move_forward.into(),
            move_backward: move_backward.into(),
            move_left: move_left.into(),
            move_right: move_right.into(),
            mouse_look: mouse_look.into(),
            primary_fire: primary_fire.into(),
            jump: None,
            select_weapon: select_weapon.into_iter().collect(),
        }
    }

    pub fn with_jump(mut self, jump: impl Into<String>) -> Self {
        self.jump = Some(jump.into());
        self
    }

    pub(crate) fn is_valid(&self) -> bool {
        let fixed_controls = [
            self.move_forward.as_str(),
            self.move_backward.as_str(),
            self.move_left.as_str(),
            self.move_right.as_str(),
            self.mouse_look.as_str(),
            self.primary_fire.as_str(),
        ];
        if fixed_controls
            .iter()
            .copied()
            .chain(self.jump.iter().map(String::as_str))
            .chain(self.select_weapon.iter().map(String::as_str))
            .any(|control| control.is_empty() || control.len() > MAX_INPUT_CONTROL_LENGTH)
        {
            return false;
        }
        let controls = fixed_controls
            .into_iter()
            .chain(self.jump.iter().map(String::as_str))
            .chain(self.select_weapon.iter().map(String::as_str))
            .collect::<Vec<_>>();
        controls
            .iter()
            .enumerate()
            .all(|(index, control)| !controls[..index].contains(control))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerTraversalConfig {
    pub max_step_height: f32,
    pub gravity_units_per_second_squared: f32,
    pub jump_impulse_units_per_second: f32,
    pub ground_probe_distance: f32,
    pub eye_height: f32,
    pub manual_jump_enabled: bool,
    pub max_air_jumps: u8,
}

impl Default for PlayerTraversalConfig {
    fn default() -> Self {
        Self {
            max_step_height: 0.0,
            gravity_units_per_second_squared: 0.0,
            jump_impulse_units_per_second: 8.0,
            ground_probe_distance: 0.05,
            eye_height: 1.2,
            manual_jump_enabled: false,
            max_air_jumps: 0,
        }
    }
}

impl PlayerTraversalConfig {
    fn is_valid(self) -> bool {
        self.max_step_height.is_finite()
            && (0.0..=MAX_PLAYER_STEP_HEIGHT).contains(&self.max_step_height)
            && self.gravity_units_per_second_squared.is_finite()
            && self.gravity_units_per_second_squared >= 0.0
            && self.gravity_units_per_second_squared <= MAX_PLAYER_GRAVITY
            && self.jump_impulse_units_per_second.is_finite()
            && self.jump_impulse_units_per_second > 0.0
            && self.jump_impulse_units_per_second <= MAX_PLAYER_JUMP_IMPULSE
            && self.ground_probe_distance.is_finite()
            && self.ground_probe_distance > 0.0
            && self.ground_probe_distance <= MAX_GROUND_PROBE_DISTANCE
            && self.eye_height.is_finite()
            && self.eye_height > 0.0
            && self.eye_height <= MAX_PLAYER_EYE_HEIGHT
            && (!self.manual_jump_enabled || self.gravity_units_per_second_squared > 0.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerControllerConfig {
    pub move_speed_units_per_second: f32,
    pub move_step_seconds: f32,
    pub look_degrees_per_unit: f32,
    pub initial_yaw_degrees: f32,
    pub initial_pitch_degrees: f32,
    pub traversal: PlayerTraversalConfig,
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
            && self.traversal.is_valid()
            && (!self.traversal.manual_jump_enabled || self.bindings.jump.is_some())
            && self.bindings.is_valid()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerControllerState {
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    pub vertical_velocity: f32,
    pub grounded: bool,
    pub remaining_air_jumps: u8,
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
    Jump,
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
    Stepped {
        entity: EntityId,
        before: Vec3,
        after: Vec3,
    },
    Jumped {
        entity: EntityId,
        impulse: f32,
    },
    Landed {
        entity: EntityId,
        translation: Vec3,
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
        let move_delta_seconds = session
            .player_controllers
            .get(&player)
            .map(|component| component.config.move_step_seconds)
            .ok_or(RuntimeError::UnknownPlayerController { player })?;
        Self::apply_with_motion_delta(session, scene, player, action, move_delta_seconds)
    }

    pub(crate) fn apply_with_motion_delta(
        session: &mut GameSession,
        scene: &VoxelCollisionScene,
        player: EntityId,
        action: ResolvedPlayerAction,
        move_delta_seconds: f32,
    ) -> Result<PlayerControlReceipt, RuntimeError> {
        if !player_action_is_valid(action) {
            return Err(RuntimeError::InvalidPlayerAction { action });
        }
        let Some(component) = session.player_controllers.get(&player).cloned() else {
            return Err(RuntimeError::UnknownPlayerController { player });
        };
        if crate::DamageService::is_dead(session, player) {
            return Err(RuntimeError::PlayerDefeated { player });
        }
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
                let scale = 1.0 / input_length.max(1.0);
                let yaw = component.state.yaw_degrees.to_radians();
                let forward_basis = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
                let right_basis = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
                let horizontal_velocity = (forward_basis * (forward * scale)
                    + right_basis * (right * scale))
                    * component.config.move_speed_units_per_second;
                let before = player_translation(session, player)?;
                let grounded_before = component.state.grounded
                    || player_is_grounded(
                        session,
                        scene,
                        player,
                        before,
                        component.config.traversal.ground_probe_distance,
                    )?;
                if grounded_before && component.state.vertical_velocity <= 0.0 {
                    settle_downward_collision(
                        session,
                        scene,
                        player,
                        before,
                        before.y - component.config.traversal.ground_probe_distance,
                    )?;
                }
                let mut vertical_velocity = component.state.vertical_velocity;
                if grounded_before && vertical_velocity <= 0.0 {
                    vertical_velocity = 0.0;
                } else {
                    vertical_velocity -=
                        component.config.traversal.gravity_units_per_second_squared
                            * move_delta_seconds;
                }

                let vertical_start = player_translation(session, player)?;
                let vertical_motion = run_velocity(
                    session,
                    scene,
                    player,
                    Vec3::new(0.0, vertical_velocity, 0.0),
                    move_delta_seconds,
                )?;
                let vertical_blocked = motion_blocked_axis(&vertical_motion, player, MotionAxis::Y);
                let downward_contact = vertical_velocity < 0.0 && vertical_blocked;
                if downward_contact {
                    settle_downward_collision(
                        session,
                        scene,
                        player,
                        vertical_start,
                        vertical_start.y + vertical_velocity * move_delta_seconds,
                    )?;
                }
                if vertical_blocked {
                    vertical_velocity = 0.0;
                }
                let horizontal_before = player_translation(session, player)?;
                let horizontal_motion = if input_length == 0.0 {
                    None
                } else {
                    Some(run_velocity(
                        session,
                        scene,
                        player,
                        horizontal_velocity,
                        move_delta_seconds,
                    )?)
                };
                let horizontal_blocked = horizontal_motion.as_ref().is_some_and(|motion| {
                    motion_blocked_axis(motion, player, MotionAxis::X)
                        || motion_blocked_axis(motion, player, MotionAxis::Z)
                });
                let stepped = if horizontal_blocked
                    && grounded_before
                    && component.config.traversal.max_step_height > 0.0
                {
                    try_step(
                        session,
                        scene,
                        player,
                        horizontal_before,
                        horizontal_velocity,
                        move_delta_seconds,
                        component.config.traversal,
                    )?
                } else {
                    false
                };
                let mut after = player_translation(session, player)?;
                let grounded_after = stepped
                    || (vertical_velocity <= 0.0
                        && player_is_grounded(
                            session,
                            scene,
                            player,
                            after,
                            component.config.traversal.ground_probe_distance,
                        )?);
                let landed = !grounded_before && grounded_after;
                if grounded_after && vertical_velocity < 0.0 {
                    settle_downward_collision(
                        session,
                        scene,
                        player,
                        after,
                        after.y - component.config.traversal.ground_probe_distance,
                    )?;
                    after = player_translation(session, player)?;
                    vertical_velocity = 0.0;
                }
                let controller = session
                    .player_controllers
                    .get_mut(&player)
                    .expect("player controller validated above");
                controller.state.vertical_velocity = vertical_velocity;
                controller.state.grounded = grounded_after;
                if grounded_after {
                    controller.state.remaining_air_jumps = component.config.traversal.max_air_jumps;
                }

                let mut facts = Vec::new();
                if after != before {
                    facts.push(PlayerControlFact::Moved {
                        entity: player,
                        before,
                        after,
                    });
                }
                if stepped {
                    facts.push(PlayerControlFact::Stepped {
                        entity: player,
                        before: horizontal_before,
                        after,
                    });
                } else if horizontal_blocked {
                    facts.push(PlayerControlFact::Blocked {
                        entity: player,
                        attempted_velocity: horizontal_velocity,
                    });
                }
                if landed {
                    facts.push(PlayerControlFact::Landed {
                        entity: player,
                        translation: after,
                    });
                }
                Ok(PlayerControlReceipt {
                    action,
                    facts,
                    motion: horizontal_motion.or(Some(vertical_motion)),
                })
            }
            ResolvedPlayerAction::Jump => {
                let grounded = component.state.grounded
                    || (component.state.vertical_velocity <= 0.0
                        && player_is_grounded(
                            session,
                            scene,
                            player,
                            player_translation(session, player)?,
                            component.config.traversal.ground_probe_distance,
                        )?);
                if !component.config.traversal.manual_jump_enabled
                    || (!grounded && component.state.remaining_air_jumps == 0)
                {
                    return Ok(PlayerControlReceipt {
                        action,
                        facts: Vec::new(),
                        motion: None,
                    });
                }
                let impulse = component.config.traversal.jump_impulse_units_per_second;
                let controller = session
                    .player_controllers
                    .get_mut(&player)
                    .expect("player controller validated above");
                controller.state.vertical_velocity = impulse;
                controller.state.grounded = false;
                if !grounded {
                    controller.state.remaining_air_jumps =
                        controller.state.remaining_air_jumps.saturating_sub(1);
                }
                Ok(PlayerControlReceipt {
                    action,
                    facts: vec![PlayerControlFact::Jumped {
                        entity: player,
                        impulse,
                    }],
                    motion: None,
                })
            }
        }
    }
}

fn run_velocity(
    session: &mut GameSession,
    scene: &VoxelCollisionScene,
    player: EntityId,
    velocity: Vec3,
    delta_seconds: f32,
) -> Result<MotionPhaseReceipt, RuntimeError> {
    session
        .entities
        .apply_batch(EntityCommandBatch::new([
            EntityCommand::SetKinematicVelocity {
                entity: player,
                velocity,
            },
        ]))
        .map_err(RuntimeError::EntityBatch)?;
    let result = KinematicMotionSystem::run_selected(
        &mut session.entities,
        scene,
        delta_seconds,
        &BTreeSet::from([player]),
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
    result.map_err(RuntimeError::Motion)
}

fn motion_blocked_axis(
    motion: &MotionPhaseReceipt,
    player: EntityId,
    expected_axis: MotionAxis,
) -> bool {
    motion.facts.iter().any(|fact| {
        matches!(fact, MotionFact::Blocked { entity, axis, .. } if *entity == player && *axis == expected_axis)
    })
}

fn player_translation(session: &GameSession, player: EntityId) -> Result<Vec3, RuntimeError> {
    session
        .entity(player)
        .expect("player controller entity validated above")
        .transform
        .map(|transform| transform.translation)
        .ok_or(RuntimeError::UnknownPlayerController { player })
}

fn set_player_translation(
    session: &mut GameSession,
    player: EntityId,
    translation: Vec3,
) -> Result<(), RuntimeError> {
    session
        .entities
        .apply_batch(EntityCommandBatch::new([EntityCommand::SetTranslation {
            entity: player,
            translation,
        }]))
        .map_err(RuntimeError::EntityBatch)?;
    Ok(())
}

fn player_bounds_at(
    session: &GameSession,
    player: EntityId,
    translation: Vec3,
) -> Result<([f64; 3], [f64; 3]), RuntimeError> {
    let view = session
        .entity(player)
        .expect("player controller entity validated above");
    let half_extents = view
        .kinematic
        .ok_or(RuntimeError::UnknownPlayerController { player })?
        .half_extents;
    Ok((
        [
            f64::from(translation.x - half_extents.x),
            f64::from(translation.y - half_extents.y),
            f64::from(translation.z - half_extents.z),
        ],
        [
            f64::from(translation.x + half_extents.x),
            f64::from(translation.y + half_extents.y),
            f64::from(translation.z + half_extents.z),
        ],
    ))
}

fn player_is_grounded(
    session: &GameSession,
    scene: &VoxelCollisionScene,
    player: EntityId,
    translation: Vec3,
    probe_distance: f32,
) -> Result<bool, RuntimeError> {
    let probe = Vec3::new(translation.x, translation.y - probe_distance, translation.z);
    let (min, max) = player_bounds_at(session, player, probe)?;
    Ok(scene.aabb_overlaps_solid(min, max))
}

fn try_step(
    session: &mut GameSession,
    scene: &VoxelCollisionScene,
    player: EntityId,
    before: Vec3,
    horizontal_velocity: Vec3,
    delta_seconds: f32,
    traversal: PlayerTraversalConfig,
) -> Result<bool, RuntimeError> {
    let normal_after = player_translation(session, player)?;
    let target_x = before.x + horizontal_velocity.x * delta_seconds;
    let target_z = before.z + horizontal_velocity.z * delta_seconds;
    for slice in 1..=STEP_SEARCH_SLICES {
        set_player_translation(session, player, before)?;
        let lift = traversal.max_step_height * slice as f32 / STEP_SEARCH_SLICES as f32;
        let lift_motion = run_velocity(
            session,
            scene,
            player,
            Vec3::new(0.0, lift / delta_seconds, 0.0),
            delta_seconds,
        )?;
        if motion_blocked_axis(&lift_motion, player, MotionAxis::Y) {
            break;
        }
        let horizontal_motion =
            run_velocity(session, scene, player, horizontal_velocity, delta_seconds)?;
        if motion_blocked_axis(&horizontal_motion, player, MotionAxis::X)
            || motion_blocked_axis(&horizontal_motion, player, MotionAxis::Z)
        {
            continue;
        }
        let raised = player_translation(session, player)?;
        if (raised.x - target_x).abs() > 0.000_1 || (raised.z - target_z).abs() > 0.000_1 {
            continue;
        }
        let mut overlapping_y = before.y;
        let mut clear_y = raised.y;
        let base_target = Vec3::new(target_x, before.y, target_z);
        let (base_min, base_max) = player_bounds_at(session, player, base_target)?;
        if !scene.aabb_overlaps_solid(base_min, base_max) {
            continue;
        }
        for _ in 0..16 {
            let middle = (overlapping_y + clear_y) * 0.5;
            let candidate = Vec3::new(target_x, middle, target_z);
            let (min, max) = player_bounds_at(session, player, candidate)?;
            if scene.aabb_overlaps_solid(min, max) {
                overlapping_y = middle;
            } else {
                clear_y = middle;
            }
        }
        let landing = Vec3::new(target_x, clear_y + 0.000_1, target_z);
        if player_is_grounded(
            session,
            scene,
            player,
            landing,
            traversal.ground_probe_distance,
        )? && !dynamic_sweep_overlaps(session, player, raised, landing)
        {
            set_player_translation(session, player, landing)?;
            return Ok(true);
        }
    }
    set_player_translation(session, player, normal_after)?;
    Ok(false)
}

fn settle_downward_collision(
    session: &mut GameSession,
    scene: &VoxelCollisionScene,
    player: EntityId,
    clear: Vec3,
    attempted_y: f32,
) -> Result<(), RuntimeError> {
    let attempted = Vec3::new(clear.x, attempted_y, clear.z);
    if dynamic_sweep_overlaps(session, player, clear, attempted) {
        return Ok(());
    }
    let (attempted_min, attempted_max) = player_bounds_at(session, player, attempted)?;
    if !scene.aabb_overlaps_solid(attempted_min, attempted_max) {
        return Ok(());
    }
    let mut overlapping_y = attempted_y;
    let mut clear_y = clear.y;
    for _ in 0..16 {
        let middle = (overlapping_y + clear_y) * 0.5;
        let candidate = Vec3::new(clear.x, middle, clear.z);
        let (min, max) = player_bounds_at(session, player, candidate)?;
        if scene.aabb_overlaps_solid(min, max) {
            overlapping_y = middle;
        } else {
            clear_y = middle;
        }
    }
    set_player_translation(
        session,
        player,
        Vec3::new(clear.x, clear_y + 0.000_1, clear.z),
    )
}

fn dynamic_sweep_overlaps(session: &GameSession, moving: EntityId, from: Vec3, to: Vec3) -> bool {
    let Ok((from_min, from_max)) = player_bounds_at(session, moving, from) else {
        return true;
    };
    let translation = [
        f64::from(to.x - from.x),
        f64::from(to.y - from.y),
        f64::from(to.z - from.z),
    ];
    let swept_min = [
        from_min[0].min(from_min[0] + translation[0]),
        from_min[1].min(from_min[1] + translation[1]),
        from_min[2].min(from_min[2] + translation[2]),
    ];
    let swept_max = [
        from_max[0].max(from_max[0] + translation[0]),
        from_max[1].max(from_max[1] + translation[1]),
        from_max[2].max(from_max[2] + translation[2]),
    ];
    session.entities().kinematic_bodies().any(|blocker| {
        if blocker.entity == moving
            || !session
                .entity(blocker.entity)
                .ok()
                .and_then(|view| view.collision)
                .is_some_and(|collision| collision.enabled)
        {
            return false;
        }
        let center = blocker.translation.to_array();
        let half = blocker.half_extents.to_array();
        let blocker_min = [
            f64::from(center[0] - half[0]),
            f64::from(center[1] - half[1]),
            f64::from(center[2] - half[2]),
        ];
        let blocker_max = [
            f64::from(center[0] + half[0]),
            f64::from(center[1] + half[1]),
            f64::from(center[2] + half[2]),
        ];
        (0..3)
            .all(|axis| swept_min[axis] < blocker_max[axis] && swept_max[axis] > blocker_min[axis])
    })
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
        ResolvedPlayerAction::Jump => true,
    }
}

fn normalize_yaw(yaw_degrees: f32) -> f32 {
    (yaw_degrees + 180.0).rem_euclid(360.0) - 180.0
}
