use std::cell::RefCell;

use rusty_engine::core_ids::EntityId;
use rusty_engine::core_time::{Tick, TickDelta};

use crate::door::{DoorService, DoorTransition};
use crate::runtime::RuntimeError;
use crate::runtime_records::GameEvent;
use crate::scheduler::{ScheduledIntent, ScheduledIntentKind, Scheduler};
use crate::session::GameSession;
use crate::switch_program::{execute_switch_program, SwitchOperation, SwitchPredicate};

pub const DEFAULT_SWITCH_ACTIVATION_RADIUS: f32 = 2.0;
pub const DEFAULT_SWITCH_PROMPT: &str = "Activate switch";
pub const DEFAULT_SWITCH_UNAVAILABLE_PRESENTATION: &str = "Switch unavailable";
pub const DEFAULT_SWITCH_REPEATABLE: bool = true;
pub const MAX_SWITCH_ACTIVATION_RADIUS: f32 = 32.0;
pub const MAX_SWITCH_PRESENTATION_BYTES: usize = 160;
pub const MAX_SWITCH_EFFECTS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SwitchEffect {
    OpenDoor(EntityId),
    CloseDoor(EntityId),
}

impl SwitchEffect {
    pub fn door(&self) -> EntityId {
        match self {
            Self::OpenDoor(door) | Self::CloseDoor(door) => *door,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SwitchConfig {
    pub activation_radius: f32,
    pub prompt: String,
    pub unavailable_presentation: String,
    pub repeatable: bool,
    pub effects: Vec<SwitchEffect>,
}

impl Default for SwitchConfig {
    fn default() -> Self {
        Self {
            activation_radius: DEFAULT_SWITCH_ACTIVATION_RADIUS,
            prompt: DEFAULT_SWITCH_PROMPT.to_owned(),
            unavailable_presentation: DEFAULT_SWITCH_UNAVAILABLE_PRESENTATION.to_owned(),
            repeatable: DEFAULT_SWITCH_REPEATABLE,
            effects: Vec::new(),
        }
    }
}

impl SwitchConfig {
    pub fn new(
        activation_radius: f32,
        prompt: impl Into<String>,
        unavailable_presentation: impl Into<String>,
        repeatable: bool,
        effects: impl IntoIterator<Item = SwitchEffect>,
    ) -> Self {
        Self {
            activation_radius,
            prompt: prompt.into(),
            unavailable_presentation: unavailable_presentation.into(),
            repeatable,
            effects: effects.into_iter().collect(),
        }
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.activation_radius.is_finite()
            && self.activation_radius > 0.0
            && self.activation_radius <= MAX_SWITCH_ACTIVATION_RADIUS
            && valid_presentation(&self.prompt)
            && valid_presentation(&self.unavailable_presentation)
            && self.effects.len() <= MAX_SWITCH_EFFECTS
            && self.effects.iter().enumerate().all(|(index, effect)| {
                self.effects[index + 1..]
                    .iter()
                    .all(|other| other != effect)
            })
    }

    pub(crate) fn push_effect_if_missing(&mut self, effect: SwitchEffect) {
        if !self.effects.contains(&effect) {
            self.effects.push(effect);
        }
    }
}

fn valid_presentation(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_SWITCH_PRESENTATION_BYTES
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SwitchComponent {
    pub config: SwitchConfig,
    pub activation_count: u64,
}

impl SwitchComponent {
    pub fn new(config: SwitchConfig) -> Self {
        Self {
            config,
            activation_count: 0,
        }
    }

    pub fn is_available(&self) -> bool {
        self.config.repeatable || self.activation_count == 0
    }

    pub fn available(&self) -> bool {
        self.is_available()
    }

    pub fn unavailable(&self) -> bool {
        !self.is_available()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SwitchView {
    pub entity: EntityId,
    pub config: SwitchConfig,
    pub activation_count: u64,
    pub available: bool,
    pub controls_targets: Vec<EntityId>,
    pub entity_view: rusty_engine::entity_state::EntityView,
}

impl SwitchView {
    pub fn is_available(&self) -> bool {
        self.available
    }

    pub fn available(&self) -> bool {
        self.is_available()
    }

    pub fn unavailable(&self) -> bool {
        !self.is_available()
    }
}

pub(crate) fn switch_is_available(session: &GameSession, entity: EntityId) -> bool {
    let Some(component) = session.switches.get(&entity) else {
        return false;
    };
    if !component.is_available() {
        return false;
    }
    component.config.effects.is_empty()
        || component.config.effects.iter().any(|effect| match effect {
            SwitchEffect::OpenDoor(door) => session.doors.get(door).is_some_and(|door| {
                matches!(
                    door.state,
                    crate::DoorState::Closed | crate::DoorState::Closing
                )
            }),
            SwitchEffect::CloseDoor(door) => session.doors.get(door).is_some_and(|door| {
                matches!(
                    door.state,
                    crate::DoorState::Opening | crate::DoorState::Open
                )
            }),
        })
}

pub(crate) struct InteractionService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PreparedSwitchInteraction {
    pub(crate) actor: EntityId,
    pub(crate) switch: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchProgramRejection {
    OperationBeforeActivation {
        switch: EntityId,
        operation: &'static str,
    },
    DuplicateActivation {
        switch: EntityId,
    },
    DuplicateFeedback {
        switch: EntityId,
    },
}

impl InteractionService {
    pub(crate) fn prepare(
        session: &GameSession,
        actor: EntityId,
        target: EntityId,
    ) -> Result<PreparedSwitchInteraction, RuntimeError> {
        if !session.entities.contains(actor) {
            return Err(RuntimeError::UnknownActor { actor });
        }
        if crate::DamageService::is_dead(session, actor) {
            return Err(RuntimeError::PlayerDefeated { player: actor });
        }
        let Some(switch) = session.switches.get(&target) else {
            return Err(RuntimeError::NotInteractable { entity: target });
        };
        let config = switch.config.clone();
        let available = switch_is_available(session, target);
        let actor_translation = session
            .gameplay_translation(actor)
            .ok_or(RuntimeError::SwitchActorMissingTransform { actor })?;
        let switch_translation = session
            .entities
            .view(target)
            .map_err(|_| RuntimeError::NotInteractable { entity: target })?
            .transform
            .ok_or(RuntimeError::SwitchMissingTransform { switch: target })?
            .translation;
        if !config.activation_radius.is_finite()
            || config.activation_radius <= 0.0
            || config.activation_radius > MAX_SWITCH_ACTIVATION_RADIUS
        {
            return Err(RuntimeError::InvalidSwitchActivationRadius {
                switch: target,
                activation_radius: config.activation_radius,
            });
        }
        let distance_squared = (actor_translation - switch_translation).length_squared();
        if !distance_squared.is_finite()
            || distance_squared > config.activation_radius * config.activation_radius
        {
            return Err(RuntimeError::SwitchOutOfRange {
                actor,
                switch: target,
                distance_squared,
                activation_radius: config.activation_radius,
            });
        }
        if !available {
            return Err(RuntimeError::SwitchUnavailable {
                switch: target,
                presentation: config.unavailable_presentation,
            });
        }
        Ok(PreparedSwitchInteraction {
            actor,
            switch: target,
        })
    }

    pub(crate) fn execute_program(
        session: &mut GameSession,
        scheduler: &mut Scheduler,
        events: &mut Vec<GameEvent>,
        tick: Tick,
        interaction: PreparedSwitchInteraction,
        program: &crate::switch_program::SwitchProgram,
    ) -> Result<(), RuntimeError> {
        let context = RefCell::new(SwitchProgramContext {
            session,
            scheduler,
            events,
            tick,
            interaction,
            activation_recorded: false,
            feedback_emitted: false,
        });
        execute_switch_program(
            program,
            &mut |predicate| context.borrow_mut().predicate(predicate),
            &mut |operation| context.borrow_mut().operation(operation),
        )
    }
}

struct SwitchProgramContext<'a> {
    session: &'a mut GameSession,
    scheduler: &'a mut Scheduler,
    events: &'a mut Vec<GameEvent>,
    tick: Tick,
    interaction: PreparedSwitchInteraction,
    activation_recorded: bool,
    feedback_emitted: bool,
}

impl SwitchProgramContext<'_> {
    fn predicate(&mut self, predicate: SwitchPredicate) -> Result<bool, RuntimeError> {
        match predicate {
            SwitchPredicate::SwitchAvailable => {
                Ok(switch_is_available(self.session, self.interaction.switch))
            }
        }
    }

    fn operation(&mut self, operation: SwitchOperation) -> Result<(), RuntimeError> {
        match operation {
            SwitchOperation::RecordActivation => self.record_activation(),
            SwitchOperation::RequestOpenBoundDoor => self.request_open_bound_doors(),
            SwitchOperation::RequestCloseBoundDoor => self.request_close_bound_doors(),
            SwitchOperation::EmitInteractionFeedback => self.emit_interaction_feedback(),
        }
    }

    fn record_activation(&mut self) -> Result<(), RuntimeError> {
        if self.activation_recorded {
            return Err(RuntimeError::SwitchProgram(
                SwitchProgramRejection::DuplicateActivation {
                    switch: self.interaction.switch,
                },
            ));
        }
        let component = self
            .session
            .switches
            .get_mut(&self.interaction.switch)
            .expect("prepared switch remains attached to candidate session");
        component.activation_count = component.activation_count.saturating_add(1);
        self.activation_recorded = true;
        Ok(())
    }

    fn request_open_bound_doors(&mut self) -> Result<(), RuntimeError> {
        self.require_activation("request-open-bound-door")?;
        let doors = self.bound_doors(|effect| match effect {
            SwitchEffect::OpenDoor(door) => Some(*door),
            SwitchEffect::CloseDoor(_) => None,
        });
        for door in doors {
            if let Some(transition) = DoorService::open(self.session, door)? {
                queue_door_transition(self.tick, self.scheduler, self.events, door, transition);
            }
        }
        Ok(())
    }

    fn request_close_bound_doors(&mut self) -> Result<(), RuntimeError> {
        self.require_activation("request-close-bound-door")?;
        let doors = self.bound_doors(|effect| match effect {
            SwitchEffect::OpenDoor(_) => None,
            SwitchEffect::CloseDoor(door) => Some(*door),
        });
        for door in doors {
            self.scheduler
                .cancel(ScheduledIntentKind::CloseDoor { door });
            if let Some(event) = DoorService::close(self.session, door)? {
                self.events.push(event);
            }
        }
        Ok(())
    }

    fn emit_interaction_feedback(&mut self) -> Result<(), RuntimeError> {
        self.require_activation("emit-interaction-feedback")?;
        if self.feedback_emitted {
            return Err(RuntimeError::SwitchProgram(
                SwitchProgramRejection::DuplicateFeedback {
                    switch: self.interaction.switch,
                },
            ));
        }
        self.events.push(GameEvent::SwitchActivated {
            switch: self.interaction.switch,
            actor: self.interaction.actor,
        });
        self.feedback_emitted = true;
        Ok(())
    }

    fn require_activation(&self, operation: &'static str) -> Result<(), RuntimeError> {
        if self.activation_recorded {
            return Ok(());
        }
        Err(RuntimeError::SwitchProgram(
            SwitchProgramRejection::OperationBeforeActivation {
                switch: self.interaction.switch,
                operation,
            },
        ))
    }

    fn bound_doors(&self, select: impl Fn(&SwitchEffect) -> Option<EntityId>) -> Vec<EntityId> {
        self.session
            .switches
            .get(&self.interaction.switch)
            .expect("prepared switch remains attached to candidate session")
            .config
            .effects
            .iter()
            .filter_map(select)
            .collect()
    }
}

fn queue_door_transition(
    tick: Tick,
    scheduler: &mut Scheduler,
    events: &mut Vec<GameEvent>,
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
    events.push(transition.event);
}
