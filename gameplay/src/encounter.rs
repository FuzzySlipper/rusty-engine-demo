use std::cell::RefCell;
use std::collections::VecDeque;

use rusty_engine::core_ids::EntityId;
use rusty_engine::core_time::{Tick, TickDelta};

use crate::combat::EnemyComponent;
use crate::combat::EnemyState;
use crate::door::{DoorService, DoorTransition};
use crate::encounter_program::{
    execute_encounter_activation_program, execute_encounter_clear_program,
    EncounterActivationOperation, EncounterActivationPredicate, EncounterClearOperation,
    EncounterClearPredicate,
};
use crate::enemy_combat::EnemyCombatComponent;
use crate::runtime::RuntimeError;
use crate::runtime_records::GameEvent;
use crate::scheduler::{ScheduledIntent, ScheduledIntentKind, Scheduler};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PreparedEncounterActivation {
    pub(crate) player: EntityId,
    pub(crate) encounter: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncounterProgramRejection {
    ActivationNotEligible {
        encounter: EntityId,
    },
    ActivationOperationBeforeRecord {
        encounter: EntityId,
        operation: &'static str,
    },
    DuplicateActivationRecord {
        encounter: EntityId,
    },
    DuplicateActivationFeedback {
        encounter: EntityId,
    },
    ClearNotEligible {
        encounter: EntityId,
    },
    ClearOperationBeforeRecord {
        encounter: EntityId,
        operation: &'static str,
    },
    DuplicateClearRecord {
        encounter: EntityId,
    },
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
            .facts::<EncounterComponent>()
            .into_iter()
            .filter_map(|(entity, encounter)| {
                let radius = encounter.config.activation_radius?;
                (encounter.state == EncounterState::Dormant).then_some((entity, radius))
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

    pub(crate) fn prepare_activation(
        session: &GameSession,
        player: EntityId,
        encounter: EntityId,
    ) -> Option<PreparedEncounterActivation> {
        Self::activation_eligible(session, player, encounter)
            .then_some(PreparedEncounterActivation { player, encounter })
    }

    pub(crate) fn run_activation_program(
        session: &mut GameSession,
        events: &mut VecDeque<GameEvent>,
        tick: Tick,
        activation: PreparedEncounterActivation,
    ) -> Result<(), RuntimeError> {
        let program_id = session
            .encounter_program_bindings
            .get(&activation.encounter)
            .cloned()
            .ok_or(RuntimeError::MissingEncounterProgramBinding {
                encounter: activation.encounter,
            })?;
        let program = session
            .encounter_programs
            .get(&program_id)
            .cloned()
            .ok_or_else(|| RuntimeError::MissingEncounterProgram {
                encounter: activation.encounter,
                program_id: program_id.clone(),
            })?;
        let context = RefCell::new(EncounterActivationContext {
            session,
            events,
            tick,
            activation,
            activation_recorded: false,
            feedback_emitted: false,
        });
        execute_encounter_activation_program(
            &program.activation,
            &mut |predicate| context.borrow_mut().predicate(predicate),
            &mut |operation| context.borrow_mut().operation(operation),
        )
    }

    pub(crate) fn run_clear_programs_for_enemy_defeat(
        session: &mut GameSession,
        scheduler: &mut Scheduler,
        events: &mut VecDeque<GameEvent>,
        tick: Tick,
        enemy: EntityId,
    ) -> Result<(), RuntimeError> {
        let candidates = session
            .facts::<EncounterComponent>()
            .into_iter()
            .filter(|(_, encounter)| {
                encounter.state == EncounterState::Active
                    && encounter.config.members.contains(&enemy)
            })
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>();
        for encounter in candidates {
            let program_id = session
                .encounter_program_bindings
                .get(&encounter)
                .cloned()
                .ok_or(RuntimeError::MissingEncounterProgramBinding { encounter })?;
            let program = session
                .encounter_programs
                .get(&program_id)
                .cloned()
                .ok_or_else(|| RuntimeError::MissingEncounterProgram {
                    encounter,
                    program_id: program_id.clone(),
                })?;
            let context = RefCell::new(EncounterClearContext {
                session,
                scheduler,
                events,
                tick,
                encounter,
                clear_recorded: false,
            });
            execute_encounter_clear_program(
                &program.clear,
                &mut |predicate| context.borrow_mut().predicate(predicate),
                &mut |operation| context.borrow_mut().operation(operation),
            )?;
        }
        Ok(())
    }

    pub(crate) fn enemy_is_active(session: &GameSession, enemy: EntityId) -> bool {
        session
            .facts::<EncounterComponent>()
            .iter()
            .find(|(_, encounter)| encounter.config.members.contains(&enemy))
            .is_none_or(|(_, encounter)| encounter.state == EncounterState::Active)
    }

    pub(crate) fn attack_cadence_multiplier(session: &GameSession, enemy: EntityId) -> u64 {
        session
            .facts::<EncounterComponent>()
            .iter()
            .find(|(_, encounter)| {
                encounter.state == EncounterState::Active
                    && encounter.config.members.contains(&enemy)
            })
            .map_or(1, |(_, encounter)| encounter.config.members.len() as u64)
            .max(1)
    }

    fn activation_eligible(session: &GameSession, player: EntityId, encounter: EntityId) -> bool {
        let Some(component) = session.fact::<EncounterComponent>(encounter) else {
            return false;
        };
        if component.state != EncounterState::Dormant {
            return false;
        }
        let Some(radius) = component.config.activation_radius else {
            return false;
        };
        let Some(player_position) = session
            .entities
            .view(player)
            .ok()
            .and_then(|view| view.transform.map(|transform| transform.translation))
        else {
            return false;
        };
        let Some(encounter_position) = session
            .entities
            .view(encounter)
            .ok()
            .and_then(|view| view.transform.map(|transform| transform.translation))
        else {
            return false;
        };
        (encounter_position - player_position).length() <= radius
    }
}

struct EncounterActivationContext<'a> {
    session: &'a mut GameSession,
    events: &'a mut VecDeque<GameEvent>,
    tick: Tick,
    activation: PreparedEncounterActivation,
    activation_recorded: bool,
    feedback_emitted: bool,
}

impl EncounterActivationContext<'_> {
    fn predicate(&mut self, predicate: EncounterActivationPredicate) -> Result<bool, RuntimeError> {
        match predicate {
            EncounterActivationPredicate::ActivationEligible => {
                Ok(EncounterService::activation_eligible(
                    self.session,
                    self.activation.player,
                    self.activation.encounter,
                ))
            }
        }
    }

    fn operation(&mut self, operation: EncounterActivationOperation) -> Result<(), RuntimeError> {
        match operation {
            EncounterActivationOperation::RecordEncounterActivation => self.record_activation(),
            EncounterActivationOperation::ActivateBoundMembers => self.activate_bound_members(),
            EncounterActivationOperation::EmitEncounterFeedback => self.emit_feedback(),
        }
    }

    fn record_activation(&mut self) -> Result<(), RuntimeError> {
        if self.activation_recorded {
            return Err(RuntimeError::EncounterProgram(
                EncounterProgramRejection::DuplicateActivationRecord {
                    encounter: self.activation.encounter,
                },
            ));
        }
        if !EncounterService::activation_eligible(
            self.session,
            self.activation.player,
            self.activation.encounter,
        ) {
            return Err(RuntimeError::EncounterProgram(
                EncounterProgramRejection::ActivationNotEligible {
                    encounter: self.activation.encounter,
                },
            ));
        }
        self.session
            .update_fact::<EncounterComponent>(self.activation.encounter, |component| {
                component.state = EncounterState::Active
            });
        self.activation_recorded = true;
        Ok(())
    }

    fn activate_bound_members(&mut self) -> Result<(), RuntimeError> {
        self.require_activation("activate-bound-members")?;
        let members = self
            .session
            .fact::<EncounterComponent>(self.activation.encounter)
            .expect("prepared encounter remains attached to candidate session")
            .config
            .members
            .clone();
        let member_count = members.len() as u64;
        for (index, member) in members.into_iter().enumerate() {
            let Some(mut combat) = self.session.fact::<EnemyCombatComponent>(member) else {
                continue;
            };
            // Give the player one full group cadence to react, then spread
            // initial attacks over every member's Rust-owned cooldown.
            let delay = combat
                .config
                .attack
                .cooldown_ticks
                .saturating_mul(member_count.saturating_add(index as u64 + 1))
                .max(1);
            let ready_at = self.tick.advance(TickDelta::new(delay));
            if combat.state.ready_at_tick.raw() < ready_at.raw() {
                combat.state.ready_at_tick = ready_at;
                self.session.store_fact(member, combat);
            }
        }
        Ok(())
    }

    fn emit_feedback(&mut self) -> Result<(), RuntimeError> {
        self.require_activation("emit-encounter-feedback")?;
        if self.feedback_emitted {
            return Err(RuntimeError::EncounterProgram(
                EncounterProgramRejection::DuplicateActivationFeedback {
                    encounter: self.activation.encounter,
                },
            ));
        }
        self.events.push_back(GameEvent::EncounterActivated {
            encounter: self.activation.encounter,
            player: self.activation.player,
        });
        self.feedback_emitted = true;
        Ok(())
    }

    fn require_activation(&self, operation: &'static str) -> Result<(), RuntimeError> {
        if self.activation_recorded {
            return Ok(());
        }
        Err(RuntimeError::EncounterProgram(
            EncounterProgramRejection::ActivationOperationBeforeRecord {
                encounter: self.activation.encounter,
                operation,
            },
        ))
    }
}

struct EncounterClearContext<'a> {
    session: &'a mut GameSession,
    scheduler: &'a mut Scheduler,
    events: &'a mut VecDeque<GameEvent>,
    tick: Tick,
    encounter: EntityId,
    clear_recorded: bool,
}

impl EncounterClearContext<'_> {
    fn predicate(&mut self, predicate: EncounterClearPredicate) -> Result<bool, RuntimeError> {
        match predicate {
            EncounterClearPredicate::MembersDefeated => Ok(self.members_defeated()),
        }
    }

    fn operation(&mut self, operation: EncounterClearOperation) -> Result<(), RuntimeError> {
        match operation {
            EncounterClearOperation::RecordEncounterCleared => self.record_clear(),
            EncounterClearOperation::OpenBoundExit => self.open_bound_exit(),
        }
    }

    fn members_defeated(&self) -> bool {
        self.session
            .fact::<EncounterComponent>(self.encounter)
            .is_some_and(|component| {
                component.state == EncounterState::Active
                    && component.config.members.iter().all(|member| {
                        self.session
                            .fact::<EnemyComponent>(*member)
                            .is_some_and(|enemy| enemy.state == EnemyState::Defeated)
                    })
            })
    }

    fn record_clear(&mut self) -> Result<(), RuntimeError> {
        if self.clear_recorded {
            return Err(RuntimeError::EncounterProgram(
                EncounterProgramRejection::DuplicateClearRecord {
                    encounter: self.encounter,
                },
            ));
        }
        if !self.members_defeated() {
            return Err(RuntimeError::EncounterProgram(
                EncounterProgramRejection::ClearNotEligible {
                    encounter: self.encounter,
                },
            ));
        }
        let exit = {
            let mut component = self
                .session
                .fact::<EncounterComponent>(self.encounter)
                .expect("clearing encounter remains attached to candidate session");
            component.state = EncounterState::Cleared;
            let exit = component.config.exit;
            self.session.store_fact(self.encounter, component);
            exit
        };
        self.events.push_back(GameEvent::EncounterCleared {
            encounter: self.encounter,
            exit,
        });
        self.clear_recorded = true;
        Ok(())
    }

    fn open_bound_exit(&mut self) -> Result<(), RuntimeError> {
        self.require_clear("open-bound-exit")?;
        let exit = self
            .session
            .fact::<EncounterComponent>(self.encounter)
            .expect("clearing encounter remains attached to candidate session")
            .config
            .exit;
        if let Some(exit) = exit {
            if let Some(transition) = DoorService::open(self.session, exit)? {
                queue_door_transition(self.tick, self.scheduler, self.events, exit, transition);
            }
        }
        Ok(())
    }

    fn require_clear(&self, operation: &'static str) -> Result<(), RuntimeError> {
        if self.clear_recorded {
            return Ok(());
        }
        Err(RuntimeError::EncounterProgram(
            EncounterProgramRejection::ClearOperationBeforeRecord {
                encounter: self.encounter,
                operation,
            },
        ))
    }
}

fn queue_door_transition(
    tick: Tick,
    scheduler: &mut Scheduler,
    events: &mut VecDeque<GameEvent>,
    door: EntityId,
    transition: DoorTransition,
) {
    let scheduled_kind = ScheduledIntentKind::CloseDoor { door };
    scheduler.cancel(scheduled_kind);
    if let Some(delay) = transition.auto_close_after {
        let delay = TickDelta::new(transition.motion_duration.raw().saturating_add(delay.raw()));
        scheduler.schedule(ScheduledIntent {
            due: tick.advance(delay),
            kind: scheduled_kind,
        });
    }
    events.push_back(transition.event);
}
