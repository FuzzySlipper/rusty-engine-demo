use std::collections::{BTreeMap, BTreeSet};

use core_ids::EntityId;
use core_math::Vec3;
use engine_spatial::{
    KinematicMotionSystem, MotionFact, MotionPhaseReceipt, NavigationStepError,
    VoxelCollisionScene, MAX_MOTION_DELTA_SECONDS,
};
use entity_state::{EntityCommand, EntityCommandBatch, EntityView};

use crate::combat::EnemyState;
use crate::runtime::RuntimeError;
use crate::session::GameSession;

pub const MAX_NAVIGATION_SPEED_UNITS_PER_SECOND: f32 = 1_000.0;
pub const MAX_NAVIGATION_QUERY_BUDGET: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavigationConfig {
    pub goal: Vec3,
    pub speed_units_per_second: f32,
    pub max_visited: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationState {
    Following,
    Arrived,
    Blocked,
    Unreachable,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavigationComponent {
    pub config: NavigationConfig,
    pub state: NavigationState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationFailure {
    StartNotWalkable,
    GoalNotWalkable,
    NoPath,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NavigationFact {
    Advanced {
        entity: EntityId,
        before: Vec3,
        after: Vec3,
        path_hash: u64,
    },
    Arrived {
        entity: EntityId,
        goal: Vec3,
    },
    Blocked {
        entity: EntityId,
        goal: Vec3,
    },
    Unreachable {
        entity: EntityId,
        goal: Vec3,
        reason: NavigationFailure,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct NavigationPhaseReceipt {
    pub agents_considered: usize,
    pub advanced_agents: usize,
    pub arrived_agents: usize,
    pub blocked_agents: usize,
    pub unreachable_agents: usize,
    pub navigation_hash: u64,
    pub facts: Vec<NavigationFact>,
    pub motion: MotionPhaseReceipt,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NavigationView {
    pub entity: EntityId,
    pub config: NavigationConfig,
    pub state: NavigationState,
    pub entity_view: EntityView,
}

pub(crate) struct EnemyNavigationSystem;

#[derive(Debug, Clone, Copy)]
struct NavigationPlan {
    goal: Vec3,
    before: Vec3,
    path_hash: u64,
    reaches_goal: bool,
}

impl EnemyNavigationSystem {
    pub(crate) fn run(
        session: &mut GameSession,
        scene: &VoxelCollisionScene,
        delta_seconds: f32,
    ) -> Result<NavigationPhaseReceipt, RuntimeError> {
        if !delta_seconds.is_finite()
            || !(0.0..=MAX_MOTION_DELTA_SECONDS).contains(&delta_seconds)
            || delta_seconds == 0.0
        {
            return Err(RuntimeError::InvalidNavigationDelta {
                actual: delta_seconds,
            });
        }

        let active: Vec<_> = session
            .navigators
            .iter()
            .filter(|(entity, navigation)| {
                matches!(
                    navigation.state,
                    NavigationState::Following | NavigationState::Blocked
                ) && session
                    .enemies
                    .get(entity)
                    .is_some_and(|enemy| enemy.state == EnemyState::Alive)
            })
            .map(|(entity, navigation)| (*entity, navigation.config))
            .collect();
        let agents_considered = active.len();
        let selected: BTreeSet<_> = active.iter().map(|(entity, _)| *entity).collect();
        let mut plans = BTreeMap::new();
        let mut velocity_commands = Vec::new();
        let mut facts = Vec::new();
        let mut unreachable_agents = 0usize;

        for (entity, config) in active {
            let view = session
                .entities
                .view(entity)
                .expect("navigation entity validated during admission");
            let before = view
                .transform
                .expect("navigation transform validated during admission")
                .translation;
            let current_velocity = view
                .kinematic
                .expect("navigation kinematic validated during admission")
                .velocity;
            match scene.navigation_step(
                before,
                config.goal,
                current_velocity,
                config.speed_units_per_second * delta_seconds,
                config.max_visited,
            ) {
                Ok(step) => {
                    let velocity = (step.next_waypoint - before) * (1.0 / delta_seconds);
                    velocity_commands
                        .push(EntityCommand::SetKinematicVelocity { entity, velocity });
                    plans.insert(
                        entity,
                        NavigationPlan {
                            goal: config.goal,
                            before,
                            path_hash: step.path_hash,
                            reaches_goal: step.reached,
                        },
                    );
                }
                Err(error) => {
                    let reason = match error {
                        NavigationStepError::StartNotWalkable { .. } => {
                            NavigationFailure::StartNotWalkable
                        }
                        NavigationStepError::GoalNotWalkable { .. } => {
                            NavigationFailure::GoalNotWalkable
                        }
                        NavigationStepError::NoPath { .. } => NavigationFailure::NoPath,
                        NavigationStepError::InvalidRequest { .. } => {
                            return Err(RuntimeError::NavigationStep {
                                entity,
                                source: error,
                            });
                        }
                    };
                    session
                        .navigators
                        .get_mut(&entity)
                        .expect("active navigation component")
                        .state = NavigationState::Unreachable;
                    velocity_commands.push(EntityCommand::SetKinematicVelocity {
                        entity,
                        velocity: Vec3::ZERO,
                    });
                    facts.push(NavigationFact::Unreachable {
                        entity,
                        goal: config.goal,
                        reason,
                    });
                    unreachable_agents += 1;
                }
            }
        }

        if !velocity_commands.is_empty() {
            session
                .entities
                .apply_batch(EntityCommandBatch::new(velocity_commands))
                .map_err(RuntimeError::EntityBatch)?;
        }
        let motion = KinematicMotionSystem::run_selected(
            &mut session.entities,
            scene,
            delta_seconds,
            &selected,
        )
        .map_err(RuntimeError::Motion)?;

        let mut advanced = BTreeSet::new();
        let mut blocked = BTreeSet::new();
        for fact in &motion.facts {
            match fact {
                MotionFact::Moved { entity, after, .. } if plans.contains_key(entity) => {
                    advanced.insert(*entity);
                    let plan = plans[entity];
                    facts.push(NavigationFact::Advanced {
                        entity: *entity,
                        before: plan.before,
                        after: *after,
                        path_hash: plan.path_hash,
                    });
                }
                MotionFact::Blocked { entity, .. } if plans.contains_key(entity) => {
                    blocked.insert(*entity);
                }
                MotionFact::Moved { .. } | MotionFact::Blocked { .. } => {}
            }
        }

        let mut arrived_agents = 0usize;
        let mut stop_commands = Vec::new();
        for (entity, plan) in plans {
            let navigation = session
                .navigators
                .get_mut(&entity)
                .expect("planned navigation component");
            if blocked.contains(&entity) {
                navigation.state = NavigationState::Blocked;
                stop_commands.push(EntityCommand::SetKinematicVelocity {
                    entity,
                    velocity: Vec3::ZERO,
                });
                facts.push(NavigationFact::Blocked {
                    entity,
                    goal: plan.goal,
                });
            } else if plan.reaches_goal {
                navigation.state = NavigationState::Arrived;
                stop_commands.push(EntityCommand::SetKinematicVelocity {
                    entity,
                    velocity: Vec3::ZERO,
                });
                facts.push(NavigationFact::Arrived {
                    entity,
                    goal: plan.goal,
                });
                arrived_agents += 1;
            } else {
                navigation.state = NavigationState::Following;
            }
        }
        if !stop_commands.is_empty() {
            session
                .entities
                .apply_batch(EntityCommandBatch::new(stop_commands))
                .map_err(RuntimeError::EntityBatch)?;
        }

        Ok(NavigationPhaseReceipt {
            agents_considered,
            advanced_agents: advanced.len(),
            arrived_agents,
            blocked_agents: blocked.len(),
            unreachable_agents,
            navigation_hash: scene.navigation_hash(),
            facts,
            motion,
        })
    }
}
