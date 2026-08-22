//! Doom-owned adaptation of the Engine gameplay-resolution lifecycle.
//!
//! Spatial targeting, weapon meaning, ammo, damage, facts, and errors stay in
//! Loading Bay. The Engine contributes only the bounded lifecycle, receipt,
//! evidence retention, and one transaction boundary.

use std::convert::Infallible;

use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::core_time::Tick;
use rusty_engine::engine_spatial::{
    SpatialOcclusionQuery, SpatialOcclusionService, VoxelCollisionScene,
};
use rusty_engine::gameplay_resolution::{
    CommitStatus, CorrelationId, PolicyResult, ResolutionId, ResolutionIdentity, ResolutionMode,
    ResolutionPlan, ResolutionPolicy, ResolutionRequest, ResolutionTraceSink,
    ResolutionTransaction, StandardResolver,
};

use crate::combat::{
    nearest_combat_target, CombatFact, CombatImpactKind, CombatMissReason, CombatResolution,
    ResolvedAttackAction,
};
use crate::gameplay_program::{DemoOperation, DemoPredicate, DemoProgram};
use crate::inventory::{
    apply_standard_stack, InventoryAction, InventoryRejection, ItemDefinitionId, WeaponAttackMode,
    WeaponDefinition,
};
use crate::runtime::RuntimeError;
use crate::runtime_records::GameEvent;
use crate::session::GameSession;
use crate::vitality::{DamageCommand, DamageService, DamageSource};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HitscanIntent {
    attacker: EntityId,
    action: ResolvedAttackAction,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum HitscanEvidence {
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

#[derive(Debug, Clone, PartialEq)]
struct HitscanFacts {
    context: HitscanContext,
    evidence: HitscanEvidence,
}

#[derive(Debug, Clone, PartialEq)]
struct HitscanContext {
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
}

#[derive(Debug, Clone, PartialEq)]
enum HitscanEffect {
    Fact(CombatFact),
    ConsumeAmmo {
        owner: EntityId,
        item: ItemDefinitionId,
        quantity: u32,
    },
    Damage {
        attacker: EntityId,
        weapon: ItemDefinitionId,
        target: EntityId,
        amount: u32,
    },
    SetReady {
        owner: EntityId,
        weapon: ItemDefinitionId,
        tick: Tick,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HitscanSemanticEvent {
    Fired,
    Hit { target: EntityId },
    Missed { reason: CombatMissReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HitscanTraceDetail {
    Evidence { outcome: &'static str },
    Operation { name: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HitscanRejection {
    MissingEvidence,
    ConflictingEvidence,
    IntentMismatch,
    UnsupportedOperation,
}

struct HitscanPolicy {
    context: HitscanContext,
    program: DemoProgram,
}

impl ResolutionPolicy for HitscanPolicy {
    type RawIntent = HitscanIntent;
    type Intent = HitscanIntent;
    type Facts = HitscanFacts;
    type Predicate = DemoPredicate;
    type Operation = DemoOperation;
    type Effect = HitscanEffect;
    type Event = HitscanSemanticEvent;
    type Evidence = HitscanEvidence;
    type Interceptor = Infallible;
    type TraceDetail = HitscanTraceDetail;
    type Rejection = HitscanRejection;
    type Fault = Infallible;
    type Suspension = Infallible;

    fn admit(
        &mut self,
        intent: &Self::RawIntent,
        _evidence: &[Self::Evidence],
        _trace: &mut dyn ResolutionTraceSink<Self::TraceDetail>,
    ) -> PolicyResult<Self::Intent, Self::Rejection, Self::Fault, Self::Suspension> {
        if intent.attacker != self.context.attacker || intent.action != self.context.action {
            return Err(rusty_engine::gameplay_resolution::PolicyFailure::Rejected(
                HitscanRejection::IntentMismatch,
            ));
        }
        Ok(intent.clone())
    }

    fn gather(
        &mut self,
        _intent: &Self::Intent,
        evidence: &[Self::Evidence],
        trace: &mut dyn ResolutionTraceSink<Self::TraceDetail>,
    ) -> PolicyResult<Self::Facts, Self::Rejection, Self::Fault, Self::Suspension> {
        let [evidence] = evidence else {
            return Err(if evidence.is_empty() {
                rusty_engine::gameplay_resolution::PolicyFailure::Rejected(
                    HitscanRejection::MissingEvidence,
                )
            } else {
                rusty_engine::gameplay_resolution::PolicyFailure::Rejected(
                    HitscanRejection::ConflictingEvidence,
                )
            });
        };
        let outcome = match evidence {
            HitscanEvidence::Hit { .. } => "hit",
            HitscanEvidence::WorldBlocked { .. } => "world-blocked",
            HitscanEvidence::NoTarget => "no-target",
        };
        trace.record(HitscanTraceDetail::Evidence { outcome });
        Ok(HitscanFacts {
            context: self.context.clone(),
            evidence: evidence.clone(),
        })
    }

    fn check(
        &mut self,
        _intent: &Self::Intent,
        _facts: &Self::Facts,
        _evidence: &[Self::Evidence],
        _trace: &mut dyn ResolutionTraceSink<Self::TraceDetail>,
    ) -> PolicyResult<(), Self::Rejection, Self::Fault, Self::Suspension> {
        Ok(())
    }

    fn plan(
        &mut self,
        _intent: &Self::Intent,
        _facts: &Self::Facts,
        _evidence: &[Self::Evidence],
        _trace: &mut dyn ResolutionTraceSink<Self::TraceDetail>,
    ) -> PolicyResult<
        rusty_engine::gameplay_resolution::PolicyProgram<Self>,
        Self::Rejection,
        Self::Fault,
        Self::Suspension,
    > {
        Ok(self.program.clone())
    }

    fn evaluate_predicate(
        &mut self,
        predicate: &Self::Predicate,
        _intent: &Self::Intent,
        facts: &Self::Facts,
        _evidence: &[Self::Evidence],
        _trace: &mut dyn ResolutionTraceSink<Self::TraceDetail>,
    ) -> PolicyResult<bool, Self::Rejection, Self::Fault, Self::Suspension> {
        match predicate {
            DemoPredicate::ImpactIsHit => Ok(matches!(facts.evidence, HitscanEvidence::Hit { .. })),
        }
    }

    fn plan_operation(
        &mut self,
        operation: &Self::Operation,
        _intent: &Self::Intent,
        facts: &Self::Facts,
        _evidence: &[Self::Evidence],
        trace: &mut dyn ResolutionTraceSink<Self::TraceDetail>,
    ) -> PolicyResult<
        ResolutionPlan<Self::Effect, Self::Event, Self::RawIntent, Self::Evidence>,
        Self::Rejection,
        Self::Fault,
        Self::Suspension,
    > {
        let context = &facts.context;
        let mut plan = ResolutionPlan::new();
        let operation_name = match operation {
            DemoOperation::RecordFired => "record-fired",
            DemoOperation::ConsumeAmmo => "consume-ammo",
            DemoOperation::ApplyHit => "apply-hit",
            DemoOperation::ApplyMiss => "apply-miss",
            DemoOperation::ApplySpreadImpacts => "apply-spread-impacts",
            DemoOperation::SetCooldown => "set-cooldown",
            DemoOperation::UseHealthSupply => "use-health-supply",
        };
        trace.record(HitscanTraceDetail::Operation {
            name: operation_name,
        });
        match operation {
            DemoOperation::RecordFired => {
                plan.push_event(HitscanSemanticEvent::Fired);
                plan.push_effect(HitscanEffect::Fact(CombatFact::AttackFired {
                    attacker: context.attacker,
                    weapon: context.weapon_item.clone(),
                    presentation: context.weapon.presentation.clone(),
                    attack_mode: context.weapon.attack_mode,
                    ammunition: context.weapon.ammunition.clone(),
                    origin: context.origin,
                    direction: context.direction,
                    ray_count: 1,
                    spread_seed: context.spread_seed,
                    ammo_before: context.ammo_before,
                    ammo_after: context.ammo_after,
                    ready_at_tick: context.ready_at_tick,
                }));
            }
            DemoOperation::ConsumeAmmo if context.weapon.ammunition_cost > 0 => {
                plan.push_effect(HitscanEffect::ConsumeAmmo {
                    owner: context.attacker,
                    item: context.weapon.ammunition.clone(),
                    quantity: context.weapon.ammunition_cost,
                });
            }
            DemoOperation::ConsumeAmmo => {}
            DemoOperation::ApplyHit => {
                let HitscanEvidence::Hit {
                    target,
                    distance,
                    damage,
                } = facts.evidence
                else {
                    unreachable!("hit operation is guarded by the impact predicate")
                };
                plan.push_event(HitscanSemanticEvent::Hit { target });
                plan.push_effect(HitscanEffect::Fact(CombatFact::AttackHit {
                    attacker: context.attacker,
                    target,
                    ray_index: 0,
                    direction: context.direction,
                    distance,
                    damage,
                }));
                plan.push_effect(HitscanEffect::Fact(CombatFact::ImpactResolved {
                    attacker: context.attacker,
                    target: Some(target),
                    kind: CombatImpactKind::Blood,
                    position: context.origin + context.direction * distance,
                    direction: context.direction,
                }));
                plan.push_effect(HitscanEffect::Damage {
                    attacker: context.attacker,
                    weapon: context.weapon_item.clone(),
                    target,
                    amount: damage,
                });
            }
            DemoOperation::ApplyMiss => match facts.evidence {
                HitscanEvidence::WorldBlocked { distance } => {
                    plan.push_event(HitscanSemanticEvent::Missed {
                        reason: CombatMissReason::WorldBlocked,
                    });
                    plan.push_effect(HitscanEffect::Fact(CombatFact::AttackMissed {
                        attacker: context.attacker,
                        ray_index: 0,
                        direction: context.direction,
                        reason: CombatMissReason::WorldBlocked,
                    }));
                    plan.push_effect(HitscanEffect::Fact(CombatFact::ImpactResolved {
                        attacker: context.attacker,
                        target: None,
                        kind: CombatImpactKind::BulletPuff,
                        position: context.origin + context.direction * distance,
                        direction: context.direction,
                    }));
                }
                HitscanEvidence::NoTarget => {
                    plan.push_event(HitscanSemanticEvent::Missed {
                        reason: CombatMissReason::NoTarget,
                    });
                    plan.push_effect(HitscanEffect::Fact(CombatFact::AttackMissed {
                        attacker: context.attacker,
                        ray_index: 0,
                        direction: context.direction,
                        reason: CombatMissReason::NoTarget,
                    }));
                }
                HitscanEvidence::Hit { .. } => {
                    unreachable!("miss operation is selected only for non-hit evidence")
                }
            },
            DemoOperation::SetCooldown => plan.push_effect(HitscanEffect::SetReady {
                owner: context.attacker,
                weapon: context.weapon_item.clone(),
                tick: context.ready_at_tick,
            }),
            DemoOperation::ApplySpreadImpacts | DemoOperation::UseHealthSupply => {
                return Err(rusty_engine::gameplay_resolution::PolicyFailure::Rejected(
                    HitscanRejection::UnsupportedOperation,
                ));
            }
        }
        Ok(plan)
    }
}

struct HitscanTransaction<'a> {
    session: &'a mut GameSession,
    staged: Vec<HitscanEffect>,
    facts: Vec<CombatFact>,
    events: Vec<GameEvent>,
}

impl ResolutionTransaction for HitscanTransaction<'_> {
    type Effect = HitscanEffect;
    type Error = RuntimeError;

    fn stage(&mut self, effect: &Self::Effect) -> Result<(), Self::Error> {
        self.staged.push(effect.clone());
        Ok(())
    }

    fn commit(&mut self) -> Result<(), Self::Error> {
        let mut candidate = self.session.clone();
        let mut facts = Vec::new();
        let mut events = Vec::new();
        for effect in &self.staged {
            match effect {
                HitscanEffect::Fact(fact) => facts.push(fact.clone()),
                HitscanEffect::ConsumeAmmo {
                    owner,
                    item,
                    quantity,
                } => {
                    let sequence = candidate
                        .inventories
                        .get(owner)
                        .and_then(|inventory| inventory.last_applied_command_sequence)
                        .map_or(Some(1), |sequence| sequence.checked_add(1))
                        .ok_or(RuntimeError::InventorySequenceOverflow { owner: *owner })?;
                    let receipt = apply_standard_stack(
                        &mut candidate,
                        *owner,
                        sequence,
                        InventoryAction::Consume {
                            item: item.clone(),
                            quantity: *quantity,
                        },
                    )
                    .map_err(|rejection| match rejection {
                        InventoryRejection::QuantityUnderflow { .. } => {
                            RuntimeError::CombatRejected {
                                entity: *owner,
                                reason: crate::CombatRejectionReason::NoAmmo,
                            }
                        }
                        other => RuntimeError::Inventory(other),
                    })?;
                    facts.extend(receipt.facts.into_iter().map(CombatFact::Inventory));
                }
                HitscanEffect::Damage {
                    attacker,
                    weapon,
                    target,
                    amount,
                } => {
                    let damage = DamageService::apply(
                        &mut candidate,
                        DamageCommand {
                            source: DamageSource::Weapon {
                                attacker: *attacker,
                                weapon: weapon.clone(),
                            },
                            target: *target,
                            amount: *amount,
                        },
                    )
                    .map_err(RuntimeError::Vitality)?;
                    facts.extend(damage.facts.into_iter().map(CombatFact::Vitality));
                    facts.extend(
                        damage
                            .explosive_props
                            .into_iter()
                            .map(CombatFact::ExplosiveProp),
                    );
                    facts.extend(damage.enemy_drops.into_iter().map(CombatFact::EnemyDrop));
                    facts.extend(
                        damage
                            .inventory
                            .into_iter()
                            .flat_map(|receipt| receipt.facts)
                            .map(CombatFact::Inventory),
                    );
                    if let Some(event) = damage.event {
                        if matches!(event, GameEvent::EnemyDefeated { .. }) {
                            facts.push(CombatFact::EnemyDefeated {
                                attacker: *attacker,
                                enemy: *target,
                            });
                        }
                        events.push(event);
                    }
                }
                HitscanEffect::SetReady {
                    owner,
                    weapon,
                    tick,
                } => {
                    candidate
                        .inventories
                        .get_mut(owner)
                        .expect("hitscan admission retains inventory")
                        .weapon_ready_at
                        .insert(weapon.clone(), *tick);
                }
            }
        }
        *self.session = candidate;
        self.facts = facts;
        self.events = events;
        Ok(())
    }

    fn abort(&mut self) {
        self.staged.clear();
        self.facts.clear();
        self.events.clear();
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_single_hitscan(
    session: &mut GameSession,
    scene: &VoxelCollisionScene,
    tick: Tick,
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
    program: DemoProgram,
) -> Result<CombatResolution, RuntimeError> {
    debug_assert!(matches!(
        weapon.attack_mode,
        WeaponAttackMode::Hitscan | WeaponAttackMode::Automatic
    ));
    let target = nearest_combat_target(session, attacker, origin, direction, weapon.max_distance);
    let ignored_entities = target.map_or([attacker, attacker], |hit| [attacker, hit.entity]);
    let ignored_entities = if target.is_some() {
        &ignored_entities[..]
    } else {
        &ignored_entities[..1]
    };
    let world_blocker = SpatialOcclusionService
        .cast_ray(
            scene,
            &session.entities,
            SpatialOcclusionQuery {
                origin: [origin.x as f64, origin.y as f64, origin.z as f64],
                direction: [direction.x as f64, direction.y as f64, direction.z as f64],
                max_distance: weapon.max_distance as f64,
                ignored_entities,
            },
        )
        .map_err(RuntimeError::SpatialOcclusion)?
        .map(|hit| hit.distance() as f32);
    let evidence = match (target, world_blocker) {
        (Some(hit), blocker)
            if blocker.is_none_or(|distance| hit.distance + 0.000_1 < distance) =>
        {
            HitscanEvidence::Hit {
                target: hit.entity,
                distance: hit.distance,
                damage: super::combat::rolled_damage(
                    weapon.damage,
                    weapon.damage_rolls,
                    spread_seed,
                    0,
                ),
            }
        }
        (_, Some(distance)) => HitscanEvidence::WorldBlocked { distance },
        (None, None) => HitscanEvidence::NoTarget,
        (Some(_), None) => unreachable!("unblocked target is handled above"),
    };
    let context = HitscanContext {
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
    };
    let mut policy = HitscanPolicy {
        context: context.clone(),
        program,
    };
    let resolution_id = ResolutionId::new(tick.raw().saturating_add(1)).expect("tick plus one");
    let correlation_id = CorrelationId::new(spread_seed.max(1)).expect("non-zero shot seed");
    let request = ResolutionRequest::new(
        ResolutionIdentity::root(resolution_id, correlation_id),
        ResolutionMode::Apply,
        HitscanIntent { attacker, action },
        vec![evidence],
    );
    let mut transaction = HitscanTransaction {
        session,
        staged: Vec::new(),
        facts: Vec::new(),
        events: Vec::new(),
    };
    let receipt = StandardResolver::default().resolve(&mut policy, &mut transaction, request);
    let semantic_events = receipt.events().to_vec();
    let commit = receipt.into_commit();
    match commit {
        CommitStatus::Applied => Ok(CombatResolution {
            action: context.action,
            facts: std::mem::take(&mut transaction.facts),
            events: std::mem::take(&mut transaction.events),
        }),
        CommitStatus::Failed(error) => Err(error),
        other => Err(RuntimeError::CombatResolutionFailed {
            reason: format!(
                "unexpected hitscan resolution outcome: {other:?}; events={semantic_events:?}"
            ),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile_gameplay_package;

    #[derive(Default)]
    struct RecordingTransaction {
        staged: Vec<HitscanEffect>,
    }

    impl ResolutionTransaction for RecordingTransaction {
        type Effect = HitscanEffect;
        type Error = Infallible;

        fn stage(&mut self, effect: &Self::Effect) -> Result<(), Self::Error> {
            self.staged.push(effect.clone());
            Ok(())
        }

        fn commit(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
        fn abort(&mut self) {
            self.staged.clear();
        }
    }

    fn resolve(program: DemoProgram) -> Vec<HitscanEffect> {
        let attacker = EntityId::new(1);
        let target = EntityId::new(2);
        let weapon_item = ItemDefinitionId::parse("weapon/test").expect("valid item id");
        let ammunition = ItemDefinitionId::parse("ammo/test").expect("valid item id");
        let weapon = WeaponDefinition {
            attack_mode: WeaponAttackMode::Hitscan,
            repeat_while_held: false,
            damage_rolls: 1,
            damage: 5,
            max_distance: 128.0,
            cooldown_ticks: 1,
            ammunition,
            ammunition_cost: 1,
            muzzle_offset: Vec3::ZERO,
            presentation: "test".into(),
            projectile: None,
        };
        let context = HitscanContext {
            attacker,
            action: ResolvedAttackAction::Attack,
            weapon_item,
            weapon,
            origin: Vec3::ZERO,
            direction: Vec3::new(0.0, 0.0, 1.0),
            spread_seed: 1,
            ammo_before: 2,
            ammo_after: 1,
            ready_at_tick: Tick::new(1),
        };
        let mut policy = HitscanPolicy { context, program };
        let mut transaction = RecordingTransaction::default();
        let request = ResolutionRequest::new(
            ResolutionIdentity::root(
                ResolutionId::new(1).unwrap(),
                CorrelationId::new(1).unwrap(),
            ),
            ResolutionMode::Apply,
            HitscanIntent {
                attacker,
                action: ResolvedAttackAction::Attack,
            },
            vec![HitscanEvidence::Hit {
                target,
                distance: 1.0,
                damage: 5,
            }],
        );
        let receipt = StandardResolver::default().resolve(&mut policy, &mut transaction, request);
        assert!(matches!(receipt.into_commit(), CommitStatus::Applied));
        transaction.staged
    }

    #[test]
    fn authored_programs_execute_with_distinct_inventory_effects() {
        let package_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../data/gameplay/loading-bay-e1m1-core.package.json");
        let package = std::fs::read(package_path).expect("committed TypeScript package exists");
        let catalog = compile_gameplay_package(&package, "e1m1-core")
            .expect("TypeScript-authored program catalog compiles")
            .gameplay_programs;
        let ammunition = resolve(catalog.get("weapon/hitscan-ammunition").unwrap().clone());
        let unarmed = resolve(catalog.get("weapon/hitscan-unarmed").unwrap().clone());
        assert!(ammunition
            .iter()
            .any(|effect| matches!(effect, HitscanEffect::ConsumeAmmo { .. })));
        assert!(!unarmed
            .iter()
            .any(|effect| matches!(effect, HitscanEffect::ConsumeAmmo { .. })));
        assert!(unarmed
            .iter()
            .any(|effect| matches!(effect, HitscanEffect::Damage { .. })));
    }
}
