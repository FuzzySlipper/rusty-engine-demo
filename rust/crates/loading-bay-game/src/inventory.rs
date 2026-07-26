use std::collections::{BTreeMap, BTreeSet};

use core_ids::EntityId;
use core_math::Vec3;
use core_time::Tick;

use crate::combat::{
    MAX_WEAPON_COOLDOWN_TICKS, MAX_WEAPON_DAMAGE, MAX_WEAPON_MUZZLE_OFFSET, MAX_WEAPON_RANGE,
};

pub const MAX_ITEM_DEFINITION_ID_BYTES: usize = 96;
pub const MAX_ITEM_QUANTITY: u32 = 1_000_000;
pub const MAX_INVENTORY_SLOTS: usize = 64;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponAttackMode {
    Hitscan,
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
}

impl WeaponDefinition {
    pub(crate) fn is_valid(&self) -> bool {
        (1..=MAX_WEAPON_DAMAGE).contains(&self.damage)
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
pub struct InventoryComponent {
    pub capacity_slots: usize,
    pub stacks: Vec<InventoryStack>,
    pub equipped_weapon: Option<ItemDefinitionId>,
    pub weapon_slots: Vec<ItemDefinitionId>,
    pub weapon_ready_at: BTreeMap<ItemDefinitionId, Tick>,
    pub(crate) last_applied_command_sequence: Option<u64>,
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
}

pub struct InventoryService;

impl InventoryService {
    pub fn apply(
        session: &mut crate::session::GameSession,
        owner: EntityId,
        command: InventoryCommand,
    ) -> Result<InventoryReceipt, InventoryRejection> {
        let Some(component) = session.inventories.get(&owner) else {
            return Err(InventoryRejection::UnknownInventory { owner });
        };
        if let Some(last_applied) = component.last_applied_command_sequence {
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

        let before = inventory_view(owner, component);
        if matches!(command.action, InventoryAction::SelectWeapon { .. })
            && session
                .health
                .get(&owner)
                .is_some_and(|health| health.current == 0)
        {
            return Err(InventoryRejection::OwnerDefeated { owner });
        }
        let mut candidate = component.clone();
        let facts = apply_action(
            &session.item_definitions,
            owner,
            &mut candidate,
            &command.action,
        )?;
        candidate.last_applied_command_sequence = Some(command.sequence);
        let after = inventory_view(owner, &candidate);
        session.inventories.insert(owner, candidate);

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
) -> Result<InventoryComponent, InventoryAdmissionError> {
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
    Ok(InventoryComponent {
        capacity_slots: config.capacity_slots,
        stacks: config.starting_stacks.clone(),
        equipped_weapon: config.initially_equipped_weapon.clone(),
        weapon_slots: config.weapon_slots.clone(),
        weapon_ready_at: config
            .weapon_slots
            .iter()
            .cloned()
            .map(|item| (item, Tick::ZERO))
            .collect(),
        last_applied_command_sequence: None,
    })
}

pub(crate) fn inventory_view(owner: EntityId, component: &InventoryComponent) -> InventoryView {
    InventoryView {
        owner,
        capacity_slots: component.capacity_slots,
        stacks: component.stacks.clone(),
        equipped_weapon: component.equipped_weapon.clone(),
        weapon_slots: component.weapon_slots.clone(),
        weapon_ready_at: component.weapon_ready_at.clone(),
    }
}

fn apply_action(
    definitions: &BTreeMap<ItemDefinitionId, ItemDefinition>,
    owner: EntityId,
    candidate: &mut InventoryComponent,
    action: &InventoryAction,
) -> Result<Vec<InventoryFact>, InventoryRejection> {
    match action {
        InventoryAction::Grant { item, quantity } => {
            let definition = require_definition(definitions, item)?;
            if *quantity == 0 {
                return Err(InventoryRejection::ZeroQuantity { item: item.clone() });
            }
            if let Some(stack) = candidate
                .stacks
                .iter_mut()
                .find(|stack| stack.item == *item)
            {
                let before = stack.quantity;
                let Some(after) = before.checked_add(*quantity) else {
                    return Err(InventoryRejection::QuantityOverflow {
                        item: item.clone(),
                        current: before,
                        requested: *quantity,
                        limit: definition.max_quantity,
                    });
                };
                if after > definition.max_quantity {
                    return Err(InventoryRejection::QuantityOverflow {
                        item: item.clone(),
                        current: before,
                        requested: *quantity,
                        limit: definition.max_quantity,
                    });
                }
                stack.quantity = after;
                return Ok(vec![InventoryFact::QuantityChanged {
                    owner,
                    item: item.clone(),
                    before,
                    after,
                }]);
            }
            if candidate.stacks.len() == candidate.capacity_slots {
                return Err(InventoryRejection::InventoryFull {
                    capacity_slots: candidate.capacity_slots,
                });
            }
            if *quantity > definition.max_quantity {
                return Err(InventoryRejection::QuantityOverflow {
                    item: item.clone(),
                    current: 0,
                    requested: *quantity,
                    limit: definition.max_quantity,
                });
            }
            candidate
                .stacks
                .push(InventoryStack::new(item.clone(), *quantity));
            Ok(vec![InventoryFact::QuantityChanged {
                owner,
                item: item.clone(),
                before: 0,
                after: *quantity,
            }])
        }
        InventoryAction::Consume { item, quantity } => {
            require_definition(definitions, item)?;
            if *quantity == 0 {
                return Err(InventoryRejection::ZeroQuantity { item: item.clone() });
            }
            let Some(index) = candidate
                .stacks
                .iter()
                .position(|stack| stack.item == *item)
            else {
                return Err(InventoryRejection::QuantityUnderflow {
                    item: item.clone(),
                    current: 0,
                    requested: *quantity,
                });
            };
            let before = candidate.stacks[index].quantity;
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
            if after == 0 {
                candidate.stacks.remove(index);
                if candidate.equipped_weapon.as_ref() == Some(item) {
                    let before_equipped = candidate.equipped_weapon.take();
                    facts.push(InventoryFact::EquippedWeaponChanged {
                        owner,
                        before: before_equipped,
                        after: None,
                    });
                }
            } else {
                candidate.stacks[index].quantity = after;
            }
            Ok(facts)
        }
        InventoryAction::MoveStack {
            from_index,
            to_index,
        } => {
            let stack_count = candidate.stacks.len();
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
            let stack = candidate.stacks.remove(*from_index);
            let item = stack.item.clone();
            candidate.stacks.insert(*to_index, stack);
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
            if !candidate.weapon_slots.contains(item) {
                return Err(InventoryRejection::IncompatibleSelection { item: item.clone() });
            }
            if !candidate.stacks.iter().any(|stack| stack.item == *item) {
                return Err(InventoryRejection::WeaponNotOwned { item: item.clone() });
            }
            if candidate.equipped_weapon.as_ref() == Some(item) {
                return Err(InventoryRejection::AlreadySelected { item: item.clone() });
            }
            let before = candidate.equipped_weapon.replace(item.clone());
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

fn is_kebab_segment(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}
