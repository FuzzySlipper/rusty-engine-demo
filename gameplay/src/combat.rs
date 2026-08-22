use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::core_time::{Tick, TickDelta};
use rusty_engine::engine_spatial::{
    SpatialOcclusionQuery, SpatialOcclusionService, VoxelCollisionScene,
};
use rusty_engine::entity_state::EntityView;
use serde::{Deserialize, Serialize};

use crate::explosive_prop::ExplosivePropFact;
use crate::gameplay_program::{
    applied_outcome, execute_program, program_operation_labels, rejected_outcome, DemoOperation,
    DemoProgram,
};
use crate::inventory::{
    apply_standard_stack, InventoryAction, InventoryFact, InventoryRejection, ItemDefinitionId,
    ItemKind, WeaponAttackMode, WeaponDefinition,
};
use crate::runtime::RuntimeError;
use crate::runtime_records::GameEvent;
use crate::session::GameSession;
use crate::vitality::{DamageCommand, DamageService, DamageSource, VitalityFact};
use crate::EnemyDropFact;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyState {
    Alive,
    Defeated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnemyComponent {
    pub state: EnemyState,
}

pub const MAX_WEAPON_DAMAGE: u32 = 1_000_000;
pub const MAX_WEAPON_AMMO: u32 = 1_000_000;
pub const MAX_WEAPON_RANGE: f32 = 100_000.0;
pub const MAX_WEAPON_COOLDOWN_TICKS: u64 = 100_000;
pub const MAX_WEAPON_MUZZLE_OFFSET: f32 = 100_000.0;
pub const MAX_WEAPON_PELLETS: u8 = 32;
pub const MAX_WEAPON_SPREAD_DEGREES: f32 = 45.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeaponState {
    pub ready_at_tick: Tick,
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
    NoEquippedWeapon,
    AttackerDefeated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatMissReason {
    NoTarget,
    WorldBlocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatImpactKind {
    Blood,
    BulletPuff,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CombatFact {
    Inventory(InventoryFact),
    AttackFired {
        attacker: EntityId,
        weapon: ItemDefinitionId,
        presentation: String,
        attack_mode: crate::WeaponAttackMode,
        ammunition: ItemDefinitionId,
        origin: Vec3,
        direction: Vec3,
        ray_count: u8,
        spread_seed: u64,
        ammo_before: u32,
        ammo_after: u32,
        ready_at_tick: Tick,
    },
    AttackHit {
        attacker: EntityId,
        target: EntityId,
        ray_index: u8,
        direction: Vec3,
        distance: f32,
        damage: u32,
    },
    AttackMissed {
        attacker: EntityId,
        ray_index: u8,
        direction: Vec3,
        reason: CombatMissReason,
    },
    ImpactResolved {
        attacker: EntityId,
        target: Option<EntityId>,
        kind: CombatImpactKind,
        position: Vec3,
        direction: Vec3,
    },
    Vitality(VitalityFact),
    ExplosiveProp(ExplosivePropFact),
    EnemyDrop(EnemyDropFact),
    EnemyDefeated {
        attacker: EntityId,
        enemy: EntityId,
    },
    ProjectileImpacted {
        entity: EntityId,
        owner: EntityId,
        target: Option<EntityId>,
        position: Vec3,
        damage: u32,
    },
    ProjectileExpired {
        entity: EntityId,
        owner: EntityId,
        position: Vec3,
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

#[derive(Debug, Clone, PartialEq)]
pub struct WeaponView {
    pub owner: EntityId,
    pub item: ItemDefinitionId,
    pub definition: WeaponDefinition,
    pub state: WeaponState,
}

pub(crate) struct CombatService;

#[derive(Debug)]
pub(crate) struct CombatResolution {
    pub(crate) action: ResolvedAttackAction,
    pub(crate) facts: Vec<CombatFact>,
    pub(crate) events: Vec<GameEvent>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CombatTargetHit {
    pub(crate) entity: EntityId,
    pub(crate) distance: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct SpreadImpact {
    ray_index: u8,
    direction: Vec3,
    outcome: SpreadImpactOutcome,
}

#[derive(Debug, Clone, PartialEq)]
enum SpreadImpactOutcome {
    Hit {
        target: EntityId,
        distance: f32,
        damage: u32,
    },
    WorldBlocked {
        distance: f32,
    },
    NoTarget,
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
        if DamageService::is_dead(session, attacker) {
            return Err(RuntimeError::CombatRejected {
                entity: attacker,
                reason: CombatRejectionReason::AttackerDefeated,
            });
        }
        let Some(inventory) = session.inventories.get(&attacker) else {
            return Err(RuntimeError::CombatRejected {
                entity: attacker,
                reason: CombatRejectionReason::NoEquippedWeapon,
            });
        };
        let Some(weapon_item) = session.equipped_weapon(attacker) else {
            return Err(RuntimeError::CombatRejected {
                entity: attacker,
                reason: CombatRejectionReason::NoEquippedWeapon,
            });
        };
        let Some(definition) = session.item_definitions.get(&weapon_item) else {
            return Err(RuntimeError::UnknownWeapon {
                entity: attacker,
                item: weapon_item,
            });
        };
        let ItemKind::Weapon(weapon) = definition.kind.clone() else {
            return Err(RuntimeError::UnknownWeapon {
                entity: attacker,
                item: definition.id.clone(),
            });
        };
        if matches!(weapon.attack_mode, WeaponAttackMode::Projectile) {
            return Err(RuntimeError::CombatResolutionFailed {
                reason: format!(
                    "weapon `{weapon_item}` uses a retired player projectile attack mode"
                ),
            });
        }
        let program_id =
            definition
                .program
                .clone()
                .ok_or_else(|| RuntimeError::CombatResolutionFailed {
                    reason: format!("weapon `{weapon_item}` has no authored gameplay program"),
                })?;
        let program = session
            .gameplay_programs
            .get(&program_id)
            .cloned()
            .ok_or_else(|| RuntimeError::CombatResolutionFailed {
                reason: format!(
                    "weapon `{weapon_item}` references unavailable gameplay program `{program_id}`"
                ),
            })?;
        let outcome_program = program.clone();
        let ready_at_tick = inventory
            .weapon_ready_at
            .get(&weapon_item)
            .copied()
            .unwrap_or(Tick::ZERO);
        if tick.raw() < ready_at_tick.raw() {
            return Err(RuntimeError::CombatRejected {
                entity: attacker,
                reason: CombatRejectionReason::Cooldown,
            });
        }
        let ammo_before = session
            .inventory(attacker)
            .expect("inventory admission remains readable")
            .stacks
            .iter()
            .find(|stack| stack.item == weapon.ammunition)
            .map_or(0, |stack| stack.quantity);
        if ammo_before < weapon.ammunition_cost {
            return Err(RuntimeError::CombatRejected {
                entity: attacker,
                reason: CombatRejectionReason::NoAmmo,
            });
        }
        let controller = session
            .player_controllers
            .get(&attacker)
            .expect("weapon admission requires a player controller");
        let look = controller
            .look_receipt()
            .map_err(RuntimeError::FirstPersonLook)?;
        let transform = session
            .entities
            .view(attacker)
            .expect("weapon admission requires an entity")
            .transform
            .expect("player controller admission requires a transform")
            .translation;
        let direction = look.forward;
        let origin = transform
            + Vec3::new(0.0, controller.eye_offset_from_center, 0.0)
            + local_aim_offset(weapon.muzzle_offset, look.right, look.forward);
        let spread_seed = shot_seed(tick, attacker, &weapon_item);
        let directions = attack_directions(direction, look.right, weapon.attack_mode, spread_seed);
        let ammo_after = ammo_before - weapon.ammunition_cost;
        let ready_at_tick = tick.advance(TickDelta::new(weapon.cooldown_ticks));
        if matches!(
            weapon.attack_mode,
            WeaponAttackMode::Hitscan | WeaponAttackMode::Automatic
        ) {
            let result = crate::combat_resolution::resolve_single_hitscan(
                session,
                scene,
                tick,
                attacker,
                action,
                weapon_item,
                weapon,
                origin,
                direction,
                spread_seed,
                ammo_before,
                ammo_after,
                ready_at_tick,
                program,
            );
            return record_program_result(session, program_id, &outcome_program, result);
        }
        if matches!(weapon.attack_mode, WeaponAttackMode::Spread { .. }) {
            let impacts = precompute_spread_impacts(
                session,
                scene,
                attacker,
                origin,
                &weapon,
                spread_seed,
                directions,
            )?;
            let result = resolve_spread_program(
                session,
                attacker,
                action,
                weapon_item,
                weapon,
                origin,
                direction,
                spread_seed,
                ammo_before,
                ammo_after,
                ready_at_tick,
                program,
                impacts,
            );
            return record_program_result(session, program_id, &outcome_program, result);
        }
        Err(RuntimeError::CombatResolutionFailed {
            reason: format!("weapon `{weapon_item}` uses an unsupported legacy player attack mode"),
        })
    }

    pub(crate) fn defeat_enemy(
        session: &mut GameSession,
        actor: EntityId,
        enemy: EntityId,
    ) -> Result<Option<GameEvent>, RuntimeError> {
        let Some(component) = session.enemies.get(&enemy).copied() else {
            return Err(RuntimeError::UnknownEnemy { enemy });
        };
        if component.state == EnemyState::Defeated {
            return Ok(None);
        }
        let amount = session
            .health(enemy)
            .map_or(1, |health| health.current.max(1));
        DamageService::apply(
            session,
            DamageCommand {
                source: DamageSource::Direct { actor },
                target: enemy,
                amount,
            },
        )
        .map(|receipt| receipt.event)
        .map_err(RuntimeError::Vitality)
    }
}

fn record_program_result(
    session: &mut GameSession,
    program_id: String,
    program: &DemoProgram,
    result: Result<CombatResolution, RuntimeError>,
) -> Result<CombatResolution, RuntimeError> {
    match result {
        Ok(resolution) => {
            let planned = program_operation_labels(program);
            let has_fired = resolution
                .facts
                .iter()
                .any(|fact| matches!(fact, CombatFact::AttackFired { .. }));
            let has_ammo = resolution
                .facts
                .iter()
                .any(|fact| matches!(fact, CombatFact::Inventory(_)));
            let has_hit = resolution
                .facts
                .iter()
                .any(|fact| matches!(fact, CombatFact::AttackHit { .. }));
            let has_miss = resolution
                .facts
                .iter()
                .any(|fact| matches!(fact, CombatFact::AttackMissed { .. }));
            let executed_operations = planned
                .iter()
                .filter(|label| match label.as_str() {
                    "record-fired" => has_fired,
                    "consume-ammo" => has_ammo,
                    "apply-hit" => has_hit,
                    "apply-miss" => has_miss,
                    "apply-spread-impacts" => has_hit || has_miss,
                    "set-cooldown" => true,
                    _ => false,
                })
                .cloned()
                .collect();
            let effects = resolution
                .facts
                .iter()
                .map(combat_effect_label)
                .chain(resolution.events.iter().map(|_| "event"))
                .map(str::to_owned)
                .collect();
            session.record_gameplay_outcome(applied_outcome(
                program_id,
                program,
                executed_operations,
                effects,
            ));
            Ok(resolution)
        }
        Err(error) => {
            session.record_gameplay_outcome(rejected_outcome(
                program_id,
                program,
                error.to_string(),
            ));
            Err(error)
        }
    }
}

fn combat_effect_label(fact: &CombatFact) -> &'static str {
    match fact {
        CombatFact::Inventory(_) => "inventory",
        CombatFact::AttackFired { .. } => "attack-fired",
        CombatFact::AttackHit { .. } => "attack-hit",
        CombatFact::AttackMissed { .. } => "attack-missed",
        CombatFact::ImpactResolved { .. } => "impact-resolved",
        CombatFact::Vitality(_) => "vitality",
        CombatFact::ExplosiveProp(_) => "explosive-prop",
        CombatFact::EnemyDrop(_) => "enemy-drop",
        CombatFact::EnemyDefeated { .. } => "enemy-defeated",
        CombatFact::ProjectileImpacted { .. } => "projectile-impacted",
        CombatFact::ProjectileExpired { .. } => "projectile-expired",
    }
}

#[allow(clippy::too_many_arguments)]
fn precompute_spread_impacts(
    session: &GameSession,
    scene: &VoxelCollisionScene,
    attacker: EntityId,
    origin: Vec3,
    weapon: &WeaponDefinition,
    spread_seed: u64,
    directions: Vec<Vec3>,
) -> Result<Vec<SpreadImpact>, RuntimeError> {
    directions
        .into_iter()
        .enumerate()
        .map(|(index, direction)| {
            let ray_index = index as u8;
            let target =
                nearest_combat_target(session, attacker, origin, direction, weapon.max_distance);
            let ignored = target.map_or([attacker, attacker], |hit| [attacker, hit.entity]);
            let ignored = if target.is_some() {
                &ignored[..]
            } else {
                &ignored[..1]
            };
            let blocker = SpatialOcclusionService
                .cast_ray(
                    scene,
                    &session.entities,
                    SpatialOcclusionQuery {
                        origin: [origin.x as f64, origin.y as f64, origin.z as f64],
                        direction: [direction.x as f64, direction.y as f64, direction.z as f64],
                        max_distance: weapon.max_distance as f64,
                        ignored_entities: ignored,
                    },
                )
                .map_err(RuntimeError::SpatialOcclusion)?
                .map(|hit| hit.distance() as f32);
            let outcome = match (target, blocker) {
                (Some(hit), blocker)
                    if blocker.is_none_or(|distance| hit.distance + 0.000_1 < distance) =>
                {
                    SpreadImpactOutcome::Hit {
                        target: hit.entity,
                        distance: hit.distance,
                        damage: rolled_damage(
                            weapon.damage,
                            weapon.damage_rolls,
                            spread_seed,
                            ray_index,
                        ),
                    }
                }
                (_, Some(distance)) => SpreadImpactOutcome::WorldBlocked { distance },
                (None, None) => SpreadImpactOutcome::NoTarget,
                (Some(_), None) => SpreadImpactOutcome::NoTarget,
            };
            Ok(SpreadImpact {
                ray_index,
                direction,
                outcome,
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn resolve_spread_program(
    session: &mut GameSession,
    attacker: EntityId,
    action: ResolvedAttackAction,
    weapon_item: ItemDefinitionId,
    weapon: WeaponDefinition,
    origin: Vec3,
    direction: Vec3,
    spread_seed: u64,
    ammo_before: u32,
    ammo_after: u32,
    ready_at_tick: Tick,
    program: crate::gameplay_program::DemoProgram,
    impacts: Vec<SpreadImpact>,
) -> Result<CombatResolution, RuntimeError> {
    let mut candidate = session.clone();
    let mut facts = Vec::new();
    let mut events = Vec::new();
    execute_program(
        &program,
        &mut |_| {
            Err(RuntimeError::CombatResolutionFailed {
                reason: "spread program cannot evaluate impact predicate".to_owned(),
            })
        },
        &mut |operation| match operation {
            DemoOperation::RecordFired => {
                facts.push(CombatFact::AttackFired {
                    attacker,
                    weapon: weapon_item.clone(),
                    presentation: weapon.presentation.clone(),
                    attack_mode: weapon.attack_mode,
                    ammunition: weapon.ammunition.clone(),
                    origin,
                    direction,
                    ray_count: impacts.len() as u8,
                    spread_seed,
                    ammo_before,
                    ammo_after,
                    ready_at_tick,
                });
                Ok(())
            }
            DemoOperation::ConsumeAmmo if weapon.ammunition_cost > 0 => {
                let sequence = candidate
                    .inventories
                    .get(&attacker)
                    .and_then(|inventory| inventory.last_applied_command_sequence)
                    .map_or(Some(1), |sequence| sequence.checked_add(1))
                    .ok_or(RuntimeError::InventorySequenceOverflow { owner: attacker })?;
                let receipt = apply_standard_stack(
                    &mut candidate,
                    attacker,
                    sequence,
                    InventoryAction::Consume {
                        item: weapon.ammunition.clone(),
                        quantity: weapon.ammunition_cost,
                    },
                )
                .map_err(|rejection| match rejection {
                    InventoryRejection::QuantityUnderflow { .. } => RuntimeError::CombatRejected {
                        entity: attacker,
                        reason: CombatRejectionReason::NoAmmo,
                    },
                    other => RuntimeError::Inventory(other),
                })?;
                facts.extend(receipt.facts.into_iter().map(CombatFact::Inventory));
                Ok(())
            }
            DemoOperation::ConsumeAmmo => Ok(()),
            DemoOperation::ApplySpreadImpacts => apply_spread_impacts(
                &mut candidate,
                attacker,
                &weapon_item,
                origin,
                &impacts,
                &mut facts,
                &mut events,
            ),
            DemoOperation::SetCooldown => {
                candidate
                    .inventories
                    .get_mut(&attacker)
                    .expect("combat preflight retains inventory")
                    .weapon_ready_at
                    .insert(weapon_item.clone(), ready_at_tick);
                Ok(())
            }
            _ => Err(RuntimeError::CombatResolutionFailed {
                reason: format!("unsupported operation {operation:?} in spread context"),
            }),
        },
    )?;
    *session = candidate;
    Ok(CombatResolution {
        action,
        facts,
        events,
    })
}

fn apply_spread_impacts(
    session: &mut GameSession,
    attacker: EntityId,
    weapon: &ItemDefinitionId,
    origin: Vec3,
    impacts: &[SpreadImpact],
    facts: &mut Vec<CombatFact>,
    events: &mut Vec<GameEvent>,
) -> Result<(), RuntimeError> {
    for impact in impacts {
        match impact.outcome {
            SpreadImpactOutcome::Hit {
                target,
                distance,
                damage,
            } => {
                facts.push(CombatFact::AttackHit {
                    attacker,
                    target,
                    ray_index: impact.ray_index,
                    direction: impact.direction,
                    distance,
                    damage,
                });
                facts.push(CombatFact::ImpactResolved {
                    attacker,
                    target: Some(target),
                    kind: CombatImpactKind::Blood,
                    position: origin + impact.direction * distance,
                    direction: impact.direction,
                });
                let receipt = DamageService::apply(
                    session,
                    DamageCommand {
                        source: DamageSource::Weapon {
                            attacker,
                            weapon: weapon.clone(),
                        },
                        target,
                        amount: damage,
                    },
                )
                .map_err(RuntimeError::Vitality)?;
                facts.extend(receipt.facts.into_iter().map(CombatFact::Vitality));
                facts.extend(
                    receipt
                        .explosive_props
                        .into_iter()
                        .map(CombatFact::ExplosiveProp),
                );
                facts.extend(receipt.enemy_drops.into_iter().map(CombatFact::EnemyDrop));
                facts.extend(
                    receipt
                        .inventory
                        .into_iter()
                        .flat_map(|receipt| receipt.facts)
                        .map(CombatFact::Inventory),
                );
                if let Some(event) = receipt.event {
                    if matches!(event, GameEvent::EnemyDefeated { .. }) {
                        facts.push(CombatFact::EnemyDefeated {
                            attacker,
                            enemy: target,
                        });
                    }
                    events.push(event);
                }
            }
            SpreadImpactOutcome::WorldBlocked { distance } => {
                facts.push(CombatFact::AttackMissed {
                    attacker,
                    ray_index: impact.ray_index,
                    direction: impact.direction,
                    reason: CombatMissReason::WorldBlocked,
                });
                facts.push(CombatFact::ImpactResolved {
                    attacker,
                    target: None,
                    kind: CombatImpactKind::BulletPuff,
                    position: origin + impact.direction * distance,
                    direction: impact.direction,
                });
            }
            SpreadImpactOutcome::NoTarget => facts.push(CombatFact::AttackMissed {
                attacker,
                ray_index: impact.ray_index,
                direction: impact.direction,
                reason: CombatMissReason::NoTarget,
            }),
        }
    }
    Ok(())
}

fn attack_directions(
    forward: Vec3,
    right: Vec3,
    attack_mode: crate::WeaponAttackMode,
    spread_seed: u64,
) -> Vec<Vec3> {
    let crate::WeaponAttackMode::Spread {
        pellet_count,
        spread_degrees,
    } = attack_mode
    else {
        return vec![forward];
    };
    let up = right.cross(forward);
    let max_offset = spread_degrees.to_radians().tan();
    let rotation = seed_unit(spread_seed) * std::f32::consts::TAU;
    let mut directions = Vec::with_capacity(pellet_count as usize);
    directions.push(forward);
    for index in 1..pellet_count {
        let sample = index as f32 / (pellet_count - 1) as f32;
        let radius = sample.sqrt() * max_offset;
        let angle = rotation + index as f32 * 2.399_963_1;
        directions.push(normalize_direction(
            forward + right * (angle.cos() * radius) + up * (angle.sin() * radius),
        ));
    }
    directions
}

fn normalize_direction(direction: Vec3) -> Vec3 {
    let length = direction.length();
    if length > f32::EPSILON {
        direction * length.recip()
    } else {
        direction
    }
}

fn shot_seed(tick: Tick, attacker: EntityId, weapon: &ItemDefinitionId) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in tick
        .raw()
        .to_le_bytes()
        .into_iter()
        .chain(attacker.raw().to_le_bytes())
        .chain(weapon.as_str().bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn seed_unit(seed: u64) -> f32 {
    ((seed >> 40) as f32) / ((1u32 << 24) - 1) as f32
}

pub(crate) fn rolled_damage(base: u32, rolls: u8, seed: u64, ray_index: u8) -> u32 {
    let mut mixed = seed ^ (u64::from(ray_index) + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    mixed ^= mixed >> 30;
    mixed = mixed.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed ^= mixed >> 27;
    mixed = mixed.wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^= mixed >> 31;
    let multiplier = (mixed % u64::from(rolls)) as u32 + 1;
    base * multiplier
}

fn local_aim_offset(offset: Vec3, right: Vec3, forward: Vec3) -> Vec3 {
    right * offset.x + Vec3::new(0.0, offset.y, 0.0) + forward * offset.z
}

pub(crate) fn nearest_combat_target(
    session: &GameSession,
    attacker: EntityId,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<CombatTargetHit> {
    let mut best = None;
    for entity in session.health.keys().copied() {
        if entity == attacker || !session.is_player_attack_target(entity) {
            continue;
        }
        let Some(health) = session.health(entity) else {
            continue;
        };
        if health.current == 0 {
            continue;
        }
        let Ok(view) = session.entities.view(entity) else {
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
            best = Some(CombatTargetHit { entity, distance });
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

#[cfg(test)]
mod tests {
    use super::rolled_damage;

    #[test]
    fn bounded_damage_multiples_are_deterministic_per_shot_and_ray() {
        let first = (0..7)
            .map(|ray| rolled_damage(5, 3, 0x1234_5678_9abc_def0, ray))
            .collect::<Vec<_>>();
        let replay = (0..7)
            .map(|ray| rolled_damage(5, 3, 0x1234_5678_9abc_def0, ray))
            .collect::<Vec<_>>();
        assert_eq!(first, replay);
        assert!(first.iter().all(|damage| matches!(damage, 5 | 10 | 15)));
        assert!(first.windows(2).any(|pair| pair[0] != pair[1]));
        assert_eq!(rolled_damage(35, 1, 77, 0), 35);
    }
}
