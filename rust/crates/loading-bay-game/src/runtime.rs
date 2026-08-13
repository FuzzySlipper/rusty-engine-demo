use std::collections::{BTreeMap, VecDeque};

use rusty_engine::core_ids::EntityId;
use rusty_engine::core_time::{Tick, TickDelta};

use rusty_engine::engine_spatial::{
    CharacterControllerError, CharacterControllerService, FirstPersonLookError,
    KinematicMotionSystem, MotionPhaseError, MotionPhaseReceipt, NavigationStepError,
    SpatialOcclusionError, TriggerVolumeSystem, VoxelCollisionScene, VoxelEditApplyError,
    VoxelEditReceipt, VoxelEditService, VoxelEditTransaction,
};

use crate::combat::{CombatReceipt, CombatRejectionReason, CombatService, ResolvedAttackAction};
use crate::content::{decode_project_content, AdmittedProject, ProjectContentError};
use crate::definition::GameEntityDefinitionError;
use crate::door::{security_door_definitions, DoorService, DoorTransition, SecurityDoorIds};
use crate::encounter::EncounterService;
use crate::enemy_combat::{
    EnemyAttackPhaseReceipt, EnemyCombatService, EnemyIntentAndMotionReceipt,
};
use crate::explosive_prop::{ExplosivePropError, ExplosivePropPhaseReceipt, ExplosivePropService};
use crate::extraction_beacon::{ExtractionBeaconReceipt, ExtractionBeaconService};
use crate::floor_action::{FloorActionPhaseReceipt, FloorActionRejection, FloorActionService};
use crate::hazard::{HazardPhaseReceipt, HazardRejection, HazardService};
use crate::interaction::InteractionService;
use crate::inventory::{InventoryCommand, InventoryReceipt, InventoryRejection, InventoryService};
use crate::lift::{LiftPhaseReceipt, LiftRejection, LiftService};
use crate::navigation::{EnemyNavigationSystem, NavigationPhaseReceipt};
use crate::pickup::{
    PickupCollectionCause, PickupCollectionCommand, PickupPhaseReceipt, PickupReceipt,
    PickupRejection, PickupService,
};
use crate::player::{
    apply_player_action, apply_player_frame, PlayerControlReceipt, PlayerFrameReceipt,
    ResolvedPlayerAction, ResolvedPlayerFrame,
};
use crate::progression::{
    DoorAccessReceipt, DoorAccessRejection, LevelExitRejection, LoadingBayInterlockRejection,
    ProgressionFact, ProgressionService, SecretPhaseReceipt, SecretRejection,
};
use crate::project_admission::decode_and_admit_stored_project;
use crate::projectile::{ProjectileError, ProjectilePhaseReceipt, ProjectileService};
use crate::runtime_records::{readout, GameEvent, JournalEntry, RuntimeReadout, RuntimeReceipt};
use crate::scheduler::{ScheduledIntent, ScheduledIntentKind, Scheduler};
use crate::session::GameSession;
use crate::vitality::VitalityRejection;

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
    SwitchActorMissingTransform {
        actor: EntityId,
    },
    SwitchMissingTransform {
        switch: EntityId,
    },
    InvalidSwitchActivationRadius {
        switch: EntityId,
        activation_radius: f32,
    },
    SwitchOutOfRange {
        actor: EntityId,
        switch: EntityId,
        distance_squared: f32,
        activation_radius: f32,
    },
    SwitchUnavailable {
        switch: EntityId,
        presentation: String,
    },
    InvalidDoorMotionDuration {
        door: EntityId,
        motion_duration: u64,
    },
    UnknownDoor {
        door: EntityId,
    },
    UnknownEnemy {
        enemy: EntityId,
    },
    UnknownWeapon {
        entity: EntityId,
        item: crate::ItemDefinitionId,
    },
    CombatRejected {
        entity: EntityId,
        reason: CombatRejectionReason,
    },
    UnknownPlayerController {
        player: EntityId,
    },
    HazardPlayerMissingVitality {
        player: EntityId,
    },
    EnemyCombatPlayerMissingVitality {
        player: EntityId,
    },
    PlayerDefeated {
        player: EntityId,
    },
    UnknownExtractionBeacon {
        beacon: EntityId,
    },
    ExtractionBeaconActorMissingTransform {
        actor: EntityId,
    },
    ExtractionBeaconAlreadyActive {
        beacon: EntityId,
    },
    ExtractionBeaconOutOfRange {
        actor: EntityId,
        beacon: EntityId,
        distance_squared: f32,
        activation_radius: f32,
    },
    InvalidPlayerAction {
        action: ResolvedPlayerAction,
    },
    InvalidPlayerFrame {
        frame: ResolvedPlayerFrame,
    },
    PlayerCommandSequenceExhausted {
        player: EntityId,
    },
    CharacterController(CharacterControllerError),
    FirstPersonLook(FirstPersonLookError),
    EntityBatch(rusty_engine::entity_state::BatchRejection),
    InvalidFloorActionConfig {
        action: EntityId,
    },
    InvalidLiftConfig {
        lift: EntityId,
    },
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
    SpatialOcclusion(SpatialOcclusionError),
    VoxelEdit(VoxelEditApplyError),
    Inventory(InventoryRejection),
    InventorySequenceOverflow {
        owner: EntityId,
    },
    Pickup(PickupRejection),
    Vitality(VitalityRejection),
    Hazard(HazardRejection),
    FloorAction(FloorActionRejection),
    Lift(LiftRejection),
    DoorAccess(DoorAccessRejection),
    LoadingBayInterlock(LoadingBayInterlockRejection),
    LevelExit(LevelExitRejection),
    Secret(SecretRejection),
    Projectile(ProjectileError),
    ExplosiveProp(ExplosivePropError),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for RuntimeError {}

#[derive(Debug, Clone, PartialEq)]
pub struct WalkTriggerPhaseReceipt {
    pub floor_action: FloorActionPhaseReceipt,
    pub lift: LiftPhaseReceipt,
}

#[derive(Debug)]
pub struct GameRuntime {
    pub(crate) session: GameSession,
    pub(crate) tick: Tick,
    pub(crate) scheduler: Scheduler,
    pub(crate) events: VecDeque<GameEvent>,
    pub(crate) journal: Vec<JournalEntry>,
    pub(crate) collision_scene: Option<VoxelCollisionScene>,
    pub(crate) pickup_triggers: TriggerVolumeSystem,
    pub(crate) hazard_triggers: TriggerVolumeSystem,
    pub(crate) secret_triggers: TriggerVolumeSystem,
    pub(crate) floor_action_triggers: TriggerVolumeSystem,
    pub(crate) lift_triggers: TriggerVolumeSystem,
    pub(crate) projectiles: ProjectileService,
    pub(crate) player_controller_services: BTreeMap<EntityId, CharacterControllerService>,
}

impl GameRuntime {
    pub fn new(session: GameSession) -> Self {
        let player_controller_services = session
            .player_controllers
            .keys()
            .copied()
            .map(|entity| (entity, CharacterControllerService::default()))
            .collect();
        let pickup_triggers = PickupService::trigger_system(&session);
        let hazard_triggers = HazardService::trigger_system(&session);
        let secret_triggers = ProgressionService::secret_trigger_system(&session);
        let floor_action_triggers = FloorActionService::trigger_system(&session);
        let lift_triggers = LiftService::trigger_system(&session);
        Self {
            session,
            tick: Tick::ZERO,
            scheduler: Scheduler::default(),
            events: VecDeque::new(),
            journal: Vec::new(),
            collision_scene: None,
            pickup_triggers,
            hazard_triggers,
            secret_triggers,
            floor_action_triggers,
            lift_triggers,
            projectiles: ProjectileService::default(),
            player_controller_services,
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

    #[cfg(test)]
    pub(crate) fn session_mut(&mut self) -> &mut GameSession {
        &mut self.session
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

    pub fn apply_inventory_command(
        &mut self,
        owner: EntityId,
        command: InventoryCommand,
    ) -> Result<InventoryReceipt, RuntimeError> {
        InventoryService::apply(&mut self.session, owner, command).map_err(RuntimeError::Inventory)
    }

    pub fn collect_pickup(
        &mut self,
        actor: EntityId,
        pickup: EntityId,
        connection_generation: u64,
        command_sequence: u64,
    ) -> Result<PickupReceipt, RuntimeError> {
        PickupService::collect(
            &mut self.session,
            &mut self.pickup_triggers,
            PickupCollectionCommand {
                pickup,
                actor,
                tick: self.tick.raw(),
                cause: PickupCollectionCause::Interaction {
                    connection_generation,
                    command_sequence,
                },
            },
        )
        .map_err(RuntimeError::Pickup)
    }

    pub(crate) fn run_pickup_phase(
        &mut self,
        actor: EntityId,
    ) -> Result<PickupPhaseReceipt, RuntimeError> {
        PickupService::reconcile_and_collect(
            &mut self.session,
            &mut self.pickup_triggers,
            actor,
            self.tick.raw(),
        )
        .map_err(RuntimeError::Pickup)
    }

    pub(crate) fn run_hazard_phase(
        &mut self,
        player: EntityId,
    ) -> Result<HazardPhaseReceipt, RuntimeError> {
        let mut receipt = HazardService::reconcile_and_apply(
            &mut self.session,
            &mut self.hazard_triggers,
            player,
            self.tick,
        )
        .map_err(RuntimeError::Hazard)?;
        self.events.extend(receipt.events.drain(..));
        receipt.events = self.drain_events()?;
        Ok(receipt)
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

    pub(crate) fn run_door_motion_phase(&mut self) -> Result<(), RuntimeError> {
        DoorService::run_motion_phase(&mut self.session)
    }

    pub(crate) fn run_walk_trigger_motion_phase(&mut self) -> Result<(), RuntimeError> {
        FloorActionService::run_motion_phase(&mut self.session)?;
        LiftService::run_motion_phase(&mut self.session)
    }

    pub fn run_walk_trigger_phase(
        &mut self,
        actor: EntityId,
    ) -> Result<WalkTriggerPhaseReceipt, RuntimeError> {
        let mut candidate_session = self.session.clone();
        let mut candidate_floor_action_triggers = self.floor_action_triggers.clone();
        let mut candidate_lift_triggers = self.lift_triggers.clone();
        let floor_action = FloorActionService::reconcile_and_activate(
            &mut candidate_session,
            &mut candidate_floor_action_triggers,
            actor,
            self.tick.raw(),
        )
        .map_err(RuntimeError::FloorAction)?;
        let lift = LiftService::reconcile_and_activate(
            &mut candidate_session,
            &mut candidate_lift_triggers,
            actor,
            self.tick.raw(),
        )
        .map_err(RuntimeError::Lift)?;
        self.session = candidate_session;
        self.floor_action_triggers = candidate_floor_action_triggers;
        self.lift_triggers = candidate_lift_triggers;
        Ok(WalkTriggerPhaseReceipt { floor_action, lift })
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

    /// Run the game-specific enemy perception and intent owner, then feed only
    /// its transient pursuit goals into the canonical bounded Engine navigation
    /// and collision-aware motion seams. The candidate session commits only
    /// after both phases succeed.
    pub(crate) fn run_encounter_activation_phase(
        &mut self,
        player: EntityId,
    ) -> Result<Vec<GameEvent>, RuntimeError> {
        let candidates = EncounterService::activation_candidates(&self.session, player);
        if self
            .events
            .len()
            .checked_add(candidates.len())
            .is_none_or(|pending| pending > MAX_EVENT_WAVE)
        {
            return Err(RuntimeError::EventWaveLimit {
                limit: MAX_EVENT_WAVE,
            });
        }
        self.events.extend(EncounterService::activate(
            &mut self.session,
            player,
            &candidates,
            self.tick,
        ));
        self.drain_events()
    }

    pub fn run_enemy_intent_and_motion_phase(
        &mut self,
        player: EntityId,
        delta_seconds: f32,
    ) -> Result<EnemyIntentAndMotionReceipt, RuntimeError> {
        let scene = self
            .collision_scene
            .as_ref()
            .ok_or(RuntimeError::MissingCollisionScene)?;
        let mut candidate = self.session.clone();
        let intent = EnemyCombatService::perceive_and_plan(&mut candidate, scene, player)?;
        let navigation = EnemyNavigationSystem::run_with_combat_goals(
            &mut candidate,
            scene,
            delta_seconds,
            &intent.navigation_goals,
        )?;
        self.session = candidate;
        Ok(EnemyIntentAndMotionReceipt {
            facts: intent.facts,
            navigation,
        })
    }

    pub fn run_enemy_unaware_phase(
        &mut self,
        delta_seconds: f32,
    ) -> Result<EnemyIntentAndMotionReceipt, RuntimeError> {
        let scene = self
            .collision_scene
            .as_ref()
            .ok_or(RuntimeError::MissingCollisionScene)?;
        let mut candidate = self.session.clone();
        let intent = EnemyCombatService::idle_without_player_awareness(&mut candidate);
        let navigation = EnemyNavigationSystem::run_with_combat_goals(
            &mut candidate,
            scene,
            delta_seconds,
            &intent.navigation_goals,
        )?;
        self.session = candidate;
        Ok(EnemyIntentAndMotionReceipt {
            facts: intent.facts,
            navigation,
        })
    }

    pub fn run_enemy_attack_phase(
        &mut self,
        player: EntityId,
    ) -> Result<EnemyAttackPhaseReceipt, RuntimeError> {
        let scene = self
            .collision_scene
            .as_ref()
            .ok_or(RuntimeError::MissingCollisionScene)?;
        let mut candidate = self.session.clone();
        let mut receipt = EnemyCombatService::attack(
            &mut candidate,
            scene,
            self.tick,
            player,
            &mut self.projectiles,
        )?;
        self.session = candidate;
        for event in receipt.events.drain(..) {
            self.events.push_back(event);
        }
        receipt.events = self.drain_events()?;
        Ok(receipt)
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
        let service = self
            .player_controller_services
            .get_mut(&player)
            .ok_or(RuntimeError::UnknownPlayerController { player })?;
        apply_player_action(&mut self.session, scene, service, player, action)
    }

    pub(crate) fn integrate_player_frame(
        &mut self,
        player: EntityId,
        frame: ResolvedPlayerFrame,
    ) -> Result<PlayerFrameReceipt, RuntimeError> {
        let scene = self
            .collision_scene
            .as_ref()
            .ok_or(RuntimeError::MissingCollisionScene)?;
        let service = self
            .player_controller_services
            .get_mut(&player)
            .ok_or(RuntimeError::UnknownPlayerController { player })?;
        apply_player_frame(&mut self.session, scene, service, player, frame)
    }

    /// Activate one game-owned extraction beacon through its named service.
    /// This direct entry point returns a typed fact and does not route through
    /// a generic event bus or method-name bridge.
    pub fn activate_extraction_beacon(
        &mut self,
        actor: EntityId,
        beacon: EntityId,
    ) -> Result<ExtractionBeaconReceipt, RuntimeError> {
        ExtractionBeaconService::activate(&mut self.session, self.tick, actor, beacon)
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

    pub fn open_keyed_door(
        &mut self,
        actor: EntityId,
        door: EntityId,
    ) -> Result<(DoorAccessReceipt, Vec<GameEvent>), RuntimeError> {
        let receipt = ProgressionService::open_keyed_door(&mut self.session, actor, door)
            .map_err(RuntimeError::DoorAccess)?;
        if let Some(transition) = receipt.transition.clone() {
            self.queue_door_transition(door, transition);
        }
        let events = self.drain_events()?;
        Ok((receipt, events))
    }

    pub fn activate_loading_bay_interlock(
        &mut self,
        actor: EntityId,
        switch: EntityId,
    ) -> Result<RuntimeReceipt, RuntimeError> {
        let event =
            InteractionService::interact(&mut self.session, actor, switch).map_err(|error| {
                RuntimeError::LoadingBayInterlock(match error {
                    RuntimeError::UnknownActor { actor } => {
                        LoadingBayInterlockRejection::UnknownActor { actor }
                    }
                    RuntimeError::PlayerDefeated { player } => {
                        LoadingBayInterlockRejection::PlayerDefeated { actor: player }
                    }
                    RuntimeError::SwitchActorMissingTransform { actor } => {
                        LoadingBayInterlockRejection::ActorMissingTransform { actor }
                    }
                    RuntimeError::SwitchMissingTransform { switch } => {
                        LoadingBayInterlockRejection::InterlockMissingTransform { switch }
                    }
                    RuntimeError::SwitchOutOfRange { actor, switch, .. } => {
                        LoadingBayInterlockRejection::OutOfRange { actor, switch }
                    }
                    RuntimeError::NotInteractable { entity } => {
                        LoadingBayInterlockRejection::UnknownInterlock { switch: entity }
                    }
                    _ => LoadingBayInterlockRejection::InteractionFailed { switch },
                })
            })?;
        self.events.push_back(event);
        let events = self.drain_events()?;
        Ok(self.receipt(events))
    }

    pub fn complete_level(
        &mut self,
        actor: EntityId,
        exit: EntityId,
    ) -> Result<Option<ProgressionFact>, RuntimeError> {
        ProgressionService::complete_level(&mut self.session, actor, exit, self.tick)
            .map_err(RuntimeError::LevelExit)
    }

    pub(crate) fn run_secret_phase(
        &mut self,
        actor: EntityId,
    ) -> Result<SecretPhaseReceipt, RuntimeError> {
        ProgressionService::reconcile_secrets(
            &mut self.session,
            &mut self.secret_triggers,
            actor,
            self.tick,
        )
        .map_err(RuntimeError::Secret)
    }

    pub fn is_level_complete(&self) -> bool {
        self.session.level_complete()
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
        let resolution = CombatService::attack(
            &mut self.session,
            scene,
            &mut self.projectiles,
            self.tick,
            attacker,
            action,
        )?;
        for event in resolution.events {
            self.events.push_back(event);
        }
        let events = self.drain_events()?;
        Ok(CombatReceipt {
            action: resolution.action,
            facts: resolution.facts,
            events,
        })
    }

    /// Advance the Engine-owned rigid-body projectile phase once.
    ///
    /// The fixed game loop calls this from its combat phase.  It is public so
    /// downstream consumers can exercise the same authoritative phase without
    /// manufacturing a second physics implementation or mutating projections
    /// directly.
    pub fn run_projectile_phase(
        &mut self,
        step_seconds: f32,
    ) -> Result<ProjectilePhaseReceipt, RuntimeError> {
        let scene = self
            .collision_scene
            .as_ref()
            .ok_or(RuntimeError::MissingCollisionScene)?;
        let mut receipt = self
            .projectiles
            .step(&mut self.session, scene, self.tick, step_seconds)
            .map_err(RuntimeError::Projectile)?;
        self.events.extend(receipt.events.drain(..));
        receipt.events = self.drain_events()?;
        Ok(receipt)
    }

    pub fn run_explosive_prop_phase(&mut self) -> Result<ExplosivePropPhaseReceipt, RuntimeError> {
        let scene = self
            .collision_scene
            .as_ref()
            .ok_or(RuntimeError::MissingCollisionScene)?;
        let mut receipt = ExplosivePropService::run(&mut self.session, scene)
            .map_err(RuntimeError::ExplosiveProp)?;
        self.events.extend(receipt.events.drain(..));
        receipt.events = self.drain_events()?;
        Ok(receipt)
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
            self.begin_fixed_tick();
            self.run_door_motion_phase()?;
            self.run_walk_trigger_motion_phase()?;
            processed.extend(self.run_scheduled_consequence_phase()?);
        }
        Ok(self.receipt(processed))
    }

    pub(crate) fn begin_fixed_tick(&mut self) {
        self.tick = self.tick.next();
    }

    pub(crate) fn run_scheduled_consequence_phase(
        &mut self,
    ) -> Result<Vec<GameEvent>, RuntimeError> {
        for intent in self.scheduler.drain_due(self.tick) {
            self.handle_scheduled_intent(intent)?;
        }
        self.drain_events()
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
                    let effects = self
                        .session
                        .switches
                        .get(switch)
                        .map(|component| component.config.effects.clone())
                        .unwrap_or_default();
                    for effect in effects {
                        match effect {
                            crate::SwitchEffect::OpenDoor(door) => {
                                if let Some(transition) =
                                    DoorService::open(&mut self.session, door)?
                                {
                                    self.queue_door_transition(door, transition);
                                }
                            }
                            crate::SwitchEffect::CloseDoor(door) => {
                                self.scheduler
                                    .cancel(ScheduledIntentKind::CloseDoor { door });
                                if let Some(event) = DoorService::close(&mut self.session, door)? {
                                    self.events.push_back(event);
                                }
                            }
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
                    if let Some(exit) = *exit {
                        if let Some(transition) = DoorService::open(&mut self.session, exit)? {
                            self.queue_door_transition(exit, transition);
                        }
                    }
                }
                GameEvent::DoorOpened { .. }
                | GameEvent::DoorClosed { .. }
                | GameEvent::EncounterActivated { .. }
                | GameEvent::PlayerDied { .. } => {}
            }
            processed.push(event);
        }
        Ok(processed)
    }

    fn queue_door_transition(&mut self, door: EntityId, transition: DoorTransition) {
        let scheduled_kind = ScheduledIntentKind::CloseDoor { door };
        self.scheduler.cancel(scheduled_kind);
        if let Some(delay) = transition.auto_close_after {
            let delay =
                TickDelta::new(transition.motion_duration.raw().saturating_add(delay.raw()));
            self.scheduler.schedule(ScheduledIntent {
                due: self.tick.advance(delay),
                kind: scheduled_kind,
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
