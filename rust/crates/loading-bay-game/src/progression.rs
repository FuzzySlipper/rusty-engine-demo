use core_ids::EntityId;
use core_time::Tick;
use engine_spatial::{
    KinematicTriggerDefinition, TriggerGeometrySource, TriggerOverlapFact, TriggerReconcileCause,
    TriggerVolumeDiagnostic, TriggerVolumeSystem,
};
use entity_state::EntityView;

use crate::door::{DoorService, DoorState, DoorTransition};
use crate::interaction::InteractionService;
use crate::inventory::{
    InventoryAction, InventoryCommand, InventoryFact, InventoryReceipt, InventoryRejection,
    InventoryService, ItemDefinitionId,
};
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
}

pub(crate) struct ProgressionService;

impl ProgressionService {
    pub(crate) fn activate_loading_bay_interlock(
        session: &mut GameSession,
        actor: EntityId,
        switch: EntityId,
    ) -> Result<crate::GameEvent, LoadingBayInterlockRejection> {
        if DamageService::is_dead(session, actor) {
            return Err(LoadingBayInterlockRejection::PlayerDefeated { actor });
        }
        let actor_translation = session
            .entities
            .view(actor)
            .map_err(|_| LoadingBayInterlockRejection::UnknownActor { actor })?
            .transform
            .ok_or(LoadingBayInterlockRejection::ActorMissingTransform { actor })?
            .translation;
        if !session.loading_bay_interlocks.contains_key(&switch) {
            return Err(LoadingBayInterlockRejection::UnknownInterlock { switch });
        }
        let switch_translation = session
            .entities
            .view(switch)
            .expect("admitted Loading Bay interlock entity")
            .transform
            .ok_or(LoadingBayInterlockRejection::InterlockMissingTransform { switch })?
            .translation;
        if (actor_translation - switch_translation).length_squared()
            > LOADING_BAY_INTERLOCK_ACTIVATION_RADIUS * LOADING_BAY_INTERLOCK_ACTIVATION_RADIUS
        {
            return Err(LoadingBayInterlockRejection::OutOfRange { actor, switch });
        }
        InteractionService::interact(session, actor, switch)
            .map_err(|_| LoadingBayInterlockRejection::InteractionFailed { switch })
    }

    pub(crate) fn secret_trigger_system(session: &GameSession) -> TriggerVolumeSystem {
        TriggerVolumeSystem::new(session.secret_regions.keys().copied().map(|secret| {
            KinematicTriggerDefinition::new(secret, SECRET_TRIGGER_SCOPE, ["secret"])
                .with_geometry_source(TriggerGeometrySource::EntityBounds)
        }))
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
        let actor_translation = session
            .entities
            .view(actor)
            .map_err(|_| DoorAccessRejection::UnknownActor { actor })?
            .transform
            .ok_or(DoorAccessRejection::ActorMissingTransform { actor })?
            .translation;
        let Some(access) = session.door_access.get(&door).cloned() else {
            return Err(DoorAccessRejection::UnknownDoor { door });
        };
        if session
            .doors
            .get(&door)
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
                InventoryService::apply(
                    &mut candidate,
                    actor,
                    InventoryCommand {
                        sequence,
                        action: InventoryAction::Consume {
                            item: access.required_key.clone(),
                            quantity: 1,
                        },
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
        let mut candidate_triggers = triggers.clone();
        let trigger_receipt = candidate_triggers
            .reconcile(
                &session.entities,
                tick.raw(),
                TriggerReconcileCause::Movement,
            )
            .map_err(|error| SecretRejection::Trigger {
                diagnostics: error.diagnostics,
            })?;
        let mut facts = Vec::new();
        let secret_ids = session.secret_regions.keys().copied().collect::<Vec<_>>();
        for secret in secret_ids {
            let component = session
                .secret_regions
                .get(&secret)
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
            facts.push(ProgressionFact::SecretDiscovered {
                secret,
                actor,
                discovered_at: tick,
                presentation: component.config.presentation.clone(),
            });
        }
        let mut candidate_session = session.clone();
        for fact in &facts {
            let ProgressionFact::SecretDiscovered {
                secret,
                actor,
                discovered_at,
                ..
            } = fact
            else {
                unreachable!("secret reconciliation only stages secret facts");
            };
            candidate_session
                .secret_regions
                .get_mut(secret)
                .expect("staged secret remains admitted")
                .state = SecretRegionState::Discovered {
                actor: *actor,
                discovered_at: *discovered_at,
            };
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
        let actor_translation = session
            .entities
            .view(actor)
            .map_err(|_| LevelExitRejection::UnknownActor { actor })?
            .transform
            .ok_or(LevelExitRejection::ActorMissingTransform { actor })?
            .translation;
        let Some(component) = session.level_exits.get(&exit).cloned() else {
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
        session
            .level_exits
            .get_mut(&exit)
            .expect("level exit remains admitted")
            .state = LevelExitState::Completed {
            actor,
            completed_at: tick,
        };
        Ok(Some(ProgressionFact::LevelCompleted {
            exit,
            actor,
            completed_at: tick,
            presentation: component.config.presentation,
        }))
    }
}

fn valid_activation_radius(radius: f32) -> bool {
    radius.is_finite() && radius > 0.0 && radius <= MAX_PROGRESSION_ACTIVATION_RADIUS
}

fn valid_presentation(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_PROGRESSION_PRESENTATION_BYTES
}
