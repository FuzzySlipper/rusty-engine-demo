use std::cell::RefCell;

use rusty_engine::core_ids::EntityId;
use rusty_engine::engine_spatial::{
    SpatialOcclusionError, SpatialOcclusionQuery, SpatialOcclusionService, VoxelCollisionScene,
};

use crate::explosive_prop_program::{
    execute_explosive_prop_program, ExplosivePropOperation, ExplosivePropPredicate,
};
use crate::runtime_records::GameEvent;
use crate::session::GameSession;
use crate::vitality::{
    DamageCommand, DamageService, DamageSource, HealthConfig, VitalityReceipt, VitalityRejection,
    MAX_DOOM_DAMAGE,
};

pub const MAX_EXPLOSION_RADIUS: f32 = 100_000.0;
pub const MAX_EXPLOSIVE_PROP_CHAIN_QUEUE: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExplosivePropConfig {
    pub damage: u32,
    pub radius: f32,
}

impl ExplosivePropConfig {
    pub(crate) fn is_valid(self) -> bool {
        (1..=MAX_DOOM_DAMAGE).contains(&self.damage)
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
    MissingProgramBinding { prop: EntityId },
    MissingProgram { prop: EntityId, program_id: String },
    ApplyBeforeTargetSelection { prop: EntityId },
    ProgramLeftPending { prop: EntityId, program_id: String },
    ChainQueueLimit { limit: usize },
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
        let mut processed = 0usize;

        loop {
            let pending = candidate
                .facts::<ExplosivePropComponent>()
                .into_iter()
                .filter_map(|(entity, component)| {
                    (component.state == ExplosivePropState::Exploded && component.pending)
                        .then_some(entity)
                })
                .collect::<Vec<_>>();
            if pending.is_empty() {
                break;
            }

            for prop in pending {
                processed += 1;
                if processed > MAX_EXPLOSIVE_PROP_CHAIN_QUEUE {
                    return Err(ExplosivePropError::ChainQueueLimit {
                        limit: MAX_EXPLOSIVE_PROP_CHAIN_QUEUE,
                    });
                }
                let program_id = candidate
                    .explosive_prop_program_bindings
                    .get(&prop)
                    .cloned()
                    .ok_or(ExplosivePropError::MissingProgramBinding { prop })?;
                let program = candidate
                    .explosive_prop_programs
                    .get(&program_id)
                    .cloned()
                    .ok_or_else(|| ExplosivePropError::MissingProgram {
                        prop,
                        program_id: program_id.clone(),
                    })?;
                let context = RefCell::new(ExplosivePropProgramContext {
                    session: &mut candidate,
                    scene,
                    prop,
                    targets: None,
                    facts: Vec::new(),
                    damage: Vec::new(),
                    events: Vec::new(),
                });
                execute_explosive_prop_program(
                    &program,
                    &mut |predicate| context.borrow_mut().predicate(predicate),
                    &mut |operation| context.borrow_mut().operation(operation),
                )?;
                let context = context.into_inner();
                if context
                    .session
                    .fact::<ExplosivePropComponent>(prop)
                    .is_some_and(|component| component.pending)
                {
                    return Err(ExplosivePropError::ProgramLeftPending { prop, program_id });
                }
                facts.extend(context.facts);
                damage.extend(context.damage);
                events.extend(context.events);
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

#[derive(Debug, Clone, Copy)]
struct RadialTarget {
    entity: EntityId,
    damage: u32,
    distance: f32,
}

struct ExplosivePropProgramContext<'a> {
    session: &'a mut GameSession,
    scene: &'a VoxelCollisionScene,
    prop: EntityId,
    targets: Option<Vec<RadialTarget>>,
    facts: Vec<ExplosivePropFact>,
    damage: Vec<VitalityReceipt>,
    events: Vec<GameEvent>,
}

impl ExplosivePropProgramContext<'_> {
    fn predicate(&mut self, predicate: ExplosivePropPredicate) -> Result<bool, ExplosivePropError> {
        match predicate {
            ExplosivePropPredicate::ExplosionPending => Ok(self
                .session
                .fact::<ExplosivePropComponent>(self.prop)
                .is_some_and(|component| {
                    component.state == ExplosivePropState::Exploded && component.pending
                })),
        }
    }

    fn operation(&mut self, operation: ExplosivePropOperation) -> Result<(), ExplosivePropError> {
        match operation {
            ExplosivePropOperation::SelectRadialTargets => self.select_radial_targets(),
            ExplosivePropOperation::ApplyScaledDamage => self.apply_scaled_damage(),
            ExplosivePropOperation::ResolveExplosion => self.resolve_explosion(),
        }
    }

    fn select_radial_targets(&mut self) -> Result<(), ExplosivePropError> {
        let config = self
            .session
            .fact::<ExplosivePropComponent>(self.prop)
            .expect("pending prop remains attached")
            .config;
        self.facts.push(ExplosivePropFact::ExplosionStarted {
            prop: self.prop,
            damage: config.damage,
            radius: config.radius,
        });
        let Some(origin) = self.session.entities.transform(self.prop) else {
            self.targets = Some(Vec::new());
            return Ok(());
        };
        let origin = origin.translation;
        let candidates = self
            .session
            .facts::<HealthConfig>()
            .into_iter()
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>();
        let mut targets = Vec::new();
        for target in candidates {
            if target == self.prop {
                continue;
            }
            let Some(health) = self.session.health(target) else {
                continue;
            };
            if health.current == 0 {
                continue;
            }
            let Some(target_transform) = self.session.entities.transform(target) else {
                continue;
            };
            let delta = target_transform.translation - origin;
            let distance = delta.length();
            if !distance.is_finite() || distance > config.radius {
                continue;
            }
            if distance > f32::EPSILON {
                let direction = delta * distance.recip();
                let ignored_entities = [self.prop, target];
                let occluded = SpatialOcclusionService
                    .cast_ray(
                        self.scene,
                        &self.session.entities,
                        SpatialOcclusionQuery {
                            origin: [origin.x as f64, origin.y as f64, origin.z as f64],
                            direction: [direction.x as f64, direction.y as f64, direction.z as f64],
                            max_distance: distance as f64,
                            ignored_entities: &ignored_entities,
                        },
                    )
                    .map_err(ExplosivePropError::SpatialOcclusion)?
                    .is_some_and(|hit| hit.distance() as f32 + 0.000_1 < distance);
                if occluded {
                    self.facts.push(ExplosivePropFact::TargetOccluded {
                        prop: self.prop,
                        target,
                    });
                    continue;
                }
            }
            let damage = ((config.damage as f32) * (1.0 - distance / config.radius).clamp(0.0, 1.0))
                .ceil() as u32;
            if damage > 0 {
                targets.push(RadialTarget {
                    entity: target,
                    damage,
                    distance,
                });
            }
        }
        self.targets = Some(targets);
        Ok(())
    }

    fn apply_scaled_damage(&mut self) -> Result<(), ExplosivePropError> {
        let targets = self
            .targets
            .as_ref()
            .ok_or(ExplosivePropError::ApplyBeforeTargetSelection { prop: self.prop })?
            .clone();
        for target in targets {
            let receipt = DamageService::apply(
                self.session,
                DamageCommand {
                    source: DamageSource::Explosion { source: self.prop },
                    target: target.entity,
                    amount: target.damage,
                },
            )
            .map_err(ExplosivePropError::Vitality)?;
            self.facts.extend(receipt.explosive_props.iter().cloned());
            if let Some(event) = receipt.event.clone() {
                self.events.push(event);
            }
            self.facts.push(ExplosivePropFact::TargetDamaged {
                prop: self.prop,
                target: target.entity,
                damage: target.damage,
                distance: target.distance,
            });
            self.damage.push(receipt);
        }
        Ok(())
    }

    fn resolve_explosion(&mut self) -> Result<(), ExplosivePropError> {
        let mut component = self
            .session
            .fact::<ExplosivePropComponent>(self.prop)
            .expect("pending prop remains attached");
        component.pending = false;
        self.session.store_fact(self.prop, component);
        self.facts
            .push(ExplosivePropFact::ExplosionResolved { prop: self.prop });
        Ok(())
    }
}
