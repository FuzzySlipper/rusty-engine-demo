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
use crate::encounter_program::{
    compile_encounter_programs, EncounterProgramCatalog, StoredEncounterProgram,
};
use crate::enemy_program::{
    compile_enemy_attack_programs, compile_enemy_defeat_programs, EnemyAttackProgramCatalog,
    EnemyDefeatProgramCatalog, StoredEnemyAttackProgram, StoredEnemyDefeatProgram,
};
use crate::explosive_prop_program::{
    compile_explosive_prop_programs, ExplosivePropProgramCatalog, StoredExplosivePropProgram,
};
use crate::floor_action_program::{
    compile_floor_action_programs, FloorActionProgramCatalog, StoredFloorActionProgram,
};
use crate::gameplay_program::{
    compile_gameplay_programs, GameplayProgramCatalog, StoredGameplayProgram,
};
use crate::hazard_program::{compile_hazard_programs, HazardProgramCatalog, StoredHazardProgram};
use crate::inventory::ItemDefinition;
use crate::level_exit_program::{
    compile_level_exit_programs, LevelExitProgramCatalog, StoredLevelExitProgram,
};
use crate::lift_program::{compile_lift_programs, LiftProgramCatalog, StoredLiftProgram};
use crate::pickup_program::{compile_pickup_programs, PickupProgramCatalog, StoredPickupProgram};
use crate::player_program::{
    compile_player_setup_programs, PlayerSetupProgramCatalog, StoredPlayerSetupProgram,
};
use crate::project_admission::authored_item_definition;
use crate::secret_program::{compile_secret_programs, SecretProgramCatalog, StoredSecretProgram};
use crate::stored_project::validate_item_definitions;
use crate::switch_program::{compile_switch_programs, StoredSwitchProgram, SwitchProgramCatalog};

/// The compiled, canonical result of one gameplay package.
#[derive(Debug, Clone)]
pub struct CompiledGameplayPackage {
    pub fingerprint: String,
    pub items: Vec<ItemDefinition>,
    #[allow(dead_code)] // Focused resolver tests execute the compiled catalog.
    pub(crate) gameplay_programs: GameplayProgramCatalog,
    pub gameplay_program_count: usize,
    #[allow(dead_code)] // Bound by project pickup profiles after composition.
    pub(crate) pickup_programs: PickupProgramCatalog,
    pub pickup_program_count: usize,
    #[allow(dead_code)] // Bound by player inventories after project composition.
    pub(crate) player_setup_programs: PlayerSetupProgramCatalog,
    pub player_setup_program_count: usize,
    #[allow(dead_code)] // Bound by enemy profiles after project composition.
    pub(crate) enemy_attack_programs: EnemyAttackProgramCatalog,
    #[allow(dead_code)] // Bound by enemy profiles after project composition.
    pub(crate) enemy_defeat_programs: EnemyDefeatProgramCatalog,
    pub enemy_attack_program_count: usize,
    pub enemy_defeat_program_count: usize,
    #[allow(dead_code)] // Bound by authored environmental components after composition.
    pub(crate) hazard_programs: HazardProgramCatalog,
    pub hazard_program_count: usize,
    #[allow(dead_code)] // Bound by authored explosive-prop components after composition.
    pub(crate) explosive_prop_programs: ExplosivePropProgramCatalog,
    pub explosive_prop_program_count: usize,
    #[allow(dead_code)] // Bound by admitted encounter entities after composition.
    pub(crate) encounter_programs: EncounterProgramCatalog,
    pub encounter_program_count: usize,
    #[allow(dead_code)] // Bound by switch profiles after project composition.
    pub(crate) switch_programs: SwitchProgramCatalog,
    pub switch_program_count: usize,
    #[allow(dead_code)]
    pub(crate) floor_action_programs: FloorActionProgramCatalog,
    pub floor_action_program_count: usize,
    #[allow(dead_code)]
    pub(crate) lift_programs: LiftProgramCatalog,
    pub lift_program_count: usize,
    #[allow(dead_code)] // Bound by admitted secret-region components after composition.
    pub(crate) secret_programs: SecretProgramCatalog,
    pub secret_program_count: usize,
    #[allow(dead_code)] // Bound by admitted level-exit components after composition.
    pub(crate) level_exit_programs: LevelExitProgramCatalog,
    pub level_exit_program_count: usize,
    /// Canonical authored catalog retained for package/project parity checks.
    pub gameplay_program_definitions: Vec<StoredGameplayProgram>,
    pub pickup_program_definitions: Vec<StoredPickupProgram>,
    pub player_setup_program_definitions: Vec<StoredPlayerSetupProgram>,
    pub enemy_attack_program_definitions: Vec<StoredEnemyAttackProgram>,
    pub enemy_defeat_program_definitions: Vec<StoredEnemyDefeatProgram>,
    pub hazard_program_definitions: Vec<StoredHazardProgram>,
    pub explosive_prop_program_definitions: Vec<StoredExplosivePropProgram>,
    pub encounter_program_definitions: Vec<StoredEncounterProgram>,
    pub switch_program_definitions: Vec<StoredSwitchProgram>,
    pub floor_action_program_definitions: Vec<StoredFloorActionProgram>,
    pub lift_program_definitions: Vec<StoredLiftProgram>,
    pub secret_program_definitions: Vec<StoredSecretProgram>,
    pub level_exit_program_definitions: Vec<StoredLevelExitProgram>,
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
    GameplayProgram(String),
    PlayerSetupProgram(String),
    EnemyProgram(String),
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
            Self::GameplayProgram(message) => {
                write!(formatter, "gameplay program rejected: {message}")
            }
            Self::PlayerSetupProgram(message) => {
                write!(formatter, "player setup program rejected: {message}")
            }
            Self::EnemyProgram(message) => write!(formatter, "enemy program rejected: {message}"),
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
    let gameplay_programs = compile_gameplay_programs(&authored.gameplay_programs)
        .map_err(|error| GameplayCompileError::GameplayProgram(error.to_string()))?;
    let pickup_programs = compile_pickup_programs(&authored.pickup_programs)
        .map_err(|error| GameplayCompileError::GameplayProgram(error.to_string()))?;
    let player_setup_programs = compile_player_setup_programs(&authored.player_setup_programs)
        .map_err(|error| GameplayCompileError::PlayerSetupProgram(error.to_string()))?;
    let enemy_attack_programs = compile_enemy_attack_programs(&authored.enemy_attack_programs)
        .map_err(|error| GameplayCompileError::EnemyProgram(error.to_string()))?;
    let enemy_defeat_programs = compile_enemy_defeat_programs(&authored.enemy_defeat_programs)
        .map_err(|error| GameplayCompileError::EnemyProgram(error.to_string()))?;
    let hazard_programs = compile_hazard_programs(&authored.hazard_programs)
        .map_err(|error| GameplayCompileError::GameplayProgram(error.to_string()))?;
    let explosive_prop_programs =
        compile_explosive_prop_programs(&authored.explosive_prop_programs)
            .map_err(|error| GameplayCompileError::GameplayProgram(error.to_string()))?;
    let encounter_programs = compile_encounter_programs(&authored.encounter_programs)
        .map_err(|error| GameplayCompileError::GameplayProgram(error.to_string()))?;
    let switch_programs = compile_switch_programs(&authored.switch_programs)
        .map_err(|error| GameplayCompileError::GameplayProgram(error.to_string()))?;
    let floor_action_programs = compile_floor_action_programs(&authored.floor_action_programs)
        .map_err(|error| GameplayCompileError::GameplayProgram(error.to_string()))?;
    let lift_programs = compile_lift_programs(&authored.lift_programs)
        .map_err(|error| GameplayCompileError::GameplayProgram(error.to_string()))?;
    let secret_programs = compile_secret_programs(&authored.secret_programs)
        .map_err(|error| GameplayCompileError::GameplayProgram(error.to_string()))?;
    let level_exit_programs = compile_level_exit_programs(&authored.level_exit_programs)
        .map_err(|error| GameplayCompileError::GameplayProgram(error.to_string()))?;

    Ok(CompiledGameplayPackage {
        fingerprint: package.fingerprint().to_string(),
        items,
        gameplay_program_count: gameplay_programs.len(),
        gameplay_programs,
        gameplay_program_definitions: authored.gameplay_programs,
        pickup_program_count: pickup_programs.len(),
        pickup_programs,
        pickup_program_definitions: authored.pickup_programs,
        player_setup_program_count: player_setup_programs.len(),
        player_setup_programs,
        player_setup_program_definitions: authored.player_setup_programs,
        enemy_attack_program_count: enemy_attack_programs.len(),
        enemy_defeat_program_count: enemy_defeat_programs.len(),
        enemy_attack_programs,
        enemy_defeat_programs,
        enemy_attack_program_definitions: authored.enemy_attack_programs,
        enemy_defeat_program_definitions: authored.enemy_defeat_programs,
        hazard_program_count: hazard_programs.len(),
        hazard_programs,
        hazard_program_definitions: authored.hazard_programs,
        explosive_prop_program_count: explosive_prop_programs.len(),
        explosive_prop_programs,
        explosive_prop_program_definitions: authored.explosive_prop_programs,
        encounter_program_count: encounter_programs.len(),
        encounter_programs,
        encounter_program_definitions: authored.encounter_programs,
        switch_program_count: switch_programs.len(),
        switch_programs,
        switch_program_definitions: authored.switch_programs,
        floor_action_program_count: floor_action_programs.len(),
        floor_action_programs,
        floor_action_program_definitions: authored.floor_action_programs,
        lift_program_count: lift_programs.len(),
        lift_programs,
        lift_program_definitions: authored.lift_programs,
        secret_program_count: secret_programs.len(),
        secret_programs,
        secret_program_definitions: authored.secret_programs,
        level_exit_program_count: level_exit_programs.len(),
        level_exit_programs,
        level_exit_program_definitions: authored.level_exit_programs,
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
            "payload": {
                "schemaVersion": 1,
                "items": items,
                "gameplayPrograms": [],
                "pickupPrograms": [],
                "enemyAttackPrograms": [],
                "enemyDefeatPrograms": [],
            },
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
        assert_eq!(compiled.gameplay_program_count, 4);
        assert_eq!(compiled.pickup_program_count, 4);
        assert_eq!(compiled.enemy_attack_program_count, 2);
        assert_eq!(compiled.enemy_defeat_program_count, 2);
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
