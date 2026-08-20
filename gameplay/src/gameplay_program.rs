//! Closed Loading Bay authored gameplay programs.
//!
//! These are deliberately Demo vocabulary, not an Engine behavior language:
//! Rust owns their meaning and the Engine only resolves the resulting bounded
//! `Program` through the existing policy lifecycle.

use std::collections::BTreeMap;

use rusty_engine::gameplay_resolution::Program;
use serde::{Deserialize, Serialize};

pub const MAX_GAMEPLAY_PROGRAMS: usize = 32;
pub const MAX_GAMEPLAY_PROGRAM_STEPS: usize = 32;
pub const MAX_GAMEPLAY_PROGRAM_NODES: usize = 128;
pub const MAX_GAMEPLAY_PROGRAM_DEPTH: usize = 16;
/// A readout is deliberately smaller than the admitted program tree. This is
/// a product status surface, not a source-level debugger.
pub const MAX_GAMEPLAY_PROGRAM_READOUT_STEPS: usize = 32;
pub const MAX_GAMEPLAY_PROGRAM_OUTCOME_OPERATIONS: usize = 16;
pub const MAX_GAMEPLAY_PROGRAM_OUTCOME_EFFECTS: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredGameplayProgram {
    pub id: String,
    pub program: StoredGameplayNode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredGameplayNode {
    Sequence {
        steps: Vec<StoredGameplayNode>,
    },
    When {
        predicate: StoredGameplayPredicate,
        then_program: Box<StoredGameplayNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        otherwise_program: Option<Box<StoredGameplayNode>>,
    },
    Operation {
        operation: StoredGameplayOperation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredGameplayPredicate {
    ImpactIsHit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredGameplayOperation {
    RecordFired,
    ConsumeAmmo,
    ApplyHit,
    ApplyMiss,
    ApplySpreadImpacts,
    SetCooldown,
    UseHealthSupply,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DemoPredicate {
    ImpactIsHit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DemoOperation {
    RecordFired,
    ConsumeAmmo,
    ApplyHit,
    ApplyMiss,
    ApplySpreadImpacts,
    SetCooldown,
    UseHealthSupply,
}

pub(crate) type DemoProgram = Program<DemoPredicate, DemoOperation>;

#[derive(Debug, Clone, Default)]
pub(crate) struct GameplayProgramCatalog {
    programs: BTreeMap<String, DemoProgram>,
}

/// Compact, transport-neutral description of one admitted program. The only
/// vocabulary exposed here is the Loading Bay vocabulary declared above.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameplayProgramReadout {
    pub programs: Vec<GameplayProgramShape>,
    pub bindings: Vec<GameplayProgramBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameplayProgramShape {
    pub id: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameplayProgramBinding {
    pub item: String,
    pub program_id: String,
}

/// Latest-value result of an admitted gameplay-program attempt. It carries no
/// trace, source identity, replay state, or mutable control surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameplayProgramOutcome {
    pub program_id: String,
    pub status: GameplayProgramOutcomeStatus,
    pub planned_operations: Vec<String>,
    pub executed_operations: Vec<String>,
    pub effects: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GameplayProgramOutcomeStatus {
    Applied,
    Rejected,
}

impl GameplayProgramCatalog {
    pub(crate) fn get(&self, id: &str) -> Option<&DemoProgram> {
        self.programs.get(id)
    }

    pub(crate) fn len(&self) -> usize {
        self.programs.len()
    }

    pub(crate) fn readout(
        &self,
        bindings: impl Iterator<Item = (String, String)>,
    ) -> GameplayProgramReadout {
        GameplayProgramReadout {
            programs: self
                .programs
                .iter()
                .map(|(id, program)| GameplayProgramShape {
                    id: id.clone(),
                    steps: program_readout_steps(program),
                })
                .collect(),
            bindings: bindings
                .take(MAX_GAMEPLAY_PROGRAMS)
                .map(|(item, program_id)| GameplayProgramBinding { item, program_id })
                .collect(),
        }
    }
}

pub(crate) fn operation_label(operation: DemoOperation) -> &'static str {
    match operation {
        DemoOperation::RecordFired => "record-fired",
        DemoOperation::ConsumeAmmo => "consume-ammo",
        DemoOperation::ApplyHit => "apply-hit",
        DemoOperation::ApplyMiss => "apply-miss",
        DemoOperation::ApplySpreadImpacts => "apply-spread-impacts",
        DemoOperation::SetCooldown => "set-cooldown",
        DemoOperation::UseHealthSupply => "use-health-supply",
    }
}

pub(crate) fn program_operation_labels(program: &DemoProgram) -> Vec<String> {
    let mut labels = Vec::new();
    flatten_operations(program, &mut labels);
    labels
}

fn program_readout_steps(program: &DemoProgram) -> Vec<String> {
    let mut steps = Vec::new();
    flatten_readout(program, &mut steps);
    steps
}

fn flatten_operations(program: &DemoProgram, labels: &mut Vec<String>) {
    if labels.len() >= MAX_GAMEPLAY_PROGRAM_OUTCOME_OPERATIONS {
        return;
    }
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                flatten_operations(step, labels);
            }
        }
        Program::When {
            then_program,
            otherwise_program,
            ..
        } => {
            flatten_operations(then_program, labels);
            if let Some(otherwise_program) = otherwise_program {
                flatten_operations(otherwise_program, labels);
            }
        }
        Program::Operation(operation) => labels.push(operation_label(*operation).to_owned()),
    }
}

fn flatten_readout(program: &DemoProgram, steps: &mut Vec<String>) {
    if steps.len() >= MAX_GAMEPLAY_PROGRAM_READOUT_STEPS {
        return;
    }
    match program {
        Program::Sequence { steps: children } => {
            for child in children {
                flatten_readout(child, steps);
            }
        }
        Program::When {
            predicate,
            then_program,
            otherwise_program,
        } => {
            let predicate = match predicate {
                DemoPredicate::ImpactIsHit => "when-impact-is-hit",
            };
            steps.push(predicate.to_owned());
            flatten_readout(then_program, steps);
            if let Some(otherwise_program) = otherwise_program {
                flatten_readout(otherwise_program, steps);
            }
        }
        Program::Operation(operation) => steps.push(operation_label(*operation).to_owned()),
    }
}

pub(crate) fn applied_outcome(
    program_id: String,
    program: &DemoProgram,
    executed_operations: Vec<String>,
    effects: Vec<String>,
) -> GameplayProgramOutcome {
    GameplayProgramOutcome {
        program_id,
        status: GameplayProgramOutcomeStatus::Applied,
        planned_operations: program_operation_labels(program),
        executed_operations: executed_operations
            .into_iter()
            .take(MAX_GAMEPLAY_PROGRAM_OUTCOME_OPERATIONS)
            .collect(),
        effects: effects
            .into_iter()
            .take(MAX_GAMEPLAY_PROGRAM_OUTCOME_EFFECTS)
            .collect(),
        rejection_reason: None,
    }
}

pub(crate) fn rejected_outcome(
    program_id: String,
    program: &DemoProgram,
    reason: impl Into<String>,
) -> GameplayProgramOutcome {
    GameplayProgramOutcome {
        program_id,
        status: GameplayProgramOutcomeStatus::Rejected,
        planned_operations: program_operation_labels(program),
        executed_operations: Vec::new(),
        effects: Vec::new(),
        rejection_reason: Some(reason.into().chars().take(160).collect()),
    }
}

/// Execute the closed authored tree in source order.  Callers provide only
/// their bounded Rust predicate and operation vocabulary; this is not a
/// cross-product evaluator or behavior IR.
pub(crate) fn execute_program<E>(
    program: &DemoProgram,
    predicate: &mut impl FnMut(DemoPredicate) -> Result<bool, E>,
    operation: &mut impl FnMut(DemoOperation) -> Result<(), E>,
) -> Result<(), E> {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                execute_program(step, predicate, operation)?;
            }
            Ok(())
        }
        Program::When {
            predicate: wanted,
            then_program,
            otherwise_program,
        } => {
            if predicate(*wanted)? {
                execute_program(then_program, predicate, operation)
            } else if let Some(otherwise_program) = otherwise_program {
                execute_program(otherwise_program, predicate, operation)
            } else {
                Ok(())
            }
        }
        Program::Operation(value) => operation(*value),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GameplayProgramCompileError {
    TooMany {
        count: usize,
    },
    DuplicateId {
        id: String,
    },
    InvalidId {
        id: String,
    },
    EmptySequence {
        id: String,
    },
    TooManySteps {
        id: String,
        count: usize,
    },
    TooManyNodes {
        id: String,
        count: usize,
    },
    TooDeep {
        id: String,
        depth: usize,
    },
    InvalidOperationContext {
        id: String,
        operation: StoredGameplayOperation,
    },
}

impl std::fmt::Display for GameplayProgramCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooMany { count } => write!(f, "gameplay program quota exceeded: {count}"),
            Self::DuplicateId { id } => write!(f, "duplicate gameplay program `{id}`"),
            Self::InvalidId { id } => write!(f, "invalid gameplay program id `{id}`"),
            Self::EmptySequence { id } => {
                write!(f, "gameplay program `{id}` has an empty sequence")
            }
            Self::TooManySteps { id, count } => {
                write!(f, "gameplay program `{id}` has {count} sequence steps")
            }
            Self::TooManyNodes { id, count } => {
                write!(f, "gameplay program `{id}` has {count} nodes")
            }
            Self::TooDeep { id, depth } => write!(f, "gameplay program `{id}` has depth {depth}"),
            Self::InvalidOperationContext { id, operation } => write!(
                f,
                "gameplay program `{id}` uses {operation:?} outside its evidence context"
            ),
        }
    }
}

pub(crate) fn compile_gameplay_programs(
    authored: &[StoredGameplayProgram],
) -> Result<GameplayProgramCatalog, GameplayProgramCompileError> {
    if authored.len() > MAX_GAMEPLAY_PROGRAMS {
        return Err(GameplayProgramCompileError::TooMany {
            count: authored.len(),
        });
    }
    let mut programs = BTreeMap::new();
    for program in authored {
        if !is_program_id(&program.id) {
            return Err(GameplayProgramCompileError::InvalidId {
                id: program.id.clone(),
            });
        }
        let mut nodes = 0;
        let compiled = compile_node(&program.id, &program.program, 1, &mut nodes, false)?;
        if programs.insert(program.id.clone(), compiled).is_some() {
            return Err(GameplayProgramCompileError::DuplicateId {
                id: program.id.clone(),
            });
        }
    }
    Ok(GameplayProgramCatalog { programs })
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

fn compile_node(
    id: &str,
    node: &StoredGameplayNode,
    depth: usize,
    nodes: &mut usize,
    hit_evidence: bool,
) -> Result<DemoProgram, GameplayProgramCompileError> {
    *nodes += 1;
    if *nodes > MAX_GAMEPLAY_PROGRAM_NODES {
        return Err(GameplayProgramCompileError::TooManyNodes {
            id: id.to_owned(),
            count: *nodes,
        });
    }
    if depth > MAX_GAMEPLAY_PROGRAM_DEPTH {
        return Err(GameplayProgramCompileError::TooDeep {
            id: id.to_owned(),
            depth,
        });
    }
    Ok(match node {
        StoredGameplayNode::Sequence { steps } => {
            if steps.is_empty() {
                return Err(GameplayProgramCompileError::EmptySequence { id: id.to_owned() });
            }
            if steps.len() > MAX_GAMEPLAY_PROGRAM_STEPS {
                return Err(GameplayProgramCompileError::TooManySteps {
                    id: id.to_owned(),
                    count: steps.len(),
                });
            }
            Program::Sequence {
                steps: steps
                    .iter()
                    .map(|step| compile_node(id, step, depth + 1, nodes, hit_evidence))
                    .collect::<Result<_, _>>()?,
            }
        }
        StoredGameplayNode::When {
            predicate,
            then_program,
            otherwise_program,
        } => Program::When {
            predicate: match predicate {
                StoredGameplayPredicate::ImpactIsHit => DemoPredicate::ImpactIsHit,
            },
            then_program: Box::new(compile_node(id, then_program, depth + 1, nodes, true)?),
            otherwise_program: otherwise_program
                .as_deref()
                .map(|node| compile_node(id, node, depth + 1, nodes, false))
                .transpose()?
                .map(Box::new),
        },
        StoredGameplayNode::Operation { operation } => {
            if matches!(operation, StoredGameplayOperation::ApplyHit) && !hit_evidence
                || matches!(operation, StoredGameplayOperation::ApplyMiss) && hit_evidence
            {
                return Err(GameplayProgramCompileError::InvalidOperationContext {
                    id: id.to_owned(),
                    operation: *operation,
                });
            }
            Program::Operation(match operation {
                StoredGameplayOperation::RecordFired => DemoOperation::RecordFired,
                StoredGameplayOperation::ConsumeAmmo => DemoOperation::ConsumeAmmo,
                StoredGameplayOperation::ApplyHit => DemoOperation::ApplyHit,
                StoredGameplayOperation::ApplyMiss => DemoOperation::ApplyMiss,
                StoredGameplayOperation::ApplySpreadImpacts => DemoOperation::ApplySpreadImpacts,
                StoredGameplayOperation::SetCooldown => DemoOperation::SetCooldown,
                StoredGameplayOperation::UseHealthSupply => DemoOperation::UseHealthSupply,
            })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_closed_hitscan_program() {
        let catalog = compile_gameplay_programs(&[StoredGameplayProgram {
            id: "test-shot".into(),
            program: StoredGameplayNode::Sequence {
                steps: vec![StoredGameplayNode::Operation {
                    operation: StoredGameplayOperation::RecordFired,
                }],
            },
        }])
        .expect("closed program compiles");
        assert_eq!(catalog.len(), 1);
    }

    #[test]
    fn rejects_an_impact_operation_without_its_evidence_context() {
        let error = compile_gameplay_programs(&[StoredGameplayProgram {
            id: "bad-shot".into(),
            program: StoredGameplayNode::Sequence {
                steps: vec![StoredGameplayNode::Operation {
                    operation: StoredGameplayOperation::ApplyHit,
                }],
            },
        }])
        .expect_err("unconditional hit operation is invalid");
        assert!(matches!(
            error,
            GameplayProgramCompileError::InvalidOperationContext { .. }
        ));
    }

    #[test]
    fn committed_catalog_readout_is_closed_and_bounded() {
        let package_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../data/gameplay/loading-bay-e1m1-core.package.json");
        let package = std::fs::read(package_path).expect("committed gameplay package exists");
        let catalog = crate::compile::compile_gameplay_package(&package, "e1m1-core")
            .expect("committed package compiles")
            .gameplay_programs;
        let readout = catalog.readout(
            [(
                "weapon/doom-pistol".to_owned(),
                "weapon/hitscan-ammunition".to_owned(),
            )]
            .into_iter(),
        );
        assert_eq!(readout.programs.len(), 4);
        assert!(readout
            .programs
            .iter()
            .all(|program| program.steps.len() <= MAX_GAMEPLAY_PROGRAM_READOUT_STEPS));
        assert_eq!(readout.bindings.len(), 1);
        assert!(readout
            .programs
            .iter()
            .any(|program| program.id == "weapon/hitscan-ammunition"));
    }

    #[test]
    fn authored_variants_produce_distinct_bounded_outcomes() {
        let with_ammo = compile_gameplay_programs(&[StoredGameplayProgram {
            id: "weapon/with-ammo".into(),
            program: StoredGameplayNode::Sequence {
                steps: vec![
                    StoredGameplayNode::Operation {
                        operation: StoredGameplayOperation::RecordFired,
                    },
                    StoredGameplayNode::Operation {
                        operation: StoredGameplayOperation::ConsumeAmmo,
                    },
                ],
            },
        }])
        .expect("program compiles");
        let without_ammo = compile_gameplay_programs(&[StoredGameplayProgram {
            id: "weapon/without-ammo".into(),
            program: StoredGameplayNode::Sequence {
                steps: vec![StoredGameplayNode::Operation {
                    operation: StoredGameplayOperation::RecordFired,
                }],
            },
        }])
        .expect("program compiles");
        let with_ammo = applied_outcome(
            "weapon/with-ammo".into(),
            with_ammo.get("weapon/with-ammo").unwrap(),
            vec!["record-fired".into(), "consume-ammo".into()],
            vec!["attack-fired".into(), "inventory".into()],
        );
        let without_ammo = applied_outcome(
            "weapon/without-ammo".into(),
            without_ammo.get("weapon/without-ammo").unwrap(),
            vec!["record-fired".into()],
            vec!["attack-fired".into()],
        );
        assert_ne!(
            with_ammo.planned_operations,
            without_ammo.planned_operations
        );
        assert!(with_ammo.planned_operations.len() <= MAX_GAMEPLAY_PROGRAM_OUTCOME_OPERATIONS);
        assert!(with_ammo.effects.len() <= MAX_GAMEPLAY_PROGRAM_OUTCOME_EFFECTS);
    }
}
