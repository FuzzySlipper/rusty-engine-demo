//! Closed authored switch interaction programs.
//!
//! TypeScript chooses source order over this small vocabulary. Rust retains
//! actor/range/repeatability admission, explicit door targets, door motion,
//! scheduling, mutation, facts, and event delivery.

use std::collections::BTreeMap;

use rusty_engine::gameplay_resolution::Program;
use serde::{Deserialize, Serialize};

pub const MAX_SWITCH_PROGRAMS: usize = 16;
pub const MAX_SWITCH_PROGRAM_BINDINGS: usize = 128;
pub const MAX_SWITCH_PROGRAM_STEPS: usize = 16;
pub const MAX_SWITCH_PROGRAM_NODES: usize = 64;
pub const MAX_SWITCH_PROGRAM_DEPTH: usize = 12;
pub const MAX_SWITCH_PROGRAM_READOUT_STEPS: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredSwitchProgram {
    pub id: String,
    pub program: StoredSwitchProgramNode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredSwitchProgramNode {
    Sequence {
        steps: Vec<StoredSwitchProgramNode>,
    },
    When {
        predicate: StoredSwitchPredicate,
        then_program: Box<StoredSwitchProgramNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        otherwise_program: Option<Box<StoredSwitchProgramNode>>,
    },
    Operation {
        operation: StoredSwitchOperation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredSwitchPredicate {
    SwitchAvailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredSwitchOperation {
    RecordActivation,
    RequestOpenBoundDoor,
    RequestCloseBoundDoor,
    EmitInteractionFeedback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SwitchPredicate {
    SwitchAvailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SwitchOperation {
    RecordActivation,
    RequestOpenBoundDoor,
    RequestCloseBoundDoor,
    EmitInteractionFeedback,
}

pub(crate) type SwitchProgram = Program<SwitchPredicate, SwitchOperation>;

#[derive(Debug, Clone, Default)]
pub(crate) struct SwitchProgramCatalog {
    programs: BTreeMap<String, SwitchProgram>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchProgramReadout {
    pub programs: Vec<SwitchProgramShape>,
    pub bindings: Vec<SwitchProgramBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchProgramShape {
    pub id: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchProgramBinding {
    pub switch: u64,
    pub program_id: String,
}

impl SwitchProgramCatalog {
    pub(crate) fn get(&self, id: &str) -> Option<&SwitchProgram> {
        self.programs.get(id)
    }

    pub(crate) fn len(&self) -> usize {
        self.programs.len()
    }

    pub(crate) fn readout(
        &self,
        bindings: impl Iterator<Item = (u64, String)>,
    ) -> SwitchProgramReadout {
        SwitchProgramReadout {
            programs: self
                .programs
                .iter()
                .map(|(id, program)| SwitchProgramShape {
                    id: id.clone(),
                    steps: readout_steps(program),
                })
                .collect(),
            bindings: bindings
                .take(MAX_SWITCH_PROGRAM_BINDINGS)
                .map(|(switch, program_id)| SwitchProgramBinding { switch, program_id })
                .collect(),
        }
    }
}

pub(crate) fn execute_switch_program<E>(
    program: &SwitchProgram,
    predicate: &mut impl FnMut(SwitchPredicate) -> Result<bool, E>,
    operation: &mut impl FnMut(SwitchOperation) -> Result<(), E>,
) -> Result<(), E> {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                execute_switch_program(step, predicate, operation)?;
            }
            Ok(())
        }
        Program::When {
            predicate: wanted,
            then_program,
            otherwise_program,
        } => {
            if predicate(*wanted)? {
                execute_switch_program(then_program, predicate, operation)
            } else if let Some(otherwise_program) = otherwise_program {
                execute_switch_program(otherwise_program, predicate, operation)
            } else {
                Ok(())
            }
        }
        Program::Operation(value) => operation(*value),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SwitchProgramCompileError {
    TooMany { count: usize },
    DuplicateId { id: String },
    InvalidId { id: String },
    EmptySequence { id: String },
    TooManySteps { id: String, count: usize },
    TooManyNodes { id: String, count: usize },
    TooDeep { id: String, depth: usize },
}

impl std::fmt::Display for SwitchProgramCompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooMany { count } => write!(formatter, "switch program quota exceeded: {count}"),
            Self::DuplicateId { id } => write!(formatter, "duplicate switch program `{id}`"),
            Self::InvalidId { id } => write!(formatter, "invalid switch program id `{id}`"),
            Self::EmptySequence { id } => {
                write!(formatter, "switch program `{id}` has an empty sequence")
            }
            Self::TooManySteps { id, count } => write!(
                formatter,
                "switch program `{id}` has {count} sequence steps"
            ),
            Self::TooManyNodes { id, count } => {
                write!(formatter, "switch program `{id}` has {count} nodes")
            }
            Self::TooDeep { id, depth } => {
                write!(formatter, "switch program `{id}` has depth {depth}")
            }
        }
    }
}

pub(crate) fn compile_switch_programs(
    authored: &[StoredSwitchProgram],
) -> Result<SwitchProgramCatalog, SwitchProgramCompileError> {
    if authored.len() > MAX_SWITCH_PROGRAMS {
        return Err(SwitchProgramCompileError::TooMany {
            count: authored.len(),
        });
    }
    let mut programs = BTreeMap::new();
    for stored in authored {
        if !is_program_id(&stored.id) {
            return Err(SwitchProgramCompileError::InvalidId {
                id: stored.id.clone(),
            });
        }
        let mut nodes = 0;
        let compiled = compile_node(&stored.id, &stored.program, 1, &mut nodes)?;
        if programs.insert(stored.id.clone(), compiled).is_some() {
            return Err(SwitchProgramCompileError::DuplicateId {
                id: stored.id.clone(),
            });
        }
    }
    Ok(SwitchProgramCatalog { programs })
}

fn compile_node(
    id: &str,
    node: &StoredSwitchProgramNode,
    depth: usize,
    nodes: &mut usize,
) -> Result<SwitchProgram, SwitchProgramCompileError> {
    *nodes += 1;
    if *nodes > MAX_SWITCH_PROGRAM_NODES {
        return Err(SwitchProgramCompileError::TooManyNodes {
            id: id.to_owned(),
            count: *nodes,
        });
    }
    if depth > MAX_SWITCH_PROGRAM_DEPTH {
        return Err(SwitchProgramCompileError::TooDeep {
            id: id.to_owned(),
            depth,
        });
    }
    match node {
        StoredSwitchProgramNode::Sequence { steps } => {
            if steps.is_empty() {
                return Err(SwitchProgramCompileError::EmptySequence { id: id.to_owned() });
            }
            if steps.len() > MAX_SWITCH_PROGRAM_STEPS {
                return Err(SwitchProgramCompileError::TooManySteps {
                    id: id.to_owned(),
                    count: steps.len(),
                });
            }
            Ok(Program::Sequence {
                steps: steps
                    .iter()
                    .map(|step| compile_node(id, step, depth + 1, nodes))
                    .collect::<Result<Vec<_>, _>>()?,
            })
        }
        StoredSwitchProgramNode::When {
            predicate,
            then_program,
            otherwise_program,
        } => Ok(Program::When {
            predicate: match predicate {
                StoredSwitchPredicate::SwitchAvailable => SwitchPredicate::SwitchAvailable,
            },
            then_program: Box::new(compile_node(id, then_program, depth + 1, nodes)?),
            otherwise_program: otherwise_program
                .as_deref()
                .map(|other| compile_node(id, other, depth + 1, nodes).map(Box::new))
                .transpose()?,
        }),
        StoredSwitchProgramNode::Operation { operation } => {
            Ok(Program::Operation(match operation {
                StoredSwitchOperation::RecordActivation => SwitchOperation::RecordActivation,
                StoredSwitchOperation::RequestOpenBoundDoor => {
                    SwitchOperation::RequestOpenBoundDoor
                }
                StoredSwitchOperation::RequestCloseBoundDoor => {
                    SwitchOperation::RequestCloseBoundDoor
                }
                StoredSwitchOperation::EmitInteractionFeedback => {
                    SwitchOperation::EmitInteractionFeedback
                }
            }))
        }
    }
}

fn readout_steps(program: &SwitchProgram) -> Vec<String> {
    let mut steps = Vec::new();
    visit_tree(program, &mut |step| {
        if steps.len() < MAX_SWITCH_PROGRAM_READOUT_STEPS {
            steps.push(step.to_owned());
        }
    });
    steps
}

fn visit_tree(program: &SwitchProgram, visit: &mut impl FnMut(&str)) {
    match program {
        Program::Sequence { steps } => steps.iter().for_each(|step| visit_tree(step, visit)),
        Program::When {
            predicate,
            then_program,
            otherwise_program,
        } => {
            visit(match predicate {
                SwitchPredicate::SwitchAvailable => "when-switch-available",
            });
            visit_tree(then_program, visit);
            if let Some(otherwise_program) = otherwise_program {
                visit_tree(otherwise_program, visit);
            }
        }
        Program::Operation(operation) => visit(match operation {
            SwitchOperation::RecordActivation => "record-activation",
            SwitchOperation::RequestOpenBoundDoor => "request-open-bound-door",
            SwitchOperation::RequestCloseBoundDoor => "request-close-bound-door",
            SwitchOperation::EmitInteractionFeedback => "emit-interaction-feedback",
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
