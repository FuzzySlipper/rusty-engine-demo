use std::collections::{BTreeMap, BTreeSet};

use core_ids::EntityId;
use core_math::Vec3;
use core_time::Tick;
use gameplay_mechanics::{
    EquipmentEquipRequest, EquipmentService as MechanicsEquipmentService, EquipmentSwapRequest,
    EquipmentUnequipRequest, InventoryMutationRequest,
    InventoryService as MechanicsInventoryService, OperationId, SourceInstanceId,
    SourceInstanceIdentity,
};

use crate::combat::{
    MAX_WEAPON_COOLDOWN_TICKS, MAX_WEAPON_DAMAGE, MAX_WEAPON_MUZZLE_OFFSET, MAX_WEAPON_PELLETS,
    MAX_WEAPON_RANGE, MAX_WEAPON_SPREAD_DEGREES,
};
use crate::mechanics::{mechanics_item_id, weapon_slot, InventoryRuntime};

pub const MAX_ITEM_DEFINITION_ID_BYTES: usize = 96;
pub const MAX_ITEM_QUANTITY: u32 = 1_000_000;
pub const MAX_INVENTORY_SLOTS: usize = 64;
pub const MAX_PROJECTILE_MASS: f32 = 100.0;
pub const MAX_PROJECTILE_RADIUS: f32 = 2.0;
pub const MAX_PROJECTILE_IMPULSE: f32 = 1_000.0;
pub const MAX_PROJECTILE_GRAVITY_SCALE: f32 = 10.0;
pub const MAX_PROJECTILE_LIFETIME_TICKS: u64 = 3_600;
pub const MAX_PROJECTILE_RESTITUTION: f32 = 1.0;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemDefinitionId(String);

impl ItemDefinitionId {
    pub fn parse(value: impl Into<String>) -> Result<Self, ItemDefinitionIdError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_ITEM_DEFINITION_ID_BYTES {
            return Err(ItemDefinitionIdError { value });
        }
        let mut segments = value.split('/');
        let first = segments.next().expect("non-empty identity has a segment");
        let mut count = 0;
        for segment in std::iter::once(first).chain(segments) {
            count += 1;
            if !is_kebab_segment(segment) {
                return Err(ItemDefinitionIdError { value });
            }
        }
        if count < 2 {
            return Err(ItemDefinitionIdError { value });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ItemDefinitionId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDefinitionIdError {
    pub value: String,
}

impl std::fmt::Display for ItemDefinitionIdError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid item definition identity `{}`",
            self.value
        )
    }
}

impl std::error::Error for ItemDefinitionIdError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WeaponAttackMode {
    Hitscan,
    Spread {
        pellet_count: u8,
        spread_degrees: f32,
    },
    Automatic,
    Projectile,
}

impl WeaponAttackMode {
    pub fn is_automatic(self) -> bool {
        matches!(self, Self::Automatic)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeaponDefinition {
    pub attack_mode: WeaponAttackMode,
    pub damage: u32,
    pub max_distance: f32,
    pub cooldown_ticks: u64,
    pub ammunition: ItemDefinitionId,
    pub ammunition_cost: u32,
    pub muzzle_offset: Vec3,
    pub presentation: String,
    pub projectile: Option<ProjectileDefinition>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectileDefinition {
    pub mass: f32,
    pub radius: f32,
    pub impulse: f32,
    pub gravity_scale: f32,
    pub lifetime_ticks: u64,
    pub restitution: f32,
}

impl ProjectileDefinition {
    pub(crate) fn is_valid(self) -> bool {
        self.mass.is_finite()
            && self.mass > 0.0
            && self.mass <= MAX_PROJECTILE_MASS
            && self.radius.is_finite()
            && self.radius > 0.0
            && self.radius <= MAX_PROJECTILE_RADIUS
            && self.impulse.is_finite()
            && self.impulse > 0.0
            && self.impulse <= MAX_PROJECTILE_IMPULSE
            && self.gravity_scale.is_finite()
            && self.gravity_scale >= 0.0
            && self.gravity_scale <= MAX_PROJECTILE_GRAVITY_SCALE
            && (1..=MAX_PROJECTILE_LIFETIME_TICKS).contains(&self.lifetime_ticks)
            && self.restitution.is_finite()
            && (0.0..=MAX_PROJECTILE_RESTITUTION).contains(&self.restitution)
    }
}

impl WeaponDefinition {
    pub(crate) fn is_valid(&self) -> bool {
        let valid_attack_mode = match self.attack_mode {
            WeaponAttackMode::Hitscan | WeaponAttackMode::Automatic => self.projectile.is_none(),
            WeaponAttackMode::Spread {
                pellet_count,
                spread_degrees,
            } => {
                (2..=MAX_WEAPON_PELLETS).contains(&pellet_count)
                    && spread_degrees.is_finite()
                    && spread_degrees > 0.0
                    && spread_degrees <= MAX_WEAPON_SPREAD_DEGREES
                    && self.projectile.is_none()
            }
            WeaponAttackMode::Projectile => self
                .projectile
                .is_some_and(|projectile| projectile.is_valid()),
        };
        valid_attack_mode
            && (1..=MAX_WEAPON_DAMAGE).contains(&self.damage)
            && self.max_distance.is_finite()
            && self.max_distance > 0.0
            && self.max_distance <= MAX_WEAPON_RANGE
            && self.cooldown_ticks <= MAX_WEAPON_COOLDOWN_TICKS
            && self.ammunition_cost > 0
            && self.ammunition_cost <= MAX_ITEM_QUANTITY
            && vec3_is_finite(self.muzzle_offset)
            && self.muzzle_offset.x.abs() <= MAX_WEAPON_MUZZLE_OFFSET
            && self.muzzle_offset.y.abs() <= MAX_WEAPON_MUZZLE_OFFSET
            && self.muzzle_offset.z.abs() <= MAX_WEAPON_MUZZLE_OFFSET
            && !self.presentation.is_empty()
            && self.presentation.len() <= MAX_ITEM_DEFINITION_ID_BYTES
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    Weapon(WeaponDefinition),
    Ammunition,
    AccessKey,
    HealthSupply { restore_health: u32 },
    Armor { protection: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemDefinition {
    pub id: ItemDefinitionId,
    pub kind: ItemKind,
    pub max_quantity: u32,
}

impl ItemDefinition {
    pub fn new(id: ItemDefinitionId, kind: ItemKind, max_quantity: u32) -> Self {
        Self {
            id,
            kind,
            max_quantity,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryStack {
    pub item: ItemDefinitionId,
    pub quantity: u32,
}

impl InventoryStack {
    pub fn new(item: ItemDefinitionId, quantity: u32) -> Self {
        Self { item, quantity }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryConfig {
    pub capacity_slots: usize,
    pub starting_stacks: Vec<InventoryStack>,
    pub initially_equipped_weapon: Option<ItemDefinitionId>,
    pub weapon_slots: Vec<ItemDefinitionId>,
}

impl InventoryConfig {
    pub fn new(
        capacity_slots: usize,
        starting_stacks: impl IntoIterator<Item = InventoryStack>,
        initially_equipped_weapon: Option<ItemDefinitionId>,
        weapon_slots: impl IntoIterator<Item = ItemDefinitionId>,
    ) -> Self {
        Self {
            capacity_slots,
            starting_stacks: starting_stacks.into_iter().collect(),
            initially_equipped_weapon,
            weapon_slots: weapon_slots.into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InventoryView {
    pub owner: EntityId,
    pub capacity_slots: usize,
    pub stacks: Vec<InventoryStack>,
    pub equipped_weapon: Option<ItemDefinitionId>,
    pub weapon_slots: Vec<ItemDefinitionId>,
    pub weapon_ready_at: BTreeMap<ItemDefinitionId, Tick>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemDefinitionView {
    pub id: ItemDefinitionId,
    pub kind: ItemKind,
    pub max_quantity: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryAction {
    Grant {
        item: ItemDefinitionId,
        quantity: u32,
    },
    Consume {
        item: ItemDefinitionId,
        quantity: u32,
    },
    MoveStack {
        from_index: usize,
        to_index: usize,
    },
    SelectWeapon {
        item: ItemDefinitionId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryCommand {
    pub sequence: u64,
    pub action: InventoryAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryFact {
    QuantityChanged {
        owner: EntityId,
        item: ItemDefinitionId,
        before: u32,
        after: u32,
    },
    StackMoved {
        owner: EntityId,
        item: ItemDefinitionId,
        from_index: usize,
        to_index: usize,
    },
    EquippedWeaponChanged {
        owner: EntityId,
        before: Option<ItemDefinitionId>,
        after: Option<ItemDefinitionId>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct InventoryReceipt {
    pub sequence: u64,
    pub action: InventoryAction,
    pub before: InventoryView,
    pub after: InventoryView,
    pub facts: Vec<InventoryFact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryRejection {
    UnknownInventory {
        owner: EntityId,
    },
    MissingDefinition {
        item: ItemDefinitionId,
    },
    ZeroQuantity {
        item: ItemDefinitionId,
    },
    QuantityOverflow {
        item: ItemDefinitionId,
        current: u32,
        requested: u32,
        limit: u32,
    },
    QuantityUnderflow {
        item: ItemDefinitionId,
        current: u32,
        requested: u32,
    },
    InventoryFull {
        capacity_slots: usize,
    },
    InvalidStackIndex {
        index: usize,
        stack_count: usize,
    },
    AlreadyInPosition {
        index: usize,
    },
    WeaponNotOwned {
        item: ItemDefinitionId,
    },
    IncompatibleSelection {
        item: ItemDefinitionId,
    },
    AlreadySelected {
        item: ItemDefinitionId,
    },
    InvalidWeaponSlot {
        slot: usize,
        slot_count: usize,
    },
    OwnerDefeated {
        owner: EntityId,
    },
    CommandSequenceOverflow {
        owner: EntityId,
    },
    RepeatedCommand {
        sequence: u64,
    },
    StaleCommand {
        sequence: u64,
        last_applied: u64,
    },
    Mechanics {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryAdmissionError {
    DuplicateItemDefinition {
        item: ItemDefinitionId,
    },
    InvalidItemDefinition {
        item: ItemDefinitionId,
    },
    MissingAmmunitionDefinition {
        weapon: ItemDefinitionId,
        ammunition: ItemDefinitionId,
    },
    IncompatibleAmmunitionDefinition {
        weapon: ItemDefinitionId,
        ammunition: ItemDefinitionId,
    },
    InventoryWithoutPlayerController {
        owner: EntityId,
    },
    InvalidCapacity {
        owner: EntityId,
        capacity_slots: usize,
    },
    TooManyStartingStacks {
        owner: EntityId,
        capacity_slots: usize,
        stack_count: usize,
    },
    DuplicateStartingStack {
        owner: EntityId,
        item: ItemDefinitionId,
    },
    MissingStartingDefinition {
        owner: EntityId,
        item: ItemDefinitionId,
    },
    InvalidStartingQuantity {
        owner: EntityId,
        item: ItemDefinitionId,
        quantity: u32,
        limit: u32,
    },
    InvalidInitialSelection {
        owner: EntityId,
        item: ItemDefinitionId,
    },
    DuplicateWeaponSlot {
        owner: EntityId,
        item: ItemDefinitionId,
    },
    MissingWeaponSlotDefinition {
        owner: EntityId,
        item: ItemDefinitionId,
    },
    IncompatibleWeaponSlot {
        owner: EntityId,
        item: ItemDefinitionId,
    },
    Mechanics {
        reason: String,
    },
}

pub struct InventoryService;

impl InventoryService {
    pub fn apply(
        session: &mut crate::session::GameSession,
        owner: EntityId,
        command: InventoryCommand,
    ) -> Result<InventoryReceipt, InventoryRejection> {
        let Some(runtime) = session.inventories.get(&owner) else {
            return Err(InventoryRejection::UnknownInventory { owner });
        };
        if let Some(last_applied) = runtime.last_applied_command_sequence {
            if command.sequence == last_applied {
                return Err(InventoryRejection::RepeatedCommand {
                    sequence: command.sequence,
                });
            }
            if command.sequence < last_applied {
                return Err(InventoryRejection::StaleCommand {
                    sequence: command.sequence,
                    last_applied,
                });
            }
        }

        let before = inventory_view(session, owner)?;
        if matches!(command.action, InventoryAction::SelectWeapon { .. })
            && crate::vitality::DamageService::is_dead(session, owner)
        {
            return Err(InventoryRejection::OwnerDefeated { owner });
        }
        let mut candidate = session.clone();
        let facts = apply_action(&mut candidate, owner, command.sequence, &command.action)?;
        candidate
            .inventories
            .get_mut(&owner)
            .expect("validated inventory remains attached")
            .last_applied_command_sequence = Some(command.sequence);
        let after = inventory_view(&candidate, owner)?;
        *session = candidate;

        Ok(InventoryReceipt {
            sequence: command.sequence,
            action: command.action,
            before,
            after,
            facts,
        })
    }

    pub(crate) fn select_weapon_slot(
        session: &mut crate::session::GameSession,
        owner: EntityId,
        slot: usize,
    ) -> Result<InventoryReceipt, InventoryRejection> {
        let Some(inventory) = session.inventories.get(&owner) else {
            return Err(InventoryRejection::UnknownInventory { owner });
        };
        let Some(item) = inventory.weapon_slots.get(slot).cloned() else {
            return Err(InventoryRejection::InvalidWeaponSlot {
                slot,
                slot_count: inventory.weapon_slots.len(),
            });
        };
        let sequence = inventory
            .last_applied_command_sequence
            .map_or(Some(1), |sequence| sequence.checked_add(1))
            .ok_or(InventoryRejection::CommandSequenceOverflow { owner })?;
        Self::apply(
            session,
            owner,
            InventoryCommand {
                sequence,
                action: InventoryAction::SelectWeapon { item },
            },
        )
    }
}

pub(crate) fn admit_item_definitions(
    definitions: impl IntoIterator<Item = ItemDefinition>,
) -> Result<BTreeMap<ItemDefinitionId, ItemDefinition>, InventoryAdmissionError> {
    let mut admitted = BTreeMap::new();
    for definition in definitions {
        if definition.max_quantity == 0
            || definition.max_quantity > MAX_ITEM_QUANTITY
            || matches!(definition.kind, ItemKind::Weapon(_) | ItemKind::AccessKey)
                && definition.max_quantity != 1
            || matches!(&definition.kind, ItemKind::Weapon(weapon) if !weapon.is_valid())
            || matches!(
                definition.kind,
                ItemKind::HealthSupply { restore_health: 0 } | ItemKind::Armor { protection: 0 }
            )
        {
            return Err(InventoryAdmissionError::InvalidItemDefinition {
                item: definition.id,
            });
        }
        let id = definition.id.clone();
        if admitted.insert(id.clone(), definition).is_some() {
            return Err(InventoryAdmissionError::DuplicateItemDefinition { item: id });
        }
    }
    for definition in admitted.values() {
        let ItemKind::Weapon(weapon) = &definition.kind else {
            continue;
        };
        let Some(ammunition_definition) = admitted.get(&weapon.ammunition) else {
            return Err(InventoryAdmissionError::MissingAmmunitionDefinition {
                weapon: definition.id.clone(),
                ammunition: weapon.ammunition.clone(),
            });
        };
        if !matches!(ammunition_definition.kind, ItemKind::Ammunition) {
            return Err(InventoryAdmissionError::IncompatibleAmmunitionDefinition {
                weapon: definition.id.clone(),
                ammunition: weapon.ammunition.clone(),
            });
        }
        if weapon.ammunition_cost > ammunition_definition.max_quantity {
            return Err(InventoryAdmissionError::InvalidItemDefinition {
                item: definition.id.clone(),
            });
        }
    }
    Ok(admitted)
}

pub(crate) fn inventory_from_config(
    owner: EntityId,
    config: &InventoryConfig,
    definitions: &BTreeMap<ItemDefinitionId, ItemDefinition>,
) -> Result<InventoryRuntime, InventoryAdmissionError> {
    if config.capacity_slots == 0 || config.capacity_slots > MAX_INVENTORY_SLOTS {
        return Err(InventoryAdmissionError::InvalidCapacity {
            owner,
            capacity_slots: config.capacity_slots,
        });
    }
    if config.starting_stacks.len() > config.capacity_slots {
        return Err(InventoryAdmissionError::TooManyStartingStacks {
            owner,
            capacity_slots: config.capacity_slots,
            stack_count: config.starting_stacks.len(),
        });
    }
    let mut seen = BTreeSet::new();
    for stack in &config.starting_stacks {
        if !seen.insert(stack.item.clone()) {
            return Err(InventoryAdmissionError::DuplicateStartingStack {
                owner,
                item: stack.item.clone(),
            });
        }
        let Some(definition) = definitions.get(&stack.item) else {
            return Err(InventoryAdmissionError::MissingStartingDefinition {
                owner,
                item: stack.item.clone(),
            });
        };
        if stack.quantity == 0 || stack.quantity > definition.max_quantity {
            return Err(InventoryAdmissionError::InvalidStartingQuantity {
                owner,
                item: stack.item.clone(),
                quantity: stack.quantity,
                limit: definition.max_quantity,
            });
        }
    }
    if let Some(equipped) = &config.initially_equipped_weapon {
        let owned = config
            .starting_stacks
            .iter()
            .any(|stack| stack.item == *equipped);
        let compatible = definitions
            .get(equipped)
            .is_some_and(|definition| matches!(definition.kind, ItemKind::Weapon(_)));
        if !owned || !compatible {
            return Err(InventoryAdmissionError::InvalidInitialSelection {
                owner,
                item: equipped.clone(),
            });
        }
    }
    let mut seen_slots = BTreeSet::new();
    for item in &config.weapon_slots {
        if !seen_slots.insert(item.clone()) {
            return Err(InventoryAdmissionError::DuplicateWeaponSlot {
                owner,
                item: item.clone(),
            });
        }
        let Some(definition) = definitions.get(item) else {
            return Err(InventoryAdmissionError::MissingWeaponSlotDefinition {
                owner,
                item: item.clone(),
            });
        };
        if !matches!(definition.kind, ItemKind::Weapon(_)) {
            return Err(InventoryAdmissionError::IncompatibleWeaponSlot {
                owner,
                item: item.clone(),
            });
        }
    }
    Ok(InventoryRuntime {
        capacity_slots: config.capacity_slots,
        stack_order: config
            .starting_stacks
            .iter()
            .map(|stack| stack.item.clone())
            .collect(),
        weapon_slots: config.weapon_slots.clone(),
        weapon_entities: BTreeMap::new(),
        weapon_ready_at: config
            .weapon_slots
            .iter()
            .cloned()
            .map(|item| (item, Tick::ZERO))
            .collect(),
        last_applied_command_sequence: None,
    })
}

pub(crate) fn inventory_view(
    session: &crate::session::GameSession,
    owner: EntityId,
) -> Result<InventoryView, InventoryRejection> {
    let runtime = session
        .inventories
        .get(&owner)
        .ok_or(InventoryRejection::UnknownInventory { owner })?;
    let canonical =
        MechanicsInventoryService::view(&session.entities, &session.mechanics.catalog, owner)
            .map_err(mechanics_rejection)?;
    let equipped_weapon = session.equipped_weapon(owner);
    let mut stacks = canonical
        .stacks()
        .iter()
        .map(|stack| {
            let item = product_item_id(&session.item_definitions, &stack.definition)?;
            let quantity =
                u32::try_from(stack.quantity).map_err(|_| InventoryRejection::Mechanics {
                    reason: format!(
                        "quantity {} does not fit product representation",
                        stack.quantity
                    ),
                })?;
            Ok(InventoryStack::new(item, quantity))
        })
        .collect::<Result<Vec<_>, InventoryRejection>>()?;
    for unique in canonical.unique_items() {
        let item = product_item_id(&session.item_definitions, &unique.definition)?;
        stacks.push(InventoryStack::new(item, 1));
    }
    stacks.sort_by_key(|stack| {
        runtime
            .stack_order
            .iter()
            .position(|item| item == &stack.item)
            .unwrap_or(usize::MAX)
    });
    Ok(InventoryView {
        owner,
        capacity_slots: runtime.capacity_slots,
        stacks,
        equipped_weapon,
        weapon_slots: runtime.weapon_slots.clone(),
        weapon_ready_at: runtime.weapon_ready_at.clone(),
    })
}

fn apply_action(
    session: &mut crate::session::GameSession,
    owner: EntityId,
    sequence: u64,
    action: &InventoryAction,
) -> Result<Vec<InventoryFact>, InventoryRejection> {
    let definitions = &session.item_definitions;
    let before_view = inventory_view(session, owner)?;
    let operation = operation_id(sequence)?;
    let source = source_identity(operation.clone())?;
    match action {
        InventoryAction::Grant { item, quantity } => {
            let definition = require_definition(definitions, item)?;
            if *quantity == 0 {
                return Err(InventoryRejection::ZeroQuantity { item: item.clone() });
            }
            let before = before_view
                .stacks
                .iter()
                .find(|stack| stack.item == *item)
                .map_or(0, |stack| stack.quantity);
            let after = before.checked_add(*quantity).ok_or_else(|| {
                InventoryRejection::QuantityOverflow {
                    item: item.clone(),
                    current: before,
                    requested: *quantity,
                    limit: definition.max_quantity,
                }
            })?;
            if after > definition.max_quantity {
                return Err(InventoryRejection::QuantityOverflow {
                    item: item.clone(),
                    current: before,
                    requested: *quantity,
                    limit: definition.max_quantity,
                });
            }
            if before == 0 && before_view.stacks.len() == before_view.capacity_slots {
                return Err(InventoryRejection::InventoryFull {
                    capacity_slots: before_view.capacity_slots,
                });
            }
            if matches!(definition.kind, ItemKind::Weapon(_)) {
                if *quantity != 1 || before != 0 {
                    return Err(InventoryRejection::QuantityOverflow {
                        item: item.clone(),
                        current: before,
                        requested: *quantity,
                        limit: 1,
                    });
                }
                let weapon = session
                    .inventories
                    .get(&owner)
                    .and_then(|runtime| runtime.weapon_entities.get(item))
                    .copied()
                    .ok_or_else(|| InventoryRejection::IncompatibleSelection {
                        item: item.clone(),
                    })?;
                crate::mechanics::set_weapon_containment(&mut session.entities, weapon, owner)
                    .map_err(|reason| InventoryRejection::Mechanics { reason })?;
            } else {
                MechanicsInventoryService::grant(
                    &mut session.entities,
                    &session.mechanics.catalog,
                    InventoryMutationRequest {
                        operation,
                        source,
                        owner,
                        item: mechanics_item_id(item)
                            .map_err(|reason| InventoryRejection::Mechanics { reason })?,
                        quantity: u64::from(*quantity),
                        expected_revision: None,
                    },
                )
                .map_err(mechanics_rejection)?;
            }
            let runtime = session
                .inventories
                .get_mut(&owner)
                .expect("validated inventory remains attached");
            if before == 0 {
                runtime.stack_order.push(item.clone());
            }
            Ok(vec![InventoryFact::QuantityChanged {
                owner,
                item: item.clone(),
                before,
                after,
            }])
        }
        InventoryAction::Consume { item, quantity } => {
            let definition = require_definition(definitions, item)?;
            if *quantity == 0 {
                return Err(InventoryRejection::ZeroQuantity { item: item.clone() });
            }
            let before = before_view
                .stacks
                .iter()
                .find(|stack| stack.item == *item)
                .map_or(0, |stack| stack.quantity);
            if *quantity > before {
                return Err(InventoryRejection::QuantityUnderflow {
                    item: item.clone(),
                    current: before,
                    requested: *quantity,
                });
            }
            let after = before - *quantity;
            let mut facts = vec![InventoryFact::QuantityChanged {
                owner,
                item: item.clone(),
                before,
                after,
            }];
            if matches!(definition.kind, ItemKind::Weapon(_)) {
                if *quantity != 1 {
                    return Err(InventoryRejection::QuantityUnderflow {
                        item: item.clone(),
                        current: before,
                        requested: *quantity,
                    });
                }
                let weapon = session
                    .inventories
                    .get(&owner)
                    .and_then(|runtime| runtime.weapon_entities.get(item))
                    .copied()
                    .ok_or_else(|| InventoryRejection::IncompatibleSelection {
                        item: item.clone(),
                    })?;
                if before_view.equipped_weapon.as_ref() == Some(item) {
                    let state_revision = session.entities.revision();
                    MechanicsEquipmentService::unequip(
                        &mut session.entities,
                        &session.mechanics.catalog,
                        EquipmentUnequipRequest {
                            operation: operation.clone(),
                            source: source.clone(),
                            owner,
                            item: weapon,
                            expected_equipment_revision: None,
                            expected_state_revision: state_revision,
                        },
                    )
                    .map_err(mechanics_rejection)?;
                    facts.push(InventoryFact::EquippedWeaponChanged {
                        owner,
                        before: Some(item.clone()),
                        after: None,
                    });
                }
                crate::mechanics::clear_weapon_containment(&mut session.entities, weapon)
                    .map_err(|reason| InventoryRejection::Mechanics { reason })?;
            } else {
                MechanicsInventoryService::consume(
                    &mut session.entities,
                    &session.mechanics.catalog,
                    InventoryMutationRequest {
                        operation,
                        source,
                        owner,
                        item: mechanics_item_id(item)
                            .map_err(|reason| InventoryRejection::Mechanics { reason })?,
                        quantity: u64::from(*quantity),
                        expected_revision: None,
                    },
                )
                .map_err(mechanics_rejection)?;
            }
            if after == 0 {
                session
                    .inventories
                    .get_mut(&owner)
                    .expect("validated inventory remains attached")
                    .stack_order
                    .retain(|candidate| candidate != item);
            }
            Ok(facts)
        }
        InventoryAction::MoveStack {
            from_index,
            to_index,
        } => {
            let stack_count = before_view.stacks.len();
            if *from_index >= stack_count {
                return Err(InventoryRejection::InvalidStackIndex {
                    index: *from_index,
                    stack_count,
                });
            }
            if *to_index >= stack_count {
                return Err(InventoryRejection::InvalidStackIndex {
                    index: *to_index,
                    stack_count,
                });
            }
            if from_index == to_index {
                return Err(InventoryRejection::AlreadyInPosition { index: *from_index });
            }
            let order = &mut session
                .inventories
                .get_mut(&owner)
                .expect("validated inventory remains attached")
                .stack_order;
            let item = order.remove(*from_index);
            order.insert(*to_index, item.clone());
            Ok(vec![InventoryFact::StackMoved {
                owner,
                item,
                from_index: *from_index,
                to_index: *to_index,
            }])
        }
        InventoryAction::SelectWeapon { item } => {
            let definition = require_definition(definitions, item)?;
            if !matches!(definition.kind, ItemKind::Weapon(_)) {
                return Err(InventoryRejection::IncompatibleSelection { item: item.clone() });
            }
            let runtime = session
                .inventories
                .get(&owner)
                .expect("validated inventory remains attached");
            if !runtime.weapon_slots.contains(item) {
                return Err(InventoryRejection::IncompatibleSelection { item: item.clone() });
            }
            if !before_view.stacks.iter().any(|stack| stack.item == *item) {
                return Err(InventoryRejection::WeaponNotOwned { item: item.clone() });
            }
            if before_view.equipped_weapon.as_ref() == Some(item) {
                return Err(InventoryRejection::AlreadySelected { item: item.clone() });
            }
            let incoming =
                runtime.weapon_entities.get(item).copied().ok_or_else(|| {
                    InventoryRejection::IncompatibleSelection { item: item.clone() }
                })?;
            let state_revision = session.entities.revision();
            if let Some(before_item) = &before_view.equipped_weapon {
                let outgoing = runtime
                    .weapon_entities
                    .get(before_item)
                    .copied()
                    .ok_or_else(|| InventoryRejection::Mechanics {
                        reason: format!("missing unique entity for equipped weapon {before_item}"),
                    })?;
                MechanicsEquipmentService::swap(
                    &mut session.entities,
                    &session.mechanics.catalog,
                    EquipmentSwapRequest {
                        operation,
                        source,
                        owner,
                        outgoing_item: outgoing,
                        incoming_item: incoming,
                        incoming_slots: vec![weapon_slot()],
                        expected_equipment_revision: None,
                        expected_state_revision: state_revision,
                    },
                )
                .map_err(mechanics_rejection)?;
            } else {
                MechanicsEquipmentService::equip(
                    &mut session.entities,
                    &session.mechanics.catalog,
                    EquipmentEquipRequest {
                        operation,
                        source,
                        owner,
                        item: incoming,
                        slots: vec![weapon_slot()],
                        expected_equipment_revision: None,
                        expected_state_revision: state_revision,
                    },
                )
                .map_err(mechanics_rejection)?;
            }
            let before = before_view.equipped_weapon;
            Ok(vec![InventoryFact::EquippedWeaponChanged {
                owner,
                before,
                after: Some(item.clone()),
            }])
        }
    }
}

fn vec3_is_finite(value: Vec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}

fn require_definition<'a>(
    definitions: &'a BTreeMap<ItemDefinitionId, ItemDefinition>,
    item: &ItemDefinitionId,
) -> Result<&'a ItemDefinition, InventoryRejection> {
    definitions
        .get(item)
        .ok_or_else(|| InventoryRejection::MissingDefinition { item: item.clone() })
}

fn product_item_id(
    definitions: &BTreeMap<ItemDefinitionId, ItemDefinition>,
    mechanics: &gameplay_mechanics::ItemDefinitionId,
) -> Result<ItemDefinitionId, InventoryRejection> {
    definitions
        .keys()
        .find(|item| mechanics_item_id(item).is_ok_and(|candidate| candidate == *mechanics))
        .cloned()
        .ok_or_else(|| InventoryRejection::Mechanics {
            reason: format!("unknown product item for canonical identity {mechanics}"),
        })
}

fn operation_id(sequence: u64) -> Result<OperationId, InventoryRejection> {
    OperationId::parse(format!("inventory-command-{sequence}")).map_err(|error| {
        InventoryRejection::Mechanics {
            reason: error.to_string(),
        }
    })
}

fn source_identity(operation: OperationId) -> Result<SourceInstanceIdentity, InventoryRejection> {
    Ok(SourceInstanceIdentity::Request {
        operation,
        instance: SourceInstanceId::parse("inventory-command").map_err(|error| {
            InventoryRejection::Mechanics {
                reason: error.to_string(),
            }
        })?,
    })
}

fn mechanics_rejection(error: gameplay_mechanics::MechanicsError) -> InventoryRejection {
    InventoryRejection::Mechanics {
        reason: error.to_string(),
    }
}

fn is_kebab_segment(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}
