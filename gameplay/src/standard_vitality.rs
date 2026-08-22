//! Loading Bay's narrow, typed extension over the standard gameplay route.
//!
//! TypeScript authors the bounded artifact with Engine's generated DSL. Rust
//! admits its canonical package and compiles its opaque payload into this named
//! Doom policy; it never becomes a generic evaluator or a live state owner.

use rusty_engine::gameplay_rules::decode_rule_package;
use rusty_engine::gameplay_standard::{
    admit_standard_extension, compile_standard_extension, decode_standard_extension,
    CapabilityRequirementId, CompileStandardExtension, StandardExtensionArtifact,
    StandardExtensionCompileError, StandardExtensionError, StandardExtensionSchema,
    StandardPackageContext,
};
use serde::Deserialize;

pub const DOOM_VITALITY_NAMESPACE: &str = "loading-bay.vitality";
pub const DOOM_VITALITY_KIND: &str = "doom.vitality-policy";
pub const DOOM_VITALITY_SUBJECT: &str = "doom-e1m1-vitality";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DoomVitalityPolicy {
    pub maximum_health: u32,
    pub maximum_armor: u32,
}

impl DoomVitalityPolicy {
    /// Compatibility limits for direct gameplay construction and historical
    /// snapshots that have no product package admission edge. Product hosts
    /// must pass the separately admitted policy instead.
    pub const fn doom_compatibility() -> Self {
        Self {
            maximum_health: 1_000_000,
            maximum_armor: 1_000_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedDoomVitalityPolicy {
    policy: DoomVitalityPolicy,
    fingerprint: String,
}

impl AdmittedDoomVitalityPolicy {
    pub const fn policy(&self) -> DoomVitalityPolicy {
        self.policy
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

pub fn admit_doom_vitality_policy(
    bytes: &[u8],
) -> Result<AdmittedDoomVitalityPolicy, DoomVitalityPolicyError> {
    let package = decode_rule_package(bytes).map_err(DoomVitalityPolicyError::Package)?;
    let artifact =
        decode_standard_extension(&package).map_err(DoomVitalityPolicyError::Standard)?;
    let context = StandardPackageContext::new(
        package.schema_version(),
        package.identity().domain().clone(),
        package.identity().package().clone(),
        package.identity().version(),
        package.dependencies().to_vec(),
        package.sources().to_vec(),
        package.provenance().to_vec(),
    );
    let admitted =
        admit_standard_extension(&context, artifact).map_err(DoomVitalityPolicyError::Standard)?;
    let compiler = DoomVitalityPolicyCompiler::new()?;
    let policy = compile_standard_extension(&admitted, &compiler)
        .map_err(DoomVitalityPolicyError::Compilation)?
        .into_output();
    if policy.maximum_health == 0 || policy.maximum_armor == 0 {
        return Err(DoomVitalityPolicyError::InvalidBounds);
    }
    Ok(AdmittedDoomVitalityPolicy {
        policy,
        fingerprint: admitted.package().fingerprint().to_string(),
    })
}

struct DoomVitalityPolicyCompiler {
    schema: StandardExtensionSchema,
}

impl DoomVitalityPolicyCompiler {
    fn new() -> Result<Self, DoomVitalityPolicyError> {
        Ok(Self {
            schema: StandardExtensionSchema::new(
                CapabilityRequirementId::parse(DOOM_VITALITY_NAMESPACE)
                    .map_err(DoomVitalityPolicyError::Identity)?,
                1,
            )
            .map_err(DoomVitalityPolicyError::Standard)?,
        })
    }
}

impl CompileStandardExtension for DoomVitalityPolicyCompiler {
    type Output = DoomVitalityPolicy;
    type Error = DoomVitalityCompileError;

    fn schema(&self) -> &StandardExtensionSchema {
        &self.schema
    }

    fn compile(&self, artifact: &StandardExtensionArtifact) -> Result<Self::Output, Self::Error> {
        if artifact.kind().as_str() != DOOM_VITALITY_KIND
            || artifact.subject().as_str() != DOOM_VITALITY_SUBJECT
        {
            return Err(DoomVitalityCompileError::WrongArtifact);
        }
        serde_json::from_value(artifact.payload().clone())
            .map_err(DoomVitalityCompileError::Payload)
    }
}

#[derive(Debug)]
pub enum DoomVitalityCompileError {
    Payload(serde_json::Error),
    WrongArtifact,
}

impl std::fmt::Display for DoomVitalityCompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Doom vitality extension compilation rejected: {self:?}"
        )
    }
}

impl std::error::Error for DoomVitalityCompileError {}

#[derive(Debug)]
pub enum DoomVitalityPolicyError {
    Package(rusty_engine::gameplay_rules::RulePackageError),
    Standard(StandardExtensionError),
    Compilation(StandardExtensionCompileError<DoomVitalityCompileError>),
    Identity(rusty_engine::gameplay_standard::RoleRequirementError),
    InvalidBounds,
}

impl std::fmt::Display for DoomVitalityPolicyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Doom vitality standard extension rejected: {self:?}"
        )
    }
}

impl std::error::Error for DoomVitalityPolicyError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_typescript_artifact_admits_and_compiles_to_the_named_doom_policy() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../data/gameplay/loading-bay-e1m1-standard-vitality.package.json");
        let admitted = admit_doom_vitality_policy(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(admitted.policy().maximum_health, 1_000_000);
        assert_eq!(admitted.policy().maximum_armor, 1_000_000);
        assert_eq!(admitted.fingerprint().len(), 64);
    }
}
