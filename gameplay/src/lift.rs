use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::core_time::TickDelta;
use rusty_engine::engine_spatial::{
    KinematicTriggerDefinition, TriggerGeometrySource, TriggerOverlapFact, TriggerOverlapFactKind,
    TriggerReconcileCause, TriggerVolumeDiagnostic, TriggerVolumeSystem,
};
use rusty_engine::entity_state::{
    EntityCommand, EntityCommandBatch, EntityFact, EntityView, MAX_ABS_TRANSLATION,
};

use crate::lift_program::{execute_lift_program, LiftOperation, LiftPredicate};
use crate::runtime::RuntimeError;
use crate::session::GameSession;

pub const LIFT_TRIGGER_SCOPE: &str = "game.lift";
pub const MAX_LIFT_OVERLAP_SUBJECTS: usize = 128;
pub const MAX_LIFT_MOTION_TICKS: u64 = 100_000;
pub const MAX_LIFT_WAIT_TICKS: u64 = 1_000_000;
pub const MAX_LIFT_PRESENTATION_BYTES: usize = 160;
pub const MAX_LIFT_SOURCE_BYTES: usize = 160;
pub const DEFAULT_LIFT_MOTION_DURATION_TICKS: u64 = 1;
pub const DEFAULT_LIFT_WAIT_TICKS: u64 = 0;
pub const DEFAULT_LIFT_PROMPT: &str = "Use lift";
pub const DEFAULT_LIFT_PRESENTATION: &str = "Lift moving";
pub const DEFAULT_LIFT_SOURCE: &str = "authored.lift";

#[derive(Debug, Clone, PartialEq)]
pub struct LiftConfig {
    pub target_platform: EntityId,
    pub raised_translation: Vec3,
    pub lowered_translation: Vec3,
    pub motion_duration: TickDelta,
    pub lowered_wait: TickDelta,
    pub prompt: String,
    pub presentation: String,
    pub source: String,
}

impl LiftConfig {
    pub fn new(
        target_platform: EntityId,
        raised_translation: Vec3,
        lowered_translation: Vec3,
        motion_duration: TickDelta,
        lowered_wait: TickDelta,
    ) -> Self {
        Self {
            target_platform,
            raised_translation,
            lowered_translation,
            motion_duration,
            lowered_wait,
            prompt: DEFAULT_LIFT_PROMPT.to_owned(),
            presentation: DEFAULT_LIFT_PRESENTATION.to_owned(),
            source: DEFAULT_LIFT_SOURCE.to_owned(),
        }
    }

    pub fn with_metadata(
        mut self,
        prompt: impl Into<String>,
        presentation: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        self.prompt = prompt.into();
        self.presentation = presentation.into();
        self.source = source.into();
        self
    }

    pub fn is_valid(&self) -> bool {
        self.target_platform != EntityId::new(0)
            && valid_translation(self.raised_translation)
            && valid_translation(self.lowered_translation)
            && (1..=MAX_LIFT_MOTION_TICKS).contains(&self.motion_duration.raw())
            && self.lowered_wait.raw() <= MAX_LIFT_WAIT_TICKS
            && valid_metadata(&self.prompt, MAX_LIFT_PRESENTATION_BYTES)
            && valid_metadata(&self.presentation, MAX_LIFT_PRESENTATION_BYTES)
            && valid_metadata(&self.source, MAX_LIFT_SOURCE_BYTES)
    }
}

impl Default for LiftConfig {
    fn default() -> Self {
        Self {
            target_platform: EntityId::new(0),
            raised_translation: Vec3::ZERO,
            lowered_translation: Vec3::ZERO,
            motion_duration: TickDelta::new(DEFAULT_LIFT_MOTION_DURATION_TICKS),
            lowered_wait: TickDelta::new(DEFAULT_LIFT_WAIT_TICKS),
            prompt: DEFAULT_LIFT_PROMPT.to_owned(),
            presentation: DEFAULT_LIFT_PRESENTATION.to_owned(),
            source: DEFAULT_LIFT_SOURCE.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiftState {
    Raised,
    Lowering,
    Waiting,
    Raising,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiftComponent {
    pub config: LiftConfig,
    pub state: LiftState,
    pub motion_elapsed: TickDelta,
    pub wait_elapsed: TickDelta,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiftView {
    pub entity: EntityId,
    pub config: LiftConfig,
    pub state: LiftState,
    pub motion_elapsed: TickDelta,
    pub wait_elapsed: TickDelta,
    pub entity_view: EntityView,
    pub target_platform_view: EntityView,
}

impl LiftView {
    pub fn motion_elapsed(&self) -> TickDelta {
        self.motion_elapsed
    }

    pub fn wait_elapsed(&self) -> TickDelta {
        self.wait_elapsed
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiftActivation {
    pub lift: EntityId,
    pub target_platform: EntityId,
    pub actor: EntityId,
    pub prompt: String,
    pub presentation: String,
    pub source: String,
    pub entity_facts: Vec<EntityFact>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiftPhaseReceipt {
    pub trigger_facts: Vec<TriggerOverlapFact>,
    pub activations: Vec<LiftActivation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiftRejection {
    UnknownActor {
        actor: EntityId,
    },
    PlayerDefeated {
        actor: EntityId,
    },
    Trigger {
        diagnostics: Vec<TriggerVolumeDiagnostic>,
    },
    InvalidConfig {
        lift: EntityId,
    },
    UnknownTarget {
        lift: EntityId,
        target_platform: EntityId,
    },
    TargetMissingCollision {
        lift: EntityId,
        target_platform: EntityId,
    },
    WorldMutationFailed {
        lift: EntityId,
        target_platform: EntityId,
    },
    MissingProgramBinding {
        lift: EntityId,
    },
    UnknownProgram {
        lift: EntityId,
        program_id: String,
    },
    InvalidProgramOperation {
        lift: EntityId,
        operation: &'static str,
    },
}

impl std::fmt::Display for LiftRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for LiftRejection {}

pub struct LiftService;

impl LiftService {
    pub(crate) fn trigger_system(session: &GameSession) -> TriggerVolumeSystem {
        TriggerVolumeSystem::new(session.lifts.keys().copied().map(|lift| {
            KinematicTriggerDefinition::new(lift, LIFT_TRIGGER_SCOPE, ["lift"])
                .with_geometry_source(TriggerGeometrySource::EntityBounds)
        }))
        .expect("admitted lift trigger identities are fixed and valid")
    }

    pub(crate) fn reconcile_and_activate(
        session: &mut GameSession,
        triggers: &mut TriggerVolumeSystem,
        actor: EntityId,
        tick: u64,
    ) -> Result<LiftPhaseReceipt, LiftRejection> {
        if !session.entities.contains(actor) {
            return Err(LiftRejection::UnknownActor { actor });
        }
        if crate::DamageService::is_dead(session, actor) {
            return Err(LiftRejection::PlayerDefeated { actor });
        }

        let trigger_receipt = triggers
            .reconcile(&session.entities, tick, TriggerReconcileCause::Movement)
            .map_err(|error| LiftRejection::Trigger {
                diagnostics: error.diagnostics,
            })?;
        let mut activations = Vec::new();
        for fact in trigger_receipt.facts.iter().filter(|fact| {
            fact.kind == TriggerOverlapFactKind::Enter && fact.pair.subject_id() == actor
        }) {
            let lift = fact.pair.trigger_id();
            let Some(component) = session.lifts.get(&lift).cloned() else {
                continue;
            };
            let program_id = session
                .lift_program_bindings
                .get(&lift)
                .ok_or(LiftRejection::MissingProgramBinding { lift })?;
            let program = session.lift_programs.get(program_id).ok_or_else(|| {
                LiftRejection::UnknownProgram {
                    lift,
                    program_id: program_id.clone(),
                }
            })?;
            let entered = component.state == LiftState::Raised;
            let mut recorded = false;
            let mut feedback = false;
            let mut entity_facts = Vec::new();
            execute_lift_program(
                program,
                &mut |predicate| {
                    Ok(match predicate {
                        LiftPredicate::ActivationEntered => entered,
                        LiftPredicate::LoweringMotionTick
                        | LiftPredicate::WaitingTick
                        | LiftPredicate::RaisingMotionTick => false,
                    })
                },
                &mut |operation| match operation {
                    LiftOperation::RecordActivation => {
                        if !entered {
                            return Err(LiftRejection::InvalidProgramOperation {
                                lift,
                                operation: "recordActivation",
                            });
                        }
                        recorded = true;
                        Ok(())
                    }
                    LiftOperation::EmitLiftFeedback => {
                        if !recorded {
                            return Err(LiftRejection::InvalidProgramOperation {
                                lift,
                                operation: "emitLiftFeedback",
                            });
                        }
                        feedback = true;
                        Ok(())
                    }
                    LiftOperation::RequestLowerBoundPlatform => {
                        if !entered || !recorded {
                            return Err(LiftRejection::InvalidProgramOperation {
                                lift,
                                operation: "requestLowerBoundPlatform",
                            });
                        }
                        let target_platform = component.config.target_platform;
                        let Some(target_view) = session.entities.view(target_platform).ok() else {
                            return Err(LiftRejection::UnknownTarget {
                                lift,
                                target_platform,
                            });
                        };
                        if target_view
                            .collision
                            .is_none_or(|collision| collision.static_collider)
                        {
                            return Err(LiftRejection::TargetMissingCollision {
                                lift,
                                target_platform,
                            });
                        }
                        if !component.config.is_valid() {
                            return Err(LiftRejection::InvalidConfig { lift });
                        }
                        let receipt = session
                            .entities
                            .apply_batch(EntityCommandBatch::new([
                                EntityCommand::SetCollisionEnabled {
                                    entity: target_platform,
                                    enabled: true,
                                },
                            ]))
                            .map_err(|_| LiftRejection::WorldMutationFailed {
                                lift,
                                target_platform,
                            })?;
                        let component =
                            session.lifts.get_mut(&lift).expect("lift remains attached");
                        component.state = LiftState::Lowering;
                        component.motion_elapsed = TickDelta::ZERO;
                        component.wait_elapsed = TickDelta::ZERO;
                        entity_facts.extend(receipt.facts);
                        Ok(())
                    }
                    LiftOperation::AdvanceLowering
                    | LiftOperation::AdvanceWait
                    | LiftOperation::AdvanceRaising => {
                        Err(LiftRejection::InvalidProgramOperation {
                            lift,
                            operation: "motion operation during activation",
                        })
                    }
                },
            )?;
            if feedback {
                let component = session.lifts.get(&lift).expect("lift remains attached");
                activations.push(LiftActivation {
                    lift,
                    target_platform: component.config.target_platform,
                    actor,
                    prompt: component.config.prompt.clone(),
                    presentation: component.config.presentation.clone(),
                    source: component.config.source.clone(),
                    entity_facts,
                });
            }
        }
        Ok(LiftPhaseReceipt {
            trigger_facts: trigger_receipt.facts,
            activations,
        })
    }

    pub(crate) fn run_motion_phase(session: &mut GameSession) -> Result<(), RuntimeError> {
        let lifts = session
            .lifts
            .iter()
            .map(|(lift, component)| (*lift, component.clone()))
            .collect::<Vec<_>>();
        for (lift, start) in lifts {
            if !start.config.is_valid() {
                return Err(RuntimeError::InvalidLiftConfig { lift });
            }
            let program_id = session
                .lift_program_bindings
                .get(&lift)
                .ok_or(RuntimeError::Lift(LiftRejection::MissingProgramBinding {
                    lift,
                }))?;
            let program = session
                .lift_programs
                .get(program_id)
                .ok_or_else(|| {
                    RuntimeError::Lift(LiftRejection::UnknownProgram {
                        lift,
                        program_id: program_id.clone(),
                    })
                })?
                .clone();
            execute_lift_program(
                &program,
                &mut |predicate| {
                    Ok::<_, LiftRejection>(matches!(
                        (predicate, start.state),
                        (LiftPredicate::LoweringMotionTick, LiftState::Lowering)
                            | (LiftPredicate::WaitingTick, LiftState::Waiting)
                            | (LiftPredicate::RaisingMotionTick, LiftState::Raising)
                    ))
                },
                &mut |operation| match operation {
                    LiftOperation::AdvanceLowering if start.state == LiftState::Lowering => {
                        advance_lowering(session, lift, &start)
                    }
                    LiftOperation::AdvanceWait if start.state == LiftState::Waiting => {
                        advance_wait(session, lift, &start)
                    }
                    LiftOperation::AdvanceRaising if start.state == LiftState::Raising => {
                        advance_raising(session, lift, &start)
                    }
                    LiftOperation::AdvanceLowering
                    | LiftOperation::AdvanceWait
                    | LiftOperation::AdvanceRaising => {
                        Err(LiftRejection::InvalidProgramOperation {
                            lift,
                            operation: "motion operation outside captured state",
                        })
                    }
                    LiftOperation::RecordActivation => {
                        Err(LiftRejection::InvalidProgramOperation {
                            lift,
                            operation: "recordActivation",
                        })
                    }
                    LiftOperation::RequestLowerBoundPlatform => {
                        Err(LiftRejection::InvalidProgramOperation {
                            lift,
                            operation: "requestLowerBoundPlatform",
                        })
                    }
                    LiftOperation::EmitLiftFeedback => {
                        Err(LiftRejection::InvalidProgramOperation {
                            lift,
                            operation: "emitLiftFeedback",
                        })
                    }
                },
            )
            .map_err(RuntimeError::Lift)?;
        }
        Ok(())
    }
}

fn set_motion(
    session: &mut GameSession,
    lift: EntityId,
    start: &LiftComponent,
    state: LiftState,
    motion_elapsed: TickDelta,
    wait_elapsed: TickDelta,
    translation: Vec3,
) -> Result<(), LiftRejection> {
    session
        .entities
        .apply_batch(EntityCommandBatch::new([EntityCommand::SetTranslation {
            entity: start.config.target_platform,
            translation,
        }]))
        .map_err(|_| LiftRejection::WorldMutationFailed {
            lift,
            target_platform: start.config.target_platform,
        })?;
    let component = session.lifts.get_mut(&lift).expect("lift remains attached");
    component.state = state;
    component.motion_elapsed = motion_elapsed;
    component.wait_elapsed = wait_elapsed;
    Ok(())
}
fn advance_lowering(
    session: &mut GameSession,
    lift: EntityId,
    start: &LiftComponent,
) -> Result<(), LiftRejection> {
    let duration = start.config.motion_duration.raw();
    let elapsed = start.motion_elapsed.raw().saturating_add(1).min(duration);
    if elapsed == duration {
        set_motion(
            session,
            lift,
            start,
            LiftState::Waiting,
            TickDelta::ZERO,
            TickDelta::ZERO,
            start.config.lowered_translation,
        )
    } else {
        set_motion(
            session,
            lift,
            start,
            LiftState::Lowering,
            TickDelta::new(elapsed),
            TickDelta::ZERO,
            interpolate(
                start.config.raised_translation,
                start.config.lowered_translation,
                elapsed,
                duration,
            ),
        )
    }
}
fn advance_wait(
    session: &mut GameSession,
    lift: EntityId,
    start: &LiftComponent,
) -> Result<(), LiftRejection> {
    if start.config.lowered_wait.raw() == 0 {
        return set_motion(
            session,
            lift,
            start,
            LiftState::Raising,
            TickDelta::ZERO,
            TickDelta::ZERO,
            start.config.lowered_translation,
        );
    }
    let elapsed = start
        .wait_elapsed
        .raw()
        .saturating_add(1)
        .min(start.config.lowered_wait.raw());
    if elapsed == start.config.lowered_wait.raw() {
        set_motion(
            session,
            lift,
            start,
            LiftState::Raising,
            TickDelta::ZERO,
            TickDelta::ZERO,
            start.config.lowered_translation,
        )
    } else {
        set_motion(
            session,
            lift,
            start,
            LiftState::Waiting,
            TickDelta::ZERO,
            TickDelta::new(elapsed),
            start.config.lowered_translation,
        )
    }
}
fn advance_raising(
    session: &mut GameSession,
    lift: EntityId,
    start: &LiftComponent,
) -> Result<(), LiftRejection> {
    let duration = start.config.motion_duration.raw();
    let elapsed = start.motion_elapsed.raw().saturating_add(1).min(duration);
    if elapsed == duration {
        set_motion(
            session,
            lift,
            start,
            LiftState::Raised,
            TickDelta::ZERO,
            TickDelta::ZERO,
            start.config.raised_translation,
        )
    } else {
        set_motion(
            session,
            lift,
            start,
            LiftState::Raising,
            TickDelta::new(elapsed),
            TickDelta::ZERO,
            interpolate(
                start.config.lowered_translation,
                start.config.raised_translation,
                elapsed,
                duration,
            ),
        )
    }
}

fn interpolate(from: Vec3, to: Vec3, elapsed: u64, duration: u64) -> Vec3 {
    from + (to - from) * (elapsed as f32 / duration as f32)
}

fn valid_translation(value: Vec3) -> bool {
    value.x.is_finite()
        && value.y.is_finite()
        && value.z.is_finite()
        && value.x.abs() <= MAX_ABS_TRANSLATION
        && value.y.abs() <= MAX_ABS_TRANSLATION
        && value.z.abs() <= MAX_ABS_TRANSLATION
}

fn valid_metadata(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max_bytes
}
