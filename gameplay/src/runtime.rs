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
use crate::definition::GameEntityDefinitionError;
use crate::door::{DoorService, DoorTransition};
use crate::encounter::{EncounterComponent, EncounterProgramRejection, EncounterService};
use crate::enemy_combat::EnemyCombatComponent;
use crate::enemy_combat::{
    EnemyAttackPhaseReceipt, EnemyCombatService, EnemyIntentAndMotionReceipt,
};
use crate::explosive_prop::ExplosivePropComponent;
use crate::explosive_prop::{ExplosivePropError, ExplosivePropPhaseReceipt, ExplosivePropService};
use crate::extraction_beacon::{ExtractionBeaconReceipt, ExtractionBeaconService};
use crate::floor_action::FloorActionComponent;
use crate::floor_action::{FloorActionPhaseReceipt, FloorActionRejection, FloorActionService};
use crate::hazard::HazardComponent;
use crate::hazard::{HazardPhaseReceipt, HazardRejection, HazardService};
use crate::interaction::SwitchComponent;
use crate::interaction::{InteractionService, SwitchProgramRejection};
use crate::inventory::{InventoryCommand, InventoryReceipt, InventoryRejection, InventoryService};
use crate::lift::LiftComponent;
use crate::lift::{LiftPhaseReceipt, LiftRejection, LiftService};
use crate::navigation::{EnemyNavigationSystem, NavigationPhaseReceipt};
use crate::pickup::PickupComponent;
use crate::pickup::{
    PickupCollectionCause, PickupCollectionCommand, PickupPhaseReceipt, PickupReceipt,
    PickupRejection, PickupService,
};
use crate::player::PlayerControllerComponent;
use crate::player::{
    apply_player_action, apply_player_frame, PlayerControlReceipt, PlayerFrameReceipt,
    ResolvedPlayerAction, ResolvedPlayerFrame,
};
use crate::progression::{
    DoorAccessReceipt, DoorAccessRejection, LevelExitRejection, LoadingBayInterlockRejection,
    ProgressionFact, ProgressionService, SecretPhaseReceipt, SecretRejection,
};
use crate::progression::{LevelExitComponent, SecretRegionComponent};
use crate::project_admission::{decode_and_admit_stored_project, AdmittedProject};
use crate::projectile::{ProjectileError, ProjectilePhaseReceipt, ProjectileService};
use crate::runtime_records::{readout, GameEvent, JournalEntry, RuntimeReadout, RuntimeReceipt};
use crate::scheduler::{ScheduledIntent, ScheduledIntentKind, Scheduler};
use crate::session::GameSession;
use crate::vitality::VitalityRejection;
use crate::vitality::{DamageDisposition, DamageService, VitalityReceipt};

pub const MAX_EVENT_WAVE: usize = 256;
pub const MAX_TICK_ADVANCE: u64 = 100_000;

#[derive(Debug)]
pub enum RuntimeError {
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
    MissingSwitchProgramBinding {
        switch: EntityId,
    },
    MissingSwitchProgram {
        switch: EntityId,
        program_id: String,
    },
    SwitchProgram(SwitchProgramRejection),
    MissingEncounterProgramBinding {
        encounter: EntityId,
    },
    MissingEncounterProgram {
        encounter: EntityId,
        program_id: String,
    },
    EncounterProgram(EncounterProgramRejection),
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
    CombatResolutionFailed {
        reason: String,
    },
    GameplayProgramRejected {
        item: crate::ItemDefinitionId,
        context: &'static str,
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
    pub session: GameSession,
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
            .facts::<PlayerControllerComponent>()
            .into_iter()
            .map(|(entity, _)| (entity, CharacterControllerService::default()))
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

    pub fn session_mut(&mut self) -> &mut GameSession {
        &mut self.session
    }

    /// A new product connection must not inherit a diagnostic result that was
    /// observed by a retired connection generation.
    pub fn clear_gameplay_outcome(&mut self) {
        self.session.clear_gameplay_outcome();
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

    /// Explicit item use is a product operation.  The program only selects the
    /// closed Rust vitality primitive; all inventory and health mutation stays
    /// in `DamageService`'s candidate transaction.
    pub fn use_health_supply(
        &mut self,
        player: EntityId,
        item: crate::ItemDefinitionId,
    ) -> Result<VitalityReceipt, RuntimeError> {
        let program_id = self
            .session
            .item_definitions
            .get(&item)
            .and_then(|definition| definition.program.clone())
            .ok_or_else(|| RuntimeError::GameplayProgramRejected {
                item: item.clone(),
                context: "explicit-health-use-missing-program",
            })?;
        let program = self
            .session
            .gameplay_programs
            .get(&program_id)
            .cloned()
            .ok_or_else(|| RuntimeError::GameplayProgramRejected {
                item: item.clone(),
                context: "explicit-health-use",
            })?;
        let outcome_program = program.clone();
        let mut candidate = self.session.clone();
        let mut receipts = Vec::new();
        let mut executed = false;
        if let Err(error) = crate::gameplay_program::execute_program(
            &program,
            &mut |_| {
                Err(RuntimeError::GameplayProgramRejected {
                    item: item.clone(),
                    context: "explicit-health-predicate",
                })
            },
            &mut |operation| match operation {
                crate::gameplay_program::DemoOperation::UseHealthSupply => {
                    executed = true;
                    receipts.push(
                        DamageService::use_health_supply(&mut candidate, player, item.clone())
                            .map_err(RuntimeError::Vitality)?,
                    );
                    Ok(())
                }
                _ => Err(RuntimeError::GameplayProgramRejected {
                    item: item.clone(),
                    context: "explicit-health-use",
                }),
            },
        ) {
            self.session
                .record_gameplay_outcome(crate::gameplay_program::rejected_outcome(
                    program_id.clone(),
                    &outcome_program,
                    error.to_string(),
                ));
            return Err(error);
        }
        if !executed {
            let error = RuntimeError::GameplayProgramRejected {
                item,
                context: "explicit-health-use",
            };
            self.session
                .record_gameplay_outcome(crate::gameplay_program::rejected_outcome(
                    program_id.clone(),
                    &outcome_program,
                    error.to_string(),
                ));
            return Err(error);
        }
        let mut result = VitalityReceipt {
            disposition: DamageDisposition::Applied,
            facts: Vec::new(),
            enemy_drops: Vec::new(),
            explosive_props: Vec::new(),
            inventory: Vec::new(),
            event: None,
        };
        for receipt in receipts {
            result.facts.extend(receipt.facts);
            result.enemy_drops.extend(receipt.enemy_drops);
            result.explosive_props.extend(receipt.explosive_props);
            result.inventory.extend(receipt.inventory);
            result.event = result.event.or(receipt.event);
        }
        self.session = candidate;
        self.session
            .record_gameplay_outcome(crate::gameplay_program::applied_outcome(
                program_id,
                &outcome_program,
                vec!["use-health-supply".to_owned()],
                vec!["vitality".to_owned()],
            ));
        Ok(result)
    }

    /// Reattach transient compiled programs after a runtime snapshot load.
    /// Snapshots intentionally persist no compiled catalog or authored binding.
    pub fn reattach_authored_gameplay_programs(
        &mut self,
        authored: &crate::StoredProject,
    ) -> Result<(), RuntimeError> {
        let catalog =
            crate::gameplay_program::compile_gameplay_programs(&authored.gameplay_programs)
                .map_err(|error| RuntimeError::CombatResolutionFailed {
                    reason: error.to_string(),
                })?;
        let pickup_catalog = crate::pickup_program::compile_pickup_programs(
            &authored.pickup_programs,
        )
        .map_err(|error| RuntimeError::CombatResolutionFailed {
            reason: error.to_string(),
        })?;
        let player_setup_catalog =
            crate::player_program::compile_player_setup_programs(&authored.player_setup_programs)
                .map_err(|error| RuntimeError::CombatResolutionFailed {
                reason: error.to_string(),
            })?;
        let enemy_attack_catalog =
            crate::enemy_program::compile_enemy_attack_programs(&authored.enemy_attack_programs)
                .map_err(|error| RuntimeError::CombatResolutionFailed {
                    reason: error.to_string(),
                })?;
        let enemy_defeat_catalog =
            crate::enemy_program::compile_enemy_defeat_programs(&authored.enemy_defeat_programs)
                .map_err(|error| RuntimeError::CombatResolutionFailed {
                    reason: error.to_string(),
                })?;
        let hazard_catalog = crate::hazard_program::compile_hazard_programs(
            &authored.hazard_programs,
        )
        .map_err(|error| RuntimeError::CombatResolutionFailed {
            reason: error.to_string(),
        })?;
        let explosive_prop_catalog =
            crate::explosive_prop_program::compile_explosive_prop_programs(
                &authored.explosive_prop_programs,
            )
            .map_err(|error| RuntimeError::CombatResolutionFailed {
                reason: error.to_string(),
            })?;
        let switch_catalog = crate::switch_program::compile_switch_programs(
            &authored.switch_programs,
        )
        .map_err(|error| RuntimeError::CombatResolutionFailed {
            reason: error.to_string(),
        })?;
        let encounter_catalog =
            crate::encounter_program::compile_encounter_programs(&authored.encounter_programs)
                .map_err(|error| RuntimeError::CombatResolutionFailed {
                    reason: error.to_string(),
                })?;
        let floor_action_catalog = crate::floor_action_program::compile_floor_action_programs(
            &authored.floor_action_programs,
        )
        .map_err(|error| RuntimeError::CombatResolutionFailed {
            reason: error.to_string(),
        })?;
        let lift_catalog = crate::lift_program::compile_lift_programs(&authored.lift_programs)
            .map_err(|error| RuntimeError::CombatResolutionFailed {
                reason: error.to_string(),
            })?;
        let secret_catalog = crate::secret_program::compile_secret_programs(
            &authored.secret_programs,
        )
        .map_err(|error| RuntimeError::CombatResolutionFailed {
            reason: error.to_string(),
        })?;
        let level_exit_catalog =
            crate::level_exit_program::compile_level_exit_programs(&authored.level_exit_programs)
                .map_err(|error| RuntimeError::CombatResolutionFailed {
                reason: error.to_string(),
            })?;
        for definition in self.session.item_definitions.values_mut() {
            definition.program = authored
                .item_definitions
                .iter()
                .find(|authored_definition| authored_definition.id == definition.id.as_str())
                .and_then(|authored_definition| authored_definition.program.clone());
        }
        self.session.gameplay_programs = catalog;
        self.session.pickup_programs = pickup_catalog;
        self.session.player_setup_programs = player_setup_catalog;
        self.session.player_setup_bindings = authored
            .scenes
            .iter()
            .flat_map(|scene| &scene.entities)
            .filter_map(|entity| {
                entity
                    .inventory
                    .as_ref()
                    .map(|inventory| inventory.setup_program.clone())
                    .filter(|_| {
                        self.session
                            .inventories
                            .contains_key(&EntityId::new(entity.id))
                    })
                    .map(|program| (EntityId::new(entity.id), program))
            })
            .collect();
        self.session.enemy_attack_programs = enemy_attack_catalog;
        self.session.enemy_defeat_programs = enemy_defeat_catalog;
        self.session.hazard_programs = hazard_catalog;
        self.session.hazard_program_bindings = authored
            .scenes
            .iter()
            .flat_map(|scene| &scene.entities)
            .filter_map(|entity| {
                entity
                    .hazard
                    .as_ref()
                    .filter(|_| {
                        self.session
                            .has_fact::<HazardComponent>(EntityId::new(entity.id))
                    })
                    .map(|hazard| (EntityId::new(entity.id), hazard.program.clone()))
            })
            .collect();
        self.session.explosive_prop_programs = explosive_prop_catalog;
        self.session.explosive_prop_program_bindings = authored
            .scenes
            .iter()
            .flat_map(|scene| &scene.entities)
            .filter_map(|entity| {
                entity
                    .explosive_prop
                    .as_ref()
                    .filter(|_| {
                        self.session
                            .has_fact::<ExplosivePropComponent>(EntityId::new(entity.id))
                    })
                    .map(|prop| (EntityId::new(entity.id), prop.program.clone()))
            })
            .collect();
        self.session.encounter_programs = encounter_catalog;
        self.session.encounter_program_bindings = authored
            .scenes
            .iter()
            .flat_map(|scene| &scene.entities)
            .filter_map(|entity| {
                entity
                    .encounter
                    .as_ref()
                    .filter(|_| {
                        self.session
                            .has_fact::<EncounterComponent>(EntityId::new(entity.id))
                    })
                    .map(|encounter| (EntityId::new(entity.id), encounter.program.clone()))
            })
            .collect();
        self.session.switch_programs = switch_catalog;
        self.session.switch_program_bindings = authored
            .scenes
            .iter()
            .flat_map(|scene| &scene.entities)
            .filter_map(|entity| {
                entity
                    .switch
                    .as_ref()
                    .filter(|_| {
                        self.session
                            .has_fact::<SwitchComponent>(EntityId::new(entity.id))
                    })
                    .map(|switch| (EntityId::new(entity.id), switch.program.clone()))
            })
            .collect();
        self.session.floor_action_programs = floor_action_catalog;
        self.session.floor_action_program_bindings = authored
            .scenes
            .iter()
            .flat_map(|scene| &scene.entities)
            .filter_map(|entity| {
                entity
                    .floor_action
                    .as_ref()
                    .filter(|_| {
                        self.session
                            .has_fact::<FloorActionComponent>(EntityId::new(entity.id))
                    })
                    .map(|floor_action| (EntityId::new(entity.id), floor_action.program.clone()))
            })
            .collect();
        self.session.lift_programs = lift_catalog;
        self.session.lift_program_bindings = authored
            .scenes
            .iter()
            .flat_map(|scene| &scene.entities)
            .filter_map(|entity| {
                entity
                    .lift
                    .as_ref()
                    .filter(|_| {
                        self.session
                            .has_fact::<LiftComponent>(EntityId::new(entity.id))
                    })
                    .map(|lift| (EntityId::new(entity.id), lift.program.clone()))
            })
            .collect();
        self.session.secret_programs = secret_catalog;
        self.session.secret_program_bindings = authored
            .scenes
            .iter()
            .flat_map(|scene| &scene.entities)
            .filter_map(|entity| {
                entity
                    .secret_region
                    .as_ref()
                    .filter(|_| {
                        self.session
                            .has_fact::<SecretRegionComponent>(EntityId::new(entity.id))
                    })
                    .map(|secret| (EntityId::new(entity.id), secret.program.clone()))
            })
            .collect();
        self.session.level_exit_programs = level_exit_catalog;
        self.session.level_exit_program_bindings = authored
            .scenes
            .iter()
            .flat_map(|scene| &scene.entities)
            .filter_map(|entity| {
                entity
                    .level_exit
                    .as_ref()
                    .filter(|_| {
                        self.session
                            .has_fact::<LevelExitComponent>(EntityId::new(entity.id))
                    })
                    .map(|exit| (EntityId::new(entity.id), exit.program.clone()))
            })
            .collect();
        for entity in authored.scenes.iter().flat_map(|scene| &scene.entities) {
            if let Some(pickup) = &entity.pickup {
                if let Some(mut runtime_pickup) = self
                    .session
                    .fact::<PickupComponent>(EntityId::new(entity.id))
                {
                    runtime_pickup.config.program = pickup.program.clone();
                    self.session
                        .store_fact(EntityId::new(entity.id), runtime_pickup);
                }
            }
            let Some(combat) = &entity.enemy_combat else {
                continue;
            };
            let Some(mut runtime_combat) = self
                .session
                .fact::<EnemyCombatComponent>(EntityId::new(entity.id))
            else {
                continue;
            };
            runtime_combat.config.attack_program = combat.attack_program.clone();
            runtime_combat.config.defeat_program = combat.defeat_program.clone();
            self.session
                .store_fact(EntityId::new(entity.id), runtime_combat);
        }
        Ok(())
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

    pub fn run_pickup_phase(
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

    pub fn run_hazard_phase(
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

    pub fn run_door_motion_phase(&mut self) -> Result<(), RuntimeError> {
        DoorService::run_motion_phase(&mut self.session)
    }

    pub fn run_walk_trigger_motion_phase(&mut self) -> Result<(), RuntimeError> {
        let mut candidate_session = self.session.clone();
        FloorActionService::run_motion_phase(&mut candidate_session)?;
        LiftService::run_motion_phase(&mut candidate_session)?;
        self.session = candidate_session;
        Ok(())
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
    pub fn run_encounter_activation_phase(
        &mut self,
        player: EntityId,
    ) -> Result<Vec<GameEvent>, RuntimeError> {
        let candidates = EncounterService::activation_candidates(&self.session, player);
        // Activation plus any consequent event delivery is one candidate
        // transaction. A malformed late authored operation cannot leak an
        // earlier encounter activation, enemy readiness change, journal fact,
        // or door schedule.
        let mut candidate_session = self.session.clone();
        let mut candidate_scheduler = self.scheduler.clone();
        let mut candidate_events = self.events.clone();
        let mut candidate_journal = self.journal.clone();
        for encounter in candidates {
            let Some(activation) =
                EncounterService::prepare_activation(&candidate_session, player, encounter)
            else {
                continue;
            };
            EncounterService::run_activation_program(
                &mut candidate_session,
                &mut candidate_events,
                self.tick,
                activation,
            )?;
        }
        let events = drain_events_state(
            &mut candidate_session,
            &mut candidate_scheduler,
            &mut candidate_events,
            &mut candidate_journal,
            self.tick,
        )?;
        self.session = candidate_session;
        self.scheduler = candidate_scheduler;
        self.events = candidate_events;
        self.journal = candidate_journal;
        Ok(events)
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
        let projectile_checkpoint = self.projectiles.spawn_checkpoint();
        let mut receipt = match EnemyCombatService::attack(
            &mut candidate,
            scene,
            self.tick,
            player,
            &mut self.projectiles,
        ) {
            Ok(receipt) => receipt,
            Err(error) => {
                self.projectiles
                    .restore_spawn_checkpoint(projectile_checkpoint);
                return Err(error);
            }
        };
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

    pub fn integrate_player_frame(
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
        let interaction = InteractionService::prepare(&self.session, actor, target)?;
        let program_id = self
            .session
            .switch_program_bindings
            .get(&target)
            .cloned()
            .ok_or(RuntimeError::MissingSwitchProgramBinding { switch: target })?;
        let program = self
            .session
            .switch_programs
            .get(&program_id)
            .cloned()
            .ok_or_else(|| RuntimeError::MissingSwitchProgram {
                switch: target,
                program_id: program_id.clone(),
            })?;

        // A switch program owns one complete interaction transaction. Door
        // transitions can schedule auto-close work, so the candidate includes
        // scheduler, queued events, and journal as well as gameplay state.
        let mut candidate_session = self.session.clone();
        let mut candidate_scheduler = self.scheduler.clone();
        let mut candidate_events = self.events.clone();
        let mut candidate_journal = self.journal.clone();
        let mut staged_events = Vec::new();
        InteractionService::execute_program(
            &mut candidate_session,
            &mut candidate_scheduler,
            &mut staged_events,
            self.tick,
            interaction,
            &program,
        )?;
        candidate_events.extend(staged_events);
        let events = drain_events_state(
            &mut candidate_session,
            &mut candidate_scheduler,
            &mut candidate_events,
            &mut candidate_journal,
            self.tick,
        )?;
        self.session = candidate_session;
        self.scheduler = candidate_scheduler;
        self.events = candidate_events;
        self.journal = candidate_journal;
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
        let receipt = self.interact(actor, switch).map_err(|error| {
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
        Ok(receipt)
    }

    pub fn complete_level(
        &mut self,
        actor: EntityId,
        exit: EntityId,
    ) -> Result<Option<ProgressionFact>, RuntimeError> {
        ProgressionService::complete_level(&mut self.session, actor, exit, self.tick)
            .map_err(RuntimeError::LevelExit)
    }

    pub fn run_secret_phase(
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
        let resolution =
            CombatService::attack(&mut self.session, scene, self.tick, attacker, action)?;
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

    pub fn begin_fixed_tick(&mut self) {
        self.tick = self.tick.next();
    }

    pub fn run_scheduled_consequence_phase(&mut self) -> Result<Vec<GameEvent>, RuntimeError> {
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
        // An event wave can select a late encounter clear program. Preserve
        // the preceding primary mutation (for example enemy damage), but make
        // the wave's encounter/door/schedule/journal consequences atomic.
        let mut candidate_session = self.session.clone();
        let mut candidate_scheduler = self.scheduler.clone();
        let mut candidate_events = self.events.clone();
        let mut candidate_journal = self.journal.clone();
        let events = drain_events_state(
            &mut candidate_session,
            &mut candidate_scheduler,
            &mut candidate_events,
            &mut candidate_journal,
            self.tick,
        )?;
        self.session = candidate_session;
        self.scheduler = candidate_scheduler;
        self.events = candidate_events;
        self.journal = candidate_journal;
        Ok(events)
    }

    fn queue_door_transition(&mut self, door: EntityId, transition: DoorTransition) {
        queue_door_transition_state(
            &mut self.scheduler,
            &mut self.events,
            self.tick,
            door,
            transition,
        );
    }

    fn receipt(&self, events: Vec<GameEvent>) -> RuntimeReceipt {
        RuntimeReceipt {
            tick: self.tick,
            events,
            projection: self.session.entities.projection(),
        }
    }
}

fn drain_events_state(
    session: &mut GameSession,
    scheduler: &mut Scheduler,
    events: &mut VecDeque<GameEvent>,
    journal: &mut Vec<JournalEntry>,
    tick: Tick,
) -> Result<Vec<GameEvent>, RuntimeError> {
    let mut processed = Vec::new();
    while let Some(event) = events.pop_front() {
        if processed.len() >= MAX_EVENT_WAVE {
            events.clear();
            return Err(RuntimeError::EventWaveLimit {
                limit: MAX_EVENT_WAVE,
            });
        }
        journal.push(JournalEntry {
            tick,
            event: event.clone(),
        });
        match &event {
            // Switch program operations have already performed every bound
            // door request inside the candidate transaction. The event is
            // presentation/fact delivery only, never a fixed-effect trigger.
            GameEvent::SwitchActivated { .. } => {}
            GameEvent::EnemyDefeated { enemy, .. } => {
                EncounterService::run_clear_programs_for_enemy_defeat(
                    session, scheduler, events, tick, *enemy,
                )?
            }
            // Encounter programs perform explicit bound-exit requests while
            // this event remains a causal presentation/fact record.
            GameEvent::EncounterCleared { .. } => {}
            GameEvent::DoorOpened { .. }
            | GameEvent::DoorClosed { .. }
            | GameEvent::EncounterActivated { .. }
            | GameEvent::PlayerDied { .. } => {}
        }
        processed.push(event);
    }
    Ok(processed)
}

fn queue_door_transition_state(
    scheduler: &mut Scheduler,
    events: &mut VecDeque<GameEvent>,
    tick: Tick,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::door::DoorState;
    use crate::switch_program::{
        compile_switch_programs, StoredSwitchOperation, StoredSwitchPredicate, StoredSwitchProgram,
        StoredSwitchProgramNode,
    };
    use serde_json::Value;

    const E1M1: &str = include_str!("../../content/projects/doom-e1m1.project.json");
    const E1M1_PLAYER: EntityId = EntityId::new(1);
    const E1M1_DOOR_SWITCH: EntityId = EntityId::new(141);

    fn authored_door_runtime(auto_close_after_ticks: Option<u64>) -> GameRuntime {
        let mut project: Value = serde_json::from_str(E1M1).expect("E1M1 project");
        let entities = project["scenes"][0]["entities"]
            .as_array_mut()
            .expect("entry entities");
        let door = entities
            .iter_mut()
            .find(|entity| entity["id"] == E1M1_DOOR_SWITCH.raw())
            .expect("canonical E1M1 door/switch");
        let translation = door["translation"].clone();
        door["door"]["motionDurationTicks"] = 1.into();
        match auto_close_after_ticks {
            Some(ticks) => door["door"]["autoCloseAfterTicks"] = ticks.into(),
            None => {
                door["door"]
                    .as_object_mut()
                    .expect("door object")
                    .remove("autoCloseAfterTicks");
            }
        }
        let player = entities
            .iter_mut()
            .find(|entity| entity["id"] == E1M1_PLAYER.raw())
            .expect("player");
        player["translation"] = translation.clone();
        for node in project["scenes"][0]["authoredScene"]["nodes"]
            .as_array_mut()
            .expect("authored scene nodes")
            .iter_mut()
        {
            if node["id"] == E1M1_PLAYER.raw() {
                node["transform"]["translation"] = translation;
                break;
            }
        }
        GameRuntime::from_stored_project(&project.to_string()).expect("current authored fixture")
    }

    fn program(id: &str, operations: &[StoredSwitchOperation]) -> StoredSwitchProgram {
        StoredSwitchProgram {
            id: id.to_owned(),
            program: StoredSwitchProgramNode::When {
                predicate: StoredSwitchPredicate::SwitchAvailable,
                then_program: Box::new(StoredSwitchProgramNode::Sequence {
                    steps: operations
                        .iter()
                        .copied()
                        .map(|operation| StoredSwitchProgramNode::Operation { operation })
                        .collect(),
                }),
                otherwise_program: None,
            },
        }
    }

    fn install_switch_program(
        runtime: &mut GameRuntime,
        switch: EntityId,
        authored: StoredSwitchProgram,
    ) {
        let id = authored.id.clone();
        runtime.session.switch_programs = compile_switch_programs(&[authored]).unwrap();
        runtime.session.switch_program_bindings.insert(switch, id);
    }

    #[test]
    fn authored_switch_program_changes_the_door_transition_without_taking_rust_authority() {
        let mut canonical = authored_door_runtime(Some(4));
        let canonical_receipt = canonical.interact(E1M1_PLAYER, E1M1_DOOR_SWITCH).unwrap();
        assert!(matches!(
            canonical_receipt.events.as_slice(),
            [
                GameEvent::SwitchActivated { .. },
                GameEvent::DoorOpened { .. },
            ]
        ));
        assert_eq!(
            canonical.session().door(E1M1_DOOR_SWITCH).unwrap().state,
            DoorState::Opening
        );
        assert_eq!(canonical.scheduler.len(), 1);

        let mut variant = authored_door_runtime(Some(4));
        install_switch_program(
            &mut variant,
            E1M1_DOOR_SWITCH,
            program(
                "switch/feedback-only",
                &[
                    StoredSwitchOperation::RecordActivation,
                    StoredSwitchOperation::EmitInteractionFeedback,
                ],
            ),
        );
        let variant_receipt = variant.interact(E1M1_PLAYER, E1M1_DOOR_SWITCH).unwrap();
        assert!(matches!(
            variant_receipt.events.as_slice(),
            [GameEvent::SwitchActivated { .. }]
        ));
        assert_eq!(
            variant.session().door(E1M1_DOOR_SWITCH).unwrap().state,
            DoorState::Closed
        );
        assert_eq!(variant.scheduler.len(), 0);
        assert!(variant.session().gameplay_outcome().is_none());
    }

    #[test]
    fn late_switch_program_failure_rolls_back_door_schedule_events_and_journal() {
        let mut runtime = authored_door_runtime(Some(4));
        install_switch_program(
            &mut runtime,
            E1M1_DOOR_SWITCH,
            program(
                "switch/late-failure",
                &[
                    StoredSwitchOperation::RecordActivation,
                    StoredSwitchOperation::EmitInteractionFeedback,
                    StoredSwitchOperation::RequestOpenBoundDoor,
                    StoredSwitchOperation::RecordActivation,
                ],
            ),
        );

        assert!(matches!(
            runtime.interact(E1M1_PLAYER, E1M1_DOOR_SWITCH),
            Err(RuntimeError::SwitchProgram(
                SwitchProgramRejection::DuplicateActivation { .. }
            ))
        ));
        assert_eq!(
            runtime.session().door(E1M1_DOOR_SWITCH).unwrap().state,
            DoorState::Closed
        );
        assert_eq!(
            runtime
                .session()
                .switch(E1M1_DOOR_SWITCH)
                .unwrap()
                .activation_count,
            0
        );
        assert!(runtime.scheduler.is_empty());
        assert!(runtime.events.is_empty());
        assert!(runtime.journal.is_empty());
    }

    #[test]
    fn switch_program_binding_rejects_wrong_family_and_missing_without_mutation() {
        let mut runtime = authored_door_runtime(None);
        runtime
            .session
            .switch_program_bindings
            .insert(E1M1_DOOR_SWITCH, "hazard/nukage".to_owned());
        assert!(matches!(
            runtime.interact(E1M1_PLAYER, E1M1_DOOR_SWITCH),
            Err(RuntimeError::MissingSwitchProgram { .. })
        ));
        runtime
            .session
            .switch_program_bindings
            .remove(&E1M1_DOOR_SWITCH);
        assert!(matches!(
            runtime.interact(E1M1_PLAYER, E1M1_DOOR_SWITCH),
            Err(RuntimeError::MissingSwitchProgramBinding { .. })
        ));
        assert_eq!(
            runtime.session().door(E1M1_DOOR_SWITCH).unwrap().state,
            DoorState::Closed
        );
        assert_eq!(
            runtime
                .session()
                .switch(E1M1_DOOR_SWITCH)
                .unwrap()
                .activation_count,
            0
        );
        assert!(runtime.scheduler.is_empty());
        assert!(runtime.events.is_empty());
        assert!(runtime.journal.is_empty());
    }
}
