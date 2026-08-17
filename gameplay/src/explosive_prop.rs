use rusty_engine::core_ids::EntityId;
use rusty_engine::engine_spatial::{
    SpatialOcclusionError, SpatialOcclusionQuery, SpatialOcclusionService, VoxelCollisionScene,
};

use crate::runtime_records::GameEvent;
use crate::session::GameSession;
use crate::vitality::{
    DamageCommand, DamageService, DamageSource, VitalityReceipt, VitalityRejection, MAX_DAMAGE,
};

pub const MAX_EXPLOSION_RADIUS: f32 = 100_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExplosivePropConfig {
    pub damage: u32,
    pub radius: f32,
}

impl ExplosivePropConfig {
    pub(crate) fn is_valid(self) -> bool {
        (1..=MAX_DAMAGE).contains(&self.damage)
            && self.radius.is_finite()
            && self.radius > 0.0
            && self.radius <= MAX_EXPLOSION_RADIUS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplosivePropState {
    Armed,
    Exploded,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExplosivePropComponent {
    pub config: ExplosivePropConfig,
    pub state: ExplosivePropState,
    pub(crate) pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExplosivePropView {
    pub entity: EntityId,
    pub config: ExplosivePropConfig,
    pub state: ExplosivePropState,
    pub pending: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExplosivePropFact {
    Triggered {
        prop: EntityId,
        source: DamageSource,
    },
    ExplosionStarted {
        prop: EntityId,
        damage: u32,
        radius: f32,
    },
    TargetOccluded {
        prop: EntityId,
        target: EntityId,
    },
    TargetDamaged {
        prop: EntityId,
        target: EntityId,
        damage: u32,
        distance: f32,
    },
    ExplosionResolved {
        prop: EntityId,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExplosivePropPhaseReceipt {
    pub facts: Vec<ExplosivePropFact>,
    pub damage: Vec<VitalityReceipt>,
    pub events: Vec<GameEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplosivePropError {
    SpatialOcclusion(SpatialOcclusionError),
    Vitality(VitalityRejection),
}

impl std::fmt::Display for ExplosivePropError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ExplosivePropError {}

pub(crate) struct ExplosivePropService;

impl ExplosivePropService {
    pub(crate) fn run(
        session: &mut GameSession,
        scene: &VoxelCollisionScene,
    ) -> Result<ExplosivePropPhaseReceipt, ExplosivePropError> {
        let mut candidate = session.clone();
        let mut facts = Vec::new();
        let mut damage = Vec::new();
        let mut events = Vec::new();

        loop {
            let pending = candidate
                .explosive_props
                .iter()
                .filter_map(|(entity, component)| {
                    (component.state == ExplosivePropState::Exploded && component.pending)
                        .then_some(*entity)
                })
                .collect::<Vec<_>>();
            if pending.is_empty() {
                break;
            }

            for prop in pending {
                let Some(component) = candidate.explosive_props.get_mut(&prop) else {
                    continue;
                };
                if !component.pending {
                    continue;
                }
                component.pending = false;
                let config = component.config;
                facts.push(ExplosivePropFact::ExplosionStarted {
                    prop,
                    damage: config.damage,
                    radius: config.radius,
                });

                let Some(origin) = candidate.entities.transform(prop) else {
                    continue;
                };
                let origin = origin.translation;
                let targets = candidate.health.keys().copied().collect::<Vec<_>>();
                for target in targets {
                    if target == prop {
                        continue;
                    }
                    let Some(health) = candidate.health(target) else {
                        continue;
                    };
                    if health.current == 0 {
                        continue;
                    }
                    let Some(target_transform) = candidate.entities.transform(target) else {
                        continue;
                    };
                    let delta = target_transform.translation - origin;
                    let distance = delta.length();
                    if !distance.is_finite() || distance > config.radius {
                        continue;
                    }
                    if distance > f32::EPSILON {
                        let direction = delta * distance.recip();
                        let ignored_entities = [prop, target];
                        let occluded = SpatialOcclusionService
                            .cast_ray(
                                scene,
                                &candidate.entities,
                                SpatialOcclusionQuery {
                                    origin: [origin.x as f64, origin.y as f64, origin.z as f64],
                                    direction: [
                                        direction.x as f64,
                                        direction.y as f64,
                                        direction.z as f64,
                                    ],
                                    max_distance: distance as f64,
                                    ignored_entities: &ignored_entities,
                                },
                            )
                            .map_err(ExplosivePropError::SpatialOcclusion)?
                            .is_some_and(|hit| hit.distance() as f32 + 0.000_1 < distance);
                        if occluded {
                            facts.push(ExplosivePropFact::TargetOccluded { prop, target });
                            continue;
                        }
                    }

                    let scaled_damage = ((config.damage as f32)
                        * (1.0 - distance / config.radius).clamp(0.0, 1.0))
                    .ceil() as u32;
                    if scaled_damage == 0 {
                        continue;
                    }
                    let receipt = DamageService::apply(
                        &mut candidate,
                        DamageCommand {
                            source: DamageSource::Explosion { source: prop },
                            target,
                            amount: scaled_damage,
                        },
                    )
                    .map_err(ExplosivePropError::Vitality)?;
                    facts.extend(receipt.explosive_props.iter().cloned());
                    if let Some(event) = receipt.event.clone() {
                        events.push(event);
                    }
                    facts.push(ExplosivePropFact::TargetDamaged {
                        prop,
                        target,
                        damage: scaled_damage,
                        distance,
                    });
                    damage.push(receipt);
                }
                facts.push(ExplosivePropFact::ExplosionResolved { prop });
            }
        }

        *session = candidate;
        Ok(ExplosivePropPhaseReceipt {
            facts,
            damage,
            events,
        })
    }
}
