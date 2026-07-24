use core_ids::EntityId;

use crate::runtime::RuntimeError;
use crate::runtime_records::GameEvent;
use crate::session::GameSession;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SwitchComponent {
    pub activation_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchView {
    pub entity: EntityId,
    pub activation_count: u64,
    pub controls_targets: Vec<EntityId>,
}

pub(crate) struct InteractionService;

impl InteractionService {
    pub(crate) fn interact(
        session: &mut GameSession,
        actor: EntityId,
        target: EntityId,
    ) -> Result<GameEvent, RuntimeError> {
        if !session.entities.contains(actor) {
            return Err(RuntimeError::UnknownActor { actor });
        }
        let Some(switch) = session.switches.get_mut(&target) else {
            return Err(RuntimeError::NotInteractable { entity: target });
        };
        switch.activation_count = switch.activation_count.saturating_add(1);
        Ok(GameEvent::SwitchActivated {
            switch: target,
            actor,
        })
    }
}
