//! Closed authored explosive-prop programs.
//!
//! This family selects only a bounded ordering over the prop already chosen by
//! Rust. Rust owns radial queries, occlusion, damage scaling, causes, and the
//! bounded chained-prop queue.

use std::collections::BTreeMap;

use rusty_engine::gameplay_resolution::Program;
use serde::{Deserialize, Serialize};

pub const MAX_EXPLOSIVE_PROP_PROGRAMS: usize = 16;
pub const MAX_EXPLOSIVE_PROP_PROGRAM_BINDINGS: usize = 128;
pub const MAX_EXPLOSIVE_PROP_PROGRAM_STEPS: usize = 16;
pub const MAX_EXPLOSIVE_PROP_PROGRAM_NODES: usize = 64;
pub const MAX_EXPLOSIVE_PROP_PROGRAM_DEPTH: usize = 12;
pub const MAX_EXPLOSIVE_PROP_PROGRAM_READOUT_STEPS: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredExplosivePropProgram {
    pub id: String,
    pub program: StoredExplosivePropProgramNode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredExplosivePropProgramNode {
    Sequence {
        steps: Vec<StoredExplosivePropProgramNode>,
    },
    When {
        predicate: StoredExplosivePropPredicate,
        then_program: Box<StoredExplosivePropProgramNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        otherwise_program: Option<Box<StoredExplosivePropProgramNode>>,
    },
    Operation {
        operation: StoredExplosivePropOperation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredExplosivePropPredicate {
    ExplosionPending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredExplosivePropOperation {
    SelectRadialTargets,
    ApplyScaledDamage,
    ResolveExplosion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExplosivePropPredicate {
    ExplosionPending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExplosivePropOperation {
    SelectRadialTargets,
    ApplyScaledDamage,
    ResolveExplosion,
}

pub(crate) type ExplosivePropProgram = Program<ExplosivePropPredicate, ExplosivePropOperation>;

#[derive(Debug, Clone, Default)]
pub(crate) struct ExplosivePropProgramCatalog {
    programs: BTreeMap<String, ExplosivePropProgram>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplosivePropProgramReadout {
    pub programs: Vec<ExplosivePropProgramShape>,
    pub bindings: Vec<ExplosivePropProgramBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplosivePropProgramShape {
    pub id: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplosivePropProgramBinding {
    pub explosive_prop: u64,
    pub program_id: String,
}

impl ExplosivePropProgramCatalog {
    pub(crate) fn get(&self, id: &str) -> Option<&ExplosivePropProgram> {
        self.programs.get(id)
    }

    pub(crate) fn len(&self) -> usize {
        self.programs.len()
    }

    pub(crate) fn readout(
        &self,
        bindings: impl Iterator<Item = (u64, String)>,
    ) -> ExplosivePropProgramReadout {
        ExplosivePropProgramReadout {
            programs: self
                .programs
                .iter()
                .map(|(id, program)| ExplosivePropProgramShape {
                    id: id.clone(),
                    steps: readout_steps(program),
                })
                .collect(),
            bindings: bindings
                .take(MAX_EXPLOSIVE_PROP_PROGRAM_BINDINGS)
                .map(|(explosive_prop, program_id)| ExplosivePropProgramBinding {
                    explosive_prop,
                    program_id,
                })
                .collect(),
        }
    }
}

pub(crate) fn execute_explosive_prop_program<E>(
    program: &ExplosivePropProgram,
    predicate: &mut impl FnMut(ExplosivePropPredicate) -> Result<bool, E>,
    operation: &mut impl FnMut(ExplosivePropOperation) -> Result<(), E>,
) -> Result<(), E> {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                execute_explosive_prop_program(step, predicate, operation)?;
            }
            Ok(())
        }
        Program::When {
            predicate: wanted,
            then_program,
            otherwise_program,
        } => {
            if predicate(*wanted)? {
                execute_explosive_prop_program(then_program, predicate, operation)
            } else if let Some(otherwise_program) = otherwise_program {
                execute_explosive_prop_program(otherwise_program, predicate, operation)
            } else {
                Ok(())
            }
        }
        Program::Operation(value) => operation(*value),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExplosivePropProgramCompileError {
    TooMany { count: usize },
    DuplicateId { id: String },
    InvalidId { id: String },
    EmptySequence { id: String },
    TooManySteps { id: String, count: usize },
    TooManyNodes { id: String, count: usize },
    TooDeep { id: String, depth: usize },
}

impl std::fmt::Display for ExplosivePropProgramCompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooMany { count } => {
                write!(formatter, "explosive prop program quota exceeded: {count}")
            }
            Self::DuplicateId { id } => {
                write!(formatter, "duplicate explosive prop program `{id}`")
            }
            Self::InvalidId { id } => write!(formatter, "invalid explosive prop program id `{id}`"),
            Self::EmptySequence { id } => write!(
                formatter,
                "explosive prop program `{id}` has an empty sequence"
            ),
            Self::TooManySteps { id, count } => write!(
                formatter,
                "explosive prop program `{id}` has {count} sequence steps"
            ),
            Self::TooManyNodes { id, count } => {
                write!(formatter, "explosive prop program `{id}` has {count} nodes")
            }
            Self::TooDeep { id, depth } => {
                write!(formatter, "explosive prop program `{id}` has depth {depth}")
            }
        }
    }
}

pub(crate) fn compile_explosive_prop_programs(
    authored: &[StoredExplosivePropProgram],
) -> Result<ExplosivePropProgramCatalog, ExplosivePropProgramCompileError> {
    if authored.len() > MAX_EXPLOSIVE_PROP_PROGRAMS {
        return Err(ExplosivePropProgramCompileError::TooMany {
            count: authored.len(),
        });
    }
    let mut programs = BTreeMap::new();
    for stored in authored {
        if !is_program_id(&stored.id) {
            return Err(ExplosivePropProgramCompileError::InvalidId {
                id: stored.id.clone(),
            });
        }
        let mut nodes = 0;
        let compiled = compile_node(&stored.id, &stored.program, 1, &mut nodes)?;
        if programs.insert(stored.id.clone(), compiled).is_some() {
            return Err(ExplosivePropProgramCompileError::DuplicateId {
                id: stored.id.clone(),
            });
        }
    }
    Ok(ExplosivePropProgramCatalog { programs })
}

fn compile_node(
    id: &str,
    node: &StoredExplosivePropProgramNode,
    depth: usize,
    nodes: &mut usize,
) -> Result<ExplosivePropProgram, ExplosivePropProgramCompileError> {
    *nodes += 1;
    if *nodes > MAX_EXPLOSIVE_PROP_PROGRAM_NODES {
        return Err(ExplosivePropProgramCompileError::TooManyNodes {
            id: id.to_owned(),
            count: *nodes,
        });
    }
    if depth > MAX_EXPLOSIVE_PROP_PROGRAM_DEPTH {
        return Err(ExplosivePropProgramCompileError::TooDeep {
            id: id.to_owned(),
            depth,
        });
    }
    match node {
        StoredExplosivePropProgramNode::Sequence { steps } => {
            if steps.is_empty() {
                return Err(ExplosivePropProgramCompileError::EmptySequence { id: id.to_owned() });
            }
            if steps.len() > MAX_EXPLOSIVE_PROP_PROGRAM_STEPS {
                return Err(ExplosivePropProgramCompileError::TooManySteps {
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
        StoredExplosivePropProgramNode::When {
            predicate,
            then_program,
            otherwise_program,
        } => Ok(Program::When {
            predicate: match predicate {
                StoredExplosivePropPredicate::ExplosionPending => {
                    ExplosivePropPredicate::ExplosionPending
                }
            },
            then_program: Box::new(compile_node(id, then_program, depth + 1, nodes)?),
            otherwise_program: otherwise_program
                .as_deref()
                .map(|other| compile_node(id, other, depth + 1, nodes).map(Box::new))
                .transpose()?,
        }),
        StoredExplosivePropProgramNode::Operation { operation } => {
            Ok(Program::Operation(match operation {
                StoredExplosivePropOperation::SelectRadialTargets => {
                    ExplosivePropOperation::SelectRadialTargets
                }
                StoredExplosivePropOperation::ApplyScaledDamage => {
                    ExplosivePropOperation::ApplyScaledDamage
                }
                StoredExplosivePropOperation::ResolveExplosion => {
                    ExplosivePropOperation::ResolveExplosion
                }
            }))
        }
    }
}

fn readout_steps(program: &ExplosivePropProgram) -> Vec<String> {
    let mut steps = Vec::new();
    visit_tree(program, &mut |step| {
        if steps.len() < MAX_EXPLOSIVE_PROP_PROGRAM_READOUT_STEPS {
            steps.push(step.to_owned());
        }
    });
    steps
}

fn visit_tree(program: &ExplosivePropProgram, visit: &mut impl FnMut(&str)) {
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
                ExplosivePropPredicate::ExplosionPending => "when-explosion-pending",
            });
            visit_tree(then_program, visit);
            if let Some(otherwise_program) = otherwise_program {
                visit_tree(otherwise_program, visit);
            }
        }
        Program::Operation(operation) => visit(match operation {
            ExplosivePropOperation::SelectRadialTargets => "select-radial-targets",
            ExplosivePropOperation::ApplyScaledDamage => "apply-scaled-damage",
            ExplosivePropOperation::ResolveExplosion => "resolve-explosion",
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
