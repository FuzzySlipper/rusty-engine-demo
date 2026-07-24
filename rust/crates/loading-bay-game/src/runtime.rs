use std::collections::VecDeque;

use core_ids::EntityId;
use core_time::{Tick, TickDelta};

use engine_spatial::{
    KinematicMotionSystem, MotionPhaseError, MotionPhaseReceipt, NavigationStepError,
    VoxelCollisionScene, VoxelEditApplyError, VoxelEditReceipt, VoxelEditService,
    VoxelEditTransaction,
};

use crate::combat::{CombatReceipt, CombatRejectionReason, CombatService, ResolvedAttackAction};
use crate::content::{decode_project_content, AdmittedProject, ProjectContentError};
use crate::definition::GameEntityDefinitionError;
use crate::door::{security_door_definitions, DoorService, DoorTransition, SecurityDoorIds};
use crate::encounter::EncounterService;
use crate::interaction::InteractionService;
use crate::navigation::{EnemyNavigationSystem, NavigationPhaseReceipt};
use crate::player::{PlayerControlReceipt, PlayerControllerService, ResolvedPlayerAction};
use crate::project_admission::decode_and_admit_stored_project;
use crate::runtime_records::{readout, GameEvent, JournalEntry, RuntimeReadout, RuntimeReceipt};
use crate::scheduler::{ScheduledIntent, ScheduledIntentKind, Scheduler};
use crate::session::GameSession;

pub const MAX_EVENT_WAVE: usize = 256;
pub const MAX_TICK_ADVANCE: u64 = 100_000;

#[derive(Debug)]
pub enum RuntimeError {
    Content(ProjectContentError),
    StoredProject(crate::StoredProjectError),
    Definition(GameEntityDefinitionError),
    UnknownActor {
        actor: EntityId,
    },
    NotInteractable {
        entity: EntityId,
    },
    UnknownDoor {
        door: EntityId,
    },
    UnknownEnemy {
        enemy: EntityId,
    },
    UnknownWeapon {
        entity: EntityId,
    },
    CombatRejected {
        entity: EntityId,
        reason: CombatRejectionReason,
    },
    UnknownPlayerController {
        player: EntityId,
    },
    InvalidPlayerAction {
        action: ResolvedPlayerAction,
    },
    EntityBatch(entity_state::BatchRejection),
    EventWaveLimit {
        limit: usize,
    },
    TickAdvanceLimit {
        requested: u64,
        limit: u64,
    },
    MissingCollisionScene,
    Motion(MotionPhaseError),
    InvalidNavigationDelta {
        actual: f32,
    },
    NavigationStep {
        entity: EntityId,
        source: NavigationStepError,
    },
    VoxelEdit(VoxelEditApplyError),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for RuntimeError {}

#[derive(Debug)]
pub struct GameRuntime {
    pub(crate) session: GameSession,
    pub(crate) tick: Tick,
    pub(crate) scheduler: Scheduler,
    pub(crate) events: VecDeque<GameEvent>,
    pub(crate) journal: Vec<JournalEntry>,
    pub(crate) collision_scene: Option<VoxelCollisionScene>,
}

impl GameRuntime {
    pub fn new(session: GameSession) -> Self {
        Self {
            session,
            tick: Tick::ZERO,
            scheduler: Scheduler::default(),
            events: VecDeque::new(),
            journal: Vec::new(),
            collision_scene: None,
        }
    }

    pub fn security_door(
        auto_close_after: Option<TickDelta>,
    ) -> Result<(SecurityDoorIds, Self), RuntimeError> {
        let (ids, definitions) = security_door_definitions(auto_close_after);
        let session =
            GameSession::from_definitions(definitions).map_err(RuntimeError::Definition)?;
        Ok((ids, Self::new(session)))
    }

    pub fn from_project_content(input: &str) -> Result<Self, RuntimeError> {
        Ok(Self::from_admitted_project(
            decode_project_content(input).map_err(RuntimeError::Content)?,
        ))
    }

    pub fn from_stored_project(input: &str) -> Result<Self, RuntimeError> {
        Ok(Self::from_admitted_project(
            decode_and_admit_stored_project(input).map_err(RuntimeError::StoredProject)?,
        ))
    }

    pub fn from_admitted_project(admitted: AdmittedProject) -> Self {
        let AdmittedProject {
            session,
            collision_scene,
        } = admitted;
        let mut runtime = Self::new(session);
        runtime.collision_scene = collision_scene;
        runtime
    }

    pub fn tick(&self) -> Tick {
        self.tick
    }

    pub fn session(&self) -> &GameSession {
        &self.session
    }

    pub fn readout(&self) -> RuntimeReadout {
        readout(self.tick, &self.session, &self.scheduler, &self.journal)
    }

    pub fn collision_scene(&self) -> Option<&VoxelCollisionScene> {
        self.collision_scene.as_ref()
    }

    pub fn apply_voxel_edits(
        &mut self,
        transaction: VoxelEditTransaction<'_>,
    ) -> Result<VoxelEditReceipt, RuntimeError> {
        let scene = self
            .collision_scene
            .as_mut()
            .ok_or(RuntimeError::MissingCollisionScene)?;
        VoxelEditService::apply(scene, transaction).map_err(RuntimeError::VoxelEdit)
    }

    /// Run the one centrally scheduled kinematic phase over every configured
    /// body. Motion is not routed through the gameplay event journal: the spatial
    /// system returns its own typed facts and commits one atomic entity batch.
    pub fn run_motion_phase(
        &mut self,
        delta_seconds: f32,
    ) -> Result<MotionPhaseReceipt, RuntimeError> {
        let scene = self
            .collision_scene
            .as_ref()
            .ok_or(RuntimeError::MissingCollisionScene)?;
        KinematicMotionSystem::run(&mut self.session.entities, scene, delta_seconds)
            .map_err(RuntimeError::Motion)
    }

    /// Run the explicit autonomous-enemy navigation phase. The system derives
    /// a fresh bounded route from the canonical voxel scene, then applies the
    /// selected entities through the same collision-aware kinematic invariant.
    pub fn run_navigation_phase(
        &mut self,
        delta_seconds: f32,
    ) -> Result<NavigationPhaseReceipt, RuntimeError> {
        let scene = self
            .collision_scene
            .as_ref()
            .ok_or(RuntimeError::MissingCollisionScene)?;
        EnemyNavigationSystem::run(&mut self.session, scene, delta_seconds)
    }

    /// Apply one semantic player action. Browser device details have already
    /// been resolved at the host border; Rust owns controller interpretation,
    /// collision, accepted pose, and typed outcome facts.
    pub fn apply_player_action(
        &mut self,
        player: EntityId,
        action: ResolvedPlayerAction,
    ) -> Result<PlayerControlReceipt, RuntimeError> {
        let scene = self
            .collision_scene
            .as_ref()
            .ok_or(RuntimeError::MissingCollisionScene)?;
        PlayerControllerService::apply(&mut self.session, scene, player, action)
    }

    pub fn interact(
        &mut self,
        actor: EntityId,
        target: EntityId,
    ) -> Result<RuntimeReceipt, RuntimeError> {
        let event = InteractionService::interact(&mut self.session, actor, target)?;
        self.events.push_back(event);
        let events = self.drain_events()?;
        Ok(self.receipt(events))
    }

    pub fn defeat_enemy(
        &mut self,
        actor: EntityId,
        enemy: EntityId,
    ) -> Result<RuntimeReceipt, RuntimeError> {
        if let Some(event) = CombatService::defeat_enemy(&mut self.session, actor, enemy)? {
            self.events.push_back(event);
        }
        let events = self.drain_events()?;
        Ok(self.receipt(events))
    }

    /// Resolve one authored attack intent against authoritative player pose,
    /// live target components, and the canonical voxel collision projection.
    pub fn attack(
        &mut self,
        attacker: EntityId,
        action: ResolvedAttackAction,
    ) -> Result<CombatReceipt, RuntimeError> {
        let scene = self
            .collision_scene
            .as_ref()
            .ok_or(RuntimeError::MissingCollisionScene)?;
        let resolution =
            CombatService::attack(&mut self.session, scene, self.tick, attacker, action)?;
        if let Some(event) = resolution.event {
            self.events.push_back(event);
        }
        let events = self.drain_events()?;
        Ok(CombatReceipt {
            action: resolution.action,
            facts: resolution.facts,
            events,
        })
    }

    pub fn advance_by(&mut self, ticks: u64) -> Result<RuntimeReceipt, RuntimeError> {
        if ticks > MAX_TICK_ADVANCE {
            return Err(RuntimeError::TickAdvanceLimit {
                requested: ticks,
                limit: MAX_TICK_ADVANCE,
            });
        }
        let mut processed = Vec::new();
        for _ in 0..ticks {
            self.tick = self.tick.next();
            for intent in self.scheduler.drain_due(self.tick) {
                self.handle_scheduled_intent(intent)?;
            }
            processed.extend(self.drain_events()?);
        }
        Ok(self.receipt(processed))
    }

    fn handle_scheduled_intent(&mut self, intent: ScheduledIntent) -> Result<(), RuntimeError> {
        match intent.kind {
            ScheduledIntentKind::CloseDoor { door } => {
                if let Some(event) = DoorService::close(&mut self.session, door)? {
                    self.events.push_back(event);
                }
            }
        }
        Ok(())
    }

    fn drain_events(&mut self) -> Result<Vec<GameEvent>, RuntimeError> {
        let mut processed = Vec::new();
        while let Some(event) = self.events.pop_front() {
            if processed.len() >= MAX_EVENT_WAVE {
                self.events.clear();
                return Err(RuntimeError::EventWaveLimit {
                    limit: MAX_EVENT_WAVE,
                });
            }
            self.journal.push(JournalEntry {
                tick: self.tick,
                event: event.clone(),
            });
            match &event {
                GameEvent::SwitchActivated { switch, .. } => {
                    let targets = self
                        .session
                        .controls
                        .get(switch)
                        .cloned()
                        .unwrap_or_default();
                    for door in targets {
                        if let Some(transition) = DoorService::open(&mut self.session, door)? {
                            self.queue_door_transition(door, transition);
                        }
                    }
                }
                GameEvent::EnemyDefeated { enemy, .. } => {
                    self.events.extend(EncounterService::observe_enemy_defeat(
                        &mut self.session,
                        *enemy,
                    ));
                }
                GameEvent::EncounterCleared { exit, .. } => {
                    if let Some(transition) = DoorService::open(&mut self.session, *exit)? {
                        self.queue_door_transition(*exit, transition);
                    }
                }
                GameEvent::DoorOpened { .. } | GameEvent::DoorClosed { .. } => {}
            }
            processed.push(event);
        }
        Ok(processed)
    }

    fn queue_door_transition(&mut self, door: EntityId, transition: DoorTransition) {
        if let Some(delay) = transition.auto_close_after {
            self.scheduler.schedule(ScheduledIntent {
                due: self.tick.advance(delay),
                kind: ScheduledIntentKind::CloseDoor { door },
            });
        }
        self.events.push_back(transition.event);
    }

    fn receipt(&self, events: Vec<GameEvent>) -> RuntimeReceipt {
        RuntimeReceipt {
            tick: self.tick,
            events,
            projection: self.session.entities.projection(),
        }
    }
}
