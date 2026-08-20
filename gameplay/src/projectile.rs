use std::collections::BTreeMap;

use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::core_time::Tick;
use rusty_engine::engine_spatial::{
    RigidBodyAction, RigidBodyService, RigidBodyStepError, RigidBodyStepReceipt,
    RigidBodyStepRequest, VoxelCollisionScene,
};
use rusty_engine::entity_state::{
    EntityAuthoringError, EntityAuthoringService, EntityDefinition, EntityTransform,
    RigidBodyComponent, RigidBodyShape,
};

use crate::combat::CombatFact;
use crate::inventory::ProjectileDefinition;
use crate::runtime_records::GameEvent;
use crate::session::GameSession;
use crate::vitality::{DamageCommand, DamageService, DamageSource, VitalityRejection};

const MAX_PROJECTILE_ENTITIES: usize = 256;
const ENEMY_PROJECTILE_ENTITY_NAME: &str = "enemy projectile";

pub(crate) fn is_projectile_entity_name(name: &str) -> bool {
    name == ENEMY_PROJECTILE_ENTITY_NAME
}

#[derive(Debug)]
pub enum ProjectileError {
    EntityAuthoring(EntityAuthoringError),
    RigidBody(RigidBodyStepError),
    Vitality(VitalityRejection),
    EntityLimit { limit: usize },
}

impl std::fmt::Display for ProjectileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EntityAuthoring(error) => {
                write!(formatter, "projectile entity authoring: {error}")
            }
            Self::RigidBody(error) => write!(formatter, "projectile rigid-body step: {error}"),
            Self::Vitality(error) => write!(formatter, "projectile damage: {error}"),
            Self::EntityLimit { limit } => {
                write!(formatter, "projectile entity limit exceeded ({limit})")
            }
        }
    }
}

impl std::error::Error for ProjectileError {}

#[derive(Debug, Clone, PartialEq)]
pub enum ProjectileFact {
    Impacted {
        entity: EntityId,
        owner: EntityId,
        target: Option<EntityId>,
        position: Vec3,
        damage: u32,
    },
    Expired {
        entity: EntityId,
        owner: EntityId,
        position: Vec3,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectilePhaseReceipt {
    pub facts: Vec<ProjectileFact>,
    pub combat: Vec<CombatFact>,
    pub events: Vec<GameEvent>,
    pub physics: Option<RigidBodyStepReceipt>,
}

#[derive(Debug, Clone, PartialEq)]
struct ActiveProjectile {
    owner: EntityId,
    target: Option<EntityId>,
    definition: ProjectileDefinition,
    damage: u32,
    expires_at: Tick,
    pending_impulse: Vec3,
}

pub(crate) struct EnemyProjectileSpawn {
    pub owner: EntityId,
    pub target: EntityId,
    pub definition: ProjectileDefinition,
    pub damage: u32,
    pub origin: Vec3,
    pub direction: Vec3,
    pub tick: Tick,
    pub visual_asset: String,
}

#[derive(Debug, Default)]
pub(crate) struct ProjectileService {
    next_entity_raw: u64,
    active: BTreeMap<EntityId, ActiveProjectile>,
    rigid_bodies: RigidBodyService,
}

/// The spawn-only portion of [`ProjectileService`] that an attack phase may
/// mutate before its enclosing session transaction commits. Spawning does not
/// touch `rigid_bodies`, so phase rollback deliberately avoids cloning Engine
/// physics state.
#[derive(Debug, Clone)]
pub(crate) struct ProjectileSpawnCheckpoint {
    next_entity_raw: u64,
    active: BTreeMap<EntityId, ActiveProjectile>,
}

impl ProjectileService {
    pub(crate) fn spawn_checkpoint(&self) -> ProjectileSpawnCheckpoint {
        ProjectileSpawnCheckpoint {
            next_entity_raw: self.next_entity_raw,
            active: self.active.clone(),
        }
    }

    pub(crate) fn restore_spawn_checkpoint(&mut self, checkpoint: ProjectileSpawnCheckpoint) {
        self.next_entity_raw = checkpoint.next_entity_raw;
        self.active = checkpoint.active;
    }

    pub(crate) fn spawn_enemy(
        &mut self,
        session: &mut GameSession,
        request: EnemyProjectileSpawn,
    ) -> Result<(EntityId, Vec3, Tick), ProjectileError> {
        let EnemyProjectileSpawn {
            owner,
            target,
            definition,
            damage,
            origin,
            direction,
            tick,
            visual_asset,
        } = request;
        if self.active.len() >= MAX_PROJECTILE_ENTITIES {
            return Err(ProjectileError::EntityLimit {
                limit: MAX_PROJECTILE_ENTITIES,
            });
        }
        let entity = self.allocate_entity(session)?;
        let radius = definition.radius;
        let definition_entity = EntityDefinition::new(entity, ENEMY_PROJECTILE_ENTITY_NAME)
            .with_full_transform(EntityTransform::at(origin))
            .with_bounds(
                Vec3::new(-radius, -radius, -radius),
                Vec3::new(radius, radius, radius),
            )
            .with_collision(true, false)
            .with_renderable(visual_asset, true);
        let authoring = EntityAuthoringService;
        let entity_revision = session.entities.revision();
        authoring
            .admit(&mut session.entities, entity_revision, [definition_entity])
            .map_err(ProjectileError::EntityAuthoring)?;
        let body = RigidBodyComponent {
            gravity_scale: definition.gravity_scale,
            restitution: definition.restitution,
            continuous_collision: true,
            ..RigidBodyComponent::dynamic(RigidBodyShape::Sphere { radius }, definition.mass)
        };
        let component_revision = session
            .entities
            .component_revision::<RigidBodyComponent>(entity)
            .expect("rigid-body component is registered by mechanics registry");
        authoring
            .attach_component(&mut session.entities, component_revision, entity, body)
            .map_err(ProjectileError::EntityAuthoring)?;
        let impulse = normalize(direction) * definition.impulse;
        let expires_at = tick.advance(rusty_engine::core_time::TickDelta::new(
            definition.lifetime_ticks,
        ));
        self.active.insert(
            entity,
            ActiveProjectile {
                owner,
                target: Some(target),
                definition,
                damage,
                expires_at,
                pending_impulse: impulse,
            },
        );
        Ok((entity, impulse, expires_at))
    }

    pub(crate) fn step(
        &mut self,
        session: &mut GameSession,
        scene: &VoxelCollisionScene,
        tick: Tick,
        step_seconds: f32,
    ) -> Result<ProjectilePhaseReceipt, ProjectileError> {
        if self.active.is_empty() {
            return Ok(ProjectilePhaseReceipt {
                facts: Vec::new(),
                combat: Vec::new(),
                events: Vec::new(),
                physics: None,
            });
        }
        let actions = self
            .active
            .iter()
            .map(|(entity, projectile)| {
                RigidBodyAction::impulse(*entity, projectile.pending_impulse)
            })
            .collect();
        let mut candidate_session = session.clone();
        let physics = self
            .rigid_bodies
            .step(
                &mut candidate_session.entities,
                scene,
                RigidBodyStepRequest {
                    step_seconds,
                    steps: 1,
                    gravity: Vec3::new(0.0, -9.81, 0.0),
                    actions,
                },
            )
            .map_err(ProjectileError::RigidBody)?;
        let positions: BTreeMap<_, _> = physics
            .facts
            .iter()
            .map(|fact| (fact.entity, fact.transform_after.translation))
            .collect();
        let static_contacts: BTreeMap<_, _> = physics
            .contacts
            .iter()
            .filter(|contact| contact.second.is_none())
            .map(|contact| (contact.first, *contact))
            .collect();
        let mut facts = Vec::new();
        let mut combat = Vec::new();
        let mut events = Vec::new();
        let mut remove = Vec::new();
        for (entity, projectile) in &self.active {
            let Some(position) = positions.get(entity).copied() else {
                continue;
            };
            let target = nearest_target(
                &candidate_session,
                projectile.owner,
                projectile.target,
                position,
                projectile.definition.radius,
            );
            let expired = tick.raw() >= projectile.expires_at.raw();
            if target.is_some() || static_contacts.contains_key(entity) {
                if let Some(target) = target {
                    let damage = DamageService::apply(
                        &mut candidate_session,
                        DamageCommand {
                            source: DamageSource::EnemyAttack {
                                attacker: projectile.owner,
                            },
                            target,
                            amount: projectile.damage,
                        },
                    )
                    .map_err(ProjectileError::Vitality)?;
                    combat.extend(damage.facts.iter().cloned().map(CombatFact::Vitality));
                    combat.extend(
                        damage
                            .enemy_drops
                            .iter()
                            .cloned()
                            .map(CombatFact::EnemyDrop),
                    );
                    combat.extend(
                        damage
                            .inventory
                            .iter()
                            .flat_map(|receipt| receipt.facts.iter().cloned())
                            .map(CombatFact::Inventory),
                    );
                    if let Some(event) = damage.event {
                        if let GameEvent::EnemyDefeated { enemy, .. } = event {
                            combat.push(CombatFact::EnemyDefeated {
                                attacker: projectile.owner,
                                enemy,
                            });
                        }
                        events.push(event);
                    }
                    facts.push(ProjectileFact::Impacted {
                        entity: *entity,
                        owner: projectile.owner,
                        target: Some(target),
                        position,
                        damage: projectile.damage,
                    });
                } else {
                    facts.push(ProjectileFact::Impacted {
                        entity: *entity,
                        owner: projectile.owner,
                        target: None,
                        position,
                        damage: 0,
                    });
                }
                remove.push(*entity);
            } else if expired {
                facts.push(ProjectileFact::Expired {
                    entity: *entity,
                    owner: projectile.owner,
                    position,
                });
                remove.push(*entity);
            }
        }
        for entity in &remove {
            let revision = candidate_session.entities.revision();
            EntityAuthoringService
                .destroy(&mut candidate_session.entities, revision, *entity)
                .map_err(ProjectileError::EntityAuthoring)?;
        }
        *session = candidate_session;
        for projectile in self.active.values_mut() {
            projectile.pending_impulse = Vec3::ZERO;
        }
        for entity in remove {
            self.active.remove(&entity);
        }
        Ok(ProjectilePhaseReceipt {
            facts,
            combat,
            events,
            physics: Some(physics),
        })
    }

    pub(crate) fn strip_from(&self, session: &mut GameSession) -> Vec<EntityId> {
        let mut stripped = Vec::new();
        for entity in self.active.keys().copied().collect::<Vec<_>>() {
            if session.entities.is_alive(entity) {
                let revision = session.entities.revision();
                let _ = EntityAuthoringService.destroy(&mut session.entities, revision, entity);
                stripped.push(entity);
            }
        }
        stripped
    }

    fn allocate_entity(&mut self, session: &GameSession) -> Result<EntityId, ProjectileError> {
        let mut raw = self.next_entity_raw.max(1);
        loop {
            let entity = EntityId::new(raw);
            raw = raw.checked_add(1).ok_or(ProjectileError::EntityLimit {
                limit: MAX_PROJECTILE_ENTITIES,
            })?;
            if !session.entities.contains(entity) {
                self.next_entity_raw = raw;
                return Ok(entity);
            }
        }
    }
}

fn nearest_target(
    session: &GameSession,
    owner: EntityId,
    explicit_target: Option<EntityId>,
    position: Vec3,
    radius: f32,
) -> Option<EntityId> {
    if let Some(target) = explicit_target {
        let health = session.health(target)?;
        if health.current == 0 {
            return None;
        }
        let transform = session.entities.transform(target)?;
        let delta = position - transform.translation;
        return (delta.x.abs() <= health.config.hitbox_half_extents.x + radius
            && delta.y.abs() <= health.config.hitbox_half_extents.y + radius
            && delta.z.abs() <= health.config.hitbox_half_extents.z + radius)
            .then_some(target);
    }
    session
        .health
        .keys()
        .copied()
        .filter(|entity| *entity != owner && session.is_player_attack_target(*entity))
        .filter_map(|entity| {
            let health = session.health(entity)?;
            let transform = session.entities.transform(entity)?;
            let delta = position - transform.translation;
            (delta.x.abs() <= health.config.hitbox_half_extents.x + radius
                && delta.y.abs() <= health.config.hitbox_half_extents.y + radius
                && delta.z.abs() <= health.config.hitbox_half_extents.z + radius)
                .then_some((entity, delta.length_squared()))
        })
        .min_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.0.cmp(&right.0))
        })
        .map(|(entity, _)| entity)
}

fn normalize(direction: Vec3) -> Vec3 {
    let length = direction.length();
    if length > f32::EPSILON {
        direction * length.recip()
    } else {
        Vec3::new(0.0, 0.0, -1.0)
    }
}
