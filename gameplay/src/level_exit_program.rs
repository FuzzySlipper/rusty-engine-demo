//! Closed authored level-exit completion programs.
//!
//! TypeScript may sequence only the two named completion operations. Rust
//! retains actor/range/death admission, exit identity, state, fact payloads,
//! and presentation delivery.

use std::collections::BTreeMap;

use rusty_engine::gameplay_resolution::Program;
use serde::{Deserialize, Serialize};

pub const MAX_LEVEL_EXIT_PROGRAMS: usize = 16;
pub const MAX_LEVEL_EXIT_PROGRAM_BINDINGS: usize = 64;
pub const MAX_LEVEL_EXIT_PROGRAM_STEPS: usize = 16;
pub const MAX_LEVEL_EXIT_PROGRAM_NODES: usize = 64;
pub const MAX_LEVEL_EXIT_PROGRAM_DEPTH: usize = 12;
pub const MAX_LEVEL_EXIT_PROGRAM_READOUT_STEPS: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredLevelExitProgram {
    pub id: String,
    pub program: StoredLevelExitProgramNode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredLevelExitProgramNode {
    Sequence {
        steps: Vec<StoredLevelExitProgramNode>,
    },
    When {
        predicate: StoredLevelExitPredicate,
        then_program: Box<StoredLevelExitProgramNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        otherwise_program: Option<Box<StoredLevelExitProgramNode>>,
    },
    Operation {
        operation: StoredLevelExitOperation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredLevelExitPredicate {
    ExitAvailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredLevelExitOperation {
    RecordCompletion,
    EmitCompletionPresentation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LevelExitPredicate {
    ExitAvailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LevelExitOperation {
    RecordCompletion,
    EmitCompletionPresentation,
}

pub(crate) type LevelExitProgram = Program<LevelExitPredicate, LevelExitOperation>;

#[derive(Debug, Clone, Default)]
pub(crate) struct LevelExitProgramCatalog {
    programs: BTreeMap<String, LevelExitProgram>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelExitProgramReadout {
    pub programs: Vec<LevelExitProgramShape>,
    pub bindings: Vec<LevelExitProgramBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelExitProgramShape {
    pub id: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelExitProgramBinding {
    pub exit: u64,
    pub program_id: String,
}

impl LevelExitProgramCatalog {
    pub(crate) fn get(&self, id: &str) -> Option<&LevelExitProgram> {
        self.programs.get(id)
    }

    pub(crate) fn len(&self) -> usize {
        self.programs.len()
    }

    pub(crate) fn readout(
        &self,
        bindings: impl Iterator<Item = (u64, String)>,
    ) -> LevelExitProgramReadout {
        LevelExitProgramReadout {
            programs: self
                .programs
                .iter()
                .map(|(id, program)| LevelExitProgramShape {
                    id: id.clone(),
                    steps: readout_steps(program),
                })
                .collect(),
            bindings: bindings
                .take(MAX_LEVEL_EXIT_PROGRAM_BINDINGS)
                .map(|(exit, program_id)| LevelExitProgramBinding { exit, program_id })
                .collect(),
        }
    }
}

pub(crate) fn execute_level_exit_program<E>(
    program: &LevelExitProgram,
    predicate: &mut impl FnMut(LevelExitPredicate) -> Result<bool, E>,
    operation: &mut impl FnMut(LevelExitOperation) -> Result<(), E>,
) -> Result<(), E> {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                execute_level_exit_program(step, predicate, operation)?;
            }
            Ok(())
        }
        Program::When {
            predicate: wanted,
            then_program,
            otherwise_program,
        } => {
            if predicate(*wanted)? {
                execute_level_exit_program(then_program, predicate, operation)
            } else if let Some(otherwise_program) = otherwise_program {
                execute_level_exit_program(otherwise_program, predicate, operation)
            } else {
                Ok(())
            }
        }
        Program::Operation(operation_value) => operation(*operation_value),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LevelExitProgramCompileError {
    TooMany { count: usize },
    DuplicateId { id: String },
    InvalidId { id: String },
    EmptySequence { id: String },
    TooManySteps { id: String, count: usize },
    TooManyNodes { id: String, count: usize },
    TooDeep { id: String, depth: usize },
}

impl std::fmt::Display for LevelExitProgramCompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

pub(crate) fn compile_level_exit_programs(
    authored: &[StoredLevelExitProgram],
) -> Result<LevelExitProgramCatalog, LevelExitProgramCompileError> {
    if authored.len() > MAX_LEVEL_EXIT_PROGRAMS {
        return Err(LevelExitProgramCompileError::TooMany {
            count: authored.len(),
        });
    }
    let mut programs = BTreeMap::new();
    for stored in authored {
        if !is_program_id(&stored.id) {
            return Err(LevelExitProgramCompileError::InvalidId {
                id: stored.id.clone(),
            });
        }
        let mut nodes = 0;
        let compiled = compile_node(&stored.id, &stored.program, 1, &mut nodes)?;
        if programs.insert(stored.id.clone(), compiled).is_some() {
            return Err(LevelExitProgramCompileError::DuplicateId {
                id: stored.id.clone(),
            });
        }
    }
    Ok(LevelExitProgramCatalog { programs })
}

fn compile_node(
    id: &str,
    node: &StoredLevelExitProgramNode,
    depth: usize,
    nodes: &mut usize,
) -> Result<LevelExitProgram, LevelExitProgramCompileError> {
    *nodes += 1;
    if *nodes > MAX_LEVEL_EXIT_PROGRAM_NODES {
        return Err(LevelExitProgramCompileError::TooManyNodes {
            id: id.to_owned(),
            count: *nodes,
        });
    }
    if depth > MAX_LEVEL_EXIT_PROGRAM_DEPTH {
        return Err(LevelExitProgramCompileError::TooDeep {
            id: id.to_owned(),
            depth,
        });
    }
    match node {
        StoredLevelExitProgramNode::Sequence { steps } => {
            if steps.is_empty() {
                return Err(LevelExitProgramCompileError::EmptySequence { id: id.to_owned() });
            }
            if steps.len() > MAX_LEVEL_EXIT_PROGRAM_STEPS {
                return Err(LevelExitProgramCompileError::TooManySteps {
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
        StoredLevelExitProgramNode::When {
            predicate,
            then_program,
            otherwise_program,
        } => Ok(Program::When {
            predicate: match predicate {
                StoredLevelExitPredicate::ExitAvailable => LevelExitPredicate::ExitAvailable,
            },
            then_program: Box::new(compile_node(id, then_program, depth + 1, nodes)?),
            otherwise_program: otherwise_program
                .as_deref()
                .map(|other| compile_node(id, other, depth + 1, nodes).map(Box::new))
                .transpose()?,
        }),
        StoredLevelExitProgramNode::Operation { operation } => {
            Ok(Program::Operation(match operation {
                StoredLevelExitOperation::RecordCompletion => LevelExitOperation::RecordCompletion,
                StoredLevelExitOperation::EmitCompletionPresentation => {
                    LevelExitOperation::EmitCompletionPresentation
                }
            }))
        }
    }
}

fn readout_steps(program: &LevelExitProgram) -> Vec<String> {
    let mut steps = Vec::new();
    visit_tree(program, &mut |step| {
        if steps.len() < MAX_LEVEL_EXIT_PROGRAM_READOUT_STEPS {
            steps.push(step.to_owned());
        }
    });
    steps
}

fn visit_tree(program: &LevelExitProgram, visit: &mut impl FnMut(&str)) {
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
                LevelExitPredicate::ExitAvailable => "when-exit-available",
            });
            visit_tree(then_program, visit);
            if let Some(otherwise_program) = otherwise_program {
                visit_tree(otherwise_program, visit);
            }
        }
        Program::Operation(operation) => visit(match operation {
            LevelExitOperation::RecordCompletion => "record-completion",
            LevelExitOperation::EmitCompletionPresentation => "emit-completion-presentation",
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
