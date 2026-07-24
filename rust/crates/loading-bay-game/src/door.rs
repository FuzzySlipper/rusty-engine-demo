use core_ids::EntityId;
use core_math::Vec3;
use core_time::TickDelta;
use entity_state::{EntityCommand, EntityCommandBatch, EntityDefinition, EntityView};

use crate::definition::GameEntityDefinition;
use crate::runtime::RuntimeError;
use crate::runtime_records::GameEvent;
use crate::session::GameSession;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoorConfig {
    pub closed_translation: Vec3,
    pub open_translation: Vec3,
    pub auto_close_after: Option<TickDelta>,
}

impl DoorConfig {
    pub fn new(
        closed_translation: Vec3,
        open_translation: Vec3,
        auto_close_after: Option<TickDelta>,
    ) -> Self {
        Self {
            closed_translation,
            open_translation,
            auto_close_after,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorState {
    Closed,
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoorComponent {
    pub config: DoorConfig,
    pub state: DoorState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoorView {
    pub entity: EntityId,
    pub config: DoorConfig,
    pub state: DoorState,
    pub entity_view: EntityView,
}

pub(crate) struct DoorTransition {
    pub(crate) event: GameEvent,
    pub(crate) auto_close_after: Option<TickDelta>,
}

pub(crate) struct DoorService;

impl DoorService {
    pub(crate) fn open(
        session: &mut GameSession,
        door: EntityId,
    ) -> Result<Option<DoorTransition>, RuntimeError> {
        let Some(component) = session.doors.get(&door).copied() else {
            return Err(RuntimeError::UnknownDoor { door });
        };
        if component.state == DoorState::Open {
            return Ok(None);
        }
        let receipt = session
            .entities
            .apply_batch(EntityCommandBatch::new([
                EntityCommand::SetTranslation {
                    entity: door,
                    translation: component.config.open_translation,
                },
                EntityCommand::SetCollisionEnabled {
                    entity: door,
                    enabled: false,
                },
            ]))
            .map_err(RuntimeError::EntityBatch)?;
        session
            .doors
            .get_mut(&door)
            .expect("door validated above")
            .state = DoorState::Open;
        Ok(Some(DoorTransition {
            event: GameEvent::DoorOpened {
                door,
                entity_facts: receipt.facts,
            },
            auto_close_after: component.config.auto_close_after,
        }))
    }

    pub(crate) fn close(
        session: &mut GameSession,
        door: EntityId,
    ) -> Result<Option<GameEvent>, RuntimeError> {
        let Some(component) = session.doors.get(&door).copied() else {
            return Err(RuntimeError::UnknownDoor { door });
        };
        if component.state == DoorState::Closed {
            return Ok(None);
        }
        let receipt = session
            .entities
            .apply_batch(EntityCommandBatch::new([
                EntityCommand::SetCollisionEnabled {
                    entity: door,
                    enabled: true,
                },
                EntityCommand::SetTranslation {
                    entity: door,
                    translation: component.config.closed_translation,
                },
            ]))
            .map_err(RuntimeError::EntityBatch)?;
        session
            .doors
            .get_mut(&door)
            .expect("door validated above")
            .state = DoorState::Closed;
        Ok(Some(GameEvent::DoorClosed {
            door,
            entity_facts: receipt.facts,
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecurityDoorIds {
    pub actor: EntityId,
    pub switch: EntityId,
    pub door: EntityId,
}

impl SecurityDoorIds {
    pub const fn standard() -> Self {
        Self {
            actor: EntityId::new(1),
            switch: EntityId::new(2),
            door: EntityId::new(3),
        }
    }
}

pub fn security_door_definitions(
    auto_close_after: Option<TickDelta>,
) -> (SecurityDoorIds, Vec<GameEntityDefinition>) {
    let ids = SecurityDoorIds::standard();
    let door_config = DoorConfig::new(Vec3::ZERO, Vec3::new(0.0, 3.0, 0.0), auto_close_after);
    (
        ids,
        vec![
            GameEntityDefinition::new(EntityDefinition::new(ids.actor, "player")),
            GameEntityDefinition::new(EntityDefinition::new(ids.switch, "security-switch"))
                .as_switch()
                .controls([ids.door]),
            GameEntityDefinition::new(
                EntityDefinition::new(ids.door, "security-door")
                    .with_transform(door_config.closed_translation)
                    .with_collision(true, true)
                    .with_renderable("mesh/security-door", true),
            )
            .as_door(door_config),
        ],
    )
}
