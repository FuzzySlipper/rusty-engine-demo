//! Closed authored lift programs. Rust owns timing, translation, state and all
//! world mutation; TypeScript selects only source order over this family.

use rusty_engine::gameplay_resolution::Program;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_LIFT_PROGRAMS: usize = 16;
pub const MAX_LIFT_PROGRAM_BINDINGS: usize = 128;
pub const MAX_LIFT_PROGRAM_STEPS: usize = 16;
pub const MAX_LIFT_PROGRAM_NODES: usize = 64;
pub const MAX_LIFT_PROGRAM_DEPTH: usize = 12;
pub const MAX_LIFT_PROGRAM_READOUT_STEPS: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredLiftProgram {
    pub id: String,
    pub program: StoredLiftProgramNode,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredLiftProgramNode {
    Sequence {
        steps: Vec<StoredLiftProgramNode>,
    },
    When {
        predicate: StoredLiftPredicate,
        then_program: Box<StoredLiftProgramNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        otherwise_program: Option<Box<StoredLiftProgramNode>>,
    },
    Operation {
        operation: StoredLiftOperation,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredLiftPredicate {
    ActivationEntered,
    LoweringMotionTick,
    WaitingTick,
    RaisingMotionTick,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredLiftOperation {
    RecordActivation,
    RequestLowerBoundPlatform,
    AdvanceLowering,
    AdvanceWait,
    AdvanceRaising,
    EmitLiftFeedback,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiftPredicate {
    ActivationEntered,
    LoweringMotionTick,
    WaitingTick,
    RaisingMotionTick,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiftOperation {
    RecordActivation,
    RequestLowerBoundPlatform,
    AdvanceLowering,
    AdvanceWait,
    AdvanceRaising,
    EmitLiftFeedback,
}
pub(crate) type LiftProgram = Program<LiftPredicate, LiftOperation>;
#[derive(Debug, Clone, Default)]
pub(crate) struct LiftProgramCatalog {
    programs: BTreeMap<String, LiftProgram>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiftProgramReadout {
    pub programs: Vec<LiftProgramShape>,
    pub bindings: Vec<LiftProgramBinding>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiftProgramShape {
    pub id: String,
    pub steps: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiftProgramBinding {
    pub lift: u64,
    pub program_id: String,
}
impl LiftProgramCatalog {
    pub(crate) fn get(&self, id: &str) -> Option<&LiftProgram> {
        self.programs.get(id)
    }
    pub(crate) fn len(&self) -> usize {
        self.programs.len()
    }
    pub(crate) fn readout(
        &self,
        bindings: impl Iterator<Item = (u64, String)>,
    ) -> LiftProgramReadout {
        LiftProgramReadout {
            programs: self
                .programs
                .iter()
                .map(|(id, program)| LiftProgramShape {
                    id: id.clone(),
                    steps: readout_steps(program),
                })
                .collect(),
            bindings: bindings
                .take(MAX_LIFT_PROGRAM_BINDINGS)
                .map(|(lift, program_id)| LiftProgramBinding { lift, program_id })
                .collect(),
        }
    }
}
pub(crate) fn execute_lift_program<E>(
    program: &LiftProgram,
    predicate: &mut impl FnMut(LiftPredicate) -> Result<bool, E>,
    operation: &mut impl FnMut(LiftOperation) -> Result<(), E>,
) -> Result<(), E> {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                execute_lift_program(step, predicate, operation)?;
            }
            Ok(())
        }
        Program::When {
            predicate: wanted,
            then_program,
            otherwise_program,
        } => {
            if predicate(*wanted)? {
                execute_lift_program(then_program, predicate, operation)
            } else if let Some(otherwise_program) = otherwise_program {
                execute_lift_program(otherwise_program, predicate, operation)
            } else {
                Ok(())
            }
        }
        Program::Operation(value) => operation(*value),
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LiftProgramCompileError {
    TooMany { count: usize },
    DuplicateId { id: String },
    InvalidId { id: String },
    EmptySequence { id: String },
    TooManySteps { id: String, count: usize },
    TooManyNodes { id: String, count: usize },
    TooDeep { id: String, depth: usize },
}
impl std::fmt::Display for LiftProgramCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
pub(crate) fn compile_lift_programs(
    authored: &[StoredLiftProgram],
) -> Result<LiftProgramCatalog, LiftProgramCompileError> {
    if authored.len() > MAX_LIFT_PROGRAMS {
        return Err(LiftProgramCompileError::TooMany {
            count: authored.len(),
        });
    }
    let mut programs = BTreeMap::new();
    for stored in authored {
        if !is_program_id(&stored.id) {
            return Err(LiftProgramCompileError::InvalidId {
                id: stored.id.clone(),
            });
        }
        let mut nodes = 0;
        let compiled = compile_node(&stored.id, &stored.program, 1, &mut nodes)?;
        if programs.insert(stored.id.clone(), compiled).is_some() {
            return Err(LiftProgramCompileError::DuplicateId {
                id: stored.id.clone(),
            });
        }
    }
    Ok(LiftProgramCatalog { programs })
}
fn compile_node(
    id: &str,
    node: &StoredLiftProgramNode,
    depth: usize,
    nodes: &mut usize,
) -> Result<LiftProgram, LiftProgramCompileError> {
    *nodes += 1;
    if *nodes > MAX_LIFT_PROGRAM_NODES {
        return Err(LiftProgramCompileError::TooManyNodes {
            id: id.to_owned(),
            count: *nodes,
        });
    }
    if depth > MAX_LIFT_PROGRAM_DEPTH {
        return Err(LiftProgramCompileError::TooDeep {
            id: id.to_owned(),
            depth,
        });
    }
    match node {
        StoredLiftProgramNode::Sequence { steps } => {
            if steps.is_empty() {
                return Err(LiftProgramCompileError::EmptySequence { id: id.to_owned() });
            }
            if steps.len() > MAX_LIFT_PROGRAM_STEPS {
                return Err(LiftProgramCompileError::TooManySteps {
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
        StoredLiftProgramNode::When {
            predicate,
            then_program,
            otherwise_program,
        } => Ok(Program::When {
            predicate: match predicate {
                StoredLiftPredicate::ActivationEntered => LiftPredicate::ActivationEntered,
                StoredLiftPredicate::LoweringMotionTick => LiftPredicate::LoweringMotionTick,
                StoredLiftPredicate::WaitingTick => LiftPredicate::WaitingTick,
                StoredLiftPredicate::RaisingMotionTick => LiftPredicate::RaisingMotionTick,
            },
            then_program: Box::new(compile_node(id, then_program, depth + 1, nodes)?),
            otherwise_program: otherwise_program
                .as_deref()
                .map(|other| compile_node(id, other, depth + 1, nodes).map(Box::new))
                .transpose()?,
        }),
        StoredLiftProgramNode::Operation { operation } => Ok(Program::Operation(match operation {
            StoredLiftOperation::RecordActivation => LiftOperation::RecordActivation,
            StoredLiftOperation::RequestLowerBoundPlatform => {
                LiftOperation::RequestLowerBoundPlatform
            }
            StoredLiftOperation::AdvanceLowering => LiftOperation::AdvanceLowering,
            StoredLiftOperation::AdvanceWait => LiftOperation::AdvanceWait,
            StoredLiftOperation::AdvanceRaising => LiftOperation::AdvanceRaising,
            StoredLiftOperation::EmitLiftFeedback => LiftOperation::EmitLiftFeedback,
        })),
    }
}
fn readout_steps(program: &LiftProgram) -> Vec<String> {
    let mut steps = Vec::new();
    visit_tree(program, &mut |step| {
        if steps.len() < MAX_LIFT_PROGRAM_READOUT_STEPS {
            steps.push(step.to_owned());
        }
    });
    steps
}
fn visit_tree(program: &LiftProgram, visit: &mut impl FnMut(&str)) {
    match program {
        Program::Sequence { steps } => steps.iter().for_each(|step| visit_tree(step, visit)),
        Program::When {
            predicate,
            then_program,
            otherwise_program,
        } => {
            visit(match predicate {
                LiftPredicate::ActivationEntered => "when-activation-entered",
                LiftPredicate::LoweringMotionTick => "when-lowering-motion-tick",
                LiftPredicate::WaitingTick => "when-waiting-tick",
                LiftPredicate::RaisingMotionTick => "when-raising-motion-tick",
            });
            visit_tree(then_program, visit);
            if let Some(otherwise_program) = otherwise_program {
                visit_tree(otherwise_program, visit);
            }
        }
        Program::Operation(operation) => visit(match operation {
            LiftOperation::RecordActivation => "record-activation",
            LiftOperation::RequestLowerBoundPlatform => "request-lower-bound-platform",
            LiftOperation::AdvanceLowering => "advance-lowering",
            LiftOperation::AdvanceWait => "advance-wait",
            LiftOperation::AdvanceRaising => "advance-raising",
            LiftOperation::EmitLiftFeedback => "emit-lift-feedback",
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
