use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::{Vec2, Vec3};
use rusty_engine::engine_spatial::{
    CharacterContactKind, CharacterControllerCommand, CharacterControllerConfig as EngineConfig,
    CharacterControllerError, CharacterControllerReceipt, CharacterControllerService,
    FirstPersonLookCommand, FirstPersonLookConfig, FirstPersonLookError, FirstPersonLookReceipt,
    FirstPersonLookService, FirstPersonLookState, VoxelCollisionScene, MAX_MOTION_DELTA_SECONDS,
};
use rusty_engine::entity_state::{
    BoundsComponent, CharacterMotionComponent, EntityDefinition, EntityView,
};
use serde::{Deserialize, Serialize};

use crate::definition::GameEntityDefinitionError;
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
pub const MAX_PLAYER_FRAME_LOOK_UNITS: f32 = 64.0;
pub const MAX_PLAYER_FRAME_STEP_SECONDS: f32 = 0.25;
const CANONICAL_STANDING_HEIGHT: f32 = 1.8;
const CANONICAL_CROUCHED_HEIGHT: f32 = 1.1;

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
            && (0.0..=MAX_PLAYER_GRAVITY).contains(&self.gravity_units_per_second_squared)
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
            && self.max_air_jumps == 0
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
            && self.move_step_seconds >= 0.001
            && self.move_step_seconds <= MAX_MOTION_DELTA_SECONDS.min(MAX_PLAYER_FRAME_STEP_SECONDS)
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
    pub(crate) engine: EngineConfig,
    pub(crate) look: FirstPersonLookConfig,
    pub(crate) look_state: FirstPersonLookState,
    pub(crate) eye_offset_from_center: f32,
}

impl PlayerControllerComponent {
    pub(crate) fn admit(
        config: PlayerControllerConfig,
        entity: &mut EntityDefinition,
    ) -> Result<Self, GameEntityDefinitionError> {
        let transform = entity.transform.as_mut().ok_or(
            GameEntityDefinitionError::PlayerControllerMissingTransform { entity: entity.id },
        )?;
        let kinematic = entity.kinematic.take().ok_or(
            GameEntityDefinitionError::PlayerControllerMissingKinematic { entity: entity.id },
        )?;
        let authored_half_height = kinematic.half_extents.y;
        let standing_height = CANONICAL_STANDING_HEIGHT.max(authored_half_height * 2.0);
        let crouched_height = CANONICAL_CROUCHED_HEIGHT.min(standing_height - 0.01);
        let radius = kinematic
            .half_extents
            .x
            .max(kinematic.half_extents.z)
            .min(crouched_height * 0.5 - 0.01);
        let center_lift = standing_height * 0.5 - authored_half_height;
        transform.translation.y += center_lift;
        entity.bounds = Some(BoundsComponent {
            min: Vec3::new(-radius, -standing_height * 0.5, -radius),
            max: Vec3::new(radius, standing_height * 0.5, radius),
        });
        entity.character_motion = Some(CharacterMotionComponent::at_rest(transform.translation.y));

        let (engine, look) = canonical_configs(&config, standing_height, crouched_height, radius)
            .map_err(|_| {
            GameEntityDefinitionError::InvalidPlayerControllerConfig { entity: entity.id }
        })?;
        Ok(Self {
            look_state: FirstPersonLookState {
                // Existing Loading Bay content authored positive yaw with the
                // old local basis. Canonical Engine yaw is positive-right, so
                // convert the authored starting heading once at admission.
                yaw_radians: -config.initial_yaw_degrees.to_radians(),
                pitch_radians: config.initial_pitch_degrees.to_radians(),
            },
            eye_offset_from_center: config.traversal.eye_height - center_lift,
            config,
            engine,
            look,
        })
    }

    pub(crate) fn restore(
        config: PlayerControllerConfig,
        look_state: FirstPersonLookState,
        standing_height: f32,
        crouched_height: f32,
        radius: f32,
        eye_offset_from_center: f32,
    ) -> Result<Self, CharacterControllerError> {
        let (engine, look) = canonical_configs(&config, standing_height, crouched_height, radius)?;
        Ok(Self {
            config,
            engine,
            look,
            look_state,
            eye_offset_from_center,
        })
    }

    pub(crate) fn state(&self, motion: &CharacterMotionComponent) -> PlayerControllerState {
        PlayerControllerState {
            yaw_degrees: self.look_state.yaw_radians.to_degrees(),
            pitch_degrees: self.look_state.pitch_radians.to_degrees(),
            vertical_velocity: (motion.controlled_velocity + motion.external_velocity).y,
            grounded: motion.grounded,
            remaining_air_jumps: 0,
        }
    }

    pub(crate) fn look_receipt(&self) -> Result<FirstPersonLookReceipt, FirstPersonLookError> {
        FirstPersonLookService.integrate(
            &self.look,
            self.look_state,
            FirstPersonLookCommand::default(),
        )
    }
}

fn canonical_configs(
    config: &PlayerControllerConfig,
    standing_height: f32,
    crouched_height: f32,
    radius: f32,
) -> Result<(EngineConfig, FirstPersonLookConfig), CharacterControllerError> {
    let mut engine = EngineConfig::responsive_fps();
    engine.shape.standing_height = standing_height;
    engine.shape.crouched_height = crouched_height;
    engine.shape.radius = radius;
    engine.ground.forward_speed = config.move_speed_units_per_second;
    engine.ground.backward_speed = config.move_speed_units_per_second;
    engine.ground.strafe_speed = config.move_speed_units_per_second;
    engine.air.maximum_speed = config.move_speed_units_per_second;
    engine.air.wish_speed_cap = config.move_speed_units_per_second;
    engine.vertical.gravity = config.traversal.gravity_units_per_second_squared;
    engine.vertical.jump_speed = config.traversal.jump_impulse_units_per_second;
    engine.vertical.terminal_fall_speed = config
        .traversal
        .gravity_units_per_second_squared
        .max(config.traversal.jump_impulse_units_per_second)
        .max(1.0);
    engine.surface.maximum_step_height = config.traversal.max_step_height;
    engine.surface.floor_snap_distance = config.traversal.ground_probe_distance;
    engine.recovery.maximum_distance = standing_height.max(config.traversal.max_step_height);
    engine.recovery.maximum_speed = 60.0;
    engine
        .validate()
        .map_err(CharacterControllerError::InvalidConfig)?;

    let radians_per_unit = config.look_degrees_per_unit.to_radians();
    let mut look = FirstPersonLookConfig::default();
    look.horizontal_radians_per_unit = radians_per_unit;
    look.vertical_radians_per_unit = radians_per_unit;
    look.minimum_pitch_radians = -89.0_f32.to_radians();
    look.maximum_pitch_radians = 89.0_f32.to_radians();
    Ok((engine, look))
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedPlayerFrame {
    pub forward: f32,
    pub right: f32,
    pub yaw_delta: f32,
    pub pitch_delta: f32,
    pub jump_pressed: bool,
    pub jump_held: bool,
    pub step_seconds: f32,
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
    pub motion: Option<CharacterControllerReceipt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerFrameReceipt {
    pub frame: ResolvedPlayerFrame,
    pub facts: Vec<PlayerControlFact>,
    pub motion: CharacterControllerReceipt,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerControllerView {
    pub entity: EntityId,
    pub config: PlayerControllerConfig,
    pub state: PlayerControllerState,
    pub eye_offset_from_center: f32,
    pub entity_view: EntityView,
}

pub(crate) fn apply_player_action(
    session: &mut GameSession,
    scene: &VoxelCollisionScene,
    service: &mut CharacterControllerService,
    player: EntityId,
    action: ResolvedPlayerAction,
) -> Result<PlayerControlReceipt, RuntimeError> {
    match action {
        ResolvedPlayerAction::Look {
            yaw_delta,
            pitch_delta,
        } => apply_look(session, player, action, yaw_delta, pitch_delta),
        ResolvedPlayerAction::Move { forward, right } => {
            if !forward.is_finite()
                || !right.is_finite()
                || !(-1.0..=1.0).contains(&forward)
                || !(-1.0..=1.0).contains(&right)
            {
                return Err(RuntimeError::InvalidPlayerAction { action });
            }
            let step_seconds = session
                .fact::<PlayerControllerComponent>(player)
                .ok_or(RuntimeError::UnknownPlayerController { player })?
                .config
                .move_step_seconds;
            let receipt = apply_player_frame(
                session,
                scene,
                service,
                player,
                ResolvedPlayerFrame {
                    forward,
                    right,
                    yaw_delta: 0.0,
                    pitch_delta: 0.0,
                    jump_pressed: false,
                    jump_held: false,
                    step_seconds,
                },
            )?;
            Ok(PlayerControlReceipt {
                action,
                facts: receipt.facts,
                motion: Some(receipt.motion),
            })
        }
        ResolvedPlayerAction::Jump => {
            let step_seconds = session
                .fact::<PlayerControllerComponent>(player)
                .ok_or(RuntimeError::UnknownPlayerController { player })?
                .config
                .move_step_seconds;
            let receipt = apply_player_frame(
                session,
                scene,
                service,
                player,
                ResolvedPlayerFrame {
                    forward: 0.0,
                    right: 0.0,
                    yaw_delta: 0.0,
                    pitch_delta: 0.0,
                    jump_pressed: true,
                    jump_held: true,
                    step_seconds,
                },
            )?;
            Ok(PlayerControlReceipt {
                action,
                facts: receipt.facts,
                motion: Some(receipt.motion),
            })
        }
    }
}

fn apply_look(
    session: &mut GameSession,
    player: EntityId,
    action: ResolvedPlayerAction,
    yaw_delta: f32,
    pitch_delta: f32,
) -> Result<PlayerControlReceipt, RuntimeError> {
    if !look_delta_is_valid(yaw_delta, pitch_delta, 1.0) {
        return Err(RuntimeError::InvalidPlayerAction { action });
    }
    let mut component = session
        .fact::<PlayerControllerComponent>(player)
        .ok_or(RuntimeError::UnknownPlayerController { player })?;
    let before = component.look_state;
    component.look_state = integrate_bounded_look(
        &component.look,
        component.look_state,
        yaw_delta,
        pitch_delta,
    )?;
    let after = component.look_state;
    session.store_fact(player, component);
    Ok(PlayerControlReceipt {
        action,
        facts: look_fact(player, before, after).into_iter().collect(),
        motion: None,
    })
}

pub(crate) fn apply_player_frame(
    session: &mut GameSession,
    scene: &VoxelCollisionScene,
    service: &mut CharacterControllerService,
    player: EntityId,
    frame: ResolvedPlayerFrame,
) -> Result<PlayerFrameReceipt, RuntimeError> {
    if !player_frame_is_valid(frame) {
        return Err(RuntimeError::InvalidPlayerFrame { frame });
    }
    if crate::DamageService::is_dead(session, player) {
        return Err(RuntimeError::PlayerDefeated { player });
    }
    let component = session
        .fact::<PlayerControllerComponent>(player)
        .ok_or(RuntimeError::UnknownPlayerController { player })?;
    let look_after = integrate_bounded_look(
        &component.look,
        component.look_state,
        frame.yaw_delta,
        frame.pitch_delta,
    )?;
    let motion_before = *session
        .entities
        .character_motion(player)
        .ok_or(RuntimeError::UnknownPlayerController { player })?;
    let before = session
        .entities
        .transform(player)
        .ok_or(RuntimeError::UnknownPlayerController { player })?
        .translation;
    let minimum_substeps = if frame.jump_pressed && !motion_before.grounded {
        2.0
    } else {
        1.0
    };
    let substeps = (frame.step_seconds / (1.0 / 60.0))
        .ceil()
        .max(minimum_substeps) as u32;
    let step_seconds = frame.step_seconds / substeps as f32;
    let mut accepted_step = None;
    let mut any_block = false;
    let mut motion = None;
    for index in 0..substeps {
        let sequence = session
            .entities
            .character_motion(player)
            .expect("player retains character motion between canonical substeps")
            .last_command_sequence
            .checked_add(1)
            .ok_or(RuntimeError::PlayerCommandSequenceExhausted { player })?;
        let receipt = service
            .step(
                &mut session.entities,
                scene,
                player,
                &component.engine,
                CharacterControllerCommand {
                    planar_intent: Vec2::new(frame.right, frame.forward),
                    heading_yaw_radians: look_after.yaw_radians,
                    jump_pressed: component.config.traversal.manual_jump_enabled
                        && frame.jump_pressed
                        && index == 0,
                    jump_held: component.config.traversal.manual_jump_enabled && frame.jump_held,
                    step_seconds,
                    sequence,
                    ..CharacterControllerCommand::idle(step_seconds, sequence)
                },
            )
            .map_err(RuntimeError::CharacterController)?;
        if receipt.step.is_some_and(|step| step.accepted) {
            accepted_step = Some((
                receipt.transform_before.translation,
                receipt.transform_after.translation,
            ));
        }
        any_block |= !receipt.blocks.is_empty()
            || receipt
                .contacts
                .iter()
                .any(|contact| contact.kind == CharacterContactKind::Wall);
        motion = Some(receipt);
    }
    let motion = motion.expect("a valid sampled frame always has one canonical substep");
    let mut controller = session
        .fact::<PlayerControllerComponent>(player)
        .expect("player controller remains attached");
    controller.look_state = look_after;
    session.store_fact(player, controller);

    let mut facts = Vec::new();
    if let Some(fact) = look_fact(player, component.look_state, look_after) {
        facts.push(fact);
    }
    let after = motion.transform_after.translation;
    if before != after {
        facts.push(PlayerControlFact::Moved {
            entity: player,
            before,
            after,
        });
    }
    if let Some((step_before, step_after)) = accepted_step {
        facts.push(PlayerControlFact::Stepped {
            entity: player,
            before: step_before,
            after: step_after,
        });
    }
    if any_block {
        facts.push(PlayerControlFact::Blocked {
            entity: player,
            attempted_velocity: motion.wish_velocity,
        });
    }
    if frame.jump_pressed
        && component.config.traversal.manual_jump_enabled
        && !motion.motion_after.grounded
        && motion.motion_after.controlled_velocity.y > motion_before.controlled_velocity.y
    {
        facts.push(PlayerControlFact::Jumped {
            entity: player,
            impulse: component.config.traversal.jump_impulse_units_per_second,
        });
    }
    if !motion_before.grounded && motion.motion_after.grounded {
        facts.push(PlayerControlFact::Landed {
            entity: player,
            translation: after,
        });
    }
    Ok(PlayerFrameReceipt {
        frame,
        facts,
        motion,
    })
}

fn integrate_bounded_look(
    config: &FirstPersonLookConfig,
    mut state: FirstPersonLookState,
    mut yaw_delta: f32,
    mut pitch_delta: f32,
) -> Result<FirstPersonLookState, RuntimeError> {
    while yaw_delta != 0.0 || pitch_delta != 0.0 {
        let yaw = yaw_delta.clamp(-1.0, 1.0);
        let pitch = pitch_delta.clamp(-1.0, 1.0);
        state = FirstPersonLookService
            .integrate(
                config,
                state,
                FirstPersonLookCommand {
                    delta: Vec2::new(yaw, pitch),
                },
            )
            .map_err(RuntimeError::FirstPersonLook)?
            .after;
        yaw_delta -= yaw;
        pitch_delta -= pitch;
    }
    Ok(state)
}

fn look_fact(
    player: EntityId,
    before: FirstPersonLookState,
    after: FirstPersonLookState,
) -> Option<PlayerControlFact> {
    (before != after).then_some(PlayerControlFact::LookChanged {
        entity: player,
        before_yaw_degrees: before.yaw_radians.to_degrees(),
        after_yaw_degrees: after.yaw_radians.to_degrees(),
        before_pitch_degrees: before.pitch_radians.to_degrees(),
        after_pitch_degrees: after.pitch_radians.to_degrees(),
    })
}

fn look_delta_is_valid(yaw: f32, pitch: f32, maximum: f32) -> bool {
    yaw.is_finite()
        && pitch.is_finite()
        && (-maximum..=maximum).contains(&yaw)
        && (-maximum..=maximum).contains(&pitch)
}

fn player_frame_is_valid(frame: ResolvedPlayerFrame) -> bool {
    frame.forward.is_finite()
        && frame.right.is_finite()
        && (-1.0..=1.0).contains(&frame.forward)
        && (-1.0..=1.0).contains(&frame.right)
        && look_delta_is_valid(
            frame.yaw_delta,
            frame.pitch_delta,
            MAX_PLAYER_FRAME_LOOK_UNITS,
        )
        && frame.step_seconds.is_finite()
        && frame.step_seconds >= 0.001
        && frame.step_seconds <= MAX_PLAYER_FRAME_STEP_SECONDS
}

impl From<CharacterControllerError> for RuntimeError {
    fn from(value: CharacterControllerError) -> Self {
        Self::CharacterController(value)
    }
}

impl From<FirstPersonLookError> for RuntimeError {
    fn from(value: FirstPersonLookError) -> Self {
        Self::FirstPersonLook(value)
    }
}
