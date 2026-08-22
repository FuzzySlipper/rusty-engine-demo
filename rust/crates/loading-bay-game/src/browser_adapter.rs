//! Renderer-neutral browser/desktop projection over the Loading Bay service.
//!
//! HTTP/WebSocket live in the `browser-host` binary.  This module contains the
//! shared product projection required by both that adapter and in-process
//! desktop IPC, with no socket, process, or window ownership.

use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use rusty_engine::engine_spatial::VoxelCollisionScene;
use rusty_engine::render_model::RenderFrameDiff;
use serde::Serialize;

use crate::{
    project_doom_e1m1_application_content, project_stored_voxel_objects, GameLoopAdvanceReceipt,
    GameplayApplicationProjector, LoadingBayProductService, LoadingBayProjectReadout,
    LoadingBayServiceCommand, LoadingBayServiceOutcome, LoadingBayServiceReceipt,
    ProjectedApplicationContent, StoredAsset, StoredProject,
};

#[allow(dead_code)]
#[path = "bin/browser_host/presentation.rs"]
mod presentation;
#[path = "bin/browser_host/state.rs"]
mod state;

pub use presentation::BrowserFeedbackProjection;
pub use state::{
    browser_dynamic_state_with_gameplay_frame, browser_state, browser_static_resources,
    browser_static_revision, BrowserDynamicState, BrowserState, BrowserStaticResources,
};

/// Transport-neutral transient evidence from the Rust fixed-step authority.
/// Browser WebSocket and desktop IPC consume this exact bounded projection;
/// only envelope/delta mechanics remain transport-specific.
pub struct ProjectionFeedback {
    pub facts: Vec<(String, Option<u64>)>,
    pub feedback: BrowserFeedbackProjection,
    pub presentation_facts: Vec<crate::GameLoopFact>,
}

pub fn emits_locomotion_feedback(tick: u64) -> bool {
    tick.is_multiple_of(6)
}

pub fn drain_projection_feedback(
    facts_from_service: Vec<crate::GameLoopFact>,
    presentation_tick: u64,
) -> ProjectionFeedback {
    let mut facts = Vec::new();
    let mut feedback = BrowserFeedbackProjection::default();
    let mut presentation_facts = Vec::new();
    for fact in facts_from_service {
        presentation_facts.push(fact.clone());
        match fact {
            crate::GameLoopFact::PlayerControl(value) => {
                facts.push((player_fact_name(&value).to_owned(), None));
                if emits_locomotion_feedback(presentation_tick) {
                    feedback.extend_player_control(std::slice::from_ref(&value));
                }
            }
            crate::GameLoopFact::Navigation(value) => {
                facts.push((navigation_fact_name(&value).to_owned(), None))
            }
            crate::GameLoopFact::EnemyCombat(value) => {
                facts.push((enemy_combat_fact_name(&value).to_owned(), None));
                feedback.extend_enemy_combat(std::slice::from_ref(&value));
            }
            crate::GameLoopFact::Combat(value) => {
                facts.push((combat_fact_name(&value).to_owned(), None));
                feedback.extend_combat(std::slice::from_ref(&value));
            }
            crate::GameLoopFact::ExtractionBeacon(value) => {
                facts.push(("ExtractionBeaconActivated".to_owned(), None));
                feedback.extend_extraction_beacon(value);
            }
            crate::GameLoopFact::Pickup(value) => {
                facts.push(("PickupCollected".to_owned(), None));
                feedback.extend_pickup(&value);
            }
            crate::GameLoopFact::Inventory(_) => {
                facts.push(("InventoryWeaponSelected".to_owned(), None))
            }
            crate::GameLoopFact::Vitality(value) => {
                facts.push((vitality_fact_name(&value).to_owned(), None));
                feedback.extend_vitality(std::slice::from_ref(&value));
            }
            crate::GameLoopFact::Hazard(crate::HazardFact::Damage(value)) => {
                facts.push(("DamageApplied".to_owned(), None));
                feedback.extend_vitality(std::slice::from_ref(&value));
            }
            crate::GameLoopFact::FloorAction(_) => {
                facts.push(("FloorActionActivated".to_owned(), None))
            }
            crate::GameLoopFact::Lift(_) => facts.push(("LiftActivated".to_owned(), None)),
            crate::GameLoopFact::Progression(value) => {
                facts.push((progression_fact_name(&value).to_owned(), None));
                feedback.extend_progression(&value);
            }
            crate::GameLoopFact::DoorAccessRejected {
                sequence,
                door,
                required_key,
                presentation,
            } => {
                facts.push(("DoorAccessRejectedLocked".to_owned(), Some(sequence)));
                feedback.extend_door_access_denied(door, &required_key, &presentation);
            }
            crate::GameLoopFact::PickupRejected { .. } => {
                facts.push(("PickupRejected".to_owned(), None))
            }
            crate::GameLoopFact::Event(value) => {
                facts.push((event_name(&value).to_owned(), None));
                feedback.extend_events(std::slice::from_ref(&value));
            }
            crate::GameLoopFact::CombatRejected {
                attacker,
                weapon,
                presentation,
                reason,
            } => {
                if reason == crate::CombatRejectionReason::NoAmmo {
                    if let (Some(weapon), Some(presentation)) = (&weapon, &presentation) {
                        feedback.extend_dry_fire(attacker, weapon, presentation);
                    }
                }
                facts.push(("CombatRejected".to_owned(), None));
            }
            crate::GameLoopFact::EdgeCommandRejected { sequence, .. } => {
                facts.push(("InputEdgeRejected".to_owned(), Some(sequence)))
            }
            crate::GameLoopFact::InputExpired { .. } => {
                facts.push(("InputExpired".to_owned(), None))
            }
            crate::GameLoopFact::RestartRequested { sequence, .. } => {
                facts.push(("RestartRequested".to_owned(), Some(sequence)))
            }
            crate::GameLoopFact::SaveRequested { sequence, .. } => {
                facts.push(("SaveRequested".to_owned(), Some(sequence)))
            }
            crate::GameLoopFact::LoadRequested { sequence, .. } => {
                facts.push(("LoadRequested".to_owned(), Some(sequence)))
            }
        }
    }
    ProjectionFeedback {
        facts,
        feedback,
        presentation_facts,
    }
}

fn player_fact_name(fact: &crate::PlayerControlFact) -> &'static str {
    match fact {
        crate::PlayerControlFact::Moved { .. } => "PlayerMoved",
        crate::PlayerControlFact::Blocked { .. } => "PlayerBlocked",
        crate::PlayerControlFact::Stepped { .. } => "PlayerStepped",
        crate::PlayerControlFact::Jumped { .. } => "PlayerJumped",
        crate::PlayerControlFact::Landed { .. } => "PlayerLanded",
        crate::PlayerControlFact::LookChanged { .. } => "PlayerLookChanged",
    }
}

fn navigation_fact_name(fact: &crate::NavigationFact) -> &'static str {
    match fact {
        crate::NavigationFact::Advanced { .. } => "NavigationAdvanced",
        crate::NavigationFact::Arrived { .. } => "NavigationArrived",
        crate::NavigationFact::Blocked { .. } => "NavigationBlocked",
        crate::NavigationFact::Unreachable { .. } => "NavigationUnreachable",
    }
}

fn enemy_combat_fact_name(fact: &crate::EnemyCombatFact) -> &'static str {
    match fact {
        crate::EnemyCombatFact::Alerted { .. } => "EnemyAlerted",
        crate::EnemyCombatFact::PostureChanged { .. } => "EnemyPostureChanged",
        crate::EnemyCombatFact::AttackFired { .. } => "EnemyAttackFired",
        crate::EnemyCombatFact::AttackHit { .. } => "EnemyAttackHit",
        crate::EnemyCombatFact::AttackMissed { .. } => "EnemyAttackMissed",
        crate::EnemyCombatFact::ProjectileSpawned { .. } => "EnemyProjectileSpawned",
        crate::EnemyCombatFact::Vitality(value) => vitality_fact_name(value),
    }
}

fn combat_fact_name(fact: &crate::CombatFact) -> &'static str {
    match fact {
        crate::CombatFact::Inventory(_) => "CombatAmmunitionConsumed",
        crate::CombatFact::AttackFired { .. } => "CombatFired",
        crate::CombatFact::AttackHit { .. } => "CombatHit",
        crate::CombatFact::AttackMissed {
            reason: crate::CombatMissReason::NoTarget,
            ..
        } => "CombatMissedNoTarget",
        crate::CombatFact::AttackMissed {
            reason: crate::CombatMissReason::WorldBlocked,
            ..
        } => "CombatMissedWorldBlocked",
        crate::CombatFact::ImpactResolved {
            kind: crate::CombatImpactKind::Blood,
            ..
        } => "CombatBloodImpactResolved",
        crate::CombatFact::ImpactResolved {
            kind: crate::CombatImpactKind::BulletPuff,
            ..
        } => "CombatBulletPuffResolved",
        crate::CombatFact::Vitality(value) => vitality_fact_name(value),
        crate::CombatFact::EnemyDefeated { .. } => "CombatEnemyDefeated",
        crate::CombatFact::EnemyDrop(_) => "EnemyDropMaterialized",
        crate::CombatFact::ExplosiveProp(_) => "ExplosivePropTriggered",
        crate::CombatFact::ProjectileImpacted { .. } => "ProjectileImpacted",
        crate::CombatFact::ProjectileExpired { .. } => "ProjectileExpired",
    }
}

fn vitality_fact_name(fact: &crate::VitalityFact) -> &'static str {
    match fact {
        crate::VitalityFact::DamageApplied { .. } => "DamageApplied",
        crate::VitalityFact::Died { .. } => "EntityDied",
        crate::VitalityFact::EnemyDefeatProgramRecorded { .. } => "EnemyDefeatProgramRecorded",
        crate::VitalityFact::ArmorGranted { .. } => "ArmorGranted",
        crate::VitalityFact::HealthRestored { .. } => "HealthRestored",
    }
}

fn progression_fact_name(fact: &crate::ProgressionFact) -> &'static str {
    match fact {
        crate::ProgressionFact::DoorAccessGranted { .. } => "DoorAccessGranted",
        crate::ProgressionFact::SecretDiscovered { .. } => "SecretDiscovered",
        crate::ProgressionFact::LevelCompleted { .. } => "LevelCompleted",
    }
}

fn event_name(event: &crate::GameEvent) -> &'static str {
    match event {
        crate::GameEvent::SwitchActivated { .. } => "SwitchActivated",
        crate::GameEvent::DoorOpened { .. } => "DoorOpened",
        crate::GameEvent::DoorClosed { .. } => "DoorClosed",
        crate::GameEvent::EnemyDefeated { .. } => "EnemyDefeated",
        crate::GameEvent::PlayerDied { .. } => "PlayerDied",
        crate::GameEvent::EncounterActivated { .. } => "EncounterActivated",
        crate::GameEvent::EncounterCleared { .. } => "EncounterCleared",
    }
}

const ACTOR: rusty_engine::core_ids::EntityId = rusty_engine::core_ids::EntityId::new(1);
const BEACON: rusty_engine::core_ids::EntityId = rusty_engine::core_ids::EntityId::new(7);
const ENCOUNTER: rusty_engine::core_ids::EntityId = rusty_engine::core_ids::EntityId::new(2);
const EXIT: rusty_engine::core_ids::EntityId = rusty_engine::core_ids::EntityId::new(3);
const FIRST_ENEMY: u64 = 4;
const MOTION_PROBE: rusty_engine::core_ids::EntityId = rusty_engine::core_ids::EntityId::new(10);

fn browser_animated_mesh_assets(document: &StoredProject) -> Vec<&StoredAsset> {
    let referenced_assets = document
        .scenes
        .iter()
        .find(|scene| scene.id == document.entry_scene)
        .into_iter()
        .flat_map(|scene| &scene.entities)
        .filter_map(|entity| entity.renderable.as_ref())
        .map(|renderable| renderable.asset.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    document
        .assets
        .iter()
        .filter(|asset| {
            asset.animated_mesh.is_some() && referenced_assets.contains(asset.id.as_str())
        })
        .collect()
}

/// The one Loading Bay admission/projection layer shared by the browser
/// transport adapter and the in-process desktop adapter. It deliberately owns
/// no socket or window lifecycle.
#[derive(Debug)]
pub struct BrowserRuntime {
    host_session_id: String,
    service: LoadingBayProductService,
    voxel_object_frame: RenderFrameDiff,
    application_content: Option<ProjectedApplicationContent>,
    gameplay_projector: Option<GameplayApplicationProjector>,
    voxel_environment_role: &'static str,
}

impl BrowserRuntime {
    pub fn load(project_path: &Path, save_root: &Path) -> Result<Self, String> {
        let service = LoadingBayProductService::admit(project_path, save_root)
            .map_err(|error| error.to_string())?;
        let voxel_object_frame =
            project_stored_voxel_objects(service.authored_project().document())
                .map_err(|error| format!("voxel-object projection failed: {error}"))?;
        let voxel_environment_role = service
            .authored_project()
            .document()
            .scenes
            .iter()
            .find(|scene| scene.id == service.authored_project().document().entry_scene)
            .and_then(|scene| scene.voxel_environment.as_ref())
            .map_or("none", |environment| {
                if environment.gameplay_proxy() {
                    "gameplayProxy"
                } else {
                    "visible"
                }
            });
        let uses_application_content = service.project().project_id == "doom-e1m1";
        let mut gameplay_projector = uses_application_content
            .then(|| GameplayApplicationProjector::new(service.authored_project().document()));
        let initial_gameplay_frame = gameplay_projector
            .as_mut()
            .map(|projector| projector.project(service.runtime().runtime()))
            .transpose()
            .map_err(|error| format!("project initial gameplay frame: {error}"))?
            .unwrap_or_else(|| RenderFrameDiff::try_from_ops(Vec::new()).expect("empty frame"));
        let application_content = if uses_application_content {
            let admitted_scene = service
                .runtime()
                .runtime()
                .collision_scene()
                .ok_or_else(|| "Doom E1M1 has no admitted voxel environment".to_owned())?;
            let rendered_scene = VoxelCollisionScene::from_material_voxels(
                admitted_scene.voxel_size(),
                admitted_scene.chunk_size(),
                admitted_scene.material_voxels().to_vec(),
            )
            .map_err(|error| format!("admit Doom E1M1 rendered volume: {error:?}"))?;
            Some(
                project_doom_e1m1_application_content(
                    service.authored_project().document(),
                    &rendered_scene,
                    &voxel_object_frame,
                    &initial_gameplay_frame,
                    &admitted_content_root(service.project_path())?,
                )
                .map_err(|error| format!("project Doom E1M1 application content: {error}"))?,
            )
        } else {
            None
        };
        Ok(Self {
            host_session_id: new_host_session_id(),
            service,
            voxel_object_frame,
            application_content,
            gameplay_projector,
            voxel_environment_role,
        })
    }

    pub fn host_session_id(&self) -> &str {
        &self.host_session_id
    }

    pub fn application_content(&self) -> Option<&ProjectedApplicationContent> {
        self.application_content.as_ref()
    }

    pub fn gameplay_projector(&self) -> Option<&GameplayApplicationProjector> {
        self.gameplay_projector.as_ref()
    }

    pub fn gameplay_projector_mut(&mut self) -> Option<&mut GameplayApplicationProjector> {
        self.gameplay_projector.as_mut()
    }

    fn project_gameplay_with_facts(
        &mut self,
        facts: &[crate::GameLoopFact],
    ) -> Result<RenderFrameDiff, String> {
        let (projector, service) = (&mut self.gameplay_projector, &self.service);
        projector
            .as_mut()
            .map(|projector| {
                projector
                    .project_with_facts(service.runtime().runtime(), facts)
                    .map_err(|error| format!("project desktop gameplay frame: {error}"))
            })
            .transpose()?
            .ok_or_else(|| "Loading Bay application projector is unavailable".to_owned())
    }

    pub fn voxel_environment_role(&self) -> &'static str {
        self.voxel_environment_role
    }

    pub fn voxel_object_frame(&self) -> &RenderFrameDiff {
        &self.voxel_object_frame
    }

    pub fn start_session(&mut self) -> u64 {
        self.service.start_session()
    }

    pub fn disconnect_session(&mut self, connection_generation: u64) {
        self.service.disconnect_session(connection_generation);
    }

    pub fn submit(
        &mut self,
        command: LoadingBayServiceCommand,
    ) -> Result<LoadingBayServiceReceipt, String> {
        self.service
            .submit(command)
            .map_err(|error| error.to_string())
    }

    pub fn discover_developer_commands(
        &self,
    ) -> Result<rusty_engine::developer_command::HostCommandDiscovery, String> {
        self.service
            .discover_developer_commands()
            .map_err(|error| error.to_string())
    }

    pub fn submit_developer_command(
        &mut self,
        request: crate::LoadingBayDeveloperCommandRequest,
    ) -> Result<(), String> {
        self.service
            .submit_developer_command(request)
            .map_err(|error| error.to_string())
    }

    pub fn poll_developer_command(
        &mut self,
        correlation: &str,
    ) -> Option<crate::LoadingBayDeveloperCommandResponse> {
        self.service.poll_developer_command(correlation)
    }

    pub fn cancel_developer_command(&mut self, correlation: &str) -> bool {
        self.service.cancel_developer_command(correlation)
    }

    pub fn advance(&mut self, elapsed: Duration) -> Result<GameLoopAdvanceReceipt, String> {
        self.service
            .advance(elapsed)
            .map_err(|error| error.to_string())
    }

    pub fn drain_outcomes(&mut self) -> Vec<crate::LoadingBayServiceOutcome> {
        self.service.drain_outcomes()
    }

    pub fn drain_game_loop_facts(&mut self) -> Vec<crate::GameLoopFact> {
        self.service.drain_game_loop_facts()
    }

    pub fn dropped_fact_count(&self) -> u64 {
        self.service
            .runtime()
            .dropped_fact_count()
            .saturating_add(self.service.dropped_outcome_count())
    }
}

fn new_host_session_id() -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(1);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "loading-bay-{:x}-{:x}-{sequence:x}",
        std::process::id(),
        time
    )
}

fn admitted_content_root(project_path: &Path) -> Result<PathBuf, String> {
    project_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            format!(
                "project path has no packaged content root: {}",
                project_path.display()
            )
        })
}

impl Deref for BrowserRuntime {
    type Target = LoadingBayProductService;
    fn deref(&self) -> &Self::Target {
        &self.service
    }
}

impl DerefMut for BrowserRuntime {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.service
    }
}

/// In-process product adapter used by the Tauri shell.
pub struct InProcessLoadingBayAdapter {
    runtime: BrowserRuntime,
    connection_generation: Option<u64>,
    outcomes: VecDeque<LoadingBayServiceOutcome>,
    projection_outcomes: VecDeque<LoadingBayServiceOutcome>,
    pending_projection_facts: VecDeque<crate::GameLoopFact>,
}

/// Typed IPC envelope: mutable gameplay readout and immutable application
/// resources are distinct transport values, matching the browser-shell contract.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InProcessProjection {
    pub dynamic: BrowserDynamicState,
    pub resources: BrowserStaticResources,
    pub outcomes: Vec<LoadingBayServiceOutcome>,
}

impl InProcessLoadingBayAdapter {
    pub fn admit(project: &Path, save_root: &Path) -> Result<Self, String> {
        Ok(Self {
            runtime: BrowserRuntime::load(project, save_root)?,
            connection_generation: None,
            outcomes: VecDeque::new(),
            projection_outcomes: VecDeque::new(),
            pending_projection_facts: VecDeque::new(),
        })
    }

    pub fn begin_session(&mut self) -> u64 {
        self.outcomes.clear();
        self.projection_outcomes.clear();
        self.pending_projection_facts.clear();
        let generation = self.runtime.start_session();
        self.connection_generation = Some(generation);
        generation
    }

    pub fn disconnect_session(&mut self, connection_generation: u64) {
        self.runtime.disconnect_session(connection_generation);
        self.outcomes.clear();
        self.projection_outcomes.clear();
        self.pending_projection_facts.clear();
        self.runtime.drain_game_loop_facts();
        if self.connection_generation == Some(connection_generation) {
            self.connection_generation = None;
        }
    }

    pub fn submit(
        &mut self,
        command: LoadingBayServiceCommand,
    ) -> Result<LoadingBayServiceReceipt, String> {
        let active = self
            .connection_generation
            .ok_or_else(|| "desktop session has not started".to_owned())?;
        let generation = match &command {
            LoadingBayServiceCommand::SetInputIntent {
                connection_generation,
                ..
            }
            | LoadingBayServiceCommand::Edge {
                connection_generation,
                ..
            }
            | LoadingBayServiceCommand::SaveGame {
                connection_generation,
                ..
            }
            | LoadingBayServiceCommand::LoadGame {
                connection_generation,
                ..
            }
            | LoadingBayServiceCommand::Restart {
                connection_generation,
                ..
            } => *connection_generation,
        };
        if generation != active {
            return Err("desktop command belongs to a retired session".to_owned());
        }
        self.runtime.submit(command)
    }

    pub fn discover_developer_commands(
        &self,
    ) -> Result<rusty_engine::developer_command::HostCommandDiscovery, String> {
        self.runtime.discover_developer_commands()
    }

    pub fn submit_developer_command(
        &mut self,
        request: crate::LoadingBayDeveloperCommandRequest,
    ) -> Result<(), String> {
        self.runtime.submit_developer_command(request)
    }

    pub fn poll_developer_command(
        &mut self,
        correlation: &str,
    ) -> Option<crate::LoadingBayDeveloperCommandResponse> {
        self.runtime.poll_developer_command(correlation)
    }

    pub fn cancel_developer_command(&mut self, correlation: &str) -> bool {
        self.runtime.cancel_developer_command(correlation)
    }

    pub fn advance(&mut self, elapsed: Duration) -> Result<u64, String> {
        let receipt = self.runtime.advance(elapsed)?;
        self.pending_projection_facts
            .extend(self.runtime.drain_game_loop_facts());
        let outcomes = self.runtime.drain_outcomes();
        self.outcomes.extend(outcomes.iter().cloned());
        self.projection_outcomes.extend(outcomes);
        while self.outcomes.len() > crate::MAX_PENDING_GAME_LOOP_FACTS {
            self.outcomes.pop_front();
        }
        while self.pending_projection_facts.len() > crate::MAX_PENDING_GAME_LOOP_FACTS {
            self.pending_projection_facts.pop_front();
        }
        while self.projection_outcomes.len() > crate::MAX_PENDING_GAME_LOOP_FACTS {
            self.projection_outcomes.pop_front();
        }
        Ok(receipt
            .fixed_ticks
            .last()
            .map_or(0, |tick| tick.driver_tick))
    }

    pub fn tick_if_session_active(&mut self, elapsed: Duration) -> Result<(), String> {
        if self.connection_generation.is_some() {
            self.advance(elapsed)?;
            self.connection_generation =
                Some(self.runtime.runtime().input_session().connection_generation);
        }
        Ok(())
    }

    pub fn active_connection_generation(&self) -> Option<u64> {
        self.connection_generation
    }

    pub fn projection(&mut self) -> Result<InProcessProjection, String> {
        let dynamic = self.dynamic_projection()?;
        Ok(InProcessProjection {
            dynamic,
            resources: browser_static_resources(&self.runtime),
            outcomes: self.outcomes.iter().cloned().collect(),
        })
    }

    pub fn dynamic_projection(&mut self) -> Result<BrowserDynamicState, String> {
        let mut shared = drain_projection_feedback(
            self.pending_projection_facts.drain(..).collect(),
            self.runtime.runtime().runtime().tick().raw(),
        );
        let mut events = shared
            .facts
            .into_iter()
            .map(|(kind, _)| kind)
            .collect::<Vec<_>>();
        let session_facts = self
            .projection_outcomes
            .drain(..)
            .map(|outcome| (outcome.kind.clone(), outcome.command_sequence))
            .collect::<Vec<_>>();
        shared.feedback.extend_session_facts(&session_facts, ACTOR);
        events.extend(session_facts.into_iter().map(|(kind, _)| kind));
        let gameplay_frame = self
            .runtime
            .project_gameplay_with_facts(&shared.presentation_facts)?;
        Ok(browser_dynamic_state_with_gameplay_frame(
            &self.runtime,
            events,
            shared.feedback,
            gameplay_frame,
        ))
    }

    pub fn project(&self) -> &LoadingBayProjectReadout {
        self.runtime.project()
    }

    pub fn save_slots(&self) -> Vec<crate::SaveSlotSummary> {
        self.runtime.save_slots().to_vec()
    }

    pub fn application_resource(&self, index: usize) -> Result<Vec<u8>, String> {
        self.runtime
            .application_content()
            .and_then(|content| content.resources.get(index))
            .map(|resource| resource.bytes.clone())
            .ok_or_else(|| "desktop application resource is unavailable".to_owned())
    }

    pub fn command_outcome(
        &self,
        connection_generation: u64,
        sequence: u64,
    ) -> Option<LoadingBayServiceOutcome> {
        self.outcomes
            .iter()
            .rev()
            .find(|outcome| {
                outcome.connection_generation == connection_generation
                    && outcome.command_sequence == Some(sequence)
            })
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relocated_packaged_content_root_supplies_nested_ipc_resources() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "loading-bay-packaged-content-{}-{unique}",
            std::process::id()
        ));
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let project = root.join("content/projects/doom-e1m1.project.json");
        std::fs::create_dir_all(project.parent().expect("project parent"))
            .expect("create project root");
        std::fs::copy(
            source_root.join("content/projects/doom-e1m1.project.json"),
            &project,
        )
        .expect("copy admitted project");
        let vitality = root.join("data/gameplay/loading-bay-e1m1-standard-vitality.package.json");
        std::fs::create_dir_all(vitality.parent().expect("vitality artifact parent"))
            .expect("create gameplay artifact root");
        std::fs::copy(
            source_root.join("data/gameplay/loading-bay-e1m1-standard-vitality.package.json"),
            vitality,
        )
        .expect("copy admitted standard vitality artifact");
        copy_directory(
            &source_root.join("content/doom-e1m1/textures"),
            &root.join("content/doom-e1m1/textures"),
        );
        copy_directory(
            &source_root.join("content/doom-e1m1/sprites"),
            &root.join("content/doom-e1m1/sprites"),
        );

        let mut adapter = InProcessLoadingBayAdapter::admit(&project, &root.join("saves"))
            .expect("admit relocated packaged content");
        let projection = adapter.projection().expect("nested IPC projection");
        let value = serde_json::to_value(projection).expect("serialize nested IPC projection");
        assert!(value["dynamic"].get("tick").is_some());
        assert!(value["resources"].get("applicationContent").is_some());
        assert_eq!(
            value["resources"]["gameplayPrograms"]["programs"]
                .as_array()
                .map(Vec::len),
            Some(4)
        );
        assert!(value["resources"]["gameplayPrograms"]["bindings"]
            .as_array()
            .is_some_and(|bindings| !bindings.is_empty()));
        assert_eq!(
            value["resources"]["pickupPrograms"]["programs"]
                .as_array()
                .map(Vec::len),
            Some(4)
        );
        assert_eq!(
            value["resources"]["pickupPrograms"]["bindings"]
                .as_array()
                .map(Vec::len),
            Some(78)
        );
        assert_eq!(
            value["resources"]["playerSetupPrograms"]["programs"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(
            value["resources"]["playerSetupPrograms"]["bindings"][0]["player"],
            1
        );
        assert_eq!(
            value["resources"]["playerSetupPrograms"]["bindings"][0]["programId"],
            "player/e1m1-pistol-start"
        );
        assert_eq!(
            value["resources"]["enemyPrograms"]["attack"]["programs"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(
            value["resources"]["enemyPrograms"]["defeat"]["programs"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(
            value["resources"]["enemyPrograms"]["attack"]["bindings"]
                .as_array()
                .map(Vec::len),
            Some(29)
        );
        assert_eq!(
            value["resources"]["enemyPrograms"]["defeat"]["bindings"]
                .as_array()
                .map(Vec::len),
            Some(29)
        );
        assert_eq!(
            value["resources"]["hazardPrograms"]["programs"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            value["resources"]["hazardPrograms"]["bindings"]
                .as_array()
                .map(Vec::len),
            Some(4)
        );
        assert_eq!(
            value["resources"]["explosivePropPrograms"]["programs"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            value["resources"]["explosivePropPrograms"]["bindings"]
                .as_array()
                .map(Vec::len),
            Some(6)
        );
        assert_eq!(
            value["resources"]["encounterPrograms"]["programs"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            value["resources"]["encounterPrograms"]["bindings"]
                .as_array()
                .map(Vec::len),
            Some(4)
        );
        assert_eq!(
            value["resources"]["encounterPrograms"]["bindings"][0]["programId"],
            "encounter/e1m1"
        );
        assert_eq!(
            value["resources"]["switchPrograms"]["programs"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            value["resources"]["switchPrograms"]["bindings"]
                .as_array()
                .map(Vec::len),
            Some(4)
        );
        assert_eq!(
            value["resources"]["switchPrograms"]["bindings"][0]["programId"],
            "switch/e1m1-door"
        );
        assert_eq!(
            value["resources"]["secretPrograms"]["programs"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            value["resources"]["secretPrograms"]["bindings"]
                .as_array()
                .map(Vec::len),
            Some(3)
        );
        assert_eq!(
            value["resources"]["secretPrograms"]["bindings"][0]["programId"],
            "secret/e1m1-discovery"
        );
        assert_eq!(
            value["resources"]["levelExitPrograms"]["programs"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            value["resources"]["levelExitPrograms"]["bindings"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            value["resources"]["levelExitPrograms"]["bindings"][0]["programId"],
            "level-exit/e1m1-completion"
        );
        assert!(value["dynamic"]["gameplayOutcome"].is_null());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn desktop_outcomes_are_pollable_but_project_events_and_checkpoint_cues_once() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-desktop-outcomes-{}-{unique}",
            std::process::id()
        ));
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/doom-e1m1.project.json");
        let mut adapter = InProcessLoadingBayAdapter::admit(&project, &save_root)
            .expect("admit E1M1 desktop adapter");
        let generation = adapter.begin_session();
        adapter
            .submit(LoadingBayServiceCommand::SaveGame {
                connection_generation: generation,
                sequence: 1,
                slot: crate::SaveSlotId::Checkpoint,
                overwrite: false,
                expected_storage_revision: None,
            })
            .expect("stage checkpoint save");
        adapter
            .advance(crate::FIXED_STEP_DURATION)
            .expect("consume checkpoint save");

        let first = serde_json::to_value(adapter.projection().expect("first projection"))
            .expect("serialize first projection");
        assert!(first["dynamic"]["lastEvents"]
            .as_array()
            .is_some_and(|events| events.iter().any(|event| event == "CheckpointSaved")));
        assert!(first["dynamic"]["presentation"]["cues"]
            .as_array()
            .is_some_and(|cues| cues.iter().any(|cue| cue["kind"] == "checkpoint")));
        assert_eq!(
            adapter
                .command_outcome(generation, 1)
                .expect("pollable checkpoint outcome")
                .kind,
            "CheckpointSaved"
        );

        let second = serde_json::to_value(adapter.projection().expect("second projection"))
            .expect("serialize second projection");
        assert!(!second["dynamic"]["lastEvents"]
            .as_array()
            .is_some_and(|events| events.iter().any(|event| event == "CheckpointSaved")));
        assert!(!second["dynamic"]["presentation"]["cues"]
            .as_array()
            .is_some_and(|cues| cues.iter().any(|cue| cue["kind"] == "checkpoint")));
        assert!(adapter.command_outcome(generation, 1).is_some());

        adapter.disconnect_session(generation);
        let replacement = adapter.begin_session();
        assert!(replacement > generation);
        let replacement_projection = serde_json::to_value(
            adapter
                .projection()
                .expect("replacement session projection"),
        )
        .expect("serialize replacement projection");
        assert!(replacement_projection["outcomes"]
            .as_array()
            .is_some_and(Vec::is_empty));
        assert!(!replacement_projection["dynamic"]["lastEvents"]
            .as_array()
            .is_some_and(|events| events.iter().any(|event| event == "CheckpointSaved")));
        let _ = std::fs::remove_dir_all(save_root);
    }

    #[test]
    fn desktop_idle_tick_projections_remain_serializable() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-desktop-idle-{}-{unique}",
            std::process::id()
        ));
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/doom-e1m1.project.json");
        let mut adapter = InProcessLoadingBayAdapter::admit(&project, &save_root)
            .expect("admit E1M1 desktop adapter");
        adapter.begin_session();
        for _ in 0..120 {
            adapter
                .tick_if_session_active(Duration::from_millis(16))
                .expect("advance idle desktop session");
            serde_json::to_value(adapter.projection().expect("idle projection"))
                .expect("serialize idle projection");
        }
        let _ = std::fs::remove_dir_all(save_root);
    }

    fn copy_directory(source: &Path, destination: &Path) {
        std::fs::create_dir_all(destination).expect("create packaged directory");
        for entry in std::fs::read_dir(source).expect("read source directory") {
            let entry = entry.expect("directory entry");
            let source_path = entry.path();
            let destination_path = destination.join(entry.file_name());
            if source_path.is_dir() {
                copy_directory(&source_path, &destination_path);
            } else {
                std::fs::copy(&source_path, &destination_path).expect("copy packaged resource");
            }
        }
    }
}
