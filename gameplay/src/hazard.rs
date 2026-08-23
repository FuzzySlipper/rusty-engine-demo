use std::cell::RefCell;

use rusty_engine::core_ids::EntityId;
use rusty_engine::core_time::{Tick, TickDelta};
use rusty_engine::engine_spatial::{
    KinematicTriggerDefinition, TriggerGeometrySource, TriggerOverlapFact, TriggerReconcileCause,
    TriggerVolumeDiagnostic, TriggerVolumeSystem,
};

use crate::hazard_program::{execute_hazard_program, HazardOperation, HazardPredicate};
use crate::runtime_records::GameEvent;
use crate::session::GameSession;
use crate::vitality::{
    DamageCommand, DamageService, DamageSource, VitalityFact, VitalityRejection, MAX_DOOM_DAMAGE,
};

pub const HAZARD_TRIGGER_SCOPE: &str = "loading-bay.hazard";
pub const MAX_HAZARD_COOLDOWN_TICKS: u64 = 100_000;
pub const MAX_HAZARD_OVERLAP_SUBJECTS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HazardConfig {
    pub damage: u32,
    pub cooldown_ticks: u64,
}

impl HazardConfig {
    pub(crate) fn is_valid(self) -> bool {
        (1..=MAX_DOOM_DAMAGE).contains(&self.damage)
            && self.cooldown_ticks <= MAX_HAZARD_COOLDOWN_TICKS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HazardComponent {
    pub config: HazardConfig,
    pub ready_at_tick: Tick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HazardView {
    pub entity: EntityId,
    pub config: HazardConfig,
    pub ready_at_tick: Tick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HazardFact {
    Damage(VitalityFact),
}

#[derive(Debug, Clone, PartialEq)]
pub struct HazardPhaseReceipt {
    pub trigger_facts: Vec<TriggerOverlapFact>,
    pub facts: Vec<HazardFact>,
    pub events: Vec<GameEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HazardRejection {
    Trigger {
        diagnostics: Vec<TriggerVolumeDiagnostic>,
    },
    Vitality(VitalityRejection),
    MissingProgramBinding {
        hazard: EntityId,
    },
    MissingProgram {
        hazard: EntityId,
        program_id: String,
    },
}

impl std::fmt::Display for HazardRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for HazardRejection {}

pub struct HazardService;

impl HazardService {
    pub(crate) fn trigger_system(session: &GameSession) -> TriggerVolumeSystem {
        TriggerVolumeSystem::new(
            session
                .facts::<HazardComponent>()
                .into_iter()
                .map(|(hazard, _)| hazard)
                .map(|hazard| {
                    KinematicTriggerDefinition::new(hazard, HAZARD_TRIGGER_SCOPE, ["hazard"])
                        .with_geometry_source(TriggerGeometrySource::EntityBounds)
                }),
        )
        .expect("admitted hazard trigger identities are fixed and valid")
    }

    pub(crate) fn reconcile_and_apply(
        session: &mut GameSession,
        triggers: &mut TriggerVolumeSystem,
        player: EntityId,
        tick: Tick,
    ) -> Result<HazardPhaseReceipt, HazardRejection> {
        // Trigger reconciliation and every selected program run are evaluated
        // on candidates. A later program failure therefore cannot leak an
        // earlier damage/cooldown mutation or trigger revision.
        let mut candidate_session = session.clone();
        let mut candidate_triggers = triggers.clone();
        let trigger_receipt = candidate_triggers
            .reconcile(
                &candidate_session.entities,
                tick.raw(),
                TriggerReconcileCause::Movement,
            )
            .map_err(|error| HazardRejection::Trigger {
                diagnostics: error.diagnostics,
            })?;
        let mut facts = Vec::new();
        let mut events = Vec::new();
        let hazard_ids = candidate_session
            .facts::<HazardComponent>()
            .into_iter()
            .map(|(hazard, _)| hazard)
            .collect::<Vec<_>>();
        for hazard in hazard_ids {
            let program_id = candidate_session
                .hazard_program_bindings
                .get(&hazard)
                .cloned()
                .ok_or(HazardRejection::MissingProgramBinding { hazard })?;
            let program = candidate_session
                .hazard_programs
                .get(&program_id)
                .cloned()
                .ok_or_else(|| HazardRejection::MissingProgram {
                    hazard,
                    program_id: program_id.clone(),
                })?;
            let context = RefCell::new(HazardProgramContext {
                session: &mut candidate_session,
                triggers: &mut candidate_triggers,
                player,
                hazard,
                tick,
                facts: Vec::new(),
                events: Vec::new(),
            });
            execute_hazard_program(
                &program,
                &mut |predicate| context.borrow_mut().predicate(predicate),
                &mut |operation| context.borrow_mut().operation(operation),
            )?;
            let context = context.into_inner();
            facts.extend(context.facts);
            events.extend(context.events);
        }
        *session = candidate_session;
        *triggers = candidate_triggers;
        Ok(HazardPhaseReceipt {
            trigger_facts: trigger_receipt.facts,
            facts,
            events,
        })
    }
}

struct HazardProgramContext<'a> {
    session: &'a mut GameSession,
    triggers: &'a mut TriggerVolumeSystem,
    player: EntityId,
    hazard: EntityId,
    tick: Tick,
    facts: Vec<HazardFact>,
    events: Vec<GameEvent>,
}

impl HazardProgramContext<'_> {
    fn predicate(&mut self, predicate: HazardPredicate) -> Result<bool, HazardRejection> {
        match predicate {
            HazardPredicate::PlayerOverlapping => self
                .triggers
                .current_overlaps(self.hazard, MAX_HAZARD_OVERLAP_SUBJECTS)
                .map(|overlaps| overlaps.subjects.contains(&self.player))
                .map_err(|error| HazardRejection::Trigger {
                    diagnostics: error.diagnostics,
                }),
            HazardPredicate::PlayerEligible => Ok(self
                .session
                .health(self.player)
                .is_some_and(|health| health.state == crate::vitality::VitalityState::Alive)),
            HazardPredicate::CooldownReady => Ok(self
                .session
                .fact::<HazardComponent>(self.hazard)
                .is_some_and(|component| self.tick.raw() >= component.ready_at_tick.raw())),
        }
    }

    fn operation(&mut self, operation: HazardOperation) -> Result<(), HazardRejection> {
        match operation {
            HazardOperation::ApplyHazardDamage => {
                let damage = self
                    .session
                    .fact::<HazardComponent>(self.hazard)
                    .expect("hazard identity came from admitted state")
                    .config
                    .damage;
                let receipt = DamageService::apply(
                    self.session,
                    DamageCommand {
                        source: DamageSource::Hazard {
                            hazard: self.hazard,
                        },
                        target: self.player,
                        amount: damage,
                    },
                )
                .map_err(HazardRejection::Vitality)?;
                self.facts
                    .extend(receipt.facts.into_iter().map(HazardFact::Damage));
                if let Some(event) = receipt.event {
                    self.events.push(event);
                }
                Ok(())
            }
            HazardOperation::ScheduleHazardCooldown => {
                let mut component = self
                    .session
                    .fact::<HazardComponent>(self.hazard)
                    .expect("hazard identity came from admitted state");
                component.ready_at_tick = self
                    .tick
                    .advance(TickDelta::new(component.config.cooldown_ticks));
                self.session.store_fact(self.hazard, component);
                Ok(())
            }
        }
    }
}
