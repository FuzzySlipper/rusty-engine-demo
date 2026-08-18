//! Semantic compiler for Loading Bay gameplay packages (guide `compile.rs`
//! role): admits the Engine envelope, decodes the downstream payload, rejects
//! unknown or inconsistent meanings, resolves references, and constructs the
//! canonical item definitions through the SAME conversion project admission
//! uses. Only this canonical result may enter live gameplay.

use std::collections::BTreeSet;

use rusty_engine::gameplay_rules::decode_rule_package;

use crate::authored::{
    AuthoredGameplayPayload, LOADING_BAY_GAMEPLAY_DOMAIN, LOADING_BAY_GAMEPLAY_SCHEMA_VERSION,
    MAX_AUTHORED_ITEMS, MAX_AUTHORED_ITEM_ID_BYTES,
};
use crate::inventory::ItemDefinition;
use crate::project_admission::authored_item_definition;
use crate::stored_project::StoredItemKind;

/// The compiled, canonical result of one gameplay package.
#[derive(Debug, Clone)]
pub struct CompiledGameplayPackage {
    pub fingerprint: String,
    pub items: Vec<ItemDefinition>,
}

#[derive(Debug)]
pub enum GameplayCompileError {
    Package(String),
    WrongPackage {
        domain: String,
        package: String,
    },
    Payload(String),
    UnsupportedSchema {
        actual: u64,
        expected: u64,
    },
    Quota {
        section: &'static str,
        count: usize,
        limit: usize,
    },
    InvalidItem {
        index: usize,
        reason: String,
    },
    DuplicateItem {
        id: String,
    },
    UnknownReference {
        item: String,
        ammunition: String,
    },
}

impl std::fmt::Display for GameplayCompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Package(message) => write!(formatter, "rule package rejected: {message}"),
            Self::WrongPackage { domain, package } => {
                write!(formatter, "unexpected package {domain}/{package}")
            }
            Self::Payload(message) => write!(formatter, "payload decode failed: {message}"),
            Self::UnsupportedSchema { actual, expected } => {
                write!(formatter, "payload schema {actual} != supported {expected}")
            }
            Self::Quota {
                section,
                count,
                limit,
            } => write!(formatter, "{section} quota exceeded: {count} > {limit}"),
            Self::InvalidItem { index, reason } => {
                write!(formatter, "item[{index}] rejected: {reason}")
            }
            Self::DuplicateItem { id } => write!(formatter, "duplicate item id {id}"),
            Self::UnknownReference { item, ammunition } => {
                write!(
                    formatter,
                    "item {item} references unknown ammunition {ammunition}"
                )
            }
        }
    }
}

impl std::error::Error for GameplayCompileError {}

/// Compile one canonical gameplay package artifact into canonical item
/// definitions. Fail-atomic: an error yields no partial catalog.
pub fn compile_gameplay_package(
    input: &[u8],
    expected_package: &str,
) -> Result<CompiledGameplayPackage, GameplayCompileError> {
    let package = decode_rule_package(input)
        .map_err(|error| GameplayCompileError::Package(error.to_string()))?;
    let identity = package.identity();
    if identity.domain().as_str() != LOADING_BAY_GAMEPLAY_DOMAIN
        || identity.package().as_str() != expected_package
    {
        return Err(GameplayCompileError::WrongPackage {
            domain: identity.domain().as_str().to_owned(),
            package: identity.package().as_str().to_owned(),
        });
    }

    let mut payload_value = package.payload().clone();
    crate::authored::normalize_binary64_integers(&mut payload_value);
    let authored: AuthoredGameplayPayload = serde_json::from_value(payload_value)
        .map_err(|error| GameplayCompileError::Payload(error.to_string()))?;
    if authored.schema_version != LOADING_BAY_GAMEPLAY_SCHEMA_VERSION {
        return Err(GameplayCompileError::UnsupportedSchema {
            actual: authored.schema_version,
            expected: LOADING_BAY_GAMEPLAY_SCHEMA_VERSION,
        });
    }
    if authored.items.len() > MAX_AUTHORED_ITEMS {
        return Err(GameplayCompileError::Quota {
            section: "items",
            count: authored.items.len(),
            limit: MAX_AUTHORED_ITEMS,
        });
    }

    // Identity validity, uniqueness, and reference resolution — facts the
    // envelope cannot know.
    let mut seen = BTreeSet::new();
    for (index, item) in authored.items.iter().enumerate() {
        if item.id.is_empty() || item.id.len() > MAX_AUTHORED_ITEM_ID_BYTES {
            return Err(GameplayCompileError::InvalidItem {
                index,
                reason: format!(
                    "id length {} outside 1..={MAX_AUTHORED_ITEM_ID_BYTES}",
                    item.id.len()
                ),
            });
        }
        if !seen.insert(item.id.clone()) {
            return Err(GameplayCompileError::DuplicateItem {
                id: item.id.clone(),
            });
        }
        if let StoredItemKind::Weapon { ammunition, .. } = &item.kind {
            if !authored.items.iter().any(|candidate| {
                candidate.id == *ammunition && matches!(candidate.kind, StoredItemKind::Ammunition)
            }) {
                return Err(GameplayCompileError::UnknownReference {
                    item: item.id.clone(),
                    ammunition: ammunition.clone(),
                });
            }
        }
    }

    // Single semantic owner: the same conversion project admission applies.
    let items = authored
        .items
        .iter()
        .enumerate()
        .map(|(index, definition)| {
            authored_item_definition(definition, index).map_err(|error| {
                GameplayCompileError::InvalidItem {
                    index,
                    reason: error.diagnostic().message.clone(),
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(CompiledGameplayPackage {
        fingerprint: package.fingerprint().to_string(),
        items,
    })
}
