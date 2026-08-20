//! Closed authored enemy programs.
//!
//! Enemy attack and defeat consequences deliberately have their own typed
//! vocabularies. They reuse only the Engine's structural `Program` tree; they
//! are not additions to the item/weapon program language.

use std::collections::BTreeMap;

use rusty_engine::gameplay_resolution::Program;
use serde::{Deserialize, Serialize};

pub const MAX_ENEMY_PROGRAMS: usize = 16;
/// Per-enemy profile bindings are separate from the bounded program catalog.
/// E1M1 currently admits 29 enemy-combat entities.
pub const MAX_ENEMY_PROGRAM_BINDINGS: usize = 64;
pub const MAX_ENEMY_PROGRAM_STEPS: usize = 16;
pub const MAX_ENEMY_PROGRAM_NODES: usize = 64;
pub const MAX_ENEMY_PROGRAM_DEPTH: usize = 12;
pub const MAX_ENEMY_PROGRAM_READOUT_STEPS: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredEnemyAttackProgram {
    pub id: String,
    pub program: StoredEnemyAttackProgramNode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredEnemyAttackProgramNode {
    Sequence {
        steps: Vec<StoredEnemyAttackProgramNode>,
    },
    When {
        predicate: StoredEnemyAttackPredicate,
        then_program: Box<StoredEnemyAttackProgramNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        otherwise_program: Option<Box<StoredEnemyAttackProgramNode>>,
    },
    Operation {
        operation: StoredEnemyAttackOperation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredEnemyAttackPredicate {
    ImpactIsHit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredEnemyAttackOperation {
    RecordEnemyAttack,
    ApplyEnemyHit,
    ApplyEnemyMiss,
    SpawnEnemyProjectile,
    SetEnemyCooldown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredEnemyDefeatProgram {
    pub id: String,
    pub program: StoredEnemyDefeatProgramNode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredEnemyDefeatProgramNode {
    Sequence {
        steps: Vec<StoredEnemyDefeatProgramNode>,
    },
    Operation {
        operation: StoredEnemyDefeatOperation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredEnemyDefeatOperation {
    RecordEnemyDefeat,
    ActivateBoundDrop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnemyAttackPredicate {
    ImpactIsHit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnemyAttackOperation {
    RecordEnemyAttack,
    ApplyEnemyHit,
    ApplyEnemyMiss,
    SpawnEnemyProjectile,
    SetEnemyCooldown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnemyDefeatOperation {
    RecordEnemyDefeat,
    ActivateBoundDrop,
}

pub(crate) type EnemyAttackProgram = Program<EnemyAttackPredicate, EnemyAttackOperation>;
pub(crate) type EnemyDefeatProgram = Program<(), EnemyDefeatOperation>;

#[derive(Debug, Clone, Default)]
pub(crate) struct EnemyAttackProgramCatalog {
    programs: BTreeMap<String, EnemyAttackProgram>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct EnemyDefeatProgramCatalog {
    programs: BTreeMap<String, EnemyDefeatProgram>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnemyProgramReadout {
    pub attack: EnemyAttackProgramReadout,
    pub defeat: EnemyDefeatProgramReadout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnemyAttackProgramReadout {
    pub programs: Vec<EnemyAttackProgramShape>,
    pub bindings: Vec<EnemyAttackProgramBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnemyAttackProgramShape {
    pub id: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnemyAttackProgramBinding {
    pub enemy: u64,
    pub program_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnemyDefeatProgramReadout {
    pub programs: Vec<EnemyDefeatProgramShape>,
    pub bindings: Vec<EnemyDefeatProgramBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnemyDefeatProgramShape {
    pub id: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnemyDefeatProgramBinding {
    pub enemy: u64,
    pub program_id: String,
}

impl EnemyAttackProgramCatalog {
    pub(crate) fn get(&self, id: &str) -> Option<&EnemyAttackProgram> {
        self.programs.get(id)
    }

    pub(crate) fn len(&self) -> usize {
        self.programs.len()
    }

    pub(crate) fn readout(
        &self,
        bindings: impl Iterator<Item = (u64, String)>,
    ) -> EnemyAttackProgramReadout {
        EnemyAttackProgramReadout {
            programs: self
                .programs
                .iter()
                .map(|(id, program)| EnemyAttackProgramShape {
                    id: id.clone(),
                    steps: attack_readout_steps(program),
                })
                .collect(),
            bindings: bindings
                .take(MAX_ENEMY_PROGRAM_BINDINGS)
                .map(|(enemy, program_id)| EnemyAttackProgramBinding { enemy, program_id })
                .collect(),
        }
    }
}

impl EnemyDefeatProgramCatalog {
    pub(crate) fn get(&self, id: &str) -> Option<&EnemyDefeatProgram> {
        self.programs.get(id)
    }

    pub(crate) fn len(&self) -> usize {
        self.programs.len()
    }

    pub(crate) fn readout(
        &self,
        bindings: impl Iterator<Item = (u64, String)>,
    ) -> EnemyDefeatProgramReadout {
        EnemyDefeatProgramReadout {
            programs: self
                .programs
                .iter()
                .map(|(id, program)| EnemyDefeatProgramShape {
                    id: id.clone(),
                    steps: defeat_readout_steps(program),
                })
                .collect(),
            bindings: bindings
                .take(MAX_ENEMY_PROGRAM_BINDINGS)
                .map(|(enemy, program_id)| EnemyDefeatProgramBinding { enemy, program_id })
                .collect(),
        }
    }
}

pub(crate) fn enemy_program_readout(
    attack: &EnemyAttackProgramCatalog,
    defeat: &EnemyDefeatProgramCatalog,
    attack_bindings: impl Iterator<Item = (u64, String)>,
    defeat_bindings: impl Iterator<Item = (u64, String)>,
) -> EnemyProgramReadout {
    EnemyProgramReadout {
        attack: attack.readout(attack_bindings),
        defeat: defeat.readout(defeat_bindings),
    }
}

pub(crate) fn attack_operation_label(operation: EnemyAttackOperation) -> &'static str {
    match operation {
        EnemyAttackOperation::RecordEnemyAttack => "record-enemy-attack",
        EnemyAttackOperation::ApplyEnemyHit => "apply-enemy-hit",
        EnemyAttackOperation::ApplyEnemyMiss => "apply-enemy-miss",
        EnemyAttackOperation::SpawnEnemyProjectile => "spawn-enemy-projectile",
        EnemyAttackOperation::SetEnemyCooldown => "set-enemy-cooldown",
    }
}

pub(crate) fn defeat_operation_label(operation: EnemyDefeatOperation) -> &'static str {
    match operation {
        EnemyDefeatOperation::RecordEnemyDefeat => "record-enemy-defeat",
        EnemyDefeatOperation::ActivateBoundDrop => "activate-bound-drop",
    }
}

pub(crate) fn attack_program_operations(program: &EnemyAttackProgram) -> Vec<EnemyAttackOperation> {
    let mut operations = Vec::new();
    visit_attack_operations(program, &mut |operation| operations.push(operation));
    operations
}

pub(crate) fn defeat_program_activates_bound_drop(program: &EnemyDefeatProgram) -> bool {
    let mut activates = false;
    visit_defeat_operations(program, &mut |operation| {
        activates |= operation == EnemyDefeatOperation::ActivateBoundDrop;
    });
    activates
}

fn attack_readout_steps(program: &EnemyAttackProgram) -> Vec<String> {
    let mut steps = Vec::new();
    visit_attack_tree(program, &mut |entry| {
        if steps.len() < MAX_ENEMY_PROGRAM_READOUT_STEPS {
            steps.push(entry.to_owned());
        }
    });
    steps
}

fn defeat_readout_steps(program: &EnemyDefeatProgram) -> Vec<String> {
    let mut steps = Vec::new();
    visit_defeat_tree(program, &mut |entry| {
        if steps.len() < MAX_ENEMY_PROGRAM_READOUT_STEPS {
            steps.push(entry.to_owned());
        }
    });
    steps
}

fn visit_attack_tree(program: &EnemyAttackProgram, visit: &mut impl FnMut(&str)) {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                visit_attack_tree(step, visit);
            }
        }
        Program::When {
            predicate,
            then_program,
            otherwise_program,
        } => {
            visit(match predicate {
                EnemyAttackPredicate::ImpactIsHit => "when-impact-is-hit",
            });
            visit_attack_tree(then_program, visit);
            if let Some(otherwise_program) = otherwise_program {
                visit_attack_tree(otherwise_program, visit);
            }
        }
        Program::Operation(operation) => visit(attack_operation_label(*operation)),
    }
}

fn visit_defeat_tree(program: &EnemyDefeatProgram, visit: &mut impl FnMut(&str)) {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                visit_defeat_tree(step, visit);
            }
        }
        Program::When { .. } => unreachable!("enemy defeat grammar has no predicates"),
        Program::Operation(operation) => visit(defeat_operation_label(*operation)),
    }
}

fn visit_attack_operations(
    program: &EnemyAttackProgram,
    visit: &mut impl FnMut(EnemyAttackOperation),
) {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                visit_attack_operations(step, visit);
            }
        }
        Program::When {
            then_program,
            otherwise_program,
            ..
        } => {
            visit_attack_operations(then_program, visit);
            if let Some(otherwise_program) = otherwise_program {
                visit_attack_operations(otherwise_program, visit);
            }
        }
        Program::Operation(operation) => visit(*operation),
    }
}

fn visit_defeat_operations(
    program: &EnemyDefeatProgram,
    visit: &mut impl FnMut(EnemyDefeatOperation),
) {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                visit_defeat_operations(step, visit);
            }
        }
        Program::When { .. } => unreachable!("enemy defeat grammar has no predicates"),
        Program::Operation(operation) => visit(*operation),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EnemyProgramCompileError {
    TooMany {
        family: &'static str,
        count: usize,
    },
    DuplicateId {
        family: &'static str,
        id: String,
    },
    InvalidId {
        family: &'static str,
        id: String,
    },
    EmptySequence {
        family: &'static str,
        id: String,
    },
    TooManySteps {
        family: &'static str,
        id: String,
        count: usize,
    },
    TooManyNodes {
        family: &'static str,
        id: String,
        count: usize,
    },
    TooDeep {
        family: &'static str,
        id: String,
        depth: usize,
    },
    InvalidAttackOperationContext {
        id: String,
        operation: StoredEnemyAttackOperation,
    },
}

impl std::fmt::Display for EnemyProgramCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooMany { family, count } => write!(f, "{family} program quota exceeded: {count}"),
            Self::DuplicateId { family, id } => write!(f, "duplicate {family} program `{id}`"),
            Self::InvalidId { family, id } => write!(f, "invalid {family} program id `{id}`"),
            Self::EmptySequence { family, id } => write!(f, "{family} program `{id}` has an empty sequence"),
            Self::TooManySteps { family, id, count } => write!(f, "{family} program `{id}` has {count} sequence steps"),
            Self::TooManyNodes { family, id, count } => write!(f, "{family} program `{id}` has {count} nodes"),
            Self::TooDeep { family, id, depth } => write!(f, "{family} program `{id}` has depth {depth}"),
            Self::InvalidAttackOperationContext { id, operation } => write!(f, "enemy attack program `{id}` uses {operation:?} outside its impact evidence context"),
        }
    }
}

pub(crate) fn compile_enemy_attack_programs(
    authored: &[StoredEnemyAttackProgram],
) -> Result<EnemyAttackProgramCatalog, EnemyProgramCompileError> {
    if authored.len() > MAX_ENEMY_PROGRAMS {
        return Err(EnemyProgramCompileError::TooMany {
            family: "enemy attack",
            count: authored.len(),
        });
    }
    let mut programs = BTreeMap::new();
    for program in authored {
        validate_program_id("enemy attack", &program.id)?;
        let mut nodes = 0;
        let compiled = compile_attack_node(&program.id, &program.program, 1, &mut nodes, false)?;
        if programs.insert(program.id.clone(), compiled).is_some() {
            return Err(EnemyProgramCompileError::DuplicateId {
                family: "enemy attack",
                id: program.id.clone(),
            });
        }
    }
    Ok(EnemyAttackProgramCatalog { programs })
}

pub(crate) fn compile_enemy_defeat_programs(
    authored: &[StoredEnemyDefeatProgram],
) -> Result<EnemyDefeatProgramCatalog, EnemyProgramCompileError> {
    if authored.len() > MAX_ENEMY_PROGRAMS {
        return Err(EnemyProgramCompileError::TooMany {
            family: "enemy defeat",
            count: authored.len(),
        });
    }
    let mut programs = BTreeMap::new();
    for program in authored {
        validate_program_id("enemy defeat", &program.id)?;
        let mut nodes = 0;
        let compiled = compile_defeat_node(&program.id, &program.program, 1, &mut nodes)?;
        if programs.insert(program.id.clone(), compiled).is_some() {
            return Err(EnemyProgramCompileError::DuplicateId {
                family: "enemy defeat",
                id: program.id.clone(),
            });
        }
    }
    Ok(EnemyDefeatProgramCatalog { programs })
}

fn validate_program_id(family: &'static str, id: &str) -> Result<(), EnemyProgramCompileError> {
    if !is_program_id(id) {
        return Err(EnemyProgramCompileError::InvalidId {
            family,
            id: id.to_owned(),
        });
    }
    Ok(())
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

fn check_node(
    family: &'static str,
    id: &str,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), EnemyProgramCompileError> {
    *nodes += 1;
    if *nodes > MAX_ENEMY_PROGRAM_NODES {
        return Err(EnemyProgramCompileError::TooManyNodes {
            family,
            id: id.to_owned(),
            count: *nodes,
        });
    }
    if depth > MAX_ENEMY_PROGRAM_DEPTH {
        return Err(EnemyProgramCompileError::TooDeep {
            family,
            id: id.to_owned(),
            depth,
        });
    }
    Ok(())
}

fn compile_attack_node(
    id: &str,
    node: &StoredEnemyAttackProgramNode,
    depth: usize,
    nodes: &mut usize,
    hit_evidence: bool,
) -> Result<EnemyAttackProgram, EnemyProgramCompileError> {
    check_node("enemy attack", id, depth, nodes)?;
    Ok(match node {
        StoredEnemyAttackProgramNode::Sequence { steps } => {
            check_sequence("enemy attack", id, steps.len())?;
            Program::Sequence {
                steps: steps
                    .iter()
                    .map(|step| compile_attack_node(id, step, depth + 1, nodes, hit_evidence))
                    .collect::<Result<_, _>>()?,
            }
        }
        StoredEnemyAttackProgramNode::When {
            predicate,
            then_program,
            otherwise_program,
        } => Program::When {
            predicate: match predicate {
                StoredEnemyAttackPredicate::ImpactIsHit => EnemyAttackPredicate::ImpactIsHit,
            },
            then_program: Box::new(compile_attack_node(
                id,
                then_program,
                depth + 1,
                nodes,
                true,
            )?),
            otherwise_program: otherwise_program
                .as_deref()
                .map(|node| compile_attack_node(id, node, depth + 1, nodes, false))
                .transpose()?
                .map(Box::new),
        },
        StoredEnemyAttackProgramNode::Operation { operation } => {
            if matches!(operation, StoredEnemyAttackOperation::ApplyEnemyHit) && !hit_evidence
                || matches!(operation, StoredEnemyAttackOperation::ApplyEnemyMiss) && hit_evidence
            {
                return Err(EnemyProgramCompileError::InvalidAttackOperationContext {
                    id: id.to_owned(),
                    operation: *operation,
                });
            }
            Program::Operation(match operation {
                StoredEnemyAttackOperation::RecordEnemyAttack => {
                    EnemyAttackOperation::RecordEnemyAttack
                }
                StoredEnemyAttackOperation::ApplyEnemyHit => EnemyAttackOperation::ApplyEnemyHit,
                StoredEnemyAttackOperation::ApplyEnemyMiss => EnemyAttackOperation::ApplyEnemyMiss,
                StoredEnemyAttackOperation::SpawnEnemyProjectile => {
                    EnemyAttackOperation::SpawnEnemyProjectile
                }
                StoredEnemyAttackOperation::SetEnemyCooldown => {
                    EnemyAttackOperation::SetEnemyCooldown
                }
            })
        }
    })
}

fn compile_defeat_node(
    id: &str,
    node: &StoredEnemyDefeatProgramNode,
    depth: usize,
    nodes: &mut usize,
) -> Result<EnemyDefeatProgram, EnemyProgramCompileError> {
    check_node("enemy defeat", id, depth, nodes)?;
    Ok(match node {
        StoredEnemyDefeatProgramNode::Sequence { steps } => {
            check_sequence("enemy defeat", id, steps.len())?;
            Program::Sequence {
                steps: steps
                    .iter()
                    .map(|step| compile_defeat_node(id, step, depth + 1, nodes))
                    .collect::<Result<_, _>>()?,
            }
        }
        StoredEnemyDefeatProgramNode::Operation { operation } => {
            Program::Operation(match operation {
                StoredEnemyDefeatOperation::RecordEnemyDefeat => {
                    EnemyDefeatOperation::RecordEnemyDefeat
                }
                StoredEnemyDefeatOperation::ActivateBoundDrop => {
                    EnemyDefeatOperation::ActivateBoundDrop
                }
            })
        }
    })
}

fn check_sequence(
    family: &'static str,
    id: &str,
    count: usize,
) -> Result<(), EnemyProgramCompileError> {
    if count == 0 {
        return Err(EnemyProgramCompileError::EmptySequence {
            family,
            id: id.to_owned(),
        });
    }
    if count > MAX_ENEMY_PROGRAM_STEPS {
        return Err(EnemyProgramCompileError::TooManySteps {
            family,
            id: id.to_owned(),
            count,
        });
    }
    Ok(())
}

/// Structural evaluator shared only by the two enemy families. All predicate
/// and operation meaning remains family-local at the call site.
pub(crate) fn execute_enemy_attack_program<E>(
    program: &EnemyAttackProgram,
    predicate: &mut impl FnMut(EnemyAttackPredicate) -> Result<bool, E>,
    operation: &mut impl FnMut(EnemyAttackOperation) -> Result<(), E>,
) -> Result<(), E> {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                execute_enemy_attack_program(step, predicate, operation)?;
            }
            Ok(())
        }
        Program::When {
            predicate: wanted,
            then_program,
            otherwise_program,
        } => {
            if predicate(*wanted)? {
                execute_enemy_attack_program(then_program, predicate, operation)
            } else if let Some(otherwise_program) = otherwise_program {
                execute_enemy_attack_program(otherwise_program, predicate, operation)
            } else {
                Ok(())
            }
        }
        Program::Operation(operation_value) => operation(*operation_value),
    }
}

pub(crate) fn execute_enemy_defeat_program<E>(
    program: &EnemyDefeatProgram,
    operation: &mut impl FnMut(EnemyDefeatOperation) -> Result<(), E>,
) -> Result<(), E> {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                execute_enemy_defeat_program(step, operation)?;
            }
            Ok(())
        }
        Program::When { .. } => unreachable!("enemy defeat grammar has no predicates"),
        Program::Operation(operation_value) => operation(*operation_value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enemy_attack_and_defeat_catalogs_are_separate_closed_families() {
        let attacks = compile_enemy_attack_programs(&[StoredEnemyAttackProgram {
            id: "enemy-attack/hitscan".into(),
            program: StoredEnemyAttackProgramNode::Sequence {
                steps: vec![StoredEnemyAttackProgramNode::Operation {
                    operation: StoredEnemyAttackOperation::RecordEnemyAttack,
                }],
            },
        }])
        .expect("attack program compiles");
        let defeats = compile_enemy_defeat_programs(&[StoredEnemyDefeatProgram {
            id: "enemy-defeat/without-drop".into(),
            program: StoredEnemyDefeatProgramNode::Operation {
                operation: StoredEnemyDefeatOperation::RecordEnemyDefeat,
            },
        }])
        .expect("defeat program compiles");

        assert!(attacks.get("enemy-defeat/without-drop").is_none());
        assert!(defeats.get("enemy-attack/hitscan").is_none());
    }

    #[test]
    fn enemy_hits_require_impact_evidence() {
        let error = compile_enemy_attack_programs(&[StoredEnemyAttackProgram {
            id: "enemy-attack/bad".into(),
            program: StoredEnemyAttackProgramNode::Operation {
                operation: StoredEnemyAttackOperation::ApplyEnemyHit,
            },
        }])
        .expect_err("unconditional hit is invalid");
        assert!(matches!(
            error,
            EnemyProgramCompileError::InvalidAttackOperationContext { .. }
        ));
    }
}
