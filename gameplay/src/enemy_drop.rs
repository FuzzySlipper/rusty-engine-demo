use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::entity_state::EntityCommand;

use crate::inventory::ItemDefinitionId;
use crate::pickup::{PickupComponent, PickupState};
use crate::session::GameSession;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnemyDropConfig {
    pub pickup: EntityId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyDropState {
    Armed,
    Materialized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnemyDropComponent {
    pub config: EnemyDropConfig,
    pub state: EnemyDropState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnemyDropView {
    pub enemy: EntityId,
    pub pickup: EntityId,
    pub state: EnemyDropState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnemyDropFact {
    pub enemy: EntityId,
    pub pickup: EntityId,
    pub item: ItemDefinitionId,
    pub quantity: u32,
    pub position: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyDropRejection {
    UnknownEnemy { enemy: EntityId },
    UnknownPickup { enemy: EntityId, pickup: EntityId },
    PickupNotDormant { enemy: EntityId, pickup: EntityId },
    MissingEnemyTransform { enemy: EntityId },
}

pub(crate) struct EnemyDropService;

impl EnemyDropService {
    pub(crate) fn stage_materialization(
        session: &mut GameSession,
        enemy: EntityId,
        commands: &mut Vec<EntityCommand>,
    ) -> Result<Option<EnemyDropFact>, EnemyDropRejection> {
        let Some(mut drop) = session.fact::<EnemyDropComponent>(enemy) else {
            return Ok(None);
        };
        if drop.state == EnemyDropState::Materialized {
            return Ok(None);
        }
        let pickup_entity = drop.config.pickup;
        let Some(mut pickup) = session.fact::<PickupComponent>(pickup_entity) else {
            return Err(EnemyDropRejection::UnknownPickup {
                enemy,
                pickup: pickup_entity,
            });
        };
        if pickup.state != PickupState::Dormant {
            return Err(EnemyDropRejection::PickupNotDormant {
                enemy,
                pickup: pickup_entity,
            });
        }
        let position = session
            .entities
            .view(enemy)
            .map_err(|_| EnemyDropRejection::UnknownEnemy { enemy })?
            .transform
            .ok_or(EnemyDropRejection::MissingEnemyTransform { enemy })?
            .translation;

        commands.push(EntityCommand::SetTranslation {
            entity: pickup_entity,
            translation: position,
        });
        commands.push(EntityCommand::SetVisible {
            entity: pickup_entity,
            visible: true,
        });

        drop.state = EnemyDropState::Materialized;
        pickup.state = PickupState::Available;

        let fact = EnemyDropFact {
            enemy,
            pickup: pickup_entity,
            item: pickup.config.item.clone(),
            quantity: pickup.config.quantity,
            position,
        };

        session.store_fact(enemy, drop);
        session.store_fact(pickup_entity, pickup);

        Ok(Some(fact))
    }
}
