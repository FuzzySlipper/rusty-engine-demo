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

use crate::floor_action_program::{
    execute_floor_action_program, FloorActionOperation, FloorActionPredicate,
};
use crate::runtime::RuntimeError;
use crate::session::GameSession;

pub const FLOOR_ACTION_TRIGGER_SCOPE: &str = "game.floor-action";
pub const MAX_FLOOR_ACTION_OVERLAP_SUBJECTS: usize = 128;
pub const MAX_FLOOR_ACTION_MOTION_TICKS: u64 = 100_000;
pub const MAX_FLOOR_ACTION_PRESENTATION_BYTES: usize = 160;
pub const MAX_FLOOR_ACTION_SOURCE_BYTES: usize = 160;
pub const DEFAULT_FLOOR_ACTION_MOTION_DURATION_TICKS: u64 = 1;
pub const DEFAULT_FLOOR_ACTION_PROMPT: &str = "Lower floor";
pub const DEFAULT_FLOOR_ACTION_PRESENTATION: &str = "Floor lowering";
pub const DEFAULT_FLOOR_ACTION_SOURCE: &str = "authored.floor-action";

#[derive(Debug, Clone, PartialEq)]
pub struct FloorActionConfig {
    pub target_platform: EntityId,
    pub upper_translation: Vec3,
    pub lowered_translation: Vec3,
    pub motion_duration: TickDelta,
    pub prompt: String,
    pub presentation: String,
    pub source: String,
}

impl FloorActionConfig {
    pub fn new(
        target_platform: EntityId,
        upper_translation: Vec3,
        lowered_translation: Vec3,
        motion_duration: TickDelta,
        prompt: impl Into<String>,
        presentation: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            target_platform,
            upper_translation,
            lowered_translation,
            motion_duration,
            prompt: prompt.into(),
            presentation: presentation.into(),
            source: source.into(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.target_platform != EntityId::new(0)
            && valid_translation(self.upper_translation)
            && valid_translation(self.lowered_translation)
            && (1..=MAX_FLOOR_ACTION_MOTION_TICKS).contains(&self.motion_duration.raw())
            && valid_metadata(&self.prompt, MAX_FLOOR_ACTION_PRESENTATION_BYTES)
            && valid_metadata(&self.presentation, MAX_FLOOR_ACTION_PRESENTATION_BYTES)
            && valid_metadata(&self.source, MAX_FLOOR_ACTION_SOURCE_BYTES)
    }
}

impl Default for FloorActionConfig {
    fn default() -> Self {
        Self {
            target_platform: EntityId::new(0),
            upper_translation: Vec3::ZERO,
            lowered_translation: Vec3::ZERO,
            motion_duration: TickDelta::new(DEFAULT_FLOOR_ACTION_MOTION_DURATION_TICKS),
            prompt: DEFAULT_FLOOR_ACTION_PROMPT.to_owned(),
            presentation: DEFAULT_FLOOR_ACTION_PRESENTATION.to_owned(),
            source: DEFAULT_FLOOR_ACTION_SOURCE.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloorActionState {
    Armed,
    Lowering,
    Lowered,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FloorActionComponent {
    pub config: FloorActionConfig,
    pub state: FloorActionState,
    pub motion_elapsed: TickDelta,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FloorActionView {
    pub entity: EntityId,
    pub config: FloorActionConfig,
    pub state: FloorActionState,
    pub motion_elapsed: TickDelta,
    pub entity_view: EntityView,
    pub target_platform_view: EntityView,
}

impl FloorActionView {
    pub fn motion_elapsed(&self) -> TickDelta {
        self.motion_elapsed
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FloorActionActivation {
    pub action: EntityId,
    pub target_platform: EntityId,
    pub actor: EntityId,
    pub prompt: String,
    pub presentation: String,
    pub source: String,
    pub entity_facts: Vec<EntityFact>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FloorActionPhaseReceipt {
    pub trigger_facts: Vec<TriggerOverlapFact>,
    pub activations: Vec<FloorActionActivation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FloorActionRejection {
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
        action: EntityId,
    },
    UnknownTarget {
        action: EntityId,
        target_platform: EntityId,
    },
    TargetMissingCollision {
        action: EntityId,
        target_platform: EntityId,
    },
    WorldMutationFailed {
        action: EntityId,
        target_platform: EntityId,
    },
    MissingProgramBinding {
        action: EntityId,
    },
    UnknownProgram {
        action: EntityId,
        program_id: String,
    },
    InvalidProgramOperation {
        action: EntityId,
        operation: &'static str,
    },
}

impl std::fmt::Display for FloorActionRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for FloorActionRejection {}

pub struct FloorActionService;

impl FloorActionService {
    pub(crate) fn trigger_system(session: &GameSession) -> TriggerVolumeSystem {
        TriggerVolumeSystem::new(session.floor_actions.keys().copied().map(|action| {
            KinematicTriggerDefinition::new(action, FLOOR_ACTION_TRIGGER_SCOPE, ["floor-action"])
                .with_geometry_source(TriggerGeometrySource::EntityBounds)
        }))
        .expect("admitted floor action trigger identities are fixed and valid")
    }

    pub(crate) fn reconcile_and_activate(
        session: &mut GameSession,
        triggers: &mut TriggerVolumeSystem,
        actor: EntityId,
        tick: u64,
    ) -> Result<FloorActionPhaseReceipt, FloorActionRejection> {
        if !session.entities.contains(actor) {
            return Err(FloorActionRejection::UnknownActor { actor });
        }
        if crate::DamageService::is_dead(session, actor) {
            return Err(FloorActionRejection::PlayerDefeated { actor });
        }

        let trigger_receipt = triggers
            .reconcile(&session.entities, tick, TriggerReconcileCause::Movement)
            .map_err(|error| FloorActionRejection::Trigger {
                diagnostics: error.diagnostics,
            })?;
        let mut activations = Vec::new();
        for fact in trigger_receipt.facts.iter().filter(|fact| {
            fact.kind == TriggerOverlapFactKind::Enter && fact.pair.subject_id() == actor
        }) {
            let action = fact.pair.trigger_id();
            let Some(component) = session.floor_actions.get(&action).cloned() else {
                continue;
            };
            let program_id = session
                .floor_action_program_bindings
                .get(&action)
                .ok_or(FloorActionRejection::MissingProgramBinding { action })?;
            let program = session
                .floor_action_programs
                .get(program_id)
                .ok_or_else(|| FloorActionRejection::UnknownProgram {
                    action,
                    program_id: program_id.clone(),
                })?;
            let entered = component.state == FloorActionState::Armed;
            let mut recorded = false;
            let mut feedback = false;
            let mut entity_facts = Vec::new();
            execute_floor_action_program(
                program,
                &mut |predicate| {
                    Ok(match predicate {
                        FloorActionPredicate::ActivationEntered => entered,
                        FloorActionPredicate::LoweringMotionTick => false,
                    })
                },
                &mut |operation| match operation {
                    FloorActionOperation::RecordActivation => {
                        if !entered {
                            return Err(FloorActionRejection::InvalidProgramOperation {
                                action,
                                operation: "recordActivation",
                            });
                        }
                        recorded = true;
                        Ok(())
                    }
                    FloorActionOperation::EmitFloorFeedback => {
                        if !recorded {
                            return Err(FloorActionRejection::InvalidProgramOperation {
                                action,
                                operation: "emitFloorFeedback",
                            });
                        }
                        feedback = true;
                        Ok(())
                    }
                    FloorActionOperation::RequestLowerBoundPlatform => {
                        if !entered || !recorded {
                            return Err(FloorActionRejection::InvalidProgramOperation {
                                action,
                                operation: "requestLowerBoundPlatform",
                            });
                        }
                        let target_platform = component.config.target_platform;
                        let Some(target_view) = session.entities.view(target_platform).ok() else {
                            return Err(FloorActionRejection::UnknownTarget {
                                action,
                                target_platform,
                            });
                        };
                        if target_view
                            .collision
                            .is_none_or(|collision| collision.static_collider)
                        {
                            return Err(FloorActionRejection::TargetMissingCollision {
                                action,
                                target_platform,
                            });
                        }
                        if !component.config.is_valid() {
                            return Err(FloorActionRejection::InvalidConfig { action });
                        }
                        let receipt = session
                            .entities
                            .apply_batch(EntityCommandBatch::new([
                                EntityCommand::SetCollisionEnabled {
                                    entity: target_platform,
                                    enabled: true,
                                },
                            ]))
                            .map_err(|_| FloorActionRejection::WorldMutationFailed {
                                action,
                                target_platform,
                            })?;
                        let component = session
                            .floor_actions
                            .get_mut(&action)
                            .expect("floor action remains attached");
                        component.state = FloorActionState::Lowering;
                        component.motion_elapsed = TickDelta::ZERO;
                        entity_facts.extend(receipt.facts);
                        Ok(())
                    }
                    FloorActionOperation::AdvanceLowering => {
                        Err(FloorActionRejection::InvalidProgramOperation {
                            action,
                            operation: "advanceLowering",
                        })
                    }
                },
            )?;
            if feedback {
                let component = session
                    .floor_actions
                    .get(&action)
                    .expect("floor action remains attached");
                activations.push(FloorActionActivation {
                    action,
                    target_platform: component.config.target_platform,
                    actor,
                    prompt: component.config.prompt.clone(),
                    presentation: component.config.presentation.clone(),
                    source: component.config.source.clone(),
                    entity_facts,
                });
            }
        }
        Ok(FloorActionPhaseReceipt {
            trigger_facts: trigger_receipt.facts,
            activations,
        })
    }

    pub(crate) fn run_motion_phase(session: &mut GameSession) -> Result<(), RuntimeError> {
        let actions = session
            .floor_actions
            .iter()
            .map(|(action, component)| (*action, component.clone()))
            .collect::<Vec<_>>();
        for (action, start) in actions {
            if !start.config.is_valid() {
                return Err(RuntimeError::InvalidFloorActionConfig { action });
            }
            let program_id = session.floor_action_program_bindings.get(&action).ok_or({
                RuntimeError::FloorAction(FloorActionRejection::MissingProgramBinding { action })
            })?;
            let program = session
                .floor_action_programs
                .get(program_id)
                .ok_or_else(|| {
                    RuntimeError::FloorAction(FloorActionRejection::UnknownProgram {
                        action,
                        program_id: program_id.clone(),
                    })
                })?
                .clone();
            execute_floor_action_program(
                &program,
                &mut |predicate| {
                    Ok::<_, FloorActionRejection>(matches!(
                        (predicate, start.state),
                        (
                            FloorActionPredicate::LoweringMotionTick,
                            FloorActionState::Lowering
                        )
                    ))
                },
                &mut |operation| match operation {
                    FloorActionOperation::AdvanceLowering
                        if start.state == FloorActionState::Lowering =>
                    {
                        advance_lowering(session, action, &start)
                    }
                    FloorActionOperation::AdvanceLowering => {
                        Err(FloorActionRejection::InvalidProgramOperation {
                            action,
                            operation: "advanceLowering",
                        })
                    }
                    FloorActionOperation::RecordActivation => {
                        Err(FloorActionRejection::InvalidProgramOperation {
                            action,
                            operation: "recordActivation",
                        })
                    }
                    FloorActionOperation::RequestLowerBoundPlatform => {
                        Err(FloorActionRejection::InvalidProgramOperation {
                            action,
                            operation: "requestLowerBoundPlatform",
                        })
                    }
                    FloorActionOperation::EmitFloorFeedback => {
                        Err(FloorActionRejection::InvalidProgramOperation {
                            action,
                            operation: "emitFloorFeedback",
                        })
                    }
                },
            )
            .map_err(RuntimeError::FloorAction)?;
        }
        Ok(())
    }
}

fn advance_lowering(
    session: &mut GameSession,
    action: EntityId,
    start: &FloorActionComponent,
) -> Result<(), FloorActionRejection> {
    let duration = start.config.motion_duration.raw();
    let elapsed = start.motion_elapsed.raw().saturating_add(1).min(duration);
    let (state, motion_elapsed, translation) = if elapsed == duration {
        (
            FloorActionState::Lowered,
            TickDelta::ZERO,
            start.config.lowered_translation,
        )
    } else {
        (
            FloorActionState::Lowering,
            TickDelta::new(elapsed),
            interpolate(
                start.config.upper_translation,
                start.config.lowered_translation,
                elapsed,
                duration,
            ),
        )
    };
    session
        .entities
        .apply_batch(EntityCommandBatch::new([EntityCommand::SetTranslation {
            entity: start.config.target_platform,
            translation,
        }]))
        .map_err(|_| FloorActionRejection::WorldMutationFailed {
            action,
            target_platform: start.config.target_platform,
        })?;
    let component = session
        .floor_actions
        .get_mut(&action)
        .expect("floor action remains attached");
    component.state = state;
    component.motion_elapsed = motion_elapsed;
    Ok(())
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
