//! Semantic compiler for Loading Bay gameplay packages (guide `compile.rs`
//! role): admits the Engine envelope, decodes the downstream payload, rejects
//! unknown or inconsistent meanings, resolves references, and constructs the
//! canonical item definitions through the SAME conversion project admission
//! uses. Only this canonical result may enter live gameplay.

use rusty_engine::gameplay_rules::decode_rule_package;

use crate::authored::{
    AuthoredGameplayPayload, LOADING_BAY_GAMEPLAY_DOMAIN, LOADING_BAY_GAMEPLAY_SCHEMA_VERSION,
    MAX_AUTHORED_ITEMS,
};
use crate::inventory::ItemDefinition;
use crate::project_admission::authored_item_definition;
use crate::stored_project::validate_item_definitions;

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
    /// A candidate violated the shared item admission invariants
    /// (`stored_project::validate_item_definitions`).
    ItemInvariants(String),
    /// Conversion from a validated candidate failed. Unreachable while the
    /// validator and conversion agree; kept so conversion can never panic.
    InvalidItem {
        index: usize,
        reason: String,
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
            Self::ItemInvariants(message) => {
                write!(formatter, "item invariants rejected: {message}")
            }
            Self::InvalidItem { index, reason } => {
                write!(formatter, "item[{index}] rejected: {reason}")
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

    // The SAME admission invariants project admission enforces: id grammar,
    // quantity/effect limits, uniqueness, weapon shape completeness, and
    // weapon -> ammunition reference resolution. Running this before
    // conversion backs the `expect` calls in `authored_item_definition`
    // on the package path exactly as the project path does.
    validate_item_definitions(&authored.items).map_err(|error| {
        GameplayCompileError::ItemInvariants(error.diagnostic().message.clone())
    })?;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn ammunition(id: &str, max_quantity: u32) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "maxQuantity": max_quantity,
            "kind": { "kind": "ammunition" },
        })
    }

    fn hitscan_weapon(id: &str, ammunition: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "maxQuantity": 1,
            "kind": {
                "kind": "weapon",
                "ammunition": ammunition,
                "attackMode": "hitscan",
                "damage": 5,
                "maxDistance": 128.0,
                "cooldownTicks": 24,
                "ammunitionCost": 1,
                "muzzleOffset": [0.0, 0.0, 0.0],
                "presentation": "test",
            },
        })
    }

    /// Wraps candidate items in the same schema-2 binary64 envelope shape the
    /// TS authoring workspace materializes.
    fn package_bytes(items: &[serde_json::Value]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "kind": "rusty.gameplay-rules.package",
            "schemaVersion": 2,
            "domain": "loading-bay",
            "package": "e1m1-core",
            "version": 1,
            "dependencies": [],
            "sources": [
                { "id": "items", "path": "gameplay/authoring/src/catalogs/items.ts" },
            ],
            "provenance": [],
            "payload": { "schemaVersion": 1, "items": items },
        }))
        .expect("test package serializes")
    }

    fn compile(
        items: &[serde_json::Value],
    ) -> Result<CompiledGameplayPackage, GameplayCompileError> {
        compile_gameplay_package(&package_bytes(items), "e1m1-core")
    }

    #[test]
    fn valid_candidates_compile() {
        let compiled = compile(&[
            ammunition("ammo/test-bullets", 200),
            hitscan_weapon("weapon/test-pistol", "ammo/test-bullets"),
        ])
        .expect("valid package compiles");
        assert_eq!(compiled.items.len(), 2);
    }

    #[test]
    fn committed_artifact_compiles() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../data/gameplay/loading-bay-e1m1-core.package.json");
        let bytes = std::fs::read(&path).expect("committed package artifact exists");
        let compiled =
            compile_gameplay_package(&bytes, "e1m1-core").expect("committed artifact compiles");
        assert_eq!(compiled.items.len(), 11);
    }

    #[test]
    fn weapon_without_attack_mode_is_rejected_not_panicked() {
        // Regression: this shape used to reach `authored_item_definition`
        // unvalidated and panic on its `expect("validated …")` call.
        let weapon = serde_json::json!({
            "id": "weapon/test-pistol",
            "maxQuantity": 1,
            "kind": {
                "kind": "weapon",
                "ammunition": "ammo/test-bullets",
                "damage": 5,
                "maxDistance": 128.0,
                "cooldownTicks": 24,
                "ammunitionCost": 1,
                "muzzleOffset": [0.0, 0.0, 0.0],
                "presentation": "test",
            },
        });
        let error = compile(&[ammunition("ammo/test-bullets", 200), weapon])
            .expect_err("attack-mode-less weapon must be rejected");
        assert!(
            matches!(error, GameplayCompileError::ItemInvariants(_)),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn spread_weapon_without_pellets_is_rejected() {
        let weapon = serde_json::json!({
            "id": "weapon/test-shotgun",
            "maxQuantity": 1,
            "kind": {
                "kind": "weapon",
                "ammunition": "ammo/test-shells",
                "attackMode": "spread",
                "spreadDegrees": 5.625,
                "damage": 5,
                "maxDistance": 128.0,
                "cooldownTicks": 63,
                "ammunitionCost": 1,
                "muzzleOffset": [0.0, 0.0, 0.0],
                "presentation": "test",
            },
        });
        let error = compile(&[ammunition("ammo/test-shells", 50), weapon])
            .expect_err("spread weapon without pelletCount must be rejected");
        assert!(matches!(error, GameplayCompileError::ItemInvariants(_)));
    }

    #[test]
    fn zero_max_quantity_is_rejected() {
        let error = compile(&[ammunition("ammo/test-bullets", 0)])
            .expect_err("zero maxQuantity must be rejected");
        assert!(matches!(error, GameplayCompileError::ItemInvariants(_)));
    }

    #[test]
    fn weapon_with_stackable_quantity_is_rejected() {
        let mut weapon = hitscan_weapon("weapon/test-pistol", "ammo/test-bullets");
        weapon["maxQuantity"] = serde_json::json!(2);
        let error = compile(&[ammunition("ammo/test-bullets", 200), weapon])
            .expect_err("weapon maxQuantity != 1 must be rejected");
        assert!(matches!(error, GameplayCompileError::ItemInvariants(_)));
    }

    #[test]
    fn zero_effect_armor_is_rejected() {
        let armor = serde_json::json!({
            "id": "armor/test",
            "maxQuantity": 1,
            "kind": {
                "kind": "armor",
                "protection": 0,
                "maximumArmor": 100,
                "absorptionDivisor": 3,
                "transition": "replace",
            },
        });
        let error = compile(&[armor]).expect_err("armor with zero protection must be rejected");
        assert!(matches!(error, GameplayCompileError::ItemInvariants(_)));
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let error = compile(&[
            ammunition("ammo/test-bullets", 200),
            ammunition("ammo/test-bullets", 50),
        ])
        .expect_err("duplicate ids must be rejected");
        assert!(matches!(error, GameplayCompileError::ItemInvariants(_)));
    }

    #[test]
    fn unknown_ammunition_reference_is_rejected() {
        let error = compile(&[hitscan_weapon("weapon/test-pistol", "ammo/missing")])
            .expect_err("weapon referencing unknown ammunition must be rejected");
        assert!(matches!(error, GameplayCompileError::ItemInvariants(_)));
    }
}
