use rusty_engine::core_ids::EntityId;
use rusty_engine::core_time::Tick;
use rusty_engine::entity_state::EntityView;

use crate::runtime::RuntimeError;
use crate::session::GameSession;

pub const MAX_EXTRACTION_BEACON_ACTIVATION_RADIUS: f32 = 32.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtractionBeaconConfig {
    pub activation_radius: f32,
}

impl ExtractionBeaconConfig {
    pub const fn new(activation_radius: f32) -> Self {
        Self { activation_radius }
    }

    pub(crate) fn is_valid(self) -> bool {
        self.activation_radius.is_finite()
            && self.activation_radius > 0.0
            && self.activation_radius <= MAX_EXTRACTION_BEACON_ACTIVATION_RADIUS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractionBeaconState {
    Standby,
    Active { actor: EntityId, activated_at: Tick },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtractionBeaconComponent {
    pub config: ExtractionBeaconConfig,
    pub state: ExtractionBeaconState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractionBeaconView {
    pub entity: EntityId,
    pub config: ExtractionBeaconConfig,
    pub state: ExtractionBeaconState,
    pub entity_view: EntityView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractionBeaconFact {
    Activated {
        beacon: EntityId,
        actor: EntityId,
        tick: Tick,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractionBeaconReceipt {
    pub fact: ExtractionBeaconFact,
}

pub(crate) struct ExtractionBeaconService;

impl ExtractionBeaconService {
    pub(crate) fn activate(
        session: &mut GameSession,
        tick: Tick,
        actor: EntityId,
        beacon: EntityId,
    ) -> Result<ExtractionBeaconReceipt, RuntimeError> {
        session
            .entities
            .view(actor)
            .map_err(|_| RuntimeError::UnknownActor { actor })?;
        let actor_translation = session
            .gameplay_translation(actor)
            .ok_or(RuntimeError::ExtractionBeaconActorMissingTransform { actor })?;
        let component = session
            .extraction_beacons
            .get(&beacon)
            .copied()
            .ok_or(RuntimeError::UnknownExtractionBeacon { beacon })?;
        if component.state != ExtractionBeaconState::Standby {
            return Err(RuntimeError::ExtractionBeaconAlreadyActive { beacon });
        }
        let beacon_translation = session
            .entities
            .view(beacon)
            .expect("admitted extraction beacon entity")
            .transform
            .expect("admitted extraction beacon transform")
            .translation;
        let distance_squared = (actor_translation - beacon_translation).length_squared();
        let radius_squared =
            component.config.activation_radius * component.config.activation_radius;
        if distance_squared > radius_squared {
            return Err(RuntimeError::ExtractionBeaconOutOfRange {
                actor,
                beacon,
                distance_squared,
                activation_radius: component.config.activation_radius,
            });
        }

        let state = ExtractionBeaconState::Active {
            actor,
            activated_at: tick,
        };
        session
            .extraction_beacons
            .get_mut(&beacon)
            .expect("beacon validated above")
            .state = state;
        Ok(ExtractionBeaconReceipt {
            fact: ExtractionBeaconFact::Activated {
                beacon,
                actor,
                tick,
            },
        })
    }
}
