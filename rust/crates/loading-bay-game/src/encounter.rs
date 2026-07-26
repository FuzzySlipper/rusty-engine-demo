use core_ids::EntityId;

use crate::combat::EnemyState;
use crate::runtime_records::GameEvent;
use crate::session::GameSession;

pub const MAX_ENCOUNTER_ACTIVATION_RADIUS: f32 = 100_000.0;

#[derive(Debug, Clone, PartialEq)]
pub struct EncounterConfig {
    pub members: Vec<EntityId>,
    pub exit: EntityId,
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
    pub exit: EntityId,
    pub activation_radius: Option<f32>,
    pub state: EncounterState,
}

pub(crate) struct EncounterService;

impl EncounterService {
    pub(crate) fn activate_for_player(
        session: &mut GameSession,
        player: EntityId,
    ) -> Vec<GameEvent> {
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
        let mut events = Vec::new();
        for (encounter, radius) in candidates {
            let Some(position) = session
                .entities
                .view(encounter)
                .ok()
                .and_then(|view| view.transform.map(|transform| transform.translation))
            else {
                continue;
            };
            if (position - player_position).length() > radius {
                continue;
            }
            session
                .encounters
                .get_mut(&encounter)
                .expect("activation candidate remains attached")
                .state = EncounterState::Active;
            events.push(GameEvent::EncounterActivated { encounter, player });
        }
        events
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
