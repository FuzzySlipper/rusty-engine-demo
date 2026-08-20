//! Closed authored player setup programs.
//!
//! Player setup is deliberately its own small family.  The program only
//! describes source-ordered initial grants and equipment selection; Rust
//! resolves it to the real inventory/equipment configuration before a session
//! is created.  It is not a live command surface or a generic behavior IR.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::inventory::{
    inventory_from_config, InventoryAdmissionError, InventoryConfig, InventoryStack,
    ItemDefinition, ItemDefinitionId, ItemKind,
};

pub const MAX_PLAYER_SETUP_PROGRAMS: usize = 16;
pub const MAX_PLAYER_SETUP_OPERATIONS: usize = 16;
pub const MAX_PLAYER_SETUP_PROGRAM_BINDINGS: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredPlayerSetupProgram {
    pub id: String,
    /// A flat sequence is intentional: each operation observes the candidate
    /// produced by its predecessors.
    pub program: Vec<StoredPlayerSetupOperation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredPlayerSetupOperation {
    GrantItem { item: String, quantity: u32 },
    EquipInitialWeapon { item: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PlayerSetupOperation {
    GrantItem {
        item: ItemDefinitionId,
        quantity: u32,
    },
    EquipInitialWeapon {
        item: ItemDefinitionId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlayerSetupProgram {
    operations: Vec<PlayerSetupOperation>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PlayerSetupProgramCatalog {
    programs: BTreeMap<String, PlayerSetupProgram>,
}

/// Read-only player-setup catalog for product/resource transparency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSetupProgramReadout {
    pub programs: Vec<PlayerSetupProgramShape>,
    pub bindings: Vec<PlayerSetupProgramBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSetupProgramShape {
    pub id: String,
    pub operations: Vec<PlayerSetupProgramOperationReadout>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PlayerSetupProgramOperationReadout {
    GrantItem { item: String, quantity: u32 },
    EquipInitialWeapon { item: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSetupProgramBinding {
    pub player: u64,
    pub program_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PlayerSetupProgramCompileError {
    TooMany { count: usize },
    DuplicateId { id: String },
    InvalidId { id: String },
    EmptyProgram { id: String },
    TooManyOperations { id: String, count: usize },
    InvalidItem { id: String, item: String },
    ZeroQuantity { id: String, item: String },
}

impl std::fmt::Display for PlayerSetupProgramCompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooMany { count } => {
                write!(formatter, "player setup program quota exceeded: {count}")
            }
            Self::DuplicateId { id } => write!(formatter, "duplicate player setup program `{id}`"),
            Self::InvalidId { id } => write!(formatter, "invalid player setup program id `{id}`"),
            Self::EmptyProgram { id } => write!(formatter, "player setup program `{id}` is empty"),
            Self::TooManyOperations { id, count } => {
                write!(
                    formatter,
                    "player setup program `{id}` has {count} operations"
                )
            }
            Self::InvalidItem { id, item } => {
                write!(
                    formatter,
                    "player setup program `{id}` has invalid item `{item}`"
                )
            }
            Self::ZeroQuantity { id, item } => {
                write!(
                    formatter,
                    "player setup program `{id}` grants zero of `{item}`"
                )
            }
        }
    }
}

impl std::error::Error for PlayerSetupProgramCompileError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PlayerSetupProgramAdmissionError {
    MissingItem {
        item: ItemDefinitionId,
    },
    WeaponNotInSlots {
        item: ItemDefinitionId,
    },
    QuantityOverflow {
        item: ItemDefinitionId,
        quantity: u32,
        limit: u32,
    },
    InventoryFull {
        capacity_slots: usize,
    },
    EquipBeforeGrant {
        item: ItemDefinitionId,
    },
    EquipNonWeapon {
        item: ItemDefinitionId,
    },
    EquipNotInSlots {
        item: ItemDefinitionId,
    },
    Inventory(InventoryAdmissionError),
}

impl std::fmt::Display for PlayerSetupProgramAdmissionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingItem { item } => {
                write!(formatter, "player setup references missing item `{item}`")
            }
            Self::WeaponNotInSlots { item } => write!(
                formatter,
                "player setup grants weapon `{item}` that is absent from authored weapon slots"
            ),
            Self::QuantityOverflow {
                item,
                quantity,
                limit,
            } => write!(
                formatter,
                "player setup quantity {quantity} for `{item}` exceeds maximum {limit}"
            ),
            Self::InventoryFull { capacity_slots } => write!(
                formatter,
                "player setup exceeds inventory capacity of {capacity_slots} slots"
            ),
            Self::EquipBeforeGrant { item } => write!(
                formatter,
                "player setup equips `{item}` before an earlier grant owns it"
            ),
            Self::EquipNonWeapon { item } => {
                write!(formatter, "player setup equips non-weapon item `{item}`")
            }
            Self::EquipNotInSlots { item } => write!(
                formatter,
                "player setup equips weapon `{item}` outside authored weapon slots"
            ),
            Self::Inventory(error) => write!(formatter, "inventory admission rejected: {error:?}"),
        }
    }
}

impl std::error::Error for PlayerSetupProgramAdmissionError {}

impl PlayerSetupProgramCatalog {
    pub(crate) fn get(&self, id: &str) -> Option<&PlayerSetupProgram> {
        self.programs.get(id)
    }

    pub(crate) fn len(&self) -> usize {
        self.programs.len()
    }

    pub(crate) fn readout(
        &self,
        bindings: impl Iterator<Item = (u64, String)>,
    ) -> PlayerSetupProgramReadout {
        PlayerSetupProgramReadout {
            programs: self
                .programs
                .iter()
                .map(|(id, program)| PlayerSetupProgramShape {
                    id: id.clone(),
                    operations: program
                        .operations
                        .iter()
                        .map(|operation| match operation {
                            PlayerSetupOperation::GrantItem { item, quantity } => {
                                PlayerSetupProgramOperationReadout::GrantItem {
                                    item: item.as_str().to_owned(),
                                    quantity: *quantity,
                                }
                            }
                            PlayerSetupOperation::EquipInitialWeapon { item } => {
                                PlayerSetupProgramOperationReadout::EquipInitialWeapon {
                                    item: item.as_str().to_owned(),
                                }
                            }
                        })
                        .collect(),
                })
                .collect(),
            bindings: bindings
                .take(MAX_PLAYER_SETUP_PROGRAM_BINDINGS)
                .map(|(player, program_id)| PlayerSetupProgramBinding { player, program_id })
                .collect(),
        }
    }
}

pub(crate) fn compile_player_setup_programs(
    authored: &[StoredPlayerSetupProgram],
) -> Result<PlayerSetupProgramCatalog, PlayerSetupProgramCompileError> {
    if authored.len() > MAX_PLAYER_SETUP_PROGRAMS {
        return Err(PlayerSetupProgramCompileError::TooMany {
            count: authored.len(),
        });
    }
    let mut programs = BTreeMap::new();
    for stored in authored {
        if !is_program_id(&stored.id) {
            return Err(PlayerSetupProgramCompileError::InvalidId {
                id: stored.id.clone(),
            });
        }
        if stored.program.is_empty() {
            return Err(PlayerSetupProgramCompileError::EmptyProgram {
                id: stored.id.clone(),
            });
        }
        if stored.program.len() > MAX_PLAYER_SETUP_OPERATIONS {
            return Err(PlayerSetupProgramCompileError::TooManyOperations {
                id: stored.id.clone(),
                count: stored.program.len(),
            });
        }
        let operations = stored
            .program
            .iter()
            .map(|operation| compile_operation(&stored.id, operation))
            .collect::<Result<Vec<_>, _>>()?;
        if programs
            .insert(stored.id.clone(), PlayerSetupProgram { operations })
            .is_some()
        {
            return Err(PlayerSetupProgramCompileError::DuplicateId {
                id: stored.id.clone(),
            });
        }
    }
    Ok(PlayerSetupProgramCatalog { programs })
}

/// Resolve source-ordered setup operations into the one initial inventory
/// configuration mechanics will admit.  No session exists until this returns
/// successfully, so malformed programs cannot leave partial state behind.
pub(crate) fn resolve_player_setup_program(
    program: &PlayerSetupProgram,
    capacity_slots: usize,
    weapon_slots: Vec<ItemDefinitionId>,
    definitions: &BTreeMap<ItemDefinitionId, ItemDefinition>,
) -> Result<InventoryConfig, PlayerSetupProgramAdmissionError> {
    let mut slots = BTreeSet::new();
    for item in &weapon_slots {
        let Some(definition) = definitions.get(item) else {
            return Err(PlayerSetupProgramAdmissionError::MissingItem { item: item.clone() });
        };
        if !matches!(definition.kind, ItemKind::Weapon(_)) {
            return Err(PlayerSetupProgramAdmissionError::EquipNonWeapon { item: item.clone() });
        }
        // The normal inventory admission owns duplicate-slot diagnostics.  A
        // duplicate still cannot make an otherwise forbidden grant valid.
        slots.insert(item.clone());
    }

    let mut stacks = Vec::<InventoryStack>::new();
    let mut positions = BTreeMap::<ItemDefinitionId, usize>::new();
    let mut equipped = None;
    for operation in &program.operations {
        match operation {
            PlayerSetupOperation::GrantItem { item, quantity } => {
                let definition = definitions.get(item).ok_or_else(|| {
                    PlayerSetupProgramAdmissionError::MissingItem { item: item.clone() }
                })?;
                if matches!(definition.kind, ItemKind::Weapon(_)) && !slots.contains(item) {
                    return Err(PlayerSetupProgramAdmissionError::WeaponNotInSlots {
                        item: item.clone(),
                    });
                }
                if let Some(index) = positions.get(item).copied() {
                    let current = stacks[index].quantity;
                    let next = current.checked_add(*quantity).ok_or_else(|| {
                        PlayerSetupProgramAdmissionError::QuantityOverflow {
                            item: item.clone(),
                            quantity: u32::MAX,
                            limit: definition.max_quantity,
                        }
                    })?;
                    if next > definition.max_quantity {
                        return Err(PlayerSetupProgramAdmissionError::QuantityOverflow {
                            item: item.clone(),
                            quantity: next,
                            limit: definition.max_quantity,
                        });
                    }
                    stacks[index].quantity = next;
                } else {
                    if stacks.len() == capacity_slots {
                        return Err(PlayerSetupProgramAdmissionError::InventoryFull {
                            capacity_slots,
                        });
                    }
                    if *quantity > definition.max_quantity {
                        return Err(PlayerSetupProgramAdmissionError::QuantityOverflow {
                            item: item.clone(),
                            quantity: *quantity,
                            limit: definition.max_quantity,
                        });
                    }
                    positions.insert(item.clone(), stacks.len());
                    stacks.push(InventoryStack::new(item.clone(), *quantity));
                }
            }
            PlayerSetupOperation::EquipInitialWeapon { item } => {
                let definition = definitions.get(item).ok_or_else(|| {
                    PlayerSetupProgramAdmissionError::MissingItem { item: item.clone() }
                })?;
                if !matches!(definition.kind, ItemKind::Weapon(_)) {
                    return Err(PlayerSetupProgramAdmissionError::EquipNonWeapon {
                        item: item.clone(),
                    });
                }
                if !slots.contains(item) {
                    return Err(PlayerSetupProgramAdmissionError::EquipNotInSlots {
                        item: item.clone(),
                    });
                }
                if positions
                    .get(item)
                    .is_none_or(|index| stacks[*index].quantity == 0)
                {
                    return Err(PlayerSetupProgramAdmissionError::EquipBeforeGrant {
                        item: item.clone(),
                    });
                }
                equipped = Some(item.clone());
            }
        }
    }

    let config = InventoryConfig::new(capacity_slots, stacks, equipped, weapon_slots);
    inventory_from_config(
        rusty_engine::core_ids::EntityId::new(0),
        &config,
        definitions,
    )
    .map_err(PlayerSetupProgramAdmissionError::Inventory)?;
    Ok(config)
}

fn compile_operation(
    program_id: &str,
    operation: &StoredPlayerSetupOperation,
) -> Result<PlayerSetupOperation, PlayerSetupProgramCompileError> {
    match operation {
        StoredPlayerSetupOperation::GrantItem { item, quantity } => {
            let item_id = ItemDefinitionId::parse(item.clone()).map_err(|_| {
                PlayerSetupProgramCompileError::InvalidItem {
                    id: program_id.to_owned(),
                    item: item.clone(),
                }
            })?;
            if *quantity == 0 {
                return Err(PlayerSetupProgramCompileError::ZeroQuantity {
                    id: program_id.to_owned(),
                    item: item.clone(),
                });
            }
            Ok(PlayerSetupOperation::GrantItem {
                item: item_id,
                quantity: *quantity,
            })
        }
        StoredPlayerSetupOperation::EquipInitialWeapon { item } => {
            Ok(PlayerSetupOperation::EquipInitialWeapon {
                item: ItemDefinitionId::parse(item.clone()).map_err(|_| {
                    PlayerSetupProgramCompileError::InvalidItem {
                        id: program_id.to_owned(),
                        item: item.clone(),
                    }
                })?,
            })
        }
    }
}

fn is_program_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.split('/').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{ItemDefinition, ItemKind, WeaponAttackMode, WeaponDefinition};
    use rusty_engine::core_math::Vec3;

    fn weapon(id: &str) -> ItemDefinition {
        ItemDefinition::new(
            ItemDefinitionId::parse(id).unwrap(),
            ItemKind::Weapon(WeaponDefinition {
                attack_mode: WeaponAttackMode::Hitscan,
                repeat_while_held: false,
                damage_rolls: 1,
                damage: 1,
                max_distance: 1.0,
                cooldown_ticks: 1,
                ammunition: ItemDefinitionId::parse("ammo/bullets").unwrap(),
                ammunition_cost: 0,
                muzzle_offset: Vec3::ZERO,
                presentation: "test".to_owned(),
                projectile: None,
            }),
            1,
        )
    }

    fn definitions() -> BTreeMap<ItemDefinitionId, ItemDefinition> {
        [
            weapon("weapon/pistol"),
            ItemDefinition::new(
                ItemDefinitionId::parse("ammo/bullets").unwrap(),
                ItemKind::Ammunition,
                200,
            ),
        ]
        .into_iter()
        .map(|definition| (definition.id.clone(), definition))
        .collect()
    }

    #[test]
    fn source_order_rejects_equip_before_grant() {
        let catalog = compile_player_setup_programs(&[StoredPlayerSetupProgram {
            id: "player/test".to_owned(),
            program: vec![
                StoredPlayerSetupOperation::EquipInitialWeapon {
                    item: "weapon/pistol".to_owned(),
                },
                StoredPlayerSetupOperation::GrantItem {
                    item: "weapon/pistol".to_owned(),
                    quantity: 1,
                },
            ],
        }])
        .unwrap();
        let error = resolve_player_setup_program(
            catalog.get("player/test").unwrap(),
            2,
            vec![ItemDefinitionId::parse("weapon/pistol").unwrap()],
            &definitions(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            PlayerSetupProgramAdmissionError::EquipBeforeGrant { .. }
        ));
    }

    #[test]
    fn weapon_grants_require_an_authored_slot() {
        let catalog = compile_player_setup_programs(&[StoredPlayerSetupProgram {
            id: "player/test".to_owned(),
            program: vec![StoredPlayerSetupOperation::GrantItem {
                item: "weapon/pistol".to_owned(),
                quantity: 1,
            }],
        }])
        .unwrap();
        let error = resolve_player_setup_program(
            catalog.get("player/test").unwrap(),
            2,
            Vec::new(),
            &definitions(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            PlayerSetupProgramAdmissionError::WeaponNotInSlots { .. }
        ));
    }

    #[test]
    fn rejects_gameplay_pickup_and_enemy_operation_vocabularies() {
        let candidate = serde_json::json!({
            "id": "player/test",
            "program": [{ "kind": "operation", "operation": "grantPickedItem" }]
        });
        assert!(serde_json::from_value::<StoredPlayerSetupProgram>(candidate).is_err());
    }
}
