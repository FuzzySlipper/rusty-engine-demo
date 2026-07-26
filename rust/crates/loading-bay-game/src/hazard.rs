use core_ids::EntityId;
use core_time::{Tick, TickDelta};
use engine_spatial::{
    KinematicTriggerDefinition, TriggerGeometrySource, TriggerOverlapFact, TriggerReconcileCause,
    TriggerVolumeDiagnostic, TriggerVolumeSystem,
};

use crate::runtime_records::GameEvent;
use crate::session::GameSession;
use crate::vitality::{
    DamageCommand, DamageDisposition, DamageService, DamageSource, VitalityFact, VitalityRejection,
    MAX_DAMAGE,
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
        (1..=MAX_DAMAGE).contains(&self.damage) && self.cooldown_ticks <= MAX_HAZARD_COOLDOWN_TICKS
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
        TriggerVolumeSystem::new(session.hazards.keys().copied().map(|hazard| {
            KinematicTriggerDefinition::new(hazard, HAZARD_TRIGGER_SCOPE, ["hazard"])
                .with_geometry_source(TriggerGeometrySource::EntityBounds)
        }))
        .expect("admitted hazard trigger identities are fixed and valid")
    }

    pub(crate) fn reconcile_and_apply(
        session: &mut GameSession,
        triggers: &mut TriggerVolumeSystem,
        player: EntityId,
        tick: Tick,
    ) -> Result<HazardPhaseReceipt, HazardRejection> {
        let trigger_receipt = triggers
            .reconcile(
                &session.entities,
                tick.raw(),
                TriggerReconcileCause::Movement,
            )
            .map_err(|error| HazardRejection::Trigger {
                diagnostics: error.diagnostics,
            })?;
        let mut facts = Vec::new();
        let mut events = Vec::new();
        let hazard_ids = session.hazards.keys().copied().collect::<Vec<_>>();
        for hazard in hazard_ids {
            let overlaps = triggers
                .current_overlaps(hazard, MAX_HAZARD_OVERLAP_SUBJECTS)
                .map_err(|error| HazardRejection::Trigger {
                    diagnostics: error.diagnostics,
                })?;
            if !overlaps.subjects.contains(&player) {
                continue;
            }
            let component = session
                .hazards
                .get(&hazard)
                .copied()
                .expect("hazard identity came from admitted state");
            if tick.raw() < component.ready_at_tick.raw() {
                continue;
            }
            let receipt = DamageService::apply(
                session,
                DamageCommand {
                    source: DamageSource::Hazard { hazard },
                    target: player,
                    amount: component.config.damage,
                },
            )
            .map_err(HazardRejection::Vitality)?;
            if receipt.disposition == DamageDisposition::Applied {
                session
                    .hazards
                    .get_mut(&hazard)
                    .expect("hazard remains attached")
                    .ready_at_tick = tick.advance(TickDelta::new(component.config.cooldown_ticks));
            }
            facts.extend(receipt.facts.into_iter().map(HazardFact::Damage));
            if let Some(event) = receipt.event {
                events.push(event);
            }
        }
        Ok(HazardPhaseReceipt {
            trigger_facts: trigger_receipt.facts,
            facts,
            events,
        })
    }
}
