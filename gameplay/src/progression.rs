use std::cell::RefCell;

use rusty_engine::core_ids::EntityId;
use rusty_engine::core_time::Tick;
use rusty_engine::engine_spatial::{
    KinematicTriggerDefinition, TriggerGeometrySource, TriggerOverlapFact, TriggerReconcileCause,
    TriggerVolumeDiagnostic, TriggerVolumeSystem,
};
use rusty_engine::entity_state::EntityView;

use crate::door::{DoorComponent, DoorService, DoorState, DoorTransition};
use crate::inventory::{
    apply_standard_stack, InventoryAction, InventoryFact, InventoryReceipt, InventoryRejection,
    ItemDefinitionId,
};
use crate::level_exit_program::{
    execute_level_exit_program, LevelExitOperation, LevelExitPredicate,
};
use crate::secret_program::{execute_secret_program, SecretOperation, SecretPredicate};
use crate::session::GameSession;
use crate::vitality::DamageService;

pub const SECRET_TRIGGER_SCOPE: &str = "loading-bay.secret";
pub const MAX_PROGRESSION_ACTIVATION_RADIUS: f32 = 32.0;
pub const MAX_PROGRESSION_PRESENTATION_BYTES: usize = 160;
pub const MAX_SECRET_OVERLAP_SUBJECTS: usize = 128;
pub const LOADING_BAY_INTERLOCK_ACTIVATION_RADIUS: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredKeyPolicy {
    Retain,
    Consume,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoorAccessConfig {
    pub required_key: ItemDefinitionId,
    pub key_policy: RequiredKeyPolicy,
    pub activation_radius: f32,
    pub denied_presentation: String,
}

impl DoorAccessConfig {
    pub(crate) fn is_valid(&self) -> bool {
        valid_activation_radius(self.activation_radius)
            && valid_presentation(&self.denied_presentation)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoorAccessView {
    pub door: EntityId,
    pub config: DoorAccessConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadingBayInterlockConfig {
    pub close_door: EntityId,
    pub open_door: EntityId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadingBayInterlockView {
    pub switch: EntityId,
    pub config: LoadingBayInterlockConfig,
    pub entity_view: EntityView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretRegionState {
    Undiscovered,
    Discovered {
        actor: EntityId,
        discovered_at: Tick,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRegionConfig {
    pub presentation: String,
}

impl SecretRegionConfig {
    pub(crate) fn is_valid(&self) -> bool {
        valid_presentation(&self.presentation)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRegionComponent {
    pub config: SecretRegionConfig,
    pub state: SecretRegionState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SecretRegionView {
    pub entity: EntityId,
    pub config: SecretRegionConfig,
    pub state: SecretRegionState,
    pub entity_view: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LevelExitConfig {
    pub activation_radius: f32,
    pub presentation: String,
}

impl LevelExitConfig {
    pub(crate) fn is_valid(&self) -> bool {
        valid_activation_radius(self.activation_radius) && valid_presentation(&self.presentation)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelExitState {
    Available,
    Completed { actor: EntityId, completed_at: Tick },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LevelExitComponent {
    pub config: LevelExitConfig,
    pub state: LevelExitState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LevelExitView {
    pub entity: EntityId,
    pub config: LevelExitConfig,
    pub state: LevelExitState,
    pub entity_view: EntityView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressionFact {
    DoorAccessGranted {
        door: EntityId,
        actor: EntityId,
        required_key: ItemDefinitionId,
        key_policy: RequiredKeyPolicy,
        inventory_facts: Vec<InventoryFact>,
    },
    SecretDiscovered {
        secret: EntityId,
        actor: EntityId,
        discovered_at: Tick,
        presentation: String,
    },
    LevelCompleted {
        exit: EntityId,
        actor: EntityId,
        completed_at: Tick,
        presentation: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoorAccessReceipt {
    pub(crate) transition: Option<DoorTransition>,
    pub inventory: Option<InventoryReceipt>,
    pub fact: Option<ProgressionFact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoorAccessRejection {
    UnknownDoor {
        door: EntityId,
    },
    UnknownActor {
        actor: EntityId,
    },
    ActorMissingTransform {
        actor: EntityId,
    },
    PlayerDefeated {
        actor: EntityId,
    },
    OutOfRange {
        actor: EntityId,
        door: EntityId,
    },
    MissingRequiredKey {
        door: EntityId,
        required_key: ItemDefinitionId,
        presentation: String,
    },
    InventorySequenceOverflow {
        actor: EntityId,
    },
    Inventory(InventoryRejection),
    WorldMutationFailed {
        door: EntityId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LevelExitRejection {
    UnknownExit { exit: EntityId },
    UnknownActor { actor: EntityId },
    ActorMissingTransform { actor: EntityId },
    PlayerDefeated { actor: EntityId },
    OutOfRange { actor: EntityId, exit: EntityId },
    MissingProgramBinding { exit: EntityId },
    MissingProgram { exit: EntityId, program_id: String },
    CompletionAlreadyRecorded { exit: EntityId },
    CompletionPresentationBeforeRecord { exit: EntityId },
    CompletionPresentationAlreadyEmitted { exit: EntityId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadingBayInterlockRejection {
    UnknownInterlock { switch: EntityId },
    UnknownActor { actor: EntityId },
    ActorMissingTransform { actor: EntityId },
    InterlockMissingTransform { switch: EntityId },
    PlayerDefeated { actor: EntityId },
    OutOfRange { actor: EntityId, switch: EntityId },
    InteractionFailed { switch: EntityId },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SecretPhaseReceipt {
    pub trigger_facts: Vec<TriggerOverlapFact>,
    pub facts: Vec<ProgressionFact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretRejection {
    Trigger {
        diagnostics: Vec<TriggerVolumeDiagnostic>,
    },
    MissingProgramBinding {
        secret: EntityId,
    },
    MissingProgram {
        secret: EntityId,
        program_id: String,
    },
    DiscoveryAlreadyRecorded {
        secret: EntityId,
    },
    SecretPresentationBeforeRecord {
        secret: EntityId,
    },
    SecretPresentationAlreadyEmitted {
        secret: EntityId,
    },
}

pub(crate) struct ProgressionService;

impl ProgressionService {
    pub(crate) fn secret_trigger_system(session: &GameSession) -> TriggerVolumeSystem {
        TriggerVolumeSystem::new(
            session
                .facts::<SecretRegionComponent>()
                .into_iter()
                .map(|(secret, _)| secret)
                .map(|secret| {
                    KinematicTriggerDefinition::new(secret, SECRET_TRIGGER_SCOPE, ["secret"])
                        .with_geometry_source(TriggerGeometrySource::EntityBounds)
                }),
        )
        .expect("admitted secret trigger identities are fixed and valid")
    }

    pub(crate) fn open_keyed_door(
        session: &mut GameSession,
        actor: EntityId,
        door: EntityId,
    ) -> Result<DoorAccessReceipt, DoorAccessRejection> {
        if DamageService::is_dead(session, actor) {
            return Err(DoorAccessRejection::PlayerDefeated { actor });
        }
        session
            .entities
            .view(actor)
            .map_err(|_| DoorAccessRejection::UnknownActor { actor })?;
        let actor_translation = session
            .gameplay_translation(actor)
            .ok_or(DoorAccessRejection::ActorMissingTransform { actor })?;
        let Some(access) = session.fact::<DoorAccessConfig>(door) else {
            return Err(DoorAccessRejection::UnknownDoor { door });
        };
        if session
            .fact::<DoorComponent>(door)
            .is_some_and(|component| component.state == DoorState::Open)
        {
            return Ok(DoorAccessReceipt {
                transition: None,
                inventory: None,
                fact: None,
            });
        }
        let door_translation = session
            .entities
            .view(door)
            .map_err(|_| DoorAccessRejection::UnknownDoor { door })?
            .transform
            .expect("admitted door has a transform")
            .translation;
        if (actor_translation - door_translation).length_squared()
            > access.activation_radius * access.activation_radius
        {
            return Err(DoorAccessRejection::OutOfRange { actor, door });
        }
        let owned = session.inventory(actor).is_some_and(|inventory| {
            inventory
                .stacks
                .iter()
                .any(|stack| stack.item == access.required_key && stack.quantity > 0)
        });
        if !owned {
            return Err(DoorAccessRejection::MissingRequiredKey {
                door,
                required_key: access.required_key,
                presentation: access.denied_presentation,
            });
        }

        let mut candidate = session.clone();
        let inventory = if access.key_policy == RequiredKeyPolicy::Consume {
            let sequence = candidate
                .inventories
                .get(&actor)
                .and_then(|inventory| inventory.last_applied_command_sequence)
                .map_or(Some(1), |sequence| sequence.checked_add(1))
                .ok_or(DoorAccessRejection::InventorySequenceOverflow { actor })?;
            Some(
                apply_standard_stack(
                    &mut candidate,
                    actor,
                    sequence,
                    InventoryAction::Consume {
                        item: access.required_key.clone(),
                        quantity: 1,
                    },
                )
                .map_err(DoorAccessRejection::Inventory)?,
            )
        } else {
            None
        };
        let transition = DoorService::open(&mut candidate, door)
            .map_err(|_| DoorAccessRejection::WorldMutationFailed { door })?;
        let fact = transition
            .as_ref()
            .map(|_| ProgressionFact::DoorAccessGranted {
                door,
                actor,
                required_key: access.required_key,
                key_policy: access.key_policy,
                inventory_facts: inventory
                    .as_ref()
                    .map_or_else(Vec::new, |receipt| receipt.facts.clone()),
            });
        *session = candidate;
        Ok(DoorAccessReceipt {
            transition,
            inventory,
            fact,
        })
    }

    pub(crate) fn reconcile_secrets(
        session: &mut GameSession,
        triggers: &mut TriggerVolumeSystem,
        actor: EntityId,
        tick: Tick,
    ) -> Result<SecretPhaseReceipt, SecretRejection> {
        // Both spatial reconciliation and every program run are candidate
        // work. A late source-order rejection cannot leak an earlier secret's
        // once-state or a trigger revision.
        let mut candidate_session = session.clone();
        let mut candidate_triggers = triggers.clone();
        let trigger_receipt = candidate_triggers
            .reconcile(
                &candidate_session.entities,
                tick.raw(),
                TriggerReconcileCause::Movement,
            )
            .map_err(|error| SecretRejection::Trigger {
                diagnostics: error.diagnostics,
            })?;
        let mut facts = Vec::new();
        let secret_ids = candidate_session
            .facts::<SecretRegionComponent>()
            .into_iter()
            .map(|(secret, _)| secret)
            .collect::<Vec<_>>();
        for secret in secret_ids {
            let component = candidate_session
                .fact::<SecretRegionComponent>(secret)
                .expect("secret identity came from admitted state");
            if component.state != SecretRegionState::Undiscovered {
                continue;
            }
            let overlaps = candidate_triggers
                .current_overlaps(secret, MAX_SECRET_OVERLAP_SUBJECTS)
                .map_err(|error| SecretRejection::Trigger {
                    diagnostics: error.diagnostics,
                })?;
            if !overlaps.subjects.contains(&actor) {
                continue;
            }
            let program_id = candidate_session
                .secret_program_bindings
                .get(&secret)
                .cloned()
                .ok_or(SecretRejection::MissingProgramBinding { secret })?;
            let program = candidate_session
                .secret_programs
                .get(&program_id)
                .cloned()
                .ok_or_else(|| SecretRejection::MissingProgram {
                    secret,
                    program_id: program_id.clone(),
                })?;
            let context = RefCell::new(SecretProgramContext {
                session: &mut candidate_session,
                triggers: &mut candidate_triggers,
                actor,
                secret,
                tick,
                discovery_recorded: false,
                presentation_emitted: false,
                facts: Vec::new(),
            });
            execute_secret_program(
                &program,
                &mut |predicate| context.borrow_mut().predicate(predicate),
                &mut |operation| context.borrow_mut().operation(operation),
            )?;
            facts.extend(context.into_inner().facts);
        }
        *session = candidate_session;
        *triggers = candidate_triggers;
        Ok(SecretPhaseReceipt {
            trigger_facts: trigger_receipt.facts,
            facts,
        })
    }

    pub(crate) fn complete_level(
        session: &mut GameSession,
        actor: EntityId,
        exit: EntityId,
        tick: Tick,
    ) -> Result<Option<ProgressionFact>, LevelExitRejection> {
        if DamageService::is_dead(session, actor) {
            return Err(LevelExitRejection::PlayerDefeated { actor });
        }
        session
            .entities
            .view(actor)
            .map_err(|_| LevelExitRejection::UnknownActor { actor })?;
        let actor_translation = session
            .gameplay_translation(actor)
            .ok_or(LevelExitRejection::ActorMissingTransform { actor })?;
        let Some(component) = session.fact::<LevelExitComponent>(exit) else {
            return Err(LevelExitRejection::UnknownExit { exit });
        };
        if matches!(component.state, LevelExitState::Completed { .. }) {
            return Ok(None);
        }
        let exit_translation = session
            .entities
            .view(exit)
            .expect("admitted level exit entity")
            .transform
            .expect("admitted level exit transform")
            .translation;
        if (actor_translation - exit_translation).length_squared()
            > component.config.activation_radius * component.config.activation_radius
        {
            return Err(LevelExitRejection::OutOfRange { actor, exit });
        }
        let program_id = session
            .level_exit_program_bindings
            .get(&exit)
            .cloned()
            .ok_or(LevelExitRejection::MissingProgramBinding { exit })?;
        let program = session
            .level_exit_programs
            .get(&program_id)
            .cloned()
            .ok_or_else(|| LevelExitRejection::MissingProgram {
                exit,
                program_id: program_id.clone(),
            })?;
        let mut candidate = session.clone();
        let context = RefCell::new(LevelExitProgramContext {
            session: &mut candidate,
            actor,
            exit,
            tick,
            completion_recorded: false,
            presentation_emitted: false,
            fact: None,
        });
        execute_level_exit_program(
            &program,
            &mut |predicate| context.borrow_mut().predicate(predicate),
            &mut |operation| context.borrow_mut().operation(operation),
        )?;
        let fact = context.into_inner().fact;
        *session = candidate;
        Ok(fact)
    }
}

struct SecretProgramContext<'a> {
    session: &'a mut GameSession,
    triggers: &'a mut TriggerVolumeSystem,
    actor: EntityId,
    secret: EntityId,
    tick: Tick,
    discovery_recorded: bool,
    presentation_emitted: bool,
    facts: Vec<ProgressionFact>,
}

impl SecretProgramContext<'_> {
    fn predicate(&mut self, predicate: SecretPredicate) -> Result<bool, SecretRejection> {
        match predicate {
            SecretPredicate::SecretRegionEntered => self
                .triggers
                .current_overlaps(self.secret, MAX_SECRET_OVERLAP_SUBJECTS)
                .map(|overlaps| overlaps.subjects.contains(&self.actor))
                .map_err(|error| SecretRejection::Trigger {
                    diagnostics: error.diagnostics,
                }),
            SecretPredicate::SecretUndiscovered => Ok(self
                .session
                .fact::<SecretRegionComponent>(self.secret)
                .is_some_and(|component| component.state == SecretRegionState::Undiscovered)),
        }
    }

    fn operation(&mut self, operation: SecretOperation) -> Result<(), SecretRejection> {
        match operation {
            SecretOperation::RecordDiscovery => {
                if self.discovery_recorded
                    || !self
                        .session
                        .fact::<SecretRegionComponent>(self.secret)
                        .is_some_and(|component| component.state == SecretRegionState::Undiscovered)
                {
                    return Err(SecretRejection::DiscoveryAlreadyRecorded {
                        secret: self.secret,
                    });
                }
                let mut secret = self
                    .session
                    .fact::<SecretRegionComponent>(self.secret)
                    .expect("secret identity came from admitted state");
                secret.state = SecretRegionState::Discovered {
                    actor: self.actor,
                    discovered_at: self.tick,
                };
                self.session.store_fact(self.secret, secret);
                self.discovery_recorded = true;
                Ok(())
            }
            SecretOperation::EmitSecretPresentation => {
                if !self.discovery_recorded {
                    return Err(SecretRejection::SecretPresentationBeforeRecord {
                        secret: self.secret,
                    });
                }
                if self.presentation_emitted {
                    return Err(SecretRejection::SecretPresentationAlreadyEmitted {
                        secret: self.secret,
                    });
                }
                let component = self
                    .session
                    .fact::<SecretRegionComponent>(self.secret)
                    .expect("secret identity came from admitted state");
                let SecretRegionState::Discovered {
                    actor,
                    discovered_at,
                } = component.state
                else {
                    return Err(SecretRejection::SecretPresentationBeforeRecord {
                        secret: self.secret,
                    });
                };
                self.facts.push(ProgressionFact::SecretDiscovered {
                    secret: self.secret,
                    actor,
                    discovered_at,
                    presentation: component.config.presentation.clone(),
                });
                self.presentation_emitted = true;
                Ok(())
            }
        }
    }
}

struct LevelExitProgramContext<'a> {
    session: &'a mut GameSession,
    actor: EntityId,
    exit: EntityId,
    tick: Tick,
    completion_recorded: bool,
    presentation_emitted: bool,
    fact: Option<ProgressionFact>,
}

impl LevelExitProgramContext<'_> {
    fn predicate(&mut self, predicate: LevelExitPredicate) -> Result<bool, LevelExitRejection> {
        match predicate {
            LevelExitPredicate::ExitAvailable => Ok(self
                .session
                .fact::<LevelExitComponent>(self.exit)
                .is_some_and(|component| component.state == LevelExitState::Available)),
        }
    }

    fn operation(&mut self, operation: LevelExitOperation) -> Result<(), LevelExitRejection> {
        match operation {
            LevelExitOperation::RecordCompletion => {
                if self.completion_recorded
                    || !self
                        .session
                        .fact::<LevelExitComponent>(self.exit)
                        .is_some_and(|component| component.state == LevelExitState::Available)
                {
                    return Err(LevelExitRejection::CompletionAlreadyRecorded { exit: self.exit });
                }
                let mut exit_component = self
                    .session
                    .fact::<LevelExitComponent>(self.exit)
                    .expect("level exit identity came from admitted state");
                exit_component.state = LevelExitState::Completed {
                    actor: self.actor,
                    completed_at: self.tick,
                };
                self.session.store_fact(self.exit, exit_component);
                self.completion_recorded = true;
                Ok(())
            }
            LevelExitOperation::EmitCompletionPresentation => {
                if !self.completion_recorded {
                    return Err(LevelExitRejection::CompletionPresentationBeforeRecord {
                        exit: self.exit,
                    });
                }
                if self.presentation_emitted {
                    return Err(LevelExitRejection::CompletionPresentationAlreadyEmitted {
                        exit: self.exit,
                    });
                }
                let component = self
                    .session
                    .fact::<LevelExitComponent>(self.exit)
                    .expect("level exit identity came from admitted state");
                let LevelExitState::Completed {
                    actor,
                    completed_at,
                } = component.state
                else {
                    return Err(LevelExitRejection::CompletionPresentationBeforeRecord {
                        exit: self.exit,
                    });
                };
                self.fact = Some(ProgressionFact::LevelCompleted {
                    exit: self.exit,
                    actor,
                    completed_at,
                    presentation: component.config.presentation.clone(),
                });
                self.presentation_emitted = true;
                Ok(())
            }
        }
    }
}

fn valid_activation_radius(radius: f32) -> bool {
    radius.is_finite() && radius > 0.0 && radius <= MAX_PROGRESSION_ACTIVATION_RADIUS
}

fn valid_presentation(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_PROGRESSION_PRESENTATION_BYTES
}
