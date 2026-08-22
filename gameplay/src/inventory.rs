use std::collections::{BTreeMap, BTreeSet};

use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::core_time::Tick;
use rusty_engine::gameplay_mechanics::{
    InventoryMutationRequest, InventoryService as MechanicsInventoryService, ItemComponent,
    OperationId, SourceInstanceId, SourceInstanceIdentity,
};
use rusty_engine::gameplay_standard::{
    CapabilityRequirementId, CapabilityRoleBinding, CapabilityRoleBindings, CapabilityRoleId,
    ExactInputBundle, StandardMechanicsReceipt, StandardOperation, StandardOperationContext,
    STANDARD_INVENTORY_CAPABILITY,
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
    /// Whether a held semantic fire intent may start another attack as soon as
    /// this weapon's authoritative cooldown expires.
    pub repeat_while_held: bool,
    /// Number of positive, equally weighted damage multiples. A value of one
    /// is fixed damage; three produces `damage`, `2 * damage`, or `3 * damage`.
    pub damage_rolls: u8,
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
            && self.damage_rolls > 0
            && self
                .damage
                .checked_mul(u32::from(self.damage_rolls))
                .is_some_and(|maximum| maximum <= MAX_WEAPON_DAMAGE)
            && self.max_distance.is_finite()
            && self.max_distance > 0.0
            && self.max_distance <= MAX_WEAPON_RANGE
            && self.cooldown_ticks <= MAX_WEAPON_COOLDOWN_TICKS
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
    HealthSupply {
        restore_health: u32,
        maximum_health: Option<u32>,
        automatic_use: bool,
        consume_at_cap: bool,
    },
    Armor {
        protection: u32,
        maximum_armor: Option<u32>,
        absorption_percent: Option<u8>,
        absorption_divisor: Option<u8>,
        grant_mode: ArmorGrantMode,
        transition: ArmorTransition,
        consume_at_cap: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmorGrantMode {
    Add,
    SetMinimum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmorTransition {
    RejectDifferent,
    Preserve,
    Replace,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemDefinition {
    pub id: ItemDefinitionId,
    pub kind: ItemKind,
    pub max_quantity: u32,
    /// Closed authored gameplay program selected by this item, when active.
    pub program: Option<String>,
}

impl ItemDefinition {
    pub fn new(id: ItemDefinitionId, kind: ItemKind, max_quantity: u32) -> Self {
        Self {
            id,
            kind,
            max_quantity,
            program: None,
        }
    }

    pub fn with_program(mut self, program: Option<String>) -> Self {
        self.program = program;
        self
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

    pub fn select_weapon_slot(
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

/// Applies one selected fungible stack leaf through the Engine standard operation path.
///
/// Loading Bay retains its stack-order presentation and one-slot-per-distinct-item policy. That
/// product capacity is intentionally checked before planning because Engine capacity costs are
/// quantity-based and cannot represent a distinct-stack slot without changing Doom semantics.
/// Catalog admission, revision capture, candidate mutation, and the canonical inventory receipt
/// otherwise belong to the standard operation. Callers must already have selected the product
/// action and own the enclosing candidate/publication boundary.
pub(crate) fn apply_standard_stack(
    session: &mut crate::session::GameSession,
    owner: EntityId,
    sequence: u64,
    action: InventoryAction,
) -> Result<InventoryReceipt, InventoryRejection> {
    let runtime = session
        .inventories
        .get(&owner)
        .ok_or(InventoryRejection::UnknownInventory { owner })?;
    if let Some(last_applied) = runtime.last_applied_command_sequence {
        if sequence == last_applied {
            return Err(InventoryRejection::RepeatedCommand { sequence });
        }
        if sequence < last_applied {
            return Err(InventoryRejection::StaleCommand {
                sequence,
                last_applied,
            });
        }
    }

    let (item, quantity, grant) = match &action {
        InventoryAction::Grant { item, quantity } => (item.clone(), *quantity, true),
        InventoryAction::Consume { item, quantity } => (item.clone(), *quantity, false),
        _ => {
            return Err(InventoryRejection::Mechanics {
                reason: "standard stack application requires grant or consume".to_string(),
            });
        }
    };
    let before = inventory_view(session, owner)?;
    let existing_quantity = before
        .stacks
        .iter()
        .find(|stack| stack.item == item)
        .map_or(0, |stack| stack.quantity);
    if grant && existing_quantity == 0 && before.stacks.len() == before.capacity_slots {
        return Err(InventoryRejection::InventoryFull {
            capacity_slots: before.capacity_slots,
        });
    }

    let role = CapabilityRoleId::parse("inventory-owner").map_err(|error| {
        InventoryRejection::Mechanics {
            reason: error.to_string(),
        }
    })?;
    let item_id =
        mechanics_item_id(&item).map_err(|reason| InventoryRejection::Mechanics { reason })?;
    let operation = if grant {
        StandardOperation::GrantStack {
            role: role.clone(),
            item: item_id,
            quantity: u64::from(quantity),
        }
    } else {
        StandardOperation::ConsumeStack {
            role: role.clone(),
            item: item_id,
            quantity: u64::from(quantity),
        }
    };
    let capability =
        CapabilityRequirementId::parse(STANDARD_INVENTORY_CAPABILITY).map_err(|error| {
            InventoryRejection::Mechanics {
                reason: error.to_string(),
            }
        })?;
    let bindings = CapabilityRoleBindings::admit(
        &operation.requirements(),
        vec![
            CapabilityRoleBinding::new(role, owner, vec![capability]).map_err(|error| {
                InventoryRejection::Mechanics {
                    reason: error.to_string(),
                }
            })?,
        ],
    )
    .map_err(|error| InventoryRejection::Mechanics {
        reason: error.to_string(),
    })?;
    let operation_id = operation_id(sequence)?;
    let source = source_identity(operation_id.clone())?;
    let context = StandardOperationContext::new(operation_id, source).map_err(|error| {
        InventoryRejection::Mechanics {
            reason: error.to_string(),
        }
    })?;
    let plan = operation
        .plan(
            &bindings,
            &ExactInputBundle::empty(),
            &session.entities,
            &session.mechanics.catalog,
            &context,
        )
        .map_err(|error| InventoryRejection::Mechanics {
            reason: error.to_string(),
        })?;
    plan.validate_source_state(&session.entities, &session.mechanics.catalog)
        .map_err(|error| InventoryRejection::Mechanics {
            reason: error.to_string(),
        })?;

    let mut candidate = session.clone();
    let receipt = plan
        .effect()
        .apply_to_candidate(&mut candidate.entities, &candidate.mechanics.catalog)
        .map_err(|error| standard_stack_rejection(error, owner, &item))?;
    let StandardMechanicsReceipt::Inventory(receipt) = receipt else {
        return Err(InventoryRejection::Mechanics {
            reason: "standard stack operation returned a non-inventory receipt".to_string(),
        });
    };
    let before_quantity =
        u32::try_from(receipt.before_quantity).map_err(|_| InventoryRejection::Mechanics {
            reason: "standard inventory quantity exceeds product representation".to_string(),
        })?;
    let after_quantity =
        u32::try_from(receipt.after_quantity).map_err(|_| InventoryRejection::Mechanics {
            reason: "standard inventory quantity exceeds product representation".to_string(),
        })?;
    let runtime = candidate
        .inventories
        .get_mut(&owner)
        .expect("standard inventory owner remains attached");
    if grant && before_quantity == 0 {
        runtime.stack_order.push(item.clone());
    }
    if !grant && after_quantity == 0 {
        runtime.stack_order.retain(|candidate| candidate != &item);
    }
    runtime.last_applied_command_sequence = Some(sequence);
    let after = inventory_view(&candidate, owner)?;
    *session = candidate;
    Ok(InventoryReceipt {
        sequence,
        action,
        before,
        after,
        facts: vec![InventoryFact::QuantityChanged {
            owner,
            item,
            before: before_quantity,
            after: after_quantity,
        }],
    })
}

/// Plans and applies selected unique-equipment leaves inside the caller's private session
/// candidate. Product code selects the weapon and slot first; this adapter owns only the
/// neutral role binding, source validation, and candidate effect application.
///
/// Every leaf is planned and source-validated against the same authoritative candidate state
/// before any effect runs. Effects then execute in order against that one candidate. A later
/// failure is never published because `InventoryService::apply` owns the enclosing candidate and
/// is its only publication point. This permits a product-owned follow-up (weapon disposal after
/// unequip) to remain atomic with the standard equipment mutation without creating a second
/// candidate or an invented inventory endpoint.
pub(crate) fn apply_standard_unique_equipment_operations(
    state: &mut rusty_engine::entity_state::EntityState,
    catalog: &rusty_engine::gameplay_mechanics::MechanicsCatalog,
    owner: EntityId,
    sequence: u64,
    operations: impl IntoIterator<Item = StandardOperation>,
) -> Result<(), InventoryRejection> {
    let operation_id = operation_id(sequence)?;
    let source = source_identity(operation_id.clone())?;
    let role = equipment_role()?;
    let mut plans = Vec::new();

    for operation in operations {
        let requirements = operation.requirements();
        let required_capabilities = requirements
            .iter()
            .find(|requirement| requirement.role() == &role)
            .map(|requirement| requirement.capabilities().to_vec())
            .ok_or_else(|| InventoryRejection::Mechanics {
                reason: "standard unique equipment operation lacks the product equipment role"
                    .to_string(),
            })?;
        let bindings = CapabilityRoleBindings::admit(
            &requirements,
            vec![
                CapabilityRoleBinding::new(role.clone(), owner, required_capabilities).map_err(
                    |error| InventoryRejection::Mechanics {
                        reason: error.to_string(),
                    },
                )?,
            ],
        )
        .map_err(|error| InventoryRejection::Mechanics {
            reason: error.to_string(),
        })?;
        let context = StandardOperationContext::new(operation_id.clone(), source.clone()).map_err(
            |error| InventoryRejection::Mechanics {
                reason: error.to_string(),
            },
        )?;
        let plan = operation
            .plan(
                &bindings,
                &ExactInputBundle::empty(),
                state,
                catalog,
                &context,
            )
            .map_err(|error| InventoryRejection::Mechanics {
                reason: error.to_string(),
            })?;
        plan.validate_source_state(state, catalog)
            .map_err(|error| InventoryRejection::Mechanics {
                reason: error.to_string(),
            })?;
        plans.push(plan);
    }

    for plan in plans {
        let receipt = plan
            .effect()
            .apply_to_candidate(state, catalog)
            .map_err(mechanics_rejection)?;
        if !matches!(receipt, StandardMechanicsReceipt::Equipment(_)) {
            return Err(InventoryRejection::Mechanics {
                reason: "standard unique equipment operation returned a non-equipment receipt"
                    .to_string(),
            });
        }
    }
    Ok(())
}

/// Apply the initially selected weapon after the caller has materialized it. Planning here
/// observes the post-materialization component revisions, so materialization itself remains a
/// no-implicit-equip operation.
pub(crate) fn equip_initial_weapon(
    state: &mut rusty_engine::entity_state::EntityState,
    catalog: &rusty_engine::gameplay_mechanics::MechanicsCatalog,
    owner: EntityId,
    weapon: EntityId,
) -> Result<(), InventoryRejection> {
    apply_standard_unique_equipment_operations(
        state,
        catalog,
        owner,
        0,
        [StandardOperation::EquipUniqueItem {
            role: equipment_role()?,
            item: weapon,
            slots: vec![weapon_slot()],
        }],
    )
}

fn equipment_role() -> Result<CapabilityRoleId, InventoryRejection> {
    CapabilityRoleId::parse("inventory-equipment-owner").map_err(|error| {
        InventoryRejection::Mechanics {
            reason: error.to_string(),
        }
    })
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
                ItemKind::HealthSupply {
                    restore_health: 0,
                    ..
                } | ItemKind::Armor { protection: 0, .. }
            )
            || matches!(
                definition.kind,
                ItemKind::HealthSupply {
                    maximum_health: Some(0),
                    ..
                } | ItemKind::Armor {
                    maximum_armor: Some(0),
                    ..
                } | ItemKind::Armor {
                    absorption_percent: Some(0 | 101..),
                    ..
                } | ItemKind::Armor {
                    absorption_divisor: Some(0),
                    ..
                }
            )
            || matches!(
                definition.kind,
                ItemKind::Armor {
                    absorption_percent: Some(_),
                    absorption_divisor: Some(_),
                    ..
                }
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
                if !session.entities.contains(weapon) {
                    // A reserved-absent slot becomes a real Engine item only when this pickup
                    // succeeds inside InventoryService's existing private session candidate.
                    // The typed receipt preserves catalog/revision provenance for this atomic
                    // product composition; materialization deliberately does not equip it.
                    let receipt = crate::mechanics::materialize_weapon(
                        &mut session.entities,
                        &session.mechanics.catalog,
                        owner,
                        item,
                        weapon,
                    )
                    .map_err(|reason| InventoryRejection::Mechanics { reason })?;
                    debug_assert_eq!(receipt.entity, weapon);
                    debug_assert_eq!(receipt.container, owner);
                } else if session.entities.contained_in(weapon).is_none() {
                    let expected = mechanics_item_id(item)
                        .map_err(|reason| InventoryRejection::Mechanics { reason })?;
                    let existing = session
                        .entities
                        .component::<ItemComponent>(weapon)
                        .map_err(|error| InventoryRejection::Mechanics {
                            reason: error.to_string(),
                        })?
                        .ok_or_else(|| InventoryRejection::Mechanics {
                            reason: format!(
                                "reserved weapon {weapon} for {item} has no canonical item component"
                            ),
                        })?;
                    if existing.definition() != &expected
                        || existing.catalog_version() != session.mechanics.catalog.version()
                    {
                        return Err(InventoryRejection::Mechanics {
                            reason: format!(
                                "reserved weapon {weapon} does not match the admitted catalog item {item}"
                            ),
                        });
                    }
                    // Existing consumed/disposed weapons retain their identity and ItemComponent.
                    // Reacquisition is only the explicit product None-to-owner relationship step;
                    // it neither rematerializes nor destroys the item.
                    crate::mechanics::set_weapon_containment(&mut session.entities, weapon, owner)
                        .map_err(|reason| InventoryRejection::Mechanics { reason })?;
                } else {
                    return Err(InventoryRejection::Mechanics {
                        reason: format!(
                            "reserved weapon {weapon} for {item} is already contained by another owner"
                        ),
                    });
                }
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
                    apply_standard_unique_equipment_operations(
                        &mut session.entities,
                        &session.mechanics.catalog,
                        owner,
                        sequence,
                        [StandardOperation::UnequipUniqueItem {
                            role: equipment_role()?,
                            item: weapon,
                        }],
                    )?;
                    facts.push(InventoryFact::EquippedWeaponChanged {
                        owner,
                        before: Some(item.clone()),
                        after: None,
                    });
                }
                // Standard unequip above has removed every equipment assignment. The remaining
                // owner-to-None containment change is Loading Bay disposal policy, not a
                // TransferUniqueItem (which has two required inventory endpoints).
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
            if let Some(before_item) = &before_view.equipped_weapon {
                let outgoing = runtime
                    .weapon_entities
                    .get(before_item)
                    .copied()
                    .ok_or_else(|| InventoryRejection::Mechanics {
                        reason: format!("missing unique entity for equipped weapon {before_item}"),
                    })?;
                apply_standard_unique_equipment_operations(
                    &mut session.entities,
                    &session.mechanics.catalog,
                    owner,
                    sequence,
                    [StandardOperation::SwapUniqueItem {
                        role: equipment_role()?,
                        outgoing_item: outgoing,
                        incoming_item: incoming,
                        incoming_slots: vec![weapon_slot()],
                    }],
                )?;
            } else {
                apply_standard_unique_equipment_operations(
                    &mut session.entities,
                    &session.mechanics.catalog,
                    owner,
                    sequence,
                    [StandardOperation::EquipUniqueItem {
                        role: equipment_role()?,
                        item: incoming,
                        slots: vec![weapon_slot()],
                    }],
                )?;
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
    mechanics: &rusty_engine::gameplay_mechanics::ItemDefinitionId,
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

fn mechanics_rejection(
    error: rusty_engine::gameplay_mechanics::MechanicsError,
) -> InventoryRejection {
    InventoryRejection::Mechanics {
        reason: error.to_string(),
    }
}

fn standard_stack_rejection(
    error: rusty_engine::gameplay_mechanics::MechanicsError,
    owner: EntityId,
    item: &ItemDefinitionId,
) -> InventoryRejection {
    match error {
        rusty_engine::gameplay_mechanics::MechanicsError::InventoryInsufficientQuantity {
            requested,
            available,
            ..
        } => match (u32::try_from(requested), u32::try_from(available)) {
            (Ok(requested), Ok(current)) => InventoryRejection::QuantityUnderflow {
                item: item.clone(),
                current,
                requested,
            },
            _ => InventoryRejection::Mechanics {
                reason: "standard inventory underflow exceeds product representation".to_string(),
            },
        },
        rusty_engine::gameplay_mechanics::MechanicsError::InventoryCapacityExceeded { .. } => {
            // Engine capacity limits are not part of Loading Bay's catalog. The product's
            // distinct-stack capacity is checked before planning; preserve any future Engine
            // capacity error as an explicit mechanics boundary failure rather than mislabeling
            // it as the product's different slot policy.
            InventoryRejection::Mechanics {
                reason: format!("unexpected Engine inventory capacity rejection for owner {owner}"),
            }
        }
        other => mechanics_rejection(other),
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

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_engine::entity_state::{EntityAuthoringService, EntityDefinition};

    const E1M1: &str = include_str!("../../content/projects/doom-e1m1.project.json");
    const PLAYER: EntityId = EntityId::new(1);

    #[test]
    fn unrelated_admitted_entity_at_a_reserved_weapon_id_rejects_without_publication() {
        let mut runtime =
            crate::runtime::GameRuntime::from_stored_project(E1M1).expect("current E1M1 admission");
        let shotgun = ItemDefinitionId::parse("weapon/shotgun").unwrap();
        let reserved = runtime
            .session()
            .inventories
            .get(&PLAYER)
            .unwrap()
            .weapon_entities
            .get(&shotgun)
            .copied()
            .unwrap();
        let session = runtime.session_mut();
        let authoring_revision = session.entities.revision();
        EntityAuthoringService
            .admit(
                &mut session.entities,
                authoring_revision,
                [EntityDefinition::new(reserved, "unrelated transient")],
            )
            .unwrap();
        let before_revision = session.entities.revision();

        assert!(matches!(
            InventoryService::apply(
                session,
                PLAYER,
                InventoryCommand {
                    sequence: 1,
                    action: InventoryAction::Grant {
                        item: shotgun,
                        quantity: 1,
                    },
                },
            ),
            Err(InventoryRejection::Mechanics { .. })
        ));
        assert_eq!(session.entities.revision(), before_revision);
        assert!(session.entities.contains(reserved));
        assert!(session
            .entities
            .component::<ItemComponent>(reserved)
            .unwrap()
            .is_none());
        assert!(!inventory_view(session, PLAYER)
            .unwrap()
            .stacks
            .iter()
            .any(|stack| stack.item.as_str() == "weapon/shotgun"));
    }
}
