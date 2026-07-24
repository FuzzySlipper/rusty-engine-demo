use core_ids::EntityId;
use core_math::Vec3;
use core_time::{Tick, TickDelta};
use engine_spatial::VoxelCollisionScene;
use entity_state::{EntityCommand, EntityCommandBatch, EntityView};
use serde::{Deserialize, Serialize};

use crate::runtime::RuntimeError;
use crate::runtime_records::GameEvent;
use crate::session::GameSession;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyState {
    Alive,
    Defeated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnemyComponent {
    pub state: EnemyState,
}

pub const MAX_HEALTH: u32 = 1_000_000;
pub const MAX_WEAPON_DAMAGE: u32 = 1_000_000;
pub const MAX_WEAPON_AMMO: u32 = 1_000_000;
pub const MAX_WEAPON_RANGE: f32 = 100_000.0;
pub const MAX_WEAPON_COOLDOWN_TICKS: u64 = 100_000;
pub const MAX_COMBAT_HITBOX_HALF_EXTENT: f32 = 100_000.0;
pub const MAX_WEAPON_MUZZLE_OFFSET: f32 = 100_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HealthConfig {
    pub max: u32,
    pub hitbox_half_extents: Vec3,
}

impl HealthConfig {
    pub(crate) fn is_valid(self) -> bool {
        (1..=MAX_HEALTH).contains(&self.max)
            && vec3_is_finite(self.hitbox_half_extents)
            && self.hitbox_half_extents.x > 0.0
            && self.hitbox_half_extents.y > 0.0
            && self.hitbox_half_extents.z > 0.0
            && self.hitbox_half_extents.x <= MAX_COMBAT_HITBOX_HALF_EXTENT
            && self.hitbox_half_extents.y <= MAX_COMBAT_HITBOX_HALF_EXTENT
            && self.hitbox_half_extents.z <= MAX_COMBAT_HITBOX_HALF_EXTENT
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HealthComponent {
    pub config: HealthConfig,
    pub current: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponConfig {
    pub damage: u32,
    pub max_distance: f32,
    pub cooldown_ticks: u64,
    pub ammo_capacity: u32,
    pub muzzle_offset: Vec3,
}

impl WeaponConfig {
    pub(crate) fn is_valid(self) -> bool {
        (1..=MAX_WEAPON_DAMAGE).contains(&self.damage)
            && self.max_distance.is_finite()
            && self.max_distance > 0.0
            && self.max_distance <= MAX_WEAPON_RANGE
            && self.cooldown_ticks <= MAX_WEAPON_COOLDOWN_TICKS
            && (1..=MAX_WEAPON_AMMO).contains(&self.ammo_capacity)
            && vec3_is_finite(self.muzzle_offset)
            && self.muzzle_offset.x.abs() <= MAX_WEAPON_MUZZLE_OFFSET
            && self.muzzle_offset.y.abs() <= MAX_WEAPON_MUZZLE_OFFSET
            && self.muzzle_offset.z.abs() <= MAX_WEAPON_MUZZLE_OFFSET
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeaponState {
    pub ammo_remaining: u32,
    pub ready_at_tick: Tick,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponComponent {
    pub config: WeaponConfig,
    pub state: WeaponState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ResolvedAttackAction {
    Attack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatRejectionReason {
    Cooldown,
    NoAmmo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatMissReason {
    NoTarget,
    WorldBlocked,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CombatFact {
    AttackFired {
        attacker: EntityId,
        origin: Vec3,
        direction: Vec3,
        ammo_before: u32,
        ammo_after: u32,
        ready_at_tick: Tick,
    },
    AttackHit {
        attacker: EntityId,
        target: EntityId,
        distance: f32,
    },
    AttackMissed {
        attacker: EntityId,
        reason: CombatMissReason,
    },
    DamageApplied {
        attacker: EntityId,
        target: EntityId,
        amount: u32,
        before: u32,
        after: u32,
    },
    EnemyDefeated {
        attacker: EntityId,
        enemy: EntityId,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CombatReceipt {
    pub action: ResolvedAttackAction,
    pub facts: Vec<CombatFact>,
    pub events: Vec<GameEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnemyView {
    pub entity: EntityId,
    pub state: EnemyState,
    pub entity_view: EntityView,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HealthView {
    pub entity: EntityId,
    pub config: HealthConfig,
    pub current: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponView {
    pub entity: EntityId,
    pub config: WeaponConfig,
    pub state: WeaponState,
}

pub(crate) struct CombatService;

#[derive(Debug)]
pub(crate) struct CombatResolution {
    pub(crate) action: ResolvedAttackAction,
    pub(crate) facts: Vec<CombatFact>,
    pub(crate) event: Option<GameEvent>,
}

#[derive(Debug, Clone, Copy)]
struct CombatTargetHit {
    entity: EntityId,
    distance: f32,
}

impl CombatService {
    pub(crate) fn attack(
        session: &mut GameSession,
        scene: &VoxelCollisionScene,
        tick: Tick,
        attacker: EntityId,
        action: ResolvedAttackAction,
    ) -> Result<CombatResolution, RuntimeError> {
        if !session.entities.contains(attacker) {
            return Err(RuntimeError::UnknownActor { actor: attacker });
        }
        let Some(weapon) = session.weapons.get(&attacker).copied() else {
            return Err(RuntimeError::UnknownWeapon { entity: attacker });
        };
        if tick.raw() < weapon.state.ready_at_tick.raw() {
            return Err(RuntimeError::CombatRejected {
                entity: attacker,
                reason: CombatRejectionReason::Cooldown,
            });
        }
        if weapon.state.ammo_remaining == 0 {
            return Err(RuntimeError::CombatRejected {
                entity: attacker,
                reason: CombatRejectionReason::NoAmmo,
            });
        }
        let controller = session
            .player_controllers
            .get(&attacker)
            .expect("weapon admission requires a player controller");
        let transform = session
            .entities
            .view(attacker)
            .expect("weapon admission requires an entity")
            .transform
            .expect("player controller admission requires a transform")
            .translation;
        let direction = aim_direction(controller.state.yaw_degrees, controller.state.pitch_degrees);
        let origin =
            transform + local_aim_offset(weapon.config.muzzle_offset, controller.state.yaw_degrees);
        let target = nearest_combat_target(
            session,
            attacker,
            origin,
            direction,
            weapon.config.max_distance,
        );
        let world_blocker = scene
            .raycast(
                [origin.x as f64, origin.y as f64, origin.z as f64],
                [direction.x as f64, direction.y as f64, direction.z as f64],
                weapon.config.max_distance as f64,
            )
            .map(|hit| hit.distance as f32);
        let ammo_after = weapon.state.ammo_remaining - 1;
        let ready_at_tick = tick.advance(TickDelta::new(weapon.config.cooldown_ticks));
        let mut facts = vec![CombatFact::AttackFired {
            attacker,
            origin,
            direction,
            ammo_before: weapon.state.ammo_remaining,
            ammo_after,
            ready_at_tick,
        }];
        let mut event = None;

        match target {
            Some(hit) if world_blocker.is_none_or(|distance| hit.distance + 0.000_1 < distance) => {
                let health = session
                    .health
                    .get(&hit.entity)
                    .copied()
                    .expect("target selection requires health");
                let amount = weapon.config.damage.min(health.current);
                let after = health.current - amount;
                facts.push(CombatFact::AttackHit {
                    attacker,
                    target: hit.entity,
                    distance: hit.distance,
                });
                facts.push(CombatFact::DamageApplied {
                    attacker,
                    target: hit.entity,
                    amount,
                    before: health.current,
                    after,
                });
                if after == 0 {
                    event = Self::defeat_enemy(session, attacker, hit.entity)?;
                    facts.push(CombatFact::EnemyDefeated {
                        attacker,
                        enemy: hit.entity,
                    });
                } else {
                    session
                        .health
                        .get_mut(&hit.entity)
                        .expect("target health remains attached")
                        .current = after;
                }
            }
            Some(_) => facts.push(CombatFact::AttackMissed {
                attacker,
                reason: CombatMissReason::WorldBlocked,
            }),
            None => facts.push(CombatFact::AttackMissed {
                attacker,
                reason: CombatMissReason::NoTarget,
            }),
        }

        session
            .weapons
            .get_mut(&attacker)
            .expect("weapon validated above")
            .state = WeaponState {
            ammo_remaining: ammo_after,
            ready_at_tick,
        };
        Ok(CombatResolution {
            action,
            facts,
            event,
        })
    }

    pub(crate) fn defeat_enemy(
        session: &mut GameSession,
        actor: EntityId,
        enemy: EntityId,
    ) -> Result<Option<GameEvent>, RuntimeError> {
        if !session.entities.contains(actor) {
            return Err(RuntimeError::UnknownActor { actor });
        }
        let Some(component) = session.enemies.get(&enemy).copied() else {
            return Err(RuntimeError::UnknownEnemy { enemy });
        };
        if component.state == EnemyState::Defeated {
            return Ok(None);
        }

        let mut commands = vec![
            EntityCommand::SetCollisionEnabled {
                entity: enemy,
                enabled: false,
            },
            EntityCommand::SetVisible {
                entity: enemy,
                visible: false,
            },
        ];
        if session
            .entities
            .view(enemy)
            .expect("enemy entity validated during admission")
            .kinematic
            .is_some()
        {
            commands.push(EntityCommand::SetKinematicVelocity {
                entity: enemy,
                velocity: Vec3::ZERO,
            });
        }
        let receipt = session
            .entities
            .apply_batch(EntityCommandBatch::new(commands))
            .map_err(RuntimeError::EntityBatch)?;
        session
            .enemies
            .get_mut(&enemy)
            .expect("enemy validated above")
            .state = EnemyState::Defeated;
        if let Some(health) = session.health.get_mut(&enemy) {
            health.current = 0;
        }
        Ok(Some(GameEvent::EnemyDefeated {
            enemy,
            actor,
            entity_facts: receipt.facts,
        }))
    }
}

fn aim_direction(yaw_degrees: f32, pitch_degrees: f32) -> Vec3 {
    let yaw = yaw_degrees.to_radians();
    let pitch = pitch_degrees.to_radians();
    let horizontal = pitch.cos();
    Vec3::new(
        -yaw.sin() * horizontal,
        pitch.sin(),
        -yaw.cos() * horizontal,
    )
}

fn local_aim_offset(offset: Vec3, yaw_degrees: f32) -> Vec3 {
    let yaw = yaw_degrees.to_radians();
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    right * offset.x + Vec3::new(0.0, offset.y, 0.0) + forward * offset.z
}

fn nearest_combat_target(
    session: &GameSession,
    attacker: EntityId,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<CombatTargetHit> {
    let mut best = None;
    for (entity, enemy) in &session.enemies {
        if *entity == attacker || enemy.state != EnemyState::Alive {
            continue;
        }
        let Some(health) = session.health.get(entity) else {
            continue;
        };
        if health.current == 0 {
            continue;
        }
        let Ok(view) = session.entities.view(*entity) else {
            continue;
        };
        if !view.collision.is_some_and(|collision| collision.enabled) {
            continue;
        }
        let Some(transform) = view.transform else {
            continue;
        };
        let min = transform.translation - health.config.hitbox_half_extents;
        let max = transform.translation + health.config.hitbox_half_extents;
        let Some(distance) = ray_aabb_distance(origin, direction, min, max) else {
            continue;
        };
        if distance > max_distance {
            continue;
        }
        if best.is_none_or(|hit: CombatTargetHit| distance < hit.distance) {
            best = Some(CombatTargetHit {
                entity: *entity,
                distance,
            });
        }
    }
    best
}

// Keep the query local and stateless: health ownership and combat consequences
// remain with the surrounding service rather than a parallel query-side model.
fn ray_aabb_distance(origin: Vec3, direction: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let mut t_min = 0.0f32;
    let mut t_max = f32::INFINITY;
    for (axis_origin, axis_direction, lo, hi) in [
        (origin.x, direction.x, min.x.min(max.x), min.x.max(max.x)),
        (origin.y, direction.y, min.y.min(max.y), min.y.max(max.y)),
        (origin.z, direction.z, min.z.min(max.z), min.z.max(max.z)),
    ] {
        if axis_direction.abs() < f32::EPSILON {
            if axis_origin < lo || axis_origin > hi {
                return None;
            }
            continue;
        }
        let inverse = axis_direction.recip();
        let mut near = (lo - axis_origin) * inverse;
        let mut far = (hi - axis_origin) * inverse;
        if near > far {
            std::mem::swap(&mut near, &mut far);
        }
        t_min = t_min.max(near);
        t_max = t_max.min(far);
        if t_min > t_max {
            return None;
        }
    }
    (t_max >= 0.0).then_some(t_min.max(0.0))
}

fn vec3_is_finite(value: Vec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}
