//! Closed authored encounter lifecycle programs.
//!
//! TypeScript chooses the source order of a very small encounter vocabulary.
//! Rust retains spatial admission, member identities and lifecycle, door
//! motion/scheduling, state mutation, facts, and event delivery.  The
//! activation and clear trees are intentionally separate typed entry points:
//! an authored encounter cannot turn a defeat observation into an arbitrary
//! activation operation (or vice versa).

use std::collections::BTreeMap;

use rusty_engine::gameplay_resolution::Program;
use serde::{Deserialize, Serialize};

pub const MAX_ENCOUNTER_PROGRAMS: usize = 16;
pub const MAX_ENCOUNTER_PROGRAM_BINDINGS: usize = 64;
pub const MAX_ENCOUNTER_PROGRAM_STEPS: usize = 16;
pub const MAX_ENCOUNTER_PROGRAM_NODES: usize = 64;
pub const MAX_ENCOUNTER_PROGRAM_DEPTH: usize = 12;
pub const MAX_ENCOUNTER_PROGRAM_READOUT_STEPS: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredEncounterProgram {
    pub id: String,
    pub activation: StoredEncounterActivationProgramNode,
    pub clear: StoredEncounterClearProgramNode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredEncounterActivationProgramNode {
    Sequence {
        steps: Vec<StoredEncounterActivationProgramNode>,
    },
    When {
        predicate: StoredEncounterActivationPredicate,
        then_program: Box<StoredEncounterActivationProgramNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        otherwise_program: Option<Box<StoredEncounterActivationProgramNode>>,
    },
    Operation {
        operation: StoredEncounterActivationOperation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredEncounterActivationPredicate {
    ActivationEligible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredEncounterActivationOperation {
    RecordEncounterActivation,
    ActivateBoundMembers,
    EmitEncounterFeedback,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredEncounterClearProgramNode {
    Sequence {
        steps: Vec<StoredEncounterClearProgramNode>,
    },
    When {
        predicate: StoredEncounterClearPredicate,
        then_program: Box<StoredEncounterClearProgramNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        otherwise_program: Option<Box<StoredEncounterClearProgramNode>>,
    },
    Operation {
        operation: StoredEncounterClearOperation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredEncounterClearPredicate {
    MembersDefeated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredEncounterClearOperation {
    RecordEncounterCleared,
    OpenBoundExit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EncounterActivationPredicate {
    ActivationEligible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EncounterActivationOperation {
    RecordEncounterActivation,
    ActivateBoundMembers,
    EmitEncounterFeedback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EncounterClearPredicate {
    MembersDefeated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EncounterClearOperation {
    RecordEncounterCleared,
    OpenBoundExit,
}

pub(crate) type EncounterActivationProgram =
    Program<EncounterActivationPredicate, EncounterActivationOperation>;
pub(crate) type EncounterClearProgram = Program<EncounterClearPredicate, EncounterClearOperation>;

#[derive(Debug, Clone)]
pub(crate) struct EncounterProgram {
    pub(crate) activation: EncounterActivationProgram,
    pub(crate) clear: EncounterClearProgram,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct EncounterProgramCatalog {
    programs: BTreeMap<String, EncounterProgram>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncounterProgramReadout {
    pub programs: Vec<EncounterProgramShape>,
    pub bindings: Vec<EncounterProgramBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncounterProgramShape {
    pub id: String,
    pub activation_steps: Vec<String>,
    pub clear_steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncounterProgramBinding {
    pub encounter: u64,
    pub program_id: String,
}

impl EncounterProgramCatalog {
    pub(crate) fn get(&self, id: &str) -> Option<&EncounterProgram> {
        self.programs.get(id)
    }

    pub(crate) fn len(&self) -> usize {
        self.programs.len()
    }

    pub(crate) fn readout(
        &self,
        bindings: impl Iterator<Item = (u64, String)>,
    ) -> EncounterProgramReadout {
        EncounterProgramReadout {
            programs: self
                .programs
                .iter()
                .map(|(id, program)| EncounterProgramShape {
                    id: id.clone(),
                    activation_steps: activation_readout_steps(&program.activation),
                    clear_steps: clear_readout_steps(&program.clear),
                })
                .collect(),
            bindings: bindings
                .take(MAX_ENCOUNTER_PROGRAM_BINDINGS)
                .map(|(encounter, program_id)| EncounterProgramBinding {
                    encounter,
                    program_id,
                })
                .collect(),
        }
    }
}

pub(crate) fn execute_encounter_activation_program<E>(
    program: &EncounterActivationProgram,
    predicate: &mut impl FnMut(EncounterActivationPredicate) -> Result<bool, E>,
    operation: &mut impl FnMut(EncounterActivationOperation) -> Result<(), E>,
) -> Result<(), E> {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                execute_encounter_activation_program(step, predicate, operation)?;
            }
            Ok(())
        }
        Program::When {
            predicate: wanted,
            then_program,
            otherwise_program,
        } => {
            if predicate(*wanted)? {
                execute_encounter_activation_program(then_program, predicate, operation)
            } else if let Some(otherwise_program) = otherwise_program {
                execute_encounter_activation_program(otherwise_program, predicate, operation)
            } else {
                Ok(())
            }
        }
        Program::Operation(value) => operation(*value),
    }
}

pub(crate) fn execute_encounter_clear_program<E>(
    program: &EncounterClearProgram,
    predicate: &mut impl FnMut(EncounterClearPredicate) -> Result<bool, E>,
    operation: &mut impl FnMut(EncounterClearOperation) -> Result<(), E>,
) -> Result<(), E> {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                execute_encounter_clear_program(step, predicate, operation)?;
            }
            Ok(())
        }
        Program::When {
            predicate: wanted,
            then_program,
            otherwise_program,
        } => {
            if predicate(*wanted)? {
                execute_encounter_clear_program(then_program, predicate, operation)
            } else if let Some(otherwise_program) = otherwise_program {
                execute_encounter_clear_program(otherwise_program, predicate, operation)
            } else {
                Ok(())
            }
        }
        Program::Operation(value) => operation(*value),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EncounterProgramCompileError {
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
        entry: &'static str,
    },
    TooManySteps {
        id: String,
        entry: &'static str,
        count: usize,
    },
    TooManyNodes {
        id: String,
        entry: &'static str,
        count: usize,
    },
    TooDeep {
        id: String,
        entry: &'static str,
        depth: usize,
    },
}

impl std::fmt::Display for EncounterProgramCompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

pub(crate) fn compile_encounter_programs(
    authored: &[StoredEncounterProgram],
) -> Result<EncounterProgramCatalog, EncounterProgramCompileError> {
    if authored.len() > MAX_ENCOUNTER_PROGRAMS {
        return Err(EncounterProgramCompileError::TooMany {
            count: authored.len(),
        });
    }
    let mut programs = BTreeMap::new();
    for stored in authored {
        if !is_program_id(&stored.id) {
            return Err(EncounterProgramCompileError::InvalidId {
                id: stored.id.clone(),
            });
        }
        let mut activation_nodes = 0;
        let activation =
            compile_activation_node(&stored.id, &stored.activation, 1, &mut activation_nodes)?;
        let mut clear_nodes = 0;
        let clear = compile_clear_node(&stored.id, &stored.clear, 1, &mut clear_nodes)?;
        if programs
            .insert(stored.id.clone(), EncounterProgram { activation, clear })
            .is_some()
        {
            return Err(EncounterProgramCompileError::DuplicateId {
                id: stored.id.clone(),
            });
        }
    }
    Ok(EncounterProgramCatalog { programs })
}

fn compile_activation_node(
    id: &str,
    node: &StoredEncounterActivationProgramNode,
    depth: usize,
    nodes: &mut usize,
) -> Result<EncounterActivationProgram, EncounterProgramCompileError> {
    *nodes += 1;
    if *nodes > MAX_ENCOUNTER_PROGRAM_NODES {
        return Err(EncounterProgramCompileError::TooManyNodes {
            id: id.to_owned(),
            entry: "activation",
            count: *nodes,
        });
    }
    if depth > MAX_ENCOUNTER_PROGRAM_DEPTH {
        return Err(EncounterProgramCompileError::TooDeep {
            id: id.to_owned(),
            entry: "activation",
            depth,
        });
    }
    match node {
        StoredEncounterActivationProgramNode::Sequence { steps } => {
            if steps.is_empty() {
                return Err(EncounterProgramCompileError::EmptySequence {
                    id: id.to_owned(),
                    entry: "activation",
                });
            }
            if steps.len() > MAX_ENCOUNTER_PROGRAM_STEPS {
                return Err(EncounterProgramCompileError::TooManySteps {
                    id: id.to_owned(),
                    entry: "activation",
                    count: steps.len(),
                });
            }
            Ok(Program::Sequence {
                steps: steps
                    .iter()
                    .map(|step| compile_activation_node(id, step, depth + 1, nodes))
                    .collect::<Result<_, _>>()?,
            })
        }
        StoredEncounterActivationProgramNode::When {
            predicate,
            then_program,
            otherwise_program,
        } => Ok(Program::When {
            predicate: match predicate {
                StoredEncounterActivationPredicate::ActivationEligible => {
                    EncounterActivationPredicate::ActivationEligible
                }
            },
            then_program: Box::new(compile_activation_node(id, then_program, depth + 1, nodes)?),
            otherwise_program: otherwise_program
                .as_ref()
                .map(|program| compile_activation_node(id, program, depth + 1, nodes).map(Box::new))
                .transpose()?,
        }),
        StoredEncounterActivationProgramNode::Operation { operation } => {
            Ok(Program::Operation(match operation {
                StoredEncounterActivationOperation::RecordEncounterActivation => {
                    EncounterActivationOperation::RecordEncounterActivation
                }
                StoredEncounterActivationOperation::ActivateBoundMembers => {
                    EncounterActivationOperation::ActivateBoundMembers
                }
                StoredEncounterActivationOperation::EmitEncounterFeedback => {
                    EncounterActivationOperation::EmitEncounterFeedback
                }
            }))
        }
    }
}

fn compile_clear_node(
    id: &str,
    node: &StoredEncounterClearProgramNode,
    depth: usize,
    nodes: &mut usize,
) -> Result<EncounterClearProgram, EncounterProgramCompileError> {
    *nodes += 1;
    if *nodes > MAX_ENCOUNTER_PROGRAM_NODES {
        return Err(EncounterProgramCompileError::TooManyNodes {
            id: id.to_owned(),
            entry: "clear",
            count: *nodes,
        });
    }
    if depth > MAX_ENCOUNTER_PROGRAM_DEPTH {
        return Err(EncounterProgramCompileError::TooDeep {
            id: id.to_owned(),
            entry: "clear",
            depth,
        });
    }
    match node {
        StoredEncounterClearProgramNode::Sequence { steps } => {
            if steps.is_empty() {
                return Err(EncounterProgramCompileError::EmptySequence {
                    id: id.to_owned(),
                    entry: "clear",
                });
            }
            if steps.len() > MAX_ENCOUNTER_PROGRAM_STEPS {
                return Err(EncounterProgramCompileError::TooManySteps {
                    id: id.to_owned(),
                    entry: "clear",
                    count: steps.len(),
                });
            }
            Ok(Program::Sequence {
                steps: steps
                    .iter()
                    .map(|step| compile_clear_node(id, step, depth + 1, nodes))
                    .collect::<Result<_, _>>()?,
            })
        }
        StoredEncounterClearProgramNode::When {
            predicate,
            then_program,
            otherwise_program,
        } => Ok(Program::When {
            predicate: match predicate {
                StoredEncounterClearPredicate::MembersDefeated => {
                    EncounterClearPredicate::MembersDefeated
                }
            },
            then_program: Box::new(compile_clear_node(id, then_program, depth + 1, nodes)?),
            otherwise_program: otherwise_program
                .as_ref()
                .map(|program| compile_clear_node(id, program, depth + 1, nodes).map(Box::new))
                .transpose()?,
        }),
        StoredEncounterClearProgramNode::Operation { operation } => {
            Ok(Program::Operation(match operation {
                StoredEncounterClearOperation::RecordEncounterCleared => {
                    EncounterClearOperation::RecordEncounterCleared
                }
                StoredEncounterClearOperation::OpenBoundExit => {
                    EncounterClearOperation::OpenBoundExit
                }
            }))
        }
    }
}

fn activation_readout_steps(program: &EncounterActivationProgram) -> Vec<String> {
    let mut steps = Vec::new();
    collect_activation_steps(program, &mut steps);
    steps.truncate(MAX_ENCOUNTER_PROGRAM_READOUT_STEPS);
    steps
}

fn collect_activation_steps(program: &EncounterActivationProgram, steps: &mut Vec<String>) {
    match program {
        Program::Sequence { steps: children } => children
            .iter()
            .for_each(|child| collect_activation_steps(child, steps)),
        Program::When {
            predicate,
            then_program,
            otherwise_program,
        } => {
            steps.push(
                match predicate {
                    EncounterActivationPredicate::ActivationEligible => "when:activationEligible",
                }
                .to_owned(),
            );
            collect_activation_steps(then_program, steps);
            if let Some(otherwise_program) = otherwise_program {
                collect_activation_steps(otherwise_program, steps);
            }
        }
        Program::Operation(operation) => steps.push(
            match operation {
                EncounterActivationOperation::RecordEncounterActivation => {
                    "recordEncounterActivation"
                }
                EncounterActivationOperation::ActivateBoundMembers => "activateBoundMembers",
                EncounterActivationOperation::EmitEncounterFeedback => "emitEncounterFeedback",
            }
            .to_owned(),
        ),
    }
}

fn clear_readout_steps(program: &EncounterClearProgram) -> Vec<String> {
    let mut steps = Vec::new();
    collect_clear_steps(program, &mut steps);
    steps.truncate(MAX_ENCOUNTER_PROGRAM_READOUT_STEPS);
    steps
}

fn collect_clear_steps(program: &EncounterClearProgram, steps: &mut Vec<String>) {
    match program {
        Program::Sequence { steps: children } => children
            .iter()
            .for_each(|child| collect_clear_steps(child, steps)),
        Program::When {
            predicate,
            then_program,
            otherwise_program,
        } => {
            steps.push(
                match predicate {
                    EncounterClearPredicate::MembersDefeated => "when:membersDefeated",
                }
                .to_owned(),
            );
            collect_clear_steps(then_program, steps);
            if let Some(otherwise_program) = otherwise_program {
                collect_clear_steps(otherwise_program, steps);
            }
        }
        Program::Operation(operation) => steps.push(
            match operation {
                EncounterClearOperation::RecordEncounterCleared => "recordEncounterCleared",
                EncounterClearOperation::OpenBoundExit => "openBoundExit",
            }
            .to_owned(),
        ),
    }
}

fn is_program_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'/' | b'-' | b'_')
        })
}
