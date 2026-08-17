use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::entity_state::EntityCommand;

use crate::inventory::ItemDefinitionId;
use crate::pickup::PickupState;
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
        let Some(drop) = session.enemy_drops.get(&enemy).copied() else {
            return Ok(None);
        };
        if drop.state == EnemyDropState::Materialized {
            return Ok(None);
        }
        let Some(pickup) = session.pickups.get(&drop.config.pickup).cloned() else {
            return Err(EnemyDropRejection::UnknownPickup {
                enemy,
                pickup: drop.config.pickup,
            });
        };
        if pickup.state != PickupState::Dormant {
            return Err(EnemyDropRejection::PickupNotDormant {
                enemy,
                pickup: drop.config.pickup,
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
            entity: drop.config.pickup,
            translation: position,
        });
        commands.push(EntityCommand::SetVisible {
            entity: drop.config.pickup,
            visible: true,
        });
        session
            .enemy_drops
            .get_mut(&enemy)
            .expect("validated enemy drop remains attached")
            .state = EnemyDropState::Materialized;
        session
            .pickups
            .get_mut(&drop.config.pickup)
            .expect("validated drop pickup remains attached")
            .state = PickupState::Available;

        Ok(Some(EnemyDropFact {
            enemy,
            pickup: drop.config.pickup,
            item: pickup.config.item,
            quantity: pickup.config.quantity,
            position,
        }))
    }
}
