//! Closed authored pickup collection programs.
//!
//! This family describes only a pickup's bounded collection consequences. It
//! shares the Engine's structural `Program` tree, but no item/weapon/enemy
//! operation or predicate vocabulary, and Rust remains the authority for the
//! pickup's concrete item, placement, lifecycle, and mutation transaction.

use std::collections::BTreeMap;

use rusty_engine::gameplay_resolution::Program;
use serde::{Deserialize, Serialize};

use crate::gameplay_program::{
    GameplayProgramOutcome, GameplayProgramOutcomeStatus, MAX_GAMEPLAY_PROGRAM_OUTCOME_EFFECTS,
    MAX_GAMEPLAY_PROGRAM_OUTCOME_OPERATIONS,
};
use crate::inventory::ItemKind;

pub const MAX_PICKUP_PROGRAMS: usize = 16;
/// E1M1 currently has 78 pickup components, including dormant enemy drops.
/// Keep the product readout independently bounded without silently losing
/// bindings.
pub const MAX_PICKUP_PROGRAM_BINDINGS: usize = 128;
pub const MAX_PICKUP_PROGRAM_STEPS: usize = 16;
pub const MAX_PICKUP_PROGRAM_NODES: usize = 64;
pub const MAX_PICKUP_PROGRAM_DEPTH: usize = 12;
pub const MAX_PICKUP_PROGRAM_READOUT_STEPS: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredPickupProgram {
    pub id: String,
    pub program: StoredPickupProgramNode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredPickupProgramNode {
    Sequence {
        steps: Vec<StoredPickupProgramNode>,
    },
    When {
        predicate: StoredPickupPredicate,
        then_program: Box<StoredPickupProgramNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        otherwise_program: Option<Box<StoredPickupProgramNode>>,
    },
    Operation {
        operation: StoredPickupOperation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredPickupPredicate {
    WeaponAlreadyOwnedWithStarterAmmunition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoredPickupOperation {
    GrantPickedItem,
    GrantStarterAmmunition,
    UseGrantedHealthSupply,
    ApplyGrantedArmor,
    ConsumePickup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickupPredicate {
    WeaponAlreadyOwnedWithStarterAmmunition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickupOperation {
    GrantPickedItem,
    GrantStarterAmmunition,
    UseGrantedHealthSupply,
    ApplyGrantedArmor,
    ConsumePickup,
}

pub(crate) type PickupProgram = Program<PickupPredicate, PickupOperation>;

#[derive(Debug, Clone, Default)]
pub(crate) struct PickupProgramCatalog {
    programs: BTreeMap<String, PickupProgram>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickupProgramReadout {
    pub programs: Vec<PickupProgramShape>,
    pub bindings: Vec<PickupProgramBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickupProgramShape {
    pub id: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickupProgramBinding {
    pub pickup: u64,
    pub program_id: String,
}

impl PickupProgramCatalog {
    pub(crate) fn get(&self, id: &str) -> Option<&PickupProgram> {
        self.programs.get(id)
    }

    pub(crate) fn len(&self) -> usize {
        self.programs.len()
    }

    pub(crate) fn readout(
        &self,
        bindings: impl Iterator<Item = (u64, String)>,
    ) -> PickupProgramReadout {
        PickupProgramReadout {
            programs: self
                .programs
                .iter()
                .map(|(id, program)| PickupProgramShape {
                    id: id.clone(),
                    steps: pickup_program_readout_steps(program),
                })
                .collect(),
            bindings: bindings
                .take(MAX_PICKUP_PROGRAM_BINDINGS)
                .map(|(pickup, program_id)| PickupProgramBinding { pickup, program_id })
                .collect(),
        }
    }
}

pub(crate) fn pickup_operation_label(operation: PickupOperation) -> &'static str {
    match operation {
        PickupOperation::GrantPickedItem => "grant-picked-item",
        PickupOperation::GrantStarterAmmunition => "grant-starter-ammunition",
        PickupOperation::UseGrantedHealthSupply => "use-granted-health-supply",
        PickupOperation::ApplyGrantedArmor => "apply-granted-armor",
        PickupOperation::ConsumePickup => "consume-pickup",
    }
}

pub(crate) fn pickup_program_operation_labels(program: &PickupProgram) -> Vec<String> {
    let mut labels = Vec::new();
    visit_pickup_operations(program, &mut |operation| {
        if labels.len() < MAX_PICKUP_PROGRAM_READOUT_STEPS {
            labels.push(pickup_operation_label(operation).to_owned());
        }
    });
    labels
}

pub(crate) fn pickup_applied_outcome(
    program_id: String,
    program: &PickupProgram,
    executed_operations: Vec<String>,
    effects: Vec<String>,
) -> GameplayProgramOutcome {
    GameplayProgramOutcome {
        program_id,
        status: GameplayProgramOutcomeStatus::Applied,
        planned_operations: pickup_program_operation_labels(program),
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

pub(crate) fn pickup_rejected_outcome(
    program_id: String,
    program: &PickupProgram,
    reason: impl Into<String>,
) -> GameplayProgramOutcome {
    GameplayProgramOutcome {
        program_id,
        status: GameplayProgramOutcomeStatus::Rejected,
        planned_operations: pickup_program_operation_labels(program),
        executed_operations: Vec::new(),
        effects: Vec::new(),
        rejection_reason: Some(reason.into().chars().take(160).collect()),
    }
}

pub(crate) fn pickup_program_uses_operation(
    program: &PickupProgram,
    wanted: PickupOperation,
) -> bool {
    let mut found = false;
    visit_pickup_operations(program, &mut |operation| found |= operation == wanted);
    found
}

/// Check the small, family-local compatibility relation at admission. This
/// inspects membership only; live routing executes the tree structurally.
pub(crate) fn pickup_program_is_compatible(
    program: &PickupProgram,
    item_kind: &ItemKind,
    has_starter_ammunition: bool,
) -> bool {
    let is_weapon_with_starter = matches!(item_kind, ItemKind::Weapon(_)) && has_starter_ammunition;
    let is_automatic_health = matches!(
        item_kind,
        ItemKind::HealthSupply {
            automatic_use: true,
            ..
        }
    );
    let is_armor = matches!(item_kind, ItemKind::Armor { .. });
    let mut compatible = true;
    fn visit_predicates(program: &PickupProgram, visit: &mut impl FnMut(PickupPredicate)) {
        match program {
            Program::Sequence { steps } => {
                for step in steps {
                    visit_predicates(step, visit);
                }
            }
            Program::When {
                predicate,
                then_program,
                otherwise_program,
            } => {
                visit(*predicate);
                visit_predicates(then_program, visit);
                if let Some(otherwise_program) = otherwise_program {
                    visit_predicates(otherwise_program, visit);
                }
            }
            Program::Operation(_) => {}
        }
    }
    visit_predicates(program, &mut |predicate| match predicate {
        PickupPredicate::WeaponAlreadyOwnedWithStarterAmmunition => {
            compatible &= is_weapon_with_starter
        }
    });
    visit_pickup_operations(program, &mut |operation| match operation {
        PickupOperation::GrantPickedItem | PickupOperation::ConsumePickup => {}
        PickupOperation::GrantStarterAmmunition => compatible &= is_weapon_with_starter,
        PickupOperation::UseGrantedHealthSupply => compatible &= is_automatic_health,
        PickupOperation::ApplyGrantedArmor => compatible &= is_armor,
    });
    compatible && pickup_program_uses_operation(program, PickupOperation::ConsumePickup)
}

fn pickup_program_readout_steps(program: &PickupProgram) -> Vec<String> {
    let mut steps = Vec::new();
    visit_pickup_tree(program, &mut |entry| {
        if steps.len() < MAX_PICKUP_PROGRAM_READOUT_STEPS {
            steps.push(entry.to_owned());
        }
    });
    steps
}

fn visit_pickup_tree(program: &PickupProgram, visit: &mut impl FnMut(&str)) {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                visit_pickup_tree(step, visit);
            }
        }
        Program::When {
            predicate,
            then_program,
            otherwise_program,
        } => {
            visit(match predicate {
                PickupPredicate::WeaponAlreadyOwnedWithStarterAmmunition => {
                    "when-weapon-already-owned-with-starter-ammunition"
                }
            });
            visit_pickup_tree(then_program, visit);
            if let Some(otherwise_program) = otherwise_program {
                visit_pickup_tree(otherwise_program, visit);
            }
        }
        Program::Operation(operation) => visit(pickup_operation_label(*operation)),
    }
}

fn visit_pickup_operations(program: &PickupProgram, visit: &mut impl FnMut(PickupOperation)) {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                visit_pickup_operations(step, visit);
            }
        }
        Program::When {
            then_program,
            otherwise_program,
            ..
        } => {
            visit_pickup_operations(then_program, visit);
            if let Some(otherwise_program) = otherwise_program {
                visit_pickup_operations(otherwise_program, visit);
            }
        }
        Program::Operation(operation) => visit(*operation),
    }
}

pub(crate) fn execute_pickup_program<C, E>(
    program: &PickupProgram,
    context: &mut C,
    predicate: &mut impl FnMut(&mut C, PickupPredicate) -> Result<bool, E>,
    operation: &mut impl FnMut(&mut C, PickupOperation) -> Result<(), E>,
) -> Result<(), E> {
    match program {
        Program::Sequence { steps } => {
            for step in steps {
                execute_pickup_program(step, context, predicate, operation)?;
            }
            Ok(())
        }
        Program::When {
            predicate: wanted,
            then_program,
            otherwise_program,
        } => {
            if predicate(context, *wanted)? {
                execute_pickup_program(then_program, context, predicate, operation)
            } else if let Some(otherwise_program) = otherwise_program {
                execute_pickup_program(otherwise_program, context, predicate, operation)
            } else {
                Ok(())
            }
        }
        Program::Operation(operation_value) => operation(context, *operation_value),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PickupProgramCompileError {
    TooMany { count: usize },
    DuplicateId { id: String },
    InvalidId { id: String },
    EmptySequence { id: String },
    TooManySteps { id: String, count: usize },
    TooManyNodes { id: String, count: usize },
    TooDeep { id: String, depth: usize },
}

impl std::fmt::Display for PickupProgramCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooMany { count } => write!(f, "pickup program quota exceeded: {count}"),
            Self::DuplicateId { id } => write!(f, "duplicate pickup program `{id}`"),
            Self::InvalidId { id } => write!(f, "invalid pickup program id `{id}`"),
            Self::EmptySequence { id } => write!(f, "pickup program `{id}` has an empty sequence"),
            Self::TooManySteps { id, count } => {
                write!(f, "pickup program `{id}` has {count} sequence steps")
            }
            Self::TooManyNodes { id, count } => {
                write!(f, "pickup program `{id}` has {count} nodes")
            }
            Self::TooDeep { id, depth } => write!(f, "pickup program `{id}` has depth {depth}"),
        }
    }
}

pub(crate) fn compile_pickup_programs(
    authored: &[StoredPickupProgram],
) -> Result<PickupProgramCatalog, PickupProgramCompileError> {
    if authored.len() > MAX_PICKUP_PROGRAMS {
        return Err(PickupProgramCompileError::TooMany {
            count: authored.len(),
        });
    }
    let mut programs = BTreeMap::new();
    for program in authored {
        if !is_program_id(&program.id) {
            return Err(PickupProgramCompileError::InvalidId {
                id: program.id.clone(),
            });
        }
        let mut nodes = 0;
        let compiled = compile_node(&program.id, &program.program, 1, &mut nodes)?;
        if programs.insert(program.id.clone(), compiled).is_some() {
            return Err(PickupProgramCompileError::DuplicateId {
                id: program.id.clone(),
            });
        }
    }
    Ok(PickupProgramCatalog { programs })
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
    node: &StoredPickupProgramNode,
    depth: usize,
    nodes: &mut usize,
) -> Result<PickupProgram, PickupProgramCompileError> {
    *nodes += 1;
    if *nodes > MAX_PICKUP_PROGRAM_NODES {
        return Err(PickupProgramCompileError::TooManyNodes {
            id: id.to_owned(),
            count: *nodes,
        });
    }
    if depth > MAX_PICKUP_PROGRAM_DEPTH {
        return Err(PickupProgramCompileError::TooDeep {
            id: id.to_owned(),
            depth,
        });
    }
    Ok(match node {
        StoredPickupProgramNode::Sequence { steps } => {
            if steps.is_empty() {
                return Err(PickupProgramCompileError::EmptySequence { id: id.to_owned() });
            }
            if steps.len() > MAX_PICKUP_PROGRAM_STEPS {
                return Err(PickupProgramCompileError::TooManySteps {
                    id: id.to_owned(),
                    count: steps.len(),
                });
            }
            Program::Sequence {
                steps: steps
                    .iter()
                    .map(|step| compile_node(id, step, depth + 1, nodes))
                    .collect::<Result<_, _>>()?,
            }
        }
        StoredPickupProgramNode::When {
            predicate,
            then_program,
            otherwise_program,
        } => Program::When {
            predicate: match predicate {
                StoredPickupPredicate::WeaponAlreadyOwnedWithStarterAmmunition => {
                    PickupPredicate::WeaponAlreadyOwnedWithStarterAmmunition
                }
            },
            then_program: Box::new(compile_node(id, then_program, depth + 1, nodes)?),
            otherwise_program: otherwise_program
                .as_deref()
                .map(|node| compile_node(id, node, depth + 1, nodes))
                .transpose()?
                .map(Box::new),
        },
        StoredPickupProgramNode::Operation { operation } => Program::Operation(match operation {
            StoredPickupOperation::GrantPickedItem => PickupOperation::GrantPickedItem,
            StoredPickupOperation::GrantStarterAmmunition => {
                PickupOperation::GrantStarterAmmunition
            }
            StoredPickupOperation::UseGrantedHealthSupply => {
                PickupOperation::UseGrantedHealthSupply
            }
            StoredPickupOperation::ApplyGrantedArmor => PickupOperation::ApplyGrantedArmor,
            StoredPickupOperation::ConsumePickup => PickupOperation::ConsumePickup,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_weapon_branch_in_source_order() {
        let catalog = compile_pickup_programs(&[StoredPickupProgram {
            id: "pickup/test".to_owned(),
            program: StoredPickupProgramNode::Sequence {
                steps: vec![
                    StoredPickupProgramNode::When {
                        predicate: StoredPickupPredicate::WeaponAlreadyOwnedWithStarterAmmunition,
                        then_program: Box::new(StoredPickupProgramNode::Operation {
                            operation: StoredPickupOperation::GrantStarterAmmunition,
                        }),
                        otherwise_program: Some(Box::new(StoredPickupProgramNode::Operation {
                            operation: StoredPickupOperation::GrantPickedItem,
                        })),
                    },
                    StoredPickupProgramNode::Operation {
                        operation: StoredPickupOperation::ConsumePickup,
                    },
                ],
            },
        }])
        .expect("closed pickup program admits");
        let program = catalog.get("pickup/test").expect("compiled entry");
        let mut operations = Vec::new();
        let mut context = ();
        execute_pickup_program(
            program,
            &mut context,
            &mut |_, predicate| {
                Ok::<_, ()>(matches!(
                    predicate,
                    PickupPredicate::WeaponAlreadyOwnedWithStarterAmmunition
                ))
            },
            &mut |_, operation| {
                operations.push(pickup_operation_label(operation));
                Ok::<_, ()>(())
            },
        )
        .expect("execution succeeds");
        assert_eq!(operations, ["grant-starter-ammunition", "consume-pickup"]);
    }

    #[test]
    fn predicate_cannot_be_bound_to_a_non_weapon_pickup() {
        let program = compile_pickup_programs(&[StoredPickupProgram {
            id: "pickup/branch".to_owned(),
            program: StoredPickupProgramNode::Sequence {
                steps: vec![
                    StoredPickupProgramNode::When {
                        predicate: StoredPickupPredicate::WeaponAlreadyOwnedWithStarterAmmunition,
                        then_program: Box::new(StoredPickupProgramNode::Operation {
                            operation: StoredPickupOperation::GrantPickedItem,
                        }),
                        otherwise_program: None,
                    },
                    StoredPickupProgramNode::Operation {
                        operation: StoredPickupOperation::ConsumePickup,
                    },
                ],
            },
        }])
        .expect("grammar compiles")
        .get("pickup/branch")
        .cloned()
        .expect("compiled entry");
        assert!(!pickup_program_is_compatible(
            &program,
            &ItemKind::Ammunition,
            false
        ));
    }
}
