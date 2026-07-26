use std::collections::BTreeMap;

use core_ids::EntityId;
use core_math::Vec3;
use core_time::{Tick, TickDelta};
use engine_spatial::VoxelCollisionScene;

use crate::combat::EnemyState;
use crate::navigation::NavigationPhaseReceipt;
use crate::runtime::RuntimeError;
use crate::runtime_records::GameEvent;
use crate::session::GameSession;
use crate::vitality::{DamageCommand, DamageService, DamageSource, VitalityFact, VitalityState};

pub const MAX_ENEMY_PERCEPTION_RANGE: f32 = 100_000.0;
pub const MAX_ENEMY_ATTACK_RANGE: f32 = 100_000.0;
pub const MAX_ENEMY_ATTACK_DAMAGE: u32 = 1_000_000;
pub const MAX_ENEMY_ATTACK_COOLDOWN_TICKS: u64 = 100_000;
pub const MAX_ENEMY_PRESENTATION_BYTES: usize = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyAttackKind {
    Melee,
    RangedHitscan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnemyPerceptionConfig {
    pub sight_range: f32,
    pub hearing_range: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnemyAttackConfig {
    pub kind: EnemyAttackKind,
    pub damage: u32,
    pub range: f32,
    pub cooldown_ticks: u64,
    pub origin_offset: Vec3,
    pub presentation: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnemyCombatConfig {
    pub perception: EnemyPerceptionConfig,
    pub attack: EnemyAttackConfig,
}

impl EnemyCombatConfig {
    pub(crate) fn is_valid(&self) -> bool {
        finite_positive_bounded(self.perception.sight_range, MAX_ENEMY_PERCEPTION_RANGE)
            && self.perception.hearing_range.is_finite()
            && (0.0..=MAX_ENEMY_PERCEPTION_RANGE).contains(&self.perception.hearing_range)
            && (1..=MAX_ENEMY_ATTACK_DAMAGE).contains(&self.attack.damage)
            && finite_positive_bounded(self.attack.range, MAX_ENEMY_ATTACK_RANGE)
            && self.attack.cooldown_ticks <= MAX_ENEMY_ATTACK_COOLDOWN_TICKS
            && vec3_is_finite(self.attack.origin_offset)
            && self.attack.origin_offset.x.abs() <= MAX_ENEMY_ATTACK_RANGE
            && self.attack.origin_offset.y.abs() <= MAX_ENEMY_ATTACK_RANGE
            && self.attack.origin_offset.z.abs() <= MAX_ENEMY_ATTACK_RANGE
            && !self.attack.presentation.is_empty()
            && self.attack.presentation.len() <= MAX_ENEMY_PRESENTATION_BYTES
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyCombatPosture {
    Sleeping,
    Alert,
    Pursuing,
    Attacking,
    Dead,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnemyCombatState {
    pub posture: EnemyCombatPosture,
    pub ready_at_tick: Tick,
    pub last_known_target_position: Option<Vec3>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnemyCombatComponent {
    pub config: EnemyCombatConfig,
    pub state: EnemyCombatState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnemyCombatView {
    pub entity: EntityId,
    pub config: EnemyCombatConfig,
    pub state: EnemyCombatState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyPerceptionCause {
    Sight,
    Hearing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyAttackMissReason {
    WorldBlocked,
    TargetOutOfRange,
    TargetDead,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EnemyCombatFact {
    Alerted {
        enemy: EntityId,
        target: EntityId,
        cause: EnemyPerceptionCause,
    },
    PostureChanged {
        enemy: EntityId,
        before: EnemyCombatPosture,
        after: EnemyCombatPosture,
    },
    AttackFired {
        enemy: EntityId,
        target: EntityId,
        kind: EnemyAttackKind,
        presentation: String,
        origin: Vec3,
        target_position: Vec3,
        distance: f32,
        ready_at_tick: Tick,
    },
    AttackHit {
        enemy: EntityId,
        target: EntityId,
        kind: EnemyAttackKind,
        damage: u32,
    },
    AttackMissed {
        enemy: EntityId,
        target: EntityId,
        kind: EnemyAttackKind,
        reason: EnemyAttackMissReason,
    },
    Vitality(VitalityFact),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnemyIntentPhaseReceipt {
    pub facts: Vec<EnemyCombatFact>,
    pub(crate) navigation_goals: BTreeMap<EntityId, Vec3>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnemyIntentAndMotionReceipt {
    pub facts: Vec<EnemyCombatFact>,
    pub navigation: NavigationPhaseReceipt,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnemyAttackPhaseReceipt {
    pub facts: Vec<EnemyCombatFact>,
    pub events: Vec<GameEvent>,
}

pub(crate) struct EnemyCombatService;

impl EnemyCombatService {
    pub(crate) fn perceive_and_plan(
        session: &mut GameSession,
        scene: &VoxelCollisionScene,
        player: EntityId,
    ) -> Result<EnemyIntentPhaseReceipt, RuntimeError> {
        let player_view = session
            .entities
            .view(player)
            .map_err(|_| RuntimeError::UnknownActor { actor: player })?;
        let player_position = player_view
            .transform
            .ok_or(RuntimeError::UnknownPlayerController { player })?
            .translation;
        let player_dead = session
            .health
            .get(&player)
            .is_some_and(|health| health.state == VitalityState::Dead);
        let enemies: Vec<EntityId> = session.enemy_combat.keys().copied().collect();
        let mut facts = Vec::new();
        let mut navigation_goals = BTreeMap::new();

        for enemy in enemies {
            let enemy_state = session
                .enemies
                .get(&enemy)
                .map(|enemy| enemy.state)
                .unwrap_or(EnemyState::Defeated);
            if enemy_state == EnemyState::Defeated {
                let component = session
                    .enemy_combat
                    .get_mut(&enemy)
                    .expect("enemy combat key remains present");
                transition_posture(enemy, component, EnemyCombatPosture::Dead, &mut facts);
                component.state.last_known_target_position = None;
                continue;
            }

            let enemy_position = session
                .entities
                .view(enemy)
                .expect("enemy combat admission requires an entity")
                .transform
                .expect("enemy combat admission requires a transform")
                .translation;
            let component = session
                .enemy_combat
                .get_mut(&enemy)
                .expect("enemy combat key remains present");

            if player_dead {
                transition_posture(enemy, component, EnemyCombatPosture::Alert, &mut facts);
                continue;
            }

            let delta = player_position - enemy_position;
            let distance = delta.length();
            let visible = distance <= component.config.perception.sight_range
                && line_of_sight(scene, enemy_position, player_position);
            let heard = distance <= component.config.perception.hearing_range;
            let perception = if visible {
                Some(EnemyPerceptionCause::Sight)
            } else if heard {
                Some(EnemyPerceptionCause::Hearing)
            } else {
                None
            };
            if perception.is_some() {
                component.state.last_known_target_position = Some(player_position);
            }

            if component.state.posture == EnemyCombatPosture::Sleeping {
                if let Some(cause) = perception {
                    facts.push(EnemyCombatFact::Alerted {
                        enemy,
                        target: player,
                        cause,
                    });
                    transition_posture(enemy, component, EnemyCombatPosture::Alert, &mut facts);
                }
                continue;
            }

            let can_attack = distance <= component.config.attack.range
                && line_of_sight(scene, enemy_position, player_position);
            if can_attack {
                transition_posture(enemy, component, EnemyCombatPosture::Attacking, &mut facts);
                continue;
            }

            if let Some(goal) = component.state.last_known_target_position {
                transition_posture(enemy, component, EnemyCombatPosture::Pursuing, &mut facts);
                navigation_goals.insert(enemy, goal);
            } else {
                transition_posture(enemy, component, EnemyCombatPosture::Alert, &mut facts);
            }
        }

        Ok(EnemyIntentPhaseReceipt {
            facts,
            navigation_goals,
        })
    }

    pub(crate) fn attack(
        session: &mut GameSession,
        scene: &VoxelCollisionScene,
        tick: Tick,
        player: EntityId,
    ) -> Result<EnemyAttackPhaseReceipt, RuntimeError> {
        let enemies: Vec<EntityId> = session.enemy_combat.keys().copied().collect();
        let mut facts = Vec::new();
        let mut events = Vec::new();

        for enemy in enemies {
            if DamageService::is_dead(session, player) {
                break;
            }
            let component = session
                .enemy_combat
                .get(&enemy)
                .expect("enemy combat key remains present")
                .clone();
            if component.state.posture != EnemyCombatPosture::Attacking
                || tick.raw() < component.state.ready_at_tick.raw()
            {
                continue;
            }
            let enemy_position = session
                .entities
                .view(enemy)
                .expect("enemy combat admission requires an entity")
                .transform
                .expect("enemy combat admission requires a transform")
                .translation;
            let player_position = session
                .entities
                .view(player)
                .map_err(|_| RuntimeError::UnknownActor { actor: player })?
                .transform
                .ok_or(RuntimeError::UnknownPlayerController { player })?
                .translation;
            let origin = enemy_position + component.config.attack.origin_offset;
            let distance = (player_position - origin).length();
            let kind = component.config.attack.kind;

            if session
                .health
                .get(&player)
                .is_some_and(|health| health.state == VitalityState::Dead)
            {
                facts.push(EnemyCombatFact::AttackMissed {
                    enemy,
                    target: player,
                    kind,
                    reason: EnemyAttackMissReason::TargetDead,
                });
                continue;
            }
            if distance > component.config.attack.range {
                facts.push(EnemyCombatFact::AttackMissed {
                    enemy,
                    target: player,
                    kind,
                    reason: EnemyAttackMissReason::TargetOutOfRange,
                });
                continue;
            }

            let ready_at_tick =
                tick.advance(TickDelta::new(component.config.attack.cooldown_ticks));
            session
                .enemy_combat
                .get_mut(&enemy)
                .expect("enemy combat key remains present")
                .state
                .ready_at_tick = ready_at_tick;
            facts.push(EnemyCombatFact::AttackFired {
                enemy,
                target: player,
                kind,
                presentation: component.config.attack.presentation.clone(),
                origin,
                target_position: player_position,
                distance,
                ready_at_tick,
            });

            if !line_of_sight(scene, origin, player_position) {
                facts.push(EnemyCombatFact::AttackMissed {
                    enemy,
                    target: player,
                    kind,
                    reason: EnemyAttackMissReason::WorldBlocked,
                });
                continue;
            }

            facts.push(EnemyCombatFact::AttackHit {
                enemy,
                target: player,
                kind,
                damage: component.config.attack.damage,
            });
            let damage = DamageService::apply(
                session,
                DamageCommand {
                    source: DamageSource::EnemyAttack { attacker: enemy },
                    target: player,
                    amount: component.config.attack.damage,
                },
            )
            .map_err(RuntimeError::Vitality)?;
            facts.extend(damage.facts.into_iter().map(EnemyCombatFact::Vitality));
            if let Some(event) = damage.event {
                events.push(event);
            }
        }

        Ok(EnemyAttackPhaseReceipt { facts, events })
    }
}

fn transition_posture(
    enemy: EntityId,
    component: &mut EnemyCombatComponent,
    after: EnemyCombatPosture,
    facts: &mut Vec<EnemyCombatFact>,
) {
    let before = component.state.posture;
    if before == after {
        return;
    }
    component.state.posture = after;
    facts.push(EnemyCombatFact::PostureChanged {
        enemy,
        before,
        after,
    });
}

fn line_of_sight(scene: &VoxelCollisionScene, origin: Vec3, target: Vec3) -> bool {
    let delta = target - origin;
    let distance = delta.length();
    if distance <= f32::EPSILON {
        return true;
    }
    let direction = delta * distance.recip();
    scene
        .raycast(
            [origin.x as f64, origin.y as f64, origin.z as f64],
            [direction.x as f64, direction.y as f64, direction.z as f64],
            distance as f64,
        )
        .is_none_or(|hit| hit.distance as f32 + 0.000_1 >= distance)
}

fn finite_positive_bounded(value: f32, maximum: f32) -> bool {
    value.is_finite() && value > 0.0 && value <= maximum
}

fn vec3_is_finite(value: Vec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}
