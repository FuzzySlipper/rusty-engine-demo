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

        let mut candidate_session = session.clone();
        let mut candidate_triggers = triggers.clone();
        let trigger_receipt = candidate_triggers
            .reconcile(
                &candidate_session.entities,
                tick,
                TriggerReconcileCause::Movement,
            )
            .map_err(|error| LiftRejection::Trigger {
                diagnostics: error.diagnostics,
            })?;
        let mut activations = Vec::new();
        for fact in trigger_receipt.facts.iter().filter(|fact| {
            fact.kind == TriggerOverlapFactKind::Enter && fact.pair.subject_id() == actor
        }) {
            let lift = fact.pair.trigger_id();
            let Some(component) = candidate_session.lifts.get(&lift).cloned() else {
                continue;
            };
            if component.state != LiftState::Raised {
                continue;
            }
            let target_platform = component.config.target_platform;
            let Some(target_view) = candidate_session.entities.view(target_platform).ok() else {
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
            let entity_receipt = candidate_session
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
            let component = candidate_session
                .lifts
                .get_mut(&lift)
                .expect("lift was validated above");
            component.state = LiftState::Lowering;
            component.motion_elapsed = TickDelta::ZERO;
            component.wait_elapsed = TickDelta::ZERO;
            activations.push(LiftActivation {
                lift,
                target_platform,
                actor,
                prompt: component.config.prompt.clone(),
                presentation: component.config.presentation.clone(),
                source: component.config.source.clone(),
                entity_facts: entity_receipt.facts,
            });
        }
        *session = candidate_session;
        *triggers = candidate_triggers;
        Ok(LiftPhaseReceipt {
            trigger_facts: trigger_receipt.facts,
            activations,
        })
    }

    pub(crate) fn run_motion_phase(session: &mut GameSession) -> Result<(), RuntimeError> {
        let mut commands = Vec::new();
        let mut updates = Vec::new();
        for (lift, component) in &session.lifts {
            if !component.config.is_valid() {
                return Err(RuntimeError::InvalidLiftConfig { lift: *lift });
            }
            let duration = component.config.motion_duration.raw();
            let (state, motion_elapsed, wait_elapsed, translation) = match component.state {
                LiftState::Raised => continue,
                LiftState::Lowering => {
                    let elapsed = component
                        .motion_elapsed
                        .raw()
                        .saturating_add(1)
                        .min(duration);
                    if elapsed == duration {
                        (
                            LiftState::Waiting,
                            TickDelta::ZERO,
                            TickDelta::ZERO,
                            component.config.lowered_translation,
                        )
                    } else {
                        (
                            LiftState::Lowering,
                            TickDelta::new(elapsed),
                            TickDelta::ZERO,
                            interpolate(
                                component.config.raised_translation,
                                component.config.lowered_translation,
                                elapsed,
                                duration,
                            ),
                        )
                    }
                }
                LiftState::Waiting => {
                    if component.config.lowered_wait.raw() == 0 {
                        (
                            LiftState::Raising,
                            TickDelta::ZERO,
                            TickDelta::ZERO,
                            component.config.lowered_translation,
                        )
                    } else {
                        let elapsed = component
                            .wait_elapsed
                            .raw()
                            .saturating_add(1)
                            .min(component.config.lowered_wait.raw());
                        if elapsed == component.config.lowered_wait.raw() {
                            (
                                LiftState::Raising,
                                TickDelta::ZERO,
                                TickDelta::ZERO,
                                component.config.lowered_translation,
                            )
                        } else {
                            (
                                LiftState::Waiting,
                                TickDelta::ZERO,
                                TickDelta::new(elapsed),
                                component.config.lowered_translation,
                            )
                        }
                    }
                }
                LiftState::Raising => {
                    let elapsed = component
                        .motion_elapsed
                        .raw()
                        .saturating_add(1)
                        .min(duration);
                    if elapsed == duration {
                        (
                            LiftState::Raised,
                            TickDelta::ZERO,
                            TickDelta::ZERO,
                            component.config.raised_translation,
                        )
                    } else {
                        (
                            LiftState::Raising,
                            TickDelta::new(elapsed),
                            TickDelta::ZERO,
                            interpolate(
                                component.config.lowered_translation,
                                component.config.raised_translation,
                                elapsed,
                                duration,
                            ),
                        )
                    }
                }
            };
            commands.push(EntityCommand::SetTranslation {
                entity: component.config.target_platform,
                translation,
            });
            updates.push((*lift, state, motion_elapsed, wait_elapsed));
        }
        if commands.is_empty() {
            return Ok(());
        }
        session
            .entities
            .apply_batch(EntityCommandBatch::new(commands))
            .map_err(RuntimeError::EntityBatch)?;
        for (lift, state, motion_elapsed, wait_elapsed) in updates {
            let component = session.lifts.get_mut(&lift).expect("lift remains attached");
            component.state = state;
            component.motion_elapsed = motion_elapsed;
            component.wait_elapsed = wait_elapsed;
        }
        Ok(())
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
