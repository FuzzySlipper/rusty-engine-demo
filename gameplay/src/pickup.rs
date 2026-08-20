use rusty_engine::core_ids::EntityId;
use rusty_engine::engine_spatial::{
    KinematicTriggerDefinition, TriggerGeometrySource, TriggerOverlapFact, TriggerReconcileCause,
    TriggerReconcileReceipt, TriggerVolumeDiagnostic, TriggerVolumeSystem,
};
use rusty_engine::entity_state::{EntityAuthoringFact, EntityAuthoringService};

use crate::inventory::{
    InventoryAction, InventoryCommand, InventoryFact, InventoryReceipt, InventoryRejection,
    InventoryService, ItemDefinitionId,
};
use crate::session::GameSession;
use crate::vitality::{DamageService, VitalityFact, VitalityRejection};

pub const PICKUP_TRIGGER_SCOPE: &str = "loading-bay.pickup";
pub const MAX_PICKUP_OVERLAP_SUBJECTS: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickupConfig {
    pub item: ItemDefinitionId,
    pub quantity: u32,
    pub starter_ammunition: Option<crate::InventoryStack>,
}

impl PickupConfig {
    pub fn new(item: ItemDefinitionId, quantity: u32) -> Self {
        Self {
            item,
            quantity,
            starter_ammunition: None,
        }
    }

    pub fn with_starter_ammunition(
        mut self,
        starter_ammunition: Option<crate::InventoryStack>,
    ) -> Self {
        self.starter_ammunition = starter_ammunition;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickupCollectionCause {
    Overlap {
        trigger_revision: u64,
    },
    Interaction {
        connection_generation: u64,
        command_sequence: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickupState {
    Dormant,
    Available,
    Collected {
        actor: EntityId,
        collected_at_tick: u64,
        cause: PickupCollectionCause,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickupComponent {
    pub config: PickupConfig,
    pub state: PickupState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickupView {
    pub entity: EntityId,
    pub config: PickupConfig,
    pub state: PickupState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickupDisposition {
    Collected,
    Repeated,
    AlreadyCollected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickupFact {
    Collected {
        pickup: EntityId,
        actor: EntityId,
        item: ItemDefinitionId,
        quantity: u32,
        collected_at_tick: u64,
        inventory_facts: Vec<InventoryFact>,
        vitality_facts: Vec<VitalityFact>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickupPresentationCue {
    pub pickup: EntityId,
    pub actor: EntityId,
    pub item: ItemDefinitionId,
    pub quantity: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickupCollectionCommand {
    pub pickup: EntityId,
    pub actor: EntityId,
    pub tick: u64,
    pub cause: PickupCollectionCause,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PickupReceipt {
    pub disposition: PickupDisposition,
    pub before: PickupView,
    pub after: PickupView,
    pub inventory: Vec<InventoryReceipt>,
    pub entity_facts: Vec<EntityAuthoringFact>,
    pub trigger_facts: Vec<TriggerOverlapFact>,
    pub facts: Vec<PickupFact>,
    pub vitality_facts: Vec<VitalityFact>,
    pub cues: Vec<PickupPresentationCue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickupRejection {
    UnknownPickup {
        pickup: EntityId,
    },
    NotMaterialized {
        pickup: EntityId,
    },
    PlayerDefeated {
        actor: EntityId,
    },
    NotOverlapping {
        pickup: EntityId,
        actor: EntityId,
        trigger_revision: u64,
    },
    InventorySequenceOverflow {
        actor: EntityId,
    },
    Inventory(InventoryRejection),
    Vitality(VitalityRejection),
    WorldMutationFailed {
        pickup: EntityId,
    },
    Trigger {
        diagnostics: Vec<TriggerVolumeDiagnostic>,
    },
}

impl std::fmt::Display for PickupRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for PickupRejection {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickupRejectedAttempt {
    pub pickup: EntityId,
    pub reason: PickupRejection,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PickupPhaseReceipt {
    pub trigger_revision: u64,
    pub trigger_facts: Vec<TriggerOverlapFact>,
    pub collected: Vec<PickupReceipt>,
    pub rejected: Vec<PickupRejectedAttempt>,
}

pub struct PickupService;

impl PickupService {
    pub(crate) fn trigger_system(session: &GameSession) -> TriggerVolumeSystem {
        TriggerVolumeSystem::new(session.pickups.keys().copied().map(|pickup| {
            KinematicTriggerDefinition::new(pickup, PICKUP_TRIGGER_SCOPE, ["pickup"])
                .with_geometry_source(TriggerGeometrySource::EntityBounds)
        }))
        .expect("admitted pickup trigger identities are fixed and valid")
    }

    pub fn collect(
        session: &mut GameSession,
        triggers: &mut TriggerVolumeSystem,
        command: PickupCollectionCommand,
    ) -> Result<PickupReceipt, PickupRejection> {
        if DamageService::is_dead(session, command.actor) {
            return Err(PickupRejection::PlayerDefeated {
                actor: command.actor,
            });
        }
        let Some(component) = session.pickups.get(&command.pickup) else {
            return Err(PickupRejection::UnknownPickup {
                pickup: command.pickup,
            });
        };
        let before = pickup_view(command.pickup, component);
        if component.state == PickupState::Dormant {
            return Err(PickupRejection::NotMaterialized {
                pickup: command.pickup,
            });
        }
        if let PickupState::Collected {
            actor,
            collected_at_tick: _,
            cause,
        } = &component.state
        {
            let disposition = if *actor == command.actor && *cause == command.cause {
                PickupDisposition::Repeated
            } else {
                PickupDisposition::AlreadyCollected
            };
            return Ok(PickupReceipt {
                disposition,
                before: before.clone(),
                after: before,
                inventory: Vec::new(),
                entity_facts: Vec::new(),
                trigger_facts: Vec::new(),
                facts: Vec::new(),
                vitality_facts: Vec::new(),
                cues: Vec::new(),
            });
        }

        let overlap = triggers
            .current_overlaps(command.pickup, MAX_PICKUP_OVERLAP_SUBJECTS)
            .map_err(|error| PickupRejection::Trigger {
                diagnostics: error.diagnostics,
            })?;
        if !overlap.subjects.contains(&command.actor) {
            return Err(PickupRejection::NotOverlapping {
                pickup: command.pickup,
                actor: command.actor,
                trigger_revision: overlap.revision,
            });
        }

        let mut candidate_session = session.clone();
        let mut candidate_triggers = triggers.clone();
        let inventory_sequence = candidate_session
            .inventories
            .get(&command.actor)
            .and_then(|inventory| inventory.last_applied_command_sequence)
            .map_or(Some(1), |sequence| sequence.checked_add(1))
            .ok_or(PickupRejection::InventorySequenceOverflow {
                actor: command.actor,
            })?;
        let duplicate_weapon_with_ammunition = component
            .config
            .starter_ammunition
            .as_ref()
            .is_some_and(|_| {
                candidate_session
                    .item_definitions
                    .get(&component.config.item)
                    .is_some_and(|definition| matches!(definition.kind, crate::ItemKind::Weapon(_)))
                    && candidate_session
                        .inventory(command.actor)
                        .is_some_and(|inventory| {
                            inventory
                                .stacks
                                .iter()
                                .any(|stack| stack.item == component.config.item)
                        })
            });
        let mut inventory = Vec::new();
        let mut next_sequence = inventory_sequence;
        if !duplicate_weapon_with_ammunition {
            inventory.push(
                InventoryService::apply(
                    &mut candidate_session,
                    command.actor,
                    InventoryCommand {
                        sequence: next_sequence,
                        action: InventoryAction::Grant {
                            item: component.config.item.clone(),
                            quantity: component.config.quantity,
                        },
                    },
                )
                .map_err(PickupRejection::Inventory)?,
            );
            next_sequence =
                next_sequence
                    .checked_add(1)
                    .ok_or(PickupRejection::InventorySequenceOverflow {
                        actor: command.actor,
                    })?;
        }
        if let Some(starter) = &component.config.starter_ammunition {
            inventory.push(
                InventoryService::apply(
                    &mut candidate_session,
                    command.actor,
                    InventoryCommand {
                        sequence: next_sequence,
                        action: InventoryAction::Grant {
                            item: starter.item.clone(),
                            quantity: starter.quantity,
                        },
                    },
                )
                .map_err(PickupRejection::Inventory)?,
            );
        }
        let mut vitality_facts = Vec::new();
        let immediate_vitality_kind = candidate_session
            .item_definitions
            .get(&component.config.item)
            .map(|definition| definition.kind.clone());
        match immediate_vitality_kind {
            Some(crate::inventory::ItemKind::Armor { .. }) => {
                let vitality = DamageService::grant_armor(
                    &mut candidate_session,
                    command.actor,
                    component.config.item.clone(),
                )
                .map_err(PickupRejection::Vitality)?;
                vitality_facts.extend(vitality.facts);
                inventory.extend(vitality.inventory);
            }
            Some(crate::inventory::ItemKind::HealthSupply {
                automatic_use: true,
                ..
            }) => {
                let vitality = DamageService::use_health_supply(
                    &mut candidate_session,
                    command.actor,
                    component.config.item.clone(),
                )
                .map_err(PickupRejection::Vitality)?;
                vitality_facts.extend(vitality.facts);
                inventory.extend(vitality.inventory);
            }
            _ => {}
        }
        let entity_revision = candidate_session.entities.revision();
        let entity_receipt = EntityAuthoringService
            .destroy(
                &mut candidate_session.entities,
                entity_revision,
                command.pickup,
            )
            .map_err(|_| PickupRejection::WorldMutationFailed {
                pickup: command.pickup,
            })?;
        let candidate_component = candidate_session
            .pickups
            .get_mut(&command.pickup)
            .expect("pickup was validated before staging");
        candidate_component.state = PickupState::Collected {
            actor: command.actor,
            collected_at_tick: command.tick,
            cause: command.cause.clone(),
        };
        let after = pickup_view(command.pickup, candidate_component);
        let trigger_receipt = candidate_triggers
            .reconcile(
                &candidate_session.entities,
                command.tick,
                TriggerReconcileCause::LifecycleChanged,
            )
            .map_err(|error| PickupRejection::Trigger {
                diagnostics: error.diagnostics,
            })?;
        let trigger_facts = prune_unavailable_pickup_overlaps(
            &candidate_session,
            &mut candidate_triggers,
            trigger_receipt.facts,
        );
        let item = component.config.item.clone();
        let quantity = component.config.quantity;
        let inventory_facts = inventory
            .iter()
            .flat_map(|receipt| receipt.facts.iter().cloned())
            .collect();

        *session = candidate_session;
        *triggers = candidate_triggers;
        Ok(PickupReceipt {
            disposition: PickupDisposition::Collected,
            before,
            after,
            inventory,
            entity_facts: entity_receipt.facts,
            trigger_facts,
            facts: vec![PickupFact::Collected {
                pickup: command.pickup,
                actor: command.actor,
                item: item.clone(),
                quantity,
                collected_at_tick: command.tick,
                inventory_facts,
                vitality_facts: vitality_facts.clone(),
            }],
            vitality_facts,
            cues: vec![PickupPresentationCue {
                pickup: command.pickup,
                actor: command.actor,
                item,
                quantity,
            }],
        })
    }

    pub(crate) fn reconcile_and_collect(
        session: &mut GameSession,
        triggers: &mut TriggerVolumeSystem,
        actor: EntityId,
        tick: u64,
    ) -> Result<PickupPhaseReceipt, PickupRejection> {
        let TriggerReconcileReceipt {
            revision, facts, ..
        } = triggers
            .reconcile(&session.entities, tick, TriggerReconcileCause::Movement)
            .map_err(|error| PickupRejection::Trigger {
                diagnostics: error.diagnostics,
            })?;
        let facts = prune_unavailable_pickup_overlaps(session, triggers, facts);
        let entered = facts
            .iter()
            .filter(|fact| {
                fact.kind == rusty_engine::engine_spatial::TriggerOverlapFactKind::Enter
                    && fact.pair.subject_id() == actor
                    && session
                        .pickups
                        .get(&fact.pair.trigger_id())
                        .is_some_and(|pickup| pickup.state == PickupState::Available)
            })
            .map(|fact| fact.pair.trigger_id())
            .collect::<Vec<_>>();
        let mut collected = Vec::new();
        let mut rejected = Vec::new();
        for pickup in entered {
            match Self::collect(
                session,
                triggers,
                PickupCollectionCommand {
                    pickup,
                    actor,
                    tick,
                    cause: PickupCollectionCause::Overlap {
                        trigger_revision: revision,
                    },
                },
            ) {
                Ok(receipt) => collected.push(receipt),
                Err(reason) => rejected.push(PickupRejectedAttempt { pickup, reason }),
            }
        }
        Ok(PickupPhaseReceipt {
            trigger_revision: revision,
            trigger_facts: facts,
            collected,
            rejected,
        })
    }
}

/// Pickup trigger definitions are permanent admission-time identities: dormant
/// enemy drops need their definitions intact so they can be materialized after
/// an enemy dies. Their overlaps are live state, however, and only available
/// pickups may retain them. Keep that distinction at the gameplay boundary so
/// snapshots and trigger facts never describe a collectable interaction for a
/// dormant or already-collected pickup.
fn prune_unavailable_pickup_overlaps(
    session: &GameSession,
    triggers: &mut TriggerVolumeSystem,
    facts: Vec<TriggerOverlapFact>,
) -> Vec<TriggerOverlapFact> {
    let unavailable = session
        .pickups
        .iter()
        .filter_map(|(entity, pickup)| (pickup.state != PickupState::Available).then_some(*entity))
        .collect::<std::collections::BTreeSet<_>>();
    if unavailable.is_empty() {
        return facts;
    }

    let mut snapshot = triggers.snapshot();
    snapshot
        .active_overlaps
        .retain(|pair| !unavailable.contains(&pair.trigger_id()));
    *triggers = TriggerVolumeSystem::from_snapshot(snapshot)
        .expect("pickup trigger definitions are admitted and only active overlaps were pruned");
    facts
        .into_iter()
        .filter(|fact| !unavailable.contains(&fact.pair.trigger_id()))
        .collect()
}

pub(crate) fn pickup_view(entity: EntityId, component: &PickupComponent) -> PickupView {
    PickupView {
        entity,
        config: component.config.clone(),
        state: component.state.clone(),
    }
}
