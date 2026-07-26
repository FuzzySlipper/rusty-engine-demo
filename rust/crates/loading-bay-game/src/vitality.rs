use core_ids::EntityId;
use core_math::Vec3;
use entity_state::{EntityCommand, EntityCommandBatch};

use crate::combat::EnemyState;
use crate::inventory::{
    InventoryAction, InventoryCommand, InventoryReceipt, InventoryRejection, InventoryService,
    ItemDefinitionId, ItemKind,
};
use crate::runtime_records::GameEvent;
use crate::session::GameSession;

pub const MAX_HEALTH: u32 = 1_000_000;
pub const MAX_ARMOR: u32 = 1_000_000;
pub const MAX_DAMAGE: u32 = 1_000_000;
pub const MAX_COMBAT_HITBOX_HALF_EXTENT: f32 = 100_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HealthConfig {
    pub max: u32,
    pub hitbox_half_extents: Vec3,
    pub max_armor: u32,
    pub armor_absorption_percent: u8,
}

impl HealthConfig {
    pub fn unarmored(max: u32, hitbox_half_extents: Vec3) -> Self {
        Self {
            max,
            hitbox_half_extents,
            max_armor: 0,
            armor_absorption_percent: 0,
        }
    }

    pub(crate) fn is_valid(self) -> bool {
        (1..=MAX_HEALTH).contains(&self.max)
            && vec3_is_finite(self.hitbox_half_extents)
            && self.hitbox_half_extents.x > 0.0
            && self.hitbox_half_extents.y > 0.0
            && self.hitbox_half_extents.z > 0.0
            && self.hitbox_half_extents.x <= MAX_COMBAT_HITBOX_HALF_EXTENT
            && self.hitbox_half_extents.y <= MAX_COMBAT_HITBOX_HALF_EXTENT
            && self.hitbox_half_extents.z <= MAX_COMBAT_HITBOX_HALF_EXTENT
            && self.max_armor <= MAX_ARMOR
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
pub struct HealthComponent {
    pub config: HealthConfig,
    pub current: u32,
    pub armor: u32,
    pub armor_item: Option<ItemDefinitionId>,
    pub state: VitalityState,
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
    Direct {
        actor: EntityId,
    },
}

impl DamageSource {
    pub fn entity(&self) -> EntityId {
        match self {
            Self::Weapon { attacker, .. } => *attacker,
            Self::Hazard { hazard } => *hazard,
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
    EntityMutation(entity_state::BatchRejection),
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
        if command.amount == 0 || command.amount > MAX_DAMAGE {
            return Err(VitalityRejection::InvalidDamage {
                amount: command.amount,
            });
        }
        let Some(before) = session.health.get(&command.target).cloned() else {
            return Err(VitalityRejection::UnknownVitality {
                entity: command.target,
            });
        };
        if before.state == VitalityState::Dead {
            return Ok(VitalityReceipt {
                disposition: DamageDisposition::AlreadyDead,
                facts: Vec::new(),
                inventory: Vec::new(),
                event: None,
            });
        }

        let maximum_absorption = u32::try_from(
            u64::from(command.amount) * u64::from(before.config.armor_absorption_percent) / 100,
        )
        .expect("bounded damage and percentage fit u32");
        let armor_absorbed = before.armor.min(maximum_absorption);
        let health_damage = before.current.min(command.amount - armor_absorbed);
        let health_after = before.current - health_damage;
        let armor_after = before.armor - armor_absorbed;
        let died = health_after == 0;

        let mut candidate = session.clone();
        {
            let vitality = candidate
                .health
                .get_mut(&command.target)
                .expect("validated vitality remains attached");
            vitality.current = health_after;
            vitality.armor = armor_after;
            if armor_after == 0 {
                vitality.armor_item = None;
            }
            if died {
                vitality.state = VitalityState::Dead;
            }
        }

        let mut event = None;
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
                commands.push(EntityCommand::SetVisible {
                    entity: command.target,
                    visible: false,
                });
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
        *session = candidate;
        Ok(VitalityReceipt {
            disposition: DamageDisposition::Applied,
            facts,
            inventory: Vec::new(),
            event,
        })
    }

    pub fn grant_armor(
        session: &mut GameSession,
        player: EntityId,
        item: ItemDefinitionId,
    ) -> Result<VitalityReceipt, VitalityRejection> {
        let Some(before) = session.health.get(&player).cloned() else {
            return Err(VitalityRejection::UnknownVitality { entity: player });
        };
        if before.state == VitalityState::Dead {
            return Err(VitalityRejection::PlayerDead { player });
        }
        if before.config.max_armor == 0 {
            return Err(VitalityRejection::ArmorUnsupported { player });
        }
        if before.armor >= before.config.max_armor {
            return Err(VitalityRejection::ArmorFull { player });
        }
        if let Some(active) = &before.armor_item {
            if active != &item {
                return Err(VitalityRejection::ArmorItemConflict {
                    player,
                    active: active.clone(),
                    offered: item,
                });
            }
        }
        let definition = session
            .item_definitions
            .get(&item)
            .ok_or_else(|| VitalityRejection::MissingItemDefinition { item: item.clone() })?;
        let ItemKind::Armor { protection } = definition.kind else {
            return Err(VitalityRejection::IncompatibleItem { item });
        };
        let mut candidate = session.clone();
        let sequence = next_inventory_sequence(&candidate, player)?;
        let inventory = InventoryService::apply(
            &mut candidate,
            player,
            InventoryCommand {
                sequence,
                action: InventoryAction::Consume {
                    item: item.clone(),
                    quantity: 1,
                },
            },
        )
        .map_err(|rejection| match rejection {
            InventoryRejection::QuantityUnderflow { .. } => VitalityRejection::ItemNotOwned {
                player,
                item: item.clone(),
            },
            other => VitalityRejection::Inventory(other),
        })?;
        let after = before
            .armor
            .saturating_add(protection)
            .min(before.config.max_armor);
        let component = candidate
            .health
            .get_mut(&player)
            .expect("validated vitality remains attached");
        component.armor = after;
        component.armor_item = Some(item.clone());
        *session = candidate;
        Ok(VitalityReceipt {
            disposition: DamageDisposition::Applied,
            facts: vec![VitalityFact::ArmorGranted {
                entity: player,
                item,
                amount: after - before.armor,
                before: before.armor,
                after,
            }],
            inventory: vec![inventory],
            event: None,
        })
    }

    pub fn use_health_supply(
        session: &mut GameSession,
        player: EntityId,
        item: ItemDefinitionId,
    ) -> Result<VitalityReceipt, VitalityRejection> {
        let Some(before) = session.health.get(&player).cloned() else {
            return Err(VitalityRejection::UnknownVitality { entity: player });
        };
        if before.state == VitalityState::Dead {
            return Err(VitalityRejection::PlayerDead { player });
        }
        if before.current == before.config.max {
            return Err(VitalityRejection::HealthFull { player });
        }
        let definition = session
            .item_definitions
            .get(&item)
            .ok_or_else(|| VitalityRejection::MissingItemDefinition { item: item.clone() })?;
        let ItemKind::HealthSupply { restore_health } = definition.kind else {
            return Err(VitalityRejection::IncompatibleItem { item });
        };
        let mut candidate = session.clone();
        let sequence = next_inventory_sequence(&candidate, player)?;
        let inventory = InventoryService::apply(
            &mut candidate,
            player,
            InventoryCommand {
                sequence,
                action: InventoryAction::Consume {
                    item: item.clone(),
                    quantity: 1,
                },
            },
        )
        .map_err(VitalityRejection::Inventory)?;
        let after = before
            .current
            .saturating_add(restore_health)
            .min(before.config.max);
        candidate
            .health
            .get_mut(&player)
            .expect("validated vitality remains attached")
            .current = after;
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
            inventory: vec![inventory],
            event: None,
        })
    }

    pub fn is_dead(session: &GameSession, entity: EntityId) -> bool {
        session
            .health
            .get(&entity)
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

fn vec3_is_finite(value: Vec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}
