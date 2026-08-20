//! Closed authored environmental hazard programs.
//!
//! This family owns only the ordering of the bounded hazard predicates and
//! consequences. Spatial overlap, vitality, cooldown values, and all state
//! mutation remain in the Rust hazard phase.

use std::collections::BTreeMap;

use rusty_engine::gameplay_resolution::Program;
use serde::{Deserialize, Serialize};

pub const MAX_HAZARD_PROGRAMS: usize = 16;
pub const MAX_HAZARD_PROGRAM_BINDINGS: usize = 128;
pub const MAX_HAZARD_PROGRAM_STEPS: usize = 16;
pub const MAX_HAZARD_PROGRAM_NODES: usize = 64;
pub const MAX_HAZARD_PROGRAM_DEPTH: usize = 12;
pub const MAX_HAZARD_PROGRAM_READOUT_STEPS: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredHazardProgram {
    pub id: String,
    pub program: StoredHazardProgramNode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredHazardProgramNode {
    Sequence {
        steps: Vec<StoredHazardProgramNode>,
    },
    When {
        predicate: StoredHazardPredicate,
        then_program: Box<StoredHazardProgramNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        otherwise_program: Option<Box<StoredHazardProgramNode>>,
    },
    Operation {
        operation: StoredHazardOperation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredHazardPredicate {
    PlayerOverlapping,
    PlayerEligible,
    CooldownReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredHazardOperation {
    ApplyHazardDamage,
    ScheduleHazardCooldown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HazardPredicate {
    PlayerOverlapping,
    PlayerEligible,
    CooldownReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HazardOperation {
    ApplyHazardDamage,
    ScheduleHazardCooldown,
}

pub(crate) type HazardProgram = Program<HazardPredicate, HazardOperation>;

#[derive(Debug, Clone, Default)]
pub(crate) struct HazardProgramCatalog {
    programs: BTreeMap<String, HazardProgram>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HazardProgramReadout {
    pub programs: Vec<HazardProgramShape>,
    pub bindings: Vec<HazardProgramBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HazardProgramShape {
    pub id: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HazardProgramBinding {
    pub hazard: u64,
    pub program_id: String,
}

impl HazardProgramCatalog {
    pub(crate) fn get(&self, id: &str) -> Option<&HazardProgram> {
        self.programs.get(id)
    }

    pub(crate) fn len(&self) -> usize {
        self.programs.len()
    }

    pub(crate) fn readout(
        &self,
        bindings: impl Iterator<Item = (u64, String)>,
    ) -> HazardProgramReadout {
        HazardProgramReadout {
            programs: self
                .programs
                .iter()
                .map(|(id, program)| HazardProgramShape {
                    id: id.clone(),
                    steps: readout_steps(program),
                })
                .collect(),
            bindings: bindings
                .take(MAX_HAZARD_PROGRAM_BINDINGS)
                .map(|(hazard, program_id)| HazardProgramBinding { hazard, program_id })
                .collect(),
        }
    }
}

pub(crate) fn execute_hazard_program<E>(
    program: &HazardProgram,
    predicate: &mut impl FnMut(HazardPredicate) -> Result<bool, E>,
    operation: &mut impl FnMut(HazardOperation) -> Result<(), E>,
) -> Result<(), E> {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                execute_hazard_program(step, predicate, operation)?;
            }
            Ok(())
        }
        Program::When {
            predicate: wanted,
            then_program,
            otherwise_program,
        } => {
            if predicate(*wanted)? {
                execute_hazard_program(then_program, predicate, operation)
            } else if let Some(otherwise_program) = otherwise_program {
                execute_hazard_program(otherwise_program, predicate, operation)
            } else {
                Ok(())
            }
        }
        Program::Operation(value) => operation(*value),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HazardProgramCompileError {
    TooMany { count: usize },
    DuplicateId { id: String },
    InvalidId { id: String },
    EmptySequence { id: String },
    TooManySteps { id: String, count: usize },
    TooManyNodes { id: String, count: usize },
    TooDeep { id: String, depth: usize },
}

impl std::fmt::Display for HazardProgramCompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooMany { count } => write!(formatter, "hazard program quota exceeded: {count}"),
            Self::DuplicateId { id } => write!(formatter, "duplicate hazard program `{id}`"),
            Self::InvalidId { id } => write!(formatter, "invalid hazard program id `{id}`"),
            Self::EmptySequence { id } => {
                write!(formatter, "hazard program `{id}` has an empty sequence")
            }
            Self::TooManySteps { id, count } => write!(
                formatter,
                "hazard program `{id}` has {count} sequence steps"
            ),
            Self::TooManyNodes { id, count } => {
                write!(formatter, "hazard program `{id}` has {count} nodes")
            }
            Self::TooDeep { id, depth } => {
                write!(formatter, "hazard program `{id}` has depth {depth}")
            }
        }
    }
}

pub(crate) fn compile_hazard_programs(
    authored: &[StoredHazardProgram],
) -> Result<HazardProgramCatalog, HazardProgramCompileError> {
    if authored.len() > MAX_HAZARD_PROGRAMS {
        return Err(HazardProgramCompileError::TooMany {
            count: authored.len(),
        });
    }
    let mut programs = BTreeMap::new();
    for stored in authored {
        if !is_program_id(&stored.id) {
            return Err(HazardProgramCompileError::InvalidId {
                id: stored.id.clone(),
            });
        }
        let mut nodes = 0;
        let compiled = compile_node(&stored.id, &stored.program, 1, &mut nodes)?;
        if programs.insert(stored.id.clone(), compiled).is_some() {
            return Err(HazardProgramCompileError::DuplicateId {
                id: stored.id.clone(),
            });
        }
    }
    Ok(HazardProgramCatalog { programs })
}

fn compile_node(
    id: &str,
    node: &StoredHazardProgramNode,
    depth: usize,
    nodes: &mut usize,
) -> Result<HazardProgram, HazardProgramCompileError> {
    *nodes += 1;
    if *nodes > MAX_HAZARD_PROGRAM_NODES {
        return Err(HazardProgramCompileError::TooManyNodes {
            id: id.to_owned(),
            count: *nodes,
        });
    }
    if depth > MAX_HAZARD_PROGRAM_DEPTH {
        return Err(HazardProgramCompileError::TooDeep {
            id: id.to_owned(),
            depth,
        });
    }
    match node {
        StoredHazardProgramNode::Sequence { steps } => {
            if steps.is_empty() {
                return Err(HazardProgramCompileError::EmptySequence { id: id.to_owned() });
            }
            if steps.len() > MAX_HAZARD_PROGRAM_STEPS {
                return Err(HazardProgramCompileError::TooManySteps {
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
        StoredHazardProgramNode::When {
            predicate,
            then_program,
            otherwise_program,
        } => Ok(Program::When {
            predicate: match predicate {
                StoredHazardPredicate::PlayerOverlapping => HazardPredicate::PlayerOverlapping,
                StoredHazardPredicate::PlayerEligible => HazardPredicate::PlayerEligible,
                StoredHazardPredicate::CooldownReady => HazardPredicate::CooldownReady,
            },
            then_program: Box::new(compile_node(id, then_program, depth + 1, nodes)?),
            otherwise_program: otherwise_program
                .as_deref()
                .map(|other| compile_node(id, other, depth + 1, nodes).map(Box::new))
                .transpose()?,
        }),
        StoredHazardProgramNode::Operation { operation } => {
            Ok(Program::Operation(match operation {
                StoredHazardOperation::ApplyHazardDamage => HazardOperation::ApplyHazardDamage,
                StoredHazardOperation::ScheduleHazardCooldown => {
                    HazardOperation::ScheduleHazardCooldown
                }
            }))
        }
    }
}

fn readout_steps(program: &HazardProgram) -> Vec<String> {
    let mut steps = Vec::new();
    visit_tree(program, &mut |step| {
        if steps.len() < MAX_HAZARD_PROGRAM_READOUT_STEPS {
            steps.push(step.to_owned());
        }
    });
    steps
}

fn visit_tree(program: &HazardProgram, visit: &mut impl FnMut(&str)) {
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
                HazardPredicate::PlayerOverlapping => "when-player-overlapping",
                HazardPredicate::PlayerEligible => "when-player-eligible",
                HazardPredicate::CooldownReady => "when-cooldown-ready",
            });
            visit_tree(then_program, visit);
            if let Some(otherwise_program) = otherwise_program {
                visit_tree(otherwise_program, visit);
            }
        }
        Program::Operation(operation) => visit(match operation {
            HazardOperation::ApplyHazardDamage => "apply-hazard-damage",
            HazardOperation::ScheduleHazardCooldown => "schedule-hazard-cooldown",
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
