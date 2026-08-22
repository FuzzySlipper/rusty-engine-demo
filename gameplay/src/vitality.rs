use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::entity_state::{EntityCommand, EntityCommandBatch};
use rusty_engine::gameplay_mechanics::{
    DamagePart, DamageRequest, DamageService as MechanicsDamageService, EffectRemovalRequest,
    EffectService, OperationId, SourceInstanceId, SourceInstanceIdentity, TrackMutationRequest,
    TrackService,
};
use rusty_engine::gameplay_standard::{
    CapabilityRequirementId, CapabilityRoleBinding, CapabilityRoleBindings, CapabilityRoleId,
    ExactInputBundle, StandardMechanicsReceipt, StandardOperation, StandardOperationContext,
    STANDARD_EFFECT_CAPABILITY,
};

use crate::combat::EnemyState;
use crate::enemy_drop::{EnemyDropFact, EnemyDropRejection, EnemyDropService};
use crate::enemy_program::{execute_enemy_defeat_program, EnemyDefeatOperation};
use crate::explosive_prop::{ExplosivePropFact, ExplosivePropState};
use crate::inventory::{
    apply_standard_stack, ArmorGrantMode, ArmorTransition, InventoryAction, InventoryReceipt,
    InventoryRejection, ItemDefinitionId, ItemKind,
};
use crate::runtime_records::GameEvent;
use crate::session::GameSession;

pub const MAX_DOOM_DAMAGE: u32 = 1_000_000;
pub const MAX_COMBAT_HITBOX_HALF_EXTENT: f32 = 100_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HealthConfig {
    pub max: u32,
    pub starting: u32,
    pub hitbox_half_extents: Vec3,
    pub max_armor: u32,
    pub armor_absorption_percent: u8,
}

impl HealthConfig {
    pub fn unarmored(max: u32, hitbox_half_extents: Vec3) -> Self {
        Self {
            max,
            starting: max,
            hitbox_half_extents,
            max_armor: 0,
            armor_absorption_percent: 0,
        }
    }

    pub(crate) fn is_valid(self, policy: crate::DoomVitalityPolicy) -> bool {
        (1..=policy.maximum_health).contains(&self.max)
            && (1..=self.max).contains(&self.starting)
            && vec3_is_finite(self.hitbox_half_extents)
            && self.hitbox_half_extents.x > 0.0
            && self.hitbox_half_extents.y > 0.0
            && self.hitbox_half_extents.z > 0.0
            && self.hitbox_half_extents.x <= MAX_COMBAT_HITBOX_HALF_EXTENT
            && self.hitbox_half_extents.y <= MAX_COMBAT_HITBOX_HALF_EXTENT
            && self.hitbox_half_extents.z <= MAX_COMBAT_HITBOX_HALF_EXTENT
            && self.max_armor <= policy.maximum_armor
            && match self.max_armor {
                0 => self.armor_absorption_percent == 0,
                _ => (1..=100).contains(&self.armor_absorption_percent),
            }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VitalityState {
    Alive,
    Dead,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HealthView {
    pub entity: EntityId,
    pub config: HealthConfig,
    pub current: u32,
    pub armor: u32,
    pub armor_item: Option<ItemDefinitionId>,
    pub state: VitalityState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DamageSource {
    Weapon {
        attacker: EntityId,
        weapon: ItemDefinitionId,
    },
    Hazard {
        hazard: EntityId,
    },
    EnemyAttack {
        attacker: EntityId,
    },
    Explosion {
        source: EntityId,
    },
    Direct {
        actor: EntityId,
    },
}

impl DamageSource {
    pub fn entity(&self) -> EntityId {
        match self {
            Self::Weapon { attacker, .. } => *attacker,
            Self::Hazard { hazard } => *hazard,
            Self::EnemyAttack { attacker } => *attacker,
            Self::Explosion { source } => *source,
            Self::Direct { actor } => *actor,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageDisposition {
    Applied,
    AlreadyDead,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VitalityFact {
    DamageApplied {
        source: DamageSource,
        target: EntityId,
        incoming: u32,
        armor_absorbed: u32,
        health_damage: u32,
        health_before: u32,
        health_after: u32,
        armor_before: u32,
        armor_after: u32,
    },
    Died {
        source: DamageSource,
        entity: EntityId,
    },
    EnemyDefeatProgramRecorded {
        enemy: EntityId,
        program_id: String,
    },
    ArmorGranted {
        entity: EntityId,
        item: ItemDefinitionId,
        amount: u32,
        before: u32,
        after: u32,
    },
    HealthRestored {
        entity: EntityId,
        item: ItemDefinitionId,
        amount: u32,
        before: u32,
        after: u32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DamageCommand {
    pub source: DamageSource,
    pub target: EntityId,
    pub amount: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VitalityReceipt {
    pub disposition: DamageDisposition,
    pub facts: Vec<VitalityFact>,
    pub enemy_drops: Vec<EnemyDropFact>,
    pub explosive_props: Vec<ExplosivePropFact>,
    pub inventory: Vec<InventoryReceipt>,
    pub event: Option<GameEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VitalityRejection {
    UnknownSource {
        source: EntityId,
    },
    UnknownVitality {
        entity: EntityId,
    },
    InvalidDamage {
        amount: u32,
    },
    PlayerDead {
        player: EntityId,
    },
    HealthFull {
        player: EntityId,
    },
    ArmorFull {
        player: EntityId,
    },
    ArmorUnsupported {
        player: EntityId,
    },
    ArmorItemConflict {
        player: EntityId,
        active: ItemDefinitionId,
        offered: ItemDefinitionId,
    },
    MissingItemDefinition {
        item: ItemDefinitionId,
    },
    IncompatibleItem {
        item: ItemDefinitionId,
    },
    ItemNotOwned {
        player: EntityId,
        item: ItemDefinitionId,
    },
    InventorySequenceOverflow {
        player: EntityId,
    },
    Inventory(InventoryRejection),
    EntityMutation(rusty_engine::entity_state::BatchRejection),
    EnemyDrop(EnemyDropRejection),
    Mechanics {
        reason: String,
    },
}

impl std::fmt::Display for VitalityRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for VitalityRejection {}

pub struct DamageService;

impl DamageService {
    pub fn apply(
        session: &mut GameSession,
        command: DamageCommand,
    ) -> Result<VitalityReceipt, VitalityRejection> {
        let source = command.source.entity();
        if !session.entities.contains(source) {
            return Err(VitalityRejection::UnknownSource { source });
        }
        if command.amount == 0 || command.amount > MAX_DOOM_DAMAGE {
            return Err(VitalityRejection::InvalidDamage {
                amount: command.amount,
            });
        }
        let Some(before) = session.health(command.target) else {
            return Err(VitalityRejection::UnknownVitality {
                entity: command.target,
            });
        };
        if before.state == VitalityState::Dead {
            return Ok(VitalityReceipt {
                disposition: DamageDisposition::AlreadyDead,
                facts: Vec::new(),
                enemy_drops: Vec::new(),
                explosive_props: Vec::new(),
                inventory: Vec::new(),
                event: None,
            });
        }

        let armor_absorption = before
            .armor_item
            .as_ref()
            .and_then(|item| session.item_definitions.get(item))
            .and_then(|definition| match definition.kind {
                ItemKind::Armor {
                    absorption_percent,
                    absorption_divisor,
                    ..
                } => Some((absorption_percent, absorption_divisor)),
                _ => None,
            });
        let armor_eligible = match armor_absorption {
            Some((_, Some(divisor))) => command.amount / u32::from(divisor),
            Some((Some(percent), None)) => {
                u32::try_from(u64::from(command.amount) * u64::from(percent) / 100)
                    .expect("bounded damage and percentage fit u32")
            }
            _ => u32::try_from(
                u64::from(command.amount) * u64::from(before.config.armor_absorption_percent) / 100,
            )
            .expect("bounded damage and percentage fit u32"),
        };
        let mut candidate = session.clone();
        let operation = operation_id("damage", source.raw(), command.target.raw())?;
        let _receipt = MechanicsDamageService::apply(
            &mut candidate.entities,
            &candidate.mechanics.catalog,
            DamageRequest {
                operation: operation.clone(),
                source: request_source(operation.clone(), "damage")?,
                actor: Some(source),
                target: command.target,
                target_track: crate::mechanics::vitality_track(
                    if candidate.explosive_props.contains_key(&command.target) {
                        crate::mechanics::VitalityPreset::DestructibleObject
                    } else {
                        crate::mechanics::VitalityPreset::ActionActor
                    },
                ),
                parts: [
                    (
                        armor_eligible,
                        crate::mechanics::armor_eligible_damage_kind(),
                    ),
                    (
                        command.amount - armor_eligible,
                        crate::mechanics::direct_damage_kind(),
                    ),
                ]
                .into_iter()
                .filter(|(amount, _)| *amount > 0)
                .map(|(amount, kind)| {
                    Ok(DamagePart {
                        amount: crate::mechanics::scalar(amount)
                            .map_err(|reason| VitalityRejection::Mechanics { reason })?,
                        kind,
                    })
                })
                .collect::<Result<Vec<_>, VitalityRejection>>()?,
                request_sources: Vec::new(),
                expected_tracks_revision: None,
            },
        )
        .map_err(mechanics_rejection)?;
        let after = candidate
            .health(command.target)
            .expect("mechanics mutation preserves admitted vitality");
        let armor_absorbed = before.armor - after.armor;
        let health_damage = before.current - after.current;
        let health_after = after.current;
        let armor_after = after.armor;
        let died = after.state == VitalityState::Dead;
        if before.armor > 0 && armor_after == 0 && before.armor_item.is_some() {
            EffectService::remove(
                &mut candidate.entities,
                &candidate.mechanics.catalog,
                EffectRemovalRequest {
                    operation,
                    entity: command.target,
                    instance: crate::mechanics::armor_effect_instance(),
                    expected_revision: None,
                },
            )
            .map_err(mechanics_rejection)?;
        }

        let mut event = None;
        let mut enemy_drops = Vec::new();
        let mut explosive_props = Vec::new();
        let mut recorded_enemy_defeat_program = None;
        if let Some(combat) = candidate.enemy_combat.get_mut(&command.target) {
            combat.state.pain_ticks_remaining = if died {
                0
            } else {
                combat.config.pain_duration_ticks
            };
        }
        if died {
            let mut commands = Vec::new();
            let view = candidate
                .entities
                .view(command.target)
                .expect("vitality admission requires an entity");
            if view.kinematic.is_some() {
                commands.push(EntityCommand::SetKinematicVelocity {
                    entity: command.target,
                    velocity: Vec3::ZERO,
                });
            }
            if candidate.enemies.contains_key(&command.target) {
                commands.push(EntityCommand::SetCollisionEnabled {
                    entity: command.target,
                    enabled: false,
                });
                if let Some(combat) = candidate.enemy_combat.get(&command.target) {
                    let program_id = combat.config.defeat_program.clone();
                    let program = candidate
                        .enemy_defeat_programs
                        .get(&program_id)
                        .expect("enemy defeat program was admitted")
                        .clone();
                    let mut recorded = false;
                    execute_enemy_defeat_program(&program, &mut |operation| match operation {
                        EnemyDefeatOperation::RecordEnemyDefeat => {
                            recorded = true;
                            Ok(())
                        }
                        EnemyDefeatOperation::ActivateBoundDrop => {
                            if let Some(fact) = EnemyDropService::stage_materialization(
                                &mut candidate,
                                command.target,
                                &mut commands,
                            )
                            .map_err(VitalityRejection::EnemyDrop)?
                            {
                                enemy_drops.push(fact);
                            }
                            Ok(())
                        }
                    })?;
                    if recorded {
                        recorded_enemy_defeat_program = Some(program_id);
                    }
                }
            }
            if let Some(prop) = candidate.explosive_props.get_mut(&command.target) {
                if prop.state == ExplosivePropState::Armed {
                    prop.state = ExplosivePropState::Exploded;
                    prop.pending = true;
                    commands.push(EntityCommand::SetCollisionEnabled {
                        entity: command.target,
                        enabled: false,
                    });
                    commands.push(EntityCommand::SetVisible {
                        entity: command.target,
                        visible: false,
                    });
                    explosive_props.push(ExplosivePropFact::Triggered {
                        prop: command.target,
                        source: command.source.clone(),
                    });
                }
            }
            let entity_facts = if commands.is_empty() {
                Vec::new()
            } else {
                candidate
                    .entities
                    .apply_batch(EntityCommandBatch::new(commands))
                    .map_err(VitalityRejection::EntityMutation)?
                    .facts
            };
            if let Some(enemy) = candidate.enemies.get_mut(&command.target) {
                enemy.state = EnemyState::Defeated;
                if let Some(combat) = candidate.enemy_combat.get_mut(&command.target) {
                    combat.state.posture = crate::EnemyCombatPosture::Dead;
                    combat.state.last_known_target_position = None;
                }
                event = Some(GameEvent::EnemyDefeated {
                    enemy: command.target,
                    actor: source,
                    entity_facts,
                });
            } else if candidate.player_controllers.contains_key(&command.target) {
                event = Some(GameEvent::PlayerDied {
                    player: command.target,
                    source,
                    entity_facts,
                });
            }
        }

        let mut facts = vec![VitalityFact::DamageApplied {
            source: command.source.clone(),
            target: command.target,
            incoming: command.amount,
            armor_absorbed,
            health_damage,
            health_before: before.current,
            health_after,
            armor_before: before.armor,
            armor_after,
        }];
        if died {
            facts.push(VitalityFact::Died {
                source: command.source,
                entity: command.target,
            });
        }
        if let Some(program_id) = recorded_enemy_defeat_program {
            facts.push(VitalityFact::EnemyDefeatProgramRecorded {
                enemy: command.target,
                program_id,
            });
        }
        *session = candidate;
        Ok(VitalityReceipt {
            disposition: DamageDisposition::Applied,
            facts,
            enemy_drops,
            explosive_props,
            inventory: Vec::new(),
            event,
        })
    }

    pub fn grant_armor(
        session: &mut GameSession,
        player: EntityId,
        item: ItemDefinitionId,
    ) -> Result<VitalityReceipt, VitalityRejection> {
        let Some(before) = session.health(player) else {
            return Err(VitalityRejection::UnknownVitality { entity: player });
        };
        if before.state == VitalityState::Dead {
            return Err(VitalityRejection::PlayerDead { player });
        }
        if before.config.max_armor == 0 {
            return Err(VitalityRejection::ArmorUnsupported { player });
        }
        let definition = session
            .item_definitions
            .get(&item)
            .ok_or_else(|| VitalityRejection::MissingItemDefinition { item: item.clone() })?;
        let ItemKind::Armor {
            protection,
            maximum_armor,
            absorption_percent: _,
            absorption_divisor: _,
            grant_mode,
            transition,
            consume_at_cap,
        } = definition.kind
        else {
            return Err(VitalityRejection::IncompatibleItem { item });
        };
        let maximum_armor = maximum_armor
            .unwrap_or(before.config.max_armor)
            .min(before.config.max_armor);
        let target_armor = match grant_mode {
            ArmorGrantMode::Add => before.armor.saturating_add(protection).min(maximum_armor),
            ArmorGrantMode::SetMinimum => before.armor.max(protection).min(maximum_armor),
        };
        if target_armor <= before.armor && !consume_at_cap {
            return Err(VitalityRejection::ArmorFull { player });
        }
        let effect_item = match (&before.armor_item, transition) {
            (Some(active), ArmorTransition::RejectDifferent) if active != &item => {
                return Err(VitalityRejection::ArmorItemConflict {
                    player,
                    active: active.clone(),
                    offered: item,
                });
            }
            (Some(active), ArmorTransition::Preserve) => active.clone(),
            _ => item.clone(),
        };
        let mut candidate = session.clone();
        let sequence = next_inventory_sequence(&candidate, player)?;
        let inventory = apply_standard_stack(
            &mut candidate,
            player,
            sequence,
            InventoryAction::Consume {
                item: item.clone(),
                quantity: 1,
            },
        )
        .map_err(|rejection| match rejection {
            InventoryRejection::QuantityUnderflow { .. } => VitalityRejection::ItemNotOwned {
                player,
                item: item.clone(),
            },
            other => VitalityRejection::Inventory(other),
        })?;
        let after =
            if target_armor > before.armor {
                let operation = operation_id("grant-armor", player.raw(), sequence)?;
                let track = TrackService::restore(
                    &mut candidate.entities,
                    &candidate.mechanics.catalog,
                    TrackMutationRequest {
                        operation: operation.clone(),
                        source: request_source(operation.clone(), "armor")?,
                        entity: player,
                        track: crate::mechanics::armor_track(),
                        amount: crate::mechanics::scalar(target_armor - before.armor)
                            .map_err(|reason| VitalityRejection::Mechanics { reason })?,
                        kind: rusty_engine::gameplay_mechanics::TrackAdjustmentKind::Restore,
                        expected_revision: None,
                    },
                )
                .map_err(mechanics_rejection)?;
                let binding = candidate
                    .mechanics
                    .armor
                    .get(&effect_item)
                    .cloned()
                    .ok_or_else(|| VitalityRejection::Mechanics {
                        reason: format!("missing admitted armor effect for {effect_item}"),
                    })?;
                let role = CapabilityRoleId::parse("armor-target").map_err(|error| {
                    VitalityRejection::Mechanics {
                        reason: error.to_string(),
                    }
                })?;
                let effect_operation = StandardOperation::ReplaceEffect {
                    role: role.clone(),
                    instance: crate::mechanics::armor_effect_instance(),
                    definition: binding.effect,
                    stacks: 1,
                };
                let capability = CapabilityRequirementId::parse(STANDARD_EFFECT_CAPABILITY)
                    .map_err(|error| VitalityRejection::Mechanics {
                        reason: error.to_string(),
                    })?;
                let bindings = CapabilityRoleBindings::admit(
                    &effect_operation.requirements(),
                    vec![
                        CapabilityRoleBinding::new(role, player, vec![capability]).map_err(
                            |error| VitalityRejection::Mechanics {
                                reason: error.to_string(),
                            },
                        )?,
                    ],
                )
                .map_err(|error| VitalityRejection::Mechanics {
                    reason: error.to_string(),
                })?;
                let context = StandardOperationContext::new(
                    operation.clone(),
                    request_source(operation, "armor-effect")?,
                )
                .map_err(|error| VitalityRejection::Mechanics {
                    reason: error.to_string(),
                })?;
                let plan = effect_operation
                    .plan(
                        &bindings,
                        &ExactInputBundle::empty(),
                        &candidate.entities,
                        &candidate.mechanics.catalog,
                        &context,
                    )
                    .map_err(|error| VitalityRejection::Mechanics {
                        reason: error.to_string(),
                    })?;
                plan.validate_source_state(&candidate.entities, &candidate.mechanics.catalog)
                    .map_err(|error| VitalityRejection::Mechanics {
                        reason: error.to_string(),
                    })?;
                let receipt = plan
                    .effect()
                    .apply_to_candidate(&mut candidate.entities, &candidate.mechanics.catalog)
                    .map_err(mechanics_rejection)?;
                if !matches!(receipt, StandardMechanicsReceipt::Effect(_)) {
                    return Err(VitalityRejection::Mechanics {
                        reason: "standard armor effect replacement returned a non-effect receipt"
                            .to_string(),
                    });
                }
                u32::try_from(track.after.get()).map_err(|_| VitalityRejection::Mechanics {
                    reason: "armor track exceeds product representation".to_string(),
                })?
            } else {
                before.armor
            };
        *session = candidate;
        Ok(VitalityReceipt {
            disposition: DamageDisposition::Applied,
            facts: vec![VitalityFact::ArmorGranted {
                entity: player,
                item: effect_item,
                amount: after - before.armor,
                before: before.armor,
                after,
            }],
            enemy_drops: Vec::new(),
            explosive_props: Vec::new(),
            inventory: vec![inventory],
            event: None,
        })
    }

    pub fn use_health_supply(
        session: &mut GameSession,
        player: EntityId,
        item: ItemDefinitionId,
    ) -> Result<VitalityReceipt, VitalityRejection> {
        let Some(before) = session.health(player) else {
            return Err(VitalityRejection::UnknownVitality { entity: player });
        };
        if before.state == VitalityState::Dead {
            return Err(VitalityRejection::PlayerDead { player });
        }
        let definition = session
            .item_definitions
            .get(&item)
            .ok_or_else(|| VitalityRejection::MissingItemDefinition { item: item.clone() })?;
        let ItemKind::HealthSupply {
            restore_health,
            maximum_health,
            consume_at_cap,
            ..
        } = definition.kind
        else {
            return Err(VitalityRejection::IncompatibleItem { item });
        };
        let maximum_health = maximum_health
            .unwrap_or(before.config.max)
            .min(before.config.max);
        if before.current >= maximum_health && !consume_at_cap {
            return Err(VitalityRejection::HealthFull { player });
        }
        let restored = restore_health.min(maximum_health.saturating_sub(before.current));
        let mut candidate = session.clone();
        let sequence = next_inventory_sequence(&candidate, player)?;
        let inventory = apply_standard_stack(
            &mut candidate,
            player,
            sequence,
            InventoryAction::Consume {
                item: item.clone(),
                quantity: 1,
            },
        )
        .map_err(VitalityRejection::Inventory)?;
        let after = if restored > 0 {
            let operation = operation_id("restore-health", player.raw(), sequence)?;
            let track = TrackService::restore(
                &mut candidate.entities,
                &candidate.mechanics.catalog,
                TrackMutationRequest {
                    operation: operation.clone(),
                    source: request_source(operation, "health")?,
                    entity: player,
                    track: crate::mechanics::health_track(),
                    amount: crate::mechanics::scalar(restored)
                        .map_err(|reason| VitalityRejection::Mechanics { reason })?,
                    kind: rusty_engine::gameplay_mechanics::TrackAdjustmentKind::Restore,
                    expected_revision: None,
                },
            )
            .map_err(mechanics_rejection)?;
            u32::try_from(track.after.get()).map_err(|_| VitalityRejection::Mechanics {
                reason: "health track exceeds product representation".to_string(),
            })?
        } else {
            before.current
        };
        *session = candidate;
        Ok(VitalityReceipt {
            disposition: DamageDisposition::Applied,
            facts: vec![VitalityFact::HealthRestored {
                entity: player,
                item,
                amount: after - before.current,
                before: before.current,
                after,
            }],
            enemy_drops: Vec::new(),
            explosive_props: Vec::new(),
            inventory: vec![inventory],
            event: None,
        })
    }

    pub fn is_dead(session: &GameSession, entity: EntityId) -> bool {
        session
            .health(entity)
            .is_some_and(|health| health.state == VitalityState::Dead)
    }
}

fn next_inventory_sequence(
    session: &GameSession,
    player: EntityId,
) -> Result<u64, VitalityRejection> {
    session
        .inventories
        .get(&player)
        .and_then(|inventory| inventory.last_applied_command_sequence)
        .map_or(Some(1), |sequence| sequence.checked_add(1))
        .ok_or(VitalityRejection::InventorySequenceOverflow { player })
}

fn operation_id(kind: &str, first: u64, second: u64) -> Result<OperationId, VitalityRejection> {
    OperationId::parse(format!("{kind}-{first}-{second}")).map_err(|error| {
        VitalityRejection::Mechanics {
            reason: error.to_string(),
        }
    })
}

fn request_source(
    operation: OperationId,
    instance: &str,
) -> Result<SourceInstanceIdentity, VitalityRejection> {
    Ok(SourceInstanceIdentity::Request {
        operation,
        instance: SourceInstanceId::parse(instance).map_err(|error| {
            VitalityRejection::Mechanics {
                reason: error.to_string(),
            }
        })?,
    })
}

fn mechanics_rejection(
    error: rusty_engine::gameplay_mechanics::MechanicsError,
) -> VitalityRejection {
    VitalityRejection::Mechanics {
        reason: error.to_string(),
    }
}

fn vec3_is_finite(value: Vec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}
