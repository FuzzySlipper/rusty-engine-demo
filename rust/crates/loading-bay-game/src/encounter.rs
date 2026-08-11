use rusty_engine::core_ids::EntityId;
use rusty_engine::core_time::{Tick, TickDelta};

use crate::combat::EnemyState;
use crate::runtime_records::GameEvent;
use crate::session::GameSession;

pub const MAX_ENCOUNTER_ACTIVATION_RADIUS: f32 = 100_000.0;

#[derive(Debug, Clone, PartialEq)]
pub struct EncounterConfig {
    pub members: Vec<EntityId>,
    pub exit: Option<EntityId>,
    pub activation_radius: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncounterState {
    Dormant,
    Active,
    Cleared,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EncounterComponent {
    pub config: EncounterConfig,
    pub state: EncounterState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EncounterView {
    pub entity: EntityId,
    pub members: Vec<EntityId>,
    pub exit: Option<EntityId>,
    pub activation_radius: Option<f32>,
    pub state: EncounterState,
}

pub(crate) struct EncounterService;

impl EncounterService {
    pub(crate) fn activation_candidates(session: &GameSession, player: EntityId) -> Vec<EntityId> {
        let Some(player_position) = session
            .entities
            .view(player)
            .ok()
            .and_then(|view| view.transform.map(|transform| transform.translation))
        else {
            return Vec::new();
        };
        let candidates = session
            .encounters
            .iter()
            .filter_map(|(entity, encounter)| {
                let radius = encounter.config.activation_radius?;
                (encounter.state == EncounterState::Dormant).then_some((*entity, radius))
            })
            .collect::<Vec<_>>();
        candidates
            .into_iter()
            .filter_map(|(encounter, radius)| {
                let position = session
                    .entities
                    .view(encounter)
                    .ok()
                    .and_then(|view| view.transform.map(|transform| transform.translation))?;
                ((position - player_position).length() <= radius).then_some(encounter)
            })
            .collect()
    }

    pub(crate) fn activate(
        session: &mut GameSession,
        player: EntityId,
        candidates: &[EntityId],
        tick: Tick,
    ) -> Vec<GameEvent> {
        candidates
            .iter()
            .map(|encounter| {
                let component = session
                    .encounters
                    .get_mut(encounter)
                    .expect("activation candidate remains attached");
                component.state = EncounterState::Active;
                let members = component.config.members.clone();
                let member_count = members.len() as u64;
                for (index, member) in members.into_iter().enumerate() {
                    let Some(combat) = session.enemy_combat.get_mut(&member) else {
                        continue;
                    };
                    // Spread first attacks over each enemy's authored cadence.
                    // The group still wakes together, while the player receives
                    // a reaction window instead of simultaneous first damage.
                    let delay = combat
                        .config
                        .attack
                        .cooldown_ticks
                        .saturating_mul(index as u64 + 1)
                        .div_ceil(member_count)
                        .max(1);
                    let ready_at = tick.advance(TickDelta::new(delay));
                    if combat.state.ready_at_tick.raw() < ready_at.raw() {
                        combat.state.ready_at_tick = ready_at;
                    }
                }
                GameEvent::EncounterActivated {
                    encounter: *encounter,
                    player,
                }
            })
            .collect()
    }

    pub(crate) fn enemy_is_active(session: &GameSession, enemy: EntityId) -> bool {
        session
            .encounters
            .values()
            .find(|encounter| encounter.config.members.contains(&enemy))
            .is_none_or(|encounter| encounter.state == EncounterState::Active)
    }

    pub(crate) fn observe_enemy_defeat(
        session: &mut GameSession,
        enemy: EntityId,
    ) -> Vec<GameEvent> {
        let candidates: Vec<EntityId> = session
            .encounters
            .iter()
            .filter(|(_, encounter)| {
                encounter.state == EncounterState::Active
                    && encounter.config.members.contains(&enemy)
            })
            .map(|(entity, _)| *entity)
            .collect();
        let mut events = Vec::new();

        for encounter in candidates {
            let cleared = session.encounters[&encounter]
                .config
                .members
                .iter()
                .all(|member| {
                    session
                        .enemies
                        .get(member)
                        .is_some_and(|enemy| enemy.state == EnemyState::Defeated)
                });
            if !cleared {
                continue;
            }
            let component = session
                .encounters
                .get_mut(&encounter)
                .expect("candidate encounter exists");
            component.state = EncounterState::Cleared;
            events.push(GameEvent::EncounterCleared {
                encounter,
                exit: component.config.exit,
            });
        }

        events
    }
}
