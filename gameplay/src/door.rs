use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::core_time::TickDelta;
use rusty_engine::entity_state::{EntityCommand, EntityCommandBatch, EntityView};

use crate::runtime::RuntimeError;
use crate::runtime_records::GameEvent;
use crate::session::GameSession;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoorConfig {
    pub closed_translation: Vec3,
    pub open_translation: Vec3,
    pub auto_close_after: Option<TickDelta>,
    pub motion_duration: TickDelta,
}

pub const DEFAULT_DOOR_MOTION_DURATION_TICKS: u64 = 1;

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
            motion_duration: TickDelta::new(DEFAULT_DOOR_MOTION_DURATION_TICKS),
        }
    }

    pub fn with_motion_duration(mut self, motion_duration: TickDelta) -> Self {
        self.motion_duration = motion_duration;
        self
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.motion_duration.raw() > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorState {
    Closed,
    Opening,
    Open,
    Closing,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoorComponent {
    pub config: DoorConfig,
    pub state: DoorState,
    pub motion_elapsed: TickDelta,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoorView {
    pub entity: EntityId,
    pub config: DoorConfig,
    pub state: DoorState,
    pub motion_elapsed: TickDelta,
    pub entity_view: EntityView,
}

impl DoorView {
    pub fn motion_elapsed(&self) -> TickDelta {
        self.motion_elapsed
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DoorTransition {
    pub(crate) event: GameEvent,
    pub(crate) auto_close_after: Option<TickDelta>,
    pub(crate) motion_duration: TickDelta,
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
        if !component.config.is_valid() {
            return Err(RuntimeError::InvalidDoorMotionDuration {
                door,
                motion_duration: component.config.motion_duration.raw(),
            });
        }
        if uses_legacy_immediate_motion(session, door, component) {
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
            let live = session.doors.get_mut(&door).expect("door validated above");
            live.state = DoorState::Open;
            live.motion_elapsed = live.config.motion_duration;
            return Ok(Some(DoorTransition {
                event: GameEvent::DoorOpened {
                    door,
                    entity_facts: receipt.facts,
                },
                auto_close_after: live.config.auto_close_after,
                motion_duration: TickDelta::ZERO,
            }));
        }
        let motion_elapsed = match component.state {
            DoorState::Closed => TickDelta::ZERO,
            DoorState::Opening | DoorState::Open => return Ok(None),
            DoorState::Closing => reverse_motion_elapsed(component),
        };
        let receipt = session
            .entities
            .apply_batch(EntityCommandBatch::new([
                EntityCommand::SetCollisionEnabled {
                    entity: door,
                    enabled: true,
                },
            ]))
            .map_err(RuntimeError::EntityBatch)?;
        let component = session.doors.get_mut(&door).expect("door validated above");
        component.state = DoorState::Opening;
        component.motion_elapsed = motion_elapsed;
        Ok(Some(DoorTransition {
            event: GameEvent::DoorOpened {
                door,
                entity_facts: receipt.facts,
            },
            auto_close_after: component.config.auto_close_after,
            motion_duration: component.config.motion_duration,
        }))
    }

    pub(crate) fn close(
        session: &mut GameSession,
        door: EntityId,
    ) -> Result<Option<GameEvent>, RuntimeError> {
        let Some(component) = session.doors.get(&door).copied() else {
            return Err(RuntimeError::UnknownDoor { door });
        };
        if !component.config.is_valid() {
            return Err(RuntimeError::InvalidDoorMotionDuration {
                door,
                motion_duration: component.config.motion_duration.raw(),
            });
        }
        if uses_legacy_immediate_motion(session, door, component) {
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
            let live = session.doors.get_mut(&door).expect("door validated above");
            live.state = DoorState::Closed;
            live.motion_elapsed = TickDelta::ZERO;
            return Ok(Some(GameEvent::DoorClosed {
                door,
                entity_facts: receipt.facts,
            }));
        }
        let motion_elapsed = match component.state {
            DoorState::Closed | DoorState::Closing => return Ok(None),
            DoorState::Open => TickDelta::ZERO,
            DoorState::Opening => reverse_motion_elapsed(component),
        };
        let receipt = session
            .entities
            .apply_batch(EntityCommandBatch::new([
                EntityCommand::SetCollisionEnabled {
                    entity: door,
                    enabled: true,
                },
            ]))
            .map_err(RuntimeError::EntityBatch)?;
        let component = session.doors.get_mut(&door).expect("door validated above");
        component.state = DoorState::Closing;
        component.motion_elapsed = motion_elapsed;
        Ok(Some(GameEvent::DoorClosed {
            door,
            entity_facts: receipt.facts,
        }))
    }

    pub(crate) fn run_motion_phase(session: &mut GameSession) -> Result<(), RuntimeError> {
        let mut commands = Vec::new();
        let mut updates = Vec::new();
        for (door, component) in &session.doors {
            if !component.config.is_valid() {
                return Err(RuntimeError::InvalidDoorMotionDuration {
                    door: *door,
                    motion_duration: component.config.motion_duration.raw(),
                });
            }
            let duration = component.config.motion_duration.raw();
            let (state, motion_elapsed, translation) = match component.state {
                DoorState::Closed | DoorState::Open => continue,
                DoorState::Opening => {
                    let elapsed = component
                        .motion_elapsed
                        .raw()
                        .saturating_add(1)
                        .min(duration);
                    let state = if elapsed == duration {
                        DoorState::Open
                    } else {
                        DoorState::Opening
                    };
                    if state == DoorState::Open {
                        commands.push(EntityCommand::SetCollisionEnabled {
                            entity: *door,
                            enabled: false,
                        });
                    }
                    (
                        state,
                        TickDelta::new(elapsed),
                        motion_translation(component.config, DoorState::Opening, elapsed),
                    )
                }
                DoorState::Closing => {
                    let elapsed = component
                        .motion_elapsed
                        .raw()
                        .saturating_add(1)
                        .min(duration);
                    let state = if elapsed == duration {
                        DoorState::Closed
                    } else {
                        DoorState::Closing
                    };
                    (
                        state,
                        if state == DoorState::Closed {
                            TickDelta::ZERO
                        } else {
                            TickDelta::new(elapsed)
                        },
                        motion_translation(component.config, DoorState::Closing, elapsed),
                    )
                }
            };
            commands.push(EntityCommand::SetTranslation {
                entity: *door,
                translation,
            });
            updates.push((*door, state, motion_elapsed));
        }
        if commands.is_empty() {
            return Ok(());
        }
        session
            .entities
            .apply_batch(EntityCommandBatch::new(commands))
            .map_err(RuntimeError::EntityBatch)?;
        for (door, state, motion_elapsed) in updates {
            let component = session.doors.get_mut(&door).expect("door validated above");
            component.state = state;
            component.motion_elapsed = motion_elapsed;
        }
        Ok(())
    }
}

fn uses_legacy_immediate_motion(
    session: &GameSession,
    door: EntityId,
    component: DoorComponent,
) -> bool {
    component.config.motion_duration.raw() == DEFAULT_DOOR_MOTION_DURATION_TICKS
        && session.entities.view(door).is_ok_and(|view| {
            view.collision
                .is_some_and(|collision| collision.static_collider)
        })
}

fn reverse_motion_elapsed(component: DoorComponent) -> TickDelta {
    TickDelta::new(
        component.config.motion_duration.raw().saturating_sub(
            component
                .motion_elapsed
                .raw()
                .min(component.config.motion_duration.raw()),
        ),
    )
}

fn motion_translation(config: DoorConfig, direction: DoorState, elapsed: u64) -> Vec3 {
    let duration = config.motion_duration.raw();
    if elapsed == 0 {
        return match direction {
            DoorState::Opening => config.closed_translation,
            DoorState::Closing => config.open_translation,
            DoorState::Closed | DoorState::Open => config.closed_translation,
        };
    }
    if elapsed >= duration {
        return match direction {
            DoorState::Opening => config.open_translation,
            DoorState::Closing => config.closed_translation,
            DoorState::Closed | DoorState::Open => config.closed_translation,
        };
    }
    let progress = elapsed as f32 / duration as f32;
    match direction {
        DoorState::Opening => {
            config.closed_translation
                + (config.open_translation - config.closed_translation) * progress
        }
        DoorState::Closing => {
            config.open_translation
                + (config.closed_translation - config.open_translation) * progress
        }
        DoorState::Closed | DoorState::Open => config.closed_translation,
    }
}
