//! Closed authored secret-discovery programs.
//!
//! TypeScript chooses source order from this deliberately small vocabulary.
//! Rust still owns spatial trigger facts, once-only state, identities, fact
//! payloads, mutation, and delivery; an authored program cannot select a
//! region or fabricate a progression fact.

use std::collections::BTreeMap;

use rusty_engine::gameplay_resolution::Program;
use serde::{Deserialize, Serialize};

pub const MAX_SECRET_PROGRAMS: usize = 16;
pub const MAX_SECRET_PROGRAM_BINDINGS: usize = 128;
pub const MAX_SECRET_PROGRAM_STEPS: usize = 16;
pub const MAX_SECRET_PROGRAM_NODES: usize = 64;
pub const MAX_SECRET_PROGRAM_DEPTH: usize = 12;
pub const MAX_SECRET_PROGRAM_READOUT_STEPS: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredSecretProgram {
    pub id: String,
    pub program: StoredSecretProgramNode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredSecretProgramNode {
    Sequence {
        steps: Vec<StoredSecretProgramNode>,
    },
    When {
        predicate: StoredSecretPredicate,
        then_program: Box<StoredSecretProgramNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        otherwise_program: Option<Box<StoredSecretProgramNode>>,
    },
    Operation {
        operation: StoredSecretOperation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredSecretPredicate {
    SecretRegionEntered,
    SecretUndiscovered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredSecretOperation {
    RecordDiscovery,
    EmitSecretPresentation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SecretPredicate {
    SecretRegionEntered,
    SecretUndiscovered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SecretOperation {
    RecordDiscovery,
    EmitSecretPresentation,
}

pub(crate) type SecretProgram = Program<SecretPredicate, SecretOperation>;

#[derive(Debug, Clone, Default)]
pub(crate) struct SecretProgramCatalog {
    programs: BTreeMap<String, SecretProgram>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretProgramReadout {
    pub programs: Vec<SecretProgramShape>,
    pub bindings: Vec<SecretProgramBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretProgramShape {
    pub id: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretProgramBinding {
    pub secret: u64,
    pub program_id: String,
}

impl SecretProgramCatalog {
    pub(crate) fn get(&self, id: &str) -> Option<&SecretProgram> {
        self.programs.get(id)
    }

    pub(crate) fn len(&self) -> usize {
        self.programs.len()
    }

    pub(crate) fn readout(
        &self,
        bindings: impl Iterator<Item = (u64, String)>,
    ) -> SecretProgramReadout {
        SecretProgramReadout {
            programs: self
                .programs
                .iter()
                .map(|(id, program)| SecretProgramShape {
                    id: id.clone(),
                    steps: readout_steps(program),
                })
                .collect(),
            bindings: bindings
                .take(MAX_SECRET_PROGRAM_BINDINGS)
                .map(|(secret, program_id)| SecretProgramBinding { secret, program_id })
                .collect(),
        }
    }
}

pub(crate) fn execute_secret_program<E>(
    program: &SecretProgram,
    predicate: &mut impl FnMut(SecretPredicate) -> Result<bool, E>,
    operation: &mut impl FnMut(SecretOperation) -> Result<(), E>,
) -> Result<(), E> {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                execute_secret_program(step, predicate, operation)?;
            }
            Ok(())
        }
        Program::When {
            predicate: wanted,
            then_program,
            otherwise_program,
        } => {
            if predicate(*wanted)? {
                execute_secret_program(then_program, predicate, operation)
            } else if let Some(otherwise_program) = otherwise_program {
                execute_secret_program(otherwise_program, predicate, operation)
            } else {
                Ok(())
            }
        }
        Program::Operation(operation_value) => operation(*operation_value),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SecretProgramCompileError {
    TooMany { count: usize },
    DuplicateId { id: String },
    InvalidId { id: String },
    EmptySequence { id: String },
    TooManySteps { id: String, count: usize },
    TooManyNodes { id: String, count: usize },
    TooDeep { id: String, depth: usize },
}

impl std::fmt::Display for SecretProgramCompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

pub(crate) fn compile_secret_programs(
    authored: &[StoredSecretProgram],
) -> Result<SecretProgramCatalog, SecretProgramCompileError> {
    if authored.len() > MAX_SECRET_PROGRAMS {
        return Err(SecretProgramCompileError::TooMany {
            count: authored.len(),
        });
    }
    let mut programs = BTreeMap::new();
    for stored in authored {
        if !is_program_id(&stored.id) {
            return Err(SecretProgramCompileError::InvalidId {
                id: stored.id.clone(),
            });
        }
        let mut nodes = 0;
        let compiled = compile_node(&stored.id, &stored.program, 1, &mut nodes)?;
        if programs.insert(stored.id.clone(), compiled).is_some() {
            return Err(SecretProgramCompileError::DuplicateId {
                id: stored.id.clone(),
            });
        }
    }
    Ok(SecretProgramCatalog { programs })
}

fn compile_node(
    id: &str,
    node: &StoredSecretProgramNode,
    depth: usize,
    nodes: &mut usize,
) -> Result<SecretProgram, SecretProgramCompileError> {
    *nodes += 1;
    if *nodes > MAX_SECRET_PROGRAM_NODES {
        return Err(SecretProgramCompileError::TooManyNodes {
            id: id.to_owned(),
            count: *nodes,
        });
    }
    if depth > MAX_SECRET_PROGRAM_DEPTH {
        return Err(SecretProgramCompileError::TooDeep {
            id: id.to_owned(),
            depth,
        });
    }
    match node {
        StoredSecretProgramNode::Sequence { steps } => {
            if steps.is_empty() {
                return Err(SecretProgramCompileError::EmptySequence { id: id.to_owned() });
            }
            if steps.len() > MAX_SECRET_PROGRAM_STEPS {
                return Err(SecretProgramCompileError::TooManySteps {
                    id: id.to_owned(),
                    count: steps.len(),
                });
            }
            Ok(Program::Sequence {
                steps: steps
                    .iter()
                    .map(|step| compile_node(id, step, depth + 1, nodes))
                    .collect::<Result<_, _>>()?,
            })
        }
        StoredSecretProgramNode::When {
            predicate,
            then_program,
            otherwise_program,
        } => Ok(Program::When {
            predicate: match predicate {
                StoredSecretPredicate::SecretRegionEntered => SecretPredicate::SecretRegionEntered,
                StoredSecretPredicate::SecretUndiscovered => SecretPredicate::SecretUndiscovered,
            },
            then_program: Box::new(compile_node(id, then_program, depth + 1, nodes)?),
            otherwise_program: otherwise_program
                .as_deref()
                .map(|other| compile_node(id, other, depth + 1, nodes).map(Box::new))
                .transpose()?,
        }),
        StoredSecretProgramNode::Operation { operation } => {
            Ok(Program::Operation(match operation {
                StoredSecretOperation::RecordDiscovery => SecretOperation::RecordDiscovery,
                StoredSecretOperation::EmitSecretPresentation => {
                    SecretOperation::EmitSecretPresentation
                }
            }))
        }
    }
}

fn readout_steps(program: &SecretProgram) -> Vec<String> {
    let mut steps = Vec::new();
    visit_tree(program, &mut |step| {
        if steps.len() < MAX_SECRET_PROGRAM_READOUT_STEPS {
            steps.push(step.to_owned());
        }
    });
    steps
}

fn visit_tree(program: &SecretProgram, visit: &mut impl FnMut(&str)) {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                visit_tree(step, visit);
            }
        }
        Program::When {
            predicate,
            then_program,
            otherwise_program,
        } => {
            visit(match predicate {
                SecretPredicate::SecretRegionEntered => "when-secret-region-entered",
                SecretPredicate::SecretUndiscovered => "when-secret-undiscovered",
            });
            visit_tree(then_program, visit);
            if let Some(otherwise_program) = otherwise_program {
                visit_tree(otherwise_program, visit);
            }
        }
        Program::Operation(operation) => visit(match operation {
            SecretOperation::RecordDiscovery => "record-discovery",
            SecretOperation::EmitSecretPresentation => "emit-secret-presentation",
        }),
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
