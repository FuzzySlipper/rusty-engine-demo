use std::collections::BTreeMap;

use core_ids::EntityId;
use core_time::Tick;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScheduledIntentKind {
    CloseDoor { door: EntityId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledIntent {
    pub due: Tick,
    pub kind: ScheduledIntentKind,
}

#[derive(Debug, Clone, Default)]
pub struct Scheduler {
    entries: BTreeMap<ScheduledIntentKind, ScheduledIntent>,
}

impl Scheduler {
    pub fn schedule(&mut self, intent: ScheduledIntent) {
        self.entries.insert(intent.kind, intent);
    }

    pub fn cancel(&mut self, kind: ScheduledIntentKind) -> bool {
        self.entries.remove(&kind).is_some()
    }

    pub fn drain_due(&mut self, tick: Tick) -> Vec<ScheduledIntent> {
        let due: Vec<ScheduledIntentKind> = self
            .entries
            .iter()
            .filter(|(_, intent)| intent.due <= tick)
            .map(|(kind, _)| *kind)
            .collect();
        due.into_iter()
            .filter_map(|kind| self.entries.remove(&kind))
            .collect()
    }

    pub fn entries(&self) -> impl Iterator<Item = ScheduledIntent> + '_ {
        self.entries.values().copied()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
