use core_ids::EntityId;
use core_time::Tick;
use entity_state::{EntityFact, ProjectionNode};

use crate::scheduler::Scheduler;
use crate::session::GameSession;

#[derive(Debug, Clone, PartialEq)]
pub enum GameEvent {
    SwitchActivated {
        switch: EntityId,
        actor: EntityId,
    },
    DoorOpened {
        door: EntityId,
        entity_facts: Vec<EntityFact>,
    },
    DoorClosed {
        door: EntityId,
        entity_facts: Vec<EntityFact>,
    },
    EnemyDefeated {
        enemy: EntityId,
        actor: EntityId,
        entity_facts: Vec<EntityFact>,
    },
    EncounterCleared {
        encounter: EntityId,
        exit: EntityId,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct JournalEntry {
    pub tick: Tick,
    pub event: GameEvent,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeReceipt {
    pub tick: Tick,
    pub events: Vec<GameEvent>,
    pub projection: Vec<ProjectionNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeReadout {
    pub tick: Tick,
    pub entity_revision: u64,
    pub projection: Vec<ProjectionNode>,
    pub pending_schedules: usize,
    pub journal: Vec<JournalEntry>,
}

pub(crate) fn readout(
    tick: Tick,
    session: &GameSession,
    scheduler: &Scheduler,
    journal: &[JournalEntry],
) -> RuntimeReadout {
    RuntimeReadout {
        tick,
        entity_revision: session.entities.revision(),
        projection: session.entities.projection(),
        pending_schedules: scheduler.len(),
        journal: journal.to_vec(),
    }
}
