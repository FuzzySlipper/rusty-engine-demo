//! Closed authored floor-action programs.
//!
//! TypeScript chooses source order over a very small vocabulary. Rust supplies
//! trigger enter facts, component state, motion interpolation, collision
//! mutation, and presentation facts; authored content cannot choose targets,
//! durations, or translations.

use std::collections::BTreeMap;

use rusty_engine::gameplay_resolution::Program;
use serde::{Deserialize, Serialize};

pub const MAX_FLOOR_ACTION_PROGRAMS: usize = 16;
pub const MAX_FLOOR_ACTION_PROGRAM_BINDINGS: usize = 128;
pub const MAX_FLOOR_ACTION_PROGRAM_STEPS: usize = 16;
pub const MAX_FLOOR_ACTION_PROGRAM_NODES: usize = 64;
pub const MAX_FLOOR_ACTION_PROGRAM_DEPTH: usize = 12;
pub const MAX_FLOOR_ACTION_PROGRAM_READOUT_STEPS: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredFloorActionProgram {
    pub id: String,
    pub program: StoredFloorActionProgramNode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredFloorActionProgramNode {
    Sequence {
        steps: Vec<StoredFloorActionProgramNode>,
    },
    When {
        predicate: StoredFloorActionPredicate,
        then_program: Box<StoredFloorActionProgramNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        otherwise_program: Option<Box<StoredFloorActionProgramNode>>,
    },
    Operation {
        operation: StoredFloorActionOperation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredFloorActionPredicate {
    ActivationEntered,
    LoweringMotionTick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredFloorActionOperation {
    RecordActivation,
    RequestLowerBoundPlatform,
    AdvanceLowering,
    EmitFloorFeedback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FloorActionPredicate {
    ActivationEntered,
    LoweringMotionTick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FloorActionOperation {
    RecordActivation,
    RequestLowerBoundPlatform,
    AdvanceLowering,
    EmitFloorFeedback,
}

pub(crate) type FloorActionProgram = Program<FloorActionPredicate, FloorActionOperation>;

#[derive(Debug, Clone, Default)]
pub(crate) struct FloorActionProgramCatalog {
    programs: BTreeMap<String, FloorActionProgram>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FloorActionProgramReadout {
    pub programs: Vec<FloorActionProgramShape>,
    pub bindings: Vec<FloorActionProgramBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FloorActionProgramShape {
    pub id: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FloorActionProgramBinding {
    pub floor_action: u64,
    pub program_id: String,
}

impl FloorActionProgramCatalog {
    pub(crate) fn get(&self, id: &str) -> Option<&FloorActionProgram> {
        self.programs.get(id)
    }
    pub(crate) fn len(&self) -> usize {
        self.programs.len()
    }

    pub(crate) fn readout(
        &self,
        bindings: impl Iterator<Item = (u64, String)>,
    ) -> FloorActionProgramReadout {
        FloorActionProgramReadout {
            programs: self
                .programs
                .iter()
                .map(|(id, program)| FloorActionProgramShape {
                    id: id.clone(),
                    steps: readout_steps(program),
                })
                .collect(),
            bindings: bindings
                .take(MAX_FLOOR_ACTION_PROGRAM_BINDINGS)
                .map(|(floor_action, program_id)| FloorActionProgramBinding {
                    floor_action,
                    program_id,
                })
                .collect(),
        }
    }
}

pub(crate) fn execute_floor_action_program<E>(
    program: &FloorActionProgram,
    predicate: &mut impl FnMut(FloorActionPredicate) -> Result<bool, E>,
    operation: &mut impl FnMut(FloorActionOperation) -> Result<(), E>,
) -> Result<(), E> {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                execute_floor_action_program(step, predicate, operation)?;
            }
            Ok(())
        }
        Program::When {
            predicate: wanted,
            then_program,
            otherwise_program,
        } => {
            if predicate(*wanted)? {
                execute_floor_action_program(then_program, predicate, operation)
            } else if let Some(otherwise_program) = otherwise_program {
                execute_floor_action_program(otherwise_program, predicate, operation)
            } else {
                Ok(())
            }
        }
        Program::Operation(value) => operation(*value),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FloorActionProgramCompileError {
    TooMany { count: usize },
    DuplicateId { id: String },
    InvalidId { id: String },
    EmptySequence { id: String },
    TooManySteps { id: String, count: usize },
    TooManyNodes { id: String, count: usize },
    TooDeep { id: String, depth: usize },
}

impl std::fmt::Display for FloorActionProgramCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

pub(crate) fn compile_floor_action_programs(
    authored: &[StoredFloorActionProgram],
) -> Result<FloorActionProgramCatalog, FloorActionProgramCompileError> {
    if authored.len() > MAX_FLOOR_ACTION_PROGRAMS {
        return Err(FloorActionProgramCompileError::TooMany {
            count: authored.len(),
        });
    }
    let mut programs = BTreeMap::new();
    for stored in authored {
        if !is_program_id(&stored.id) {
            return Err(FloorActionProgramCompileError::InvalidId {
                id: stored.id.clone(),
            });
        }
        let mut nodes = 0;
        let compiled = compile_node(&stored.id, &stored.program, 1, &mut nodes)?;
        if programs.insert(stored.id.clone(), compiled).is_some() {
            return Err(FloorActionProgramCompileError::DuplicateId {
                id: stored.id.clone(),
            });
        }
    }
    Ok(FloorActionProgramCatalog { programs })
}

fn compile_node(
    id: &str,
    node: &StoredFloorActionProgramNode,
    depth: usize,
    nodes: &mut usize,
) -> Result<FloorActionProgram, FloorActionProgramCompileError> {
    *nodes += 1;
    if *nodes > MAX_FLOOR_ACTION_PROGRAM_NODES {
        return Err(FloorActionProgramCompileError::TooManyNodes {
            id: id.to_owned(),
            count: *nodes,
        });
    }
    if depth > MAX_FLOOR_ACTION_PROGRAM_DEPTH {
        return Err(FloorActionProgramCompileError::TooDeep {
            id: id.to_owned(),
            depth,
        });
    }
    match node {
        StoredFloorActionProgramNode::Sequence { steps } => {
            if steps.is_empty() {
                return Err(FloorActionProgramCompileError::EmptySequence { id: id.to_owned() });
            }
            if steps.len() > MAX_FLOOR_ACTION_PROGRAM_STEPS {
                return Err(FloorActionProgramCompileError::TooManySteps {
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
        StoredFloorActionProgramNode::When {
            predicate,
            then_program,
            otherwise_program,
        } => Ok(Program::When {
            predicate: match predicate {
                StoredFloorActionPredicate::ActivationEntered => {
                    FloorActionPredicate::ActivationEntered
                }
                StoredFloorActionPredicate::LoweringMotionTick => {
                    FloorActionPredicate::LoweringMotionTick
                }
            },
            then_program: Box::new(compile_node(id, then_program, depth + 1, nodes)?),
            otherwise_program: otherwise_program
                .as_deref()
                .map(|other| compile_node(id, other, depth + 1, nodes).map(Box::new))
                .transpose()?,
        }),
        StoredFloorActionProgramNode::Operation { operation } => {
            Ok(Program::Operation(match operation {
                StoredFloorActionOperation::RecordActivation => {
                    FloorActionOperation::RecordActivation
                }
                StoredFloorActionOperation::RequestLowerBoundPlatform => {
                    FloorActionOperation::RequestLowerBoundPlatform
                }
                StoredFloorActionOperation::AdvanceLowering => {
                    FloorActionOperation::AdvanceLowering
                }
                StoredFloorActionOperation::EmitFloorFeedback => {
                    FloorActionOperation::EmitFloorFeedback
                }
            }))
        }
    }
}

fn readout_steps(program: &FloorActionProgram) -> Vec<String> {
    let mut steps = Vec::new();
    visit_tree(program, &mut |step| {
        if steps.len() < MAX_FLOOR_ACTION_PROGRAM_READOUT_STEPS {
            steps.push(step.to_owned());
        }
    });
    steps
}
fn visit_tree(program: &FloorActionProgram, visit: &mut impl FnMut(&str)) {
    match program {
        Program::Sequence { steps } => steps.iter().for_each(|step| visit_tree(step, visit)),
        Program::When {
            predicate,
            then_program,
            otherwise_program,
        } => {
            visit(match predicate {
                FloorActionPredicate::ActivationEntered => "when-activation-entered",
                FloorActionPredicate::LoweringMotionTick => "when-lowering-motion-tick",
            });
            visit_tree(then_program, visit);
            if let Some(otherwise_program) = otherwise_program {
                visit_tree(otherwise_program, visit);
            }
        }
        Program::Operation(operation) => visit(match operation {
            FloorActionOperation::RecordActivation => "record-activation",
            FloorActionOperation::RequestLowerBoundPlatform => "request-lower-bound-platform",
            FloorActionOperation::AdvanceLowering => "advance-lowering",
            FloorActionOperation::EmitFloorFeedback => "emit-floor-feedback",
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
