//! The Loading Bay product's transport-neutral runtime service.
//!
//! Browser WebSocket and Tauri IPC are adapters over this service.  It owns
//! project admission, the fixed-step game loop, save-slot identity, and the
//! typed semantic commands that may reach gameplay.  It intentionally does
//! not know about HTTP, WebSocket, Tauri, or renderer implementation details.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::developer_command::{
    create_bindings, developer_runtime_identity, LoadingBayDeveloperCommandResponse,
    PendingDeveloperCommand, PendingDeveloperPlay,
};
use rusty_engine::core_ids::EntityId;
use rusty_engine::developer_command::CommandBindings;
use serde::{Deserialize, Serialize};

use crate::{
    admit_stored_project_with_document_and_vitality_policy, encode_project_document,
    AdmittedDoomVitalityPolicy, AdmittedStoredProject, DoomVitalityPolicy, EncounterProgramReadout,
    EnemyProgramReadout, ExplosivePropProgramReadout, FloorActionProgramReadout,
    GameLoopAdvanceReceipt, GameLoopEdgeCommand, GameLoopEdgeCommandKind, GameRuntime,
    GameplayProgramOutcome, GameplayProgramReadout, HazardProgramReadout, InputCommandReceipt,
    LevelExitProgramReadout, LiftProgramReadout, LoadingBayGameLoop, PickupProgramReadout,
    PlayerInputCommand, PlayerInputIntent, PlayerSetupProgramReadout, ProjectStore, SaveGameStore,
    SaveLoadRequest, SaveProjectIdentity, SaveSlotId, SaveSlotSummary, SaveWriteRequest,
    SecretProgramReadout, SwitchProgramReadout,
};

/// The player entity used by the authored Loading Bay product.
pub const LOADING_BAY_PLAYER: EntityId = EntityId::new(1);

/// Stable project identity readout for host adapters and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadingBayProjectReadout {
    pub project_id: String,
    pub source_schema_version: u32,
    pub current_schema_version: u32,
    pub entry_scene: String,
    pub asset_count: usize,
    pub scene_count: usize,
    pub entity_count: usize,
    pub content_hash: String,
}

/// Semantic client intent accepted by the Loading Bay product service.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LoadingBayServiceCommand {
    SetInputIntent {
        connection_generation: u64,
        sequence: u64,
        movement: [f32; 2],
        look_delta: [f32; 2],
        #[serde(default)]
        jump_held: bool,
        primary_fire_held: bool,
    },
    Edge {
        connection_generation: u64,
        sequence: u64,
        command: GameLoopEdgeCommandKind,
    },
    SaveGame {
        connection_generation: u64,
        sequence: u64,
        slot: SaveSlotId,
        overwrite: bool,
        expected_storage_revision: Option<String>,
    },
    LoadGame {
        connection_generation: u64,
        sequence: u64,
        slot: SaveSlotId,
        expected_storage_revision: Option<String>,
    },
    Restart {
        connection_generation: u64,
        sequence: u64,
        mode: crate::GameRestartMode,
    },
}

/// Typed acknowledgement returned by [`LoadingBayProductService::submit`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LoadingBayServiceReceipt {
    Input {
        connection_generation: u64,
        acknowledged_sequence: u64,
        consumed_sequence: u64,
        repeated: bool,
    },
    Edge {
        connection_generation: u64,
        acknowledged_sequence: u64,
        consumed_sequence: u64,
        repeated: bool,
    },
}

/// Error returned before a semantic command can enter the game loop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadingBayServiceError {
    pub code: &'static str,
    pub message: String,
}

/// A bounded product-level result emitted when a queued semantic command is
/// consumed by the fixed-step authority.  Adapters must wait for this value
/// rather than treating admission as completion: saves can fail, and loads or
/// restarts replace the input session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadingBayServiceOutcome {
    pub kind: String,
    pub connection_generation: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub session_replaced: bool,
}

impl std::fmt::Display for LoadingBayServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for LoadingBayServiceError {}

/// Transport-neutral Loading Bay runtime authority.
#[derive(Debug)]
pub struct LoadingBayProductService {
    pub(crate) runtime: LoadingBayGameLoop,
    authored: AdmittedStoredProject,
    project_path: PathBuf,
    pub(crate) project: LoadingBayProjectReadout,
    standard_vitality: Option<AdmittedDoomVitalityPolicy>,
    save_store: SaveGameStore,
    save_root: PathBuf,
    save_identity: SaveProjectIdentity,
    save_slots: Vec<SaveSlotSummary>,
    pending_saves: BTreeMap<u64, PendingSave>,
    pending_loads: BTreeMap<u64, PendingLoad>,
    pending_restarts: BTreeMap<u64, PendingRestart>,
    pending_commands: BTreeMap<u64, PendingCommand>,
    pub(crate) pending_outcomes: VecDeque<LoadingBayServiceOutcome>,
    dropped_outcome_count: u64,
    pub(crate) developer_generation: Option<u64>,
    pub(crate) developer_bindings: Option<CommandBindings>,
    pub(crate) pending_developer_commands: VecDeque<PendingDeveloperCommand>,
    pub(crate) pending_developer_plays: BTreeMap<String, PendingDeveloperPlay>,
    pub(crate) developer_results: VecDeque<LoadingBayDeveloperCommandResponse>,
}

#[derive(Debug)]
struct PendingSave {
    connection_generation: u64,
    slot: SaveSlotId,
    overwrite: bool,
    expected_storage_revision: Option<String>,
}

#[derive(Debug)]
struct PendingLoad {
    connection_generation: u64,
    slot: SaveSlotId,
    expected_storage_revision: Option<String>,
}

#[derive(Debug)]
struct PendingRestart {
    connection_generation: u64,
    mode: crate::GameRestartMode,
}

#[derive(Debug)]
struct PendingCommand {
    connection_generation: u64,
    operation: bool,
}

impl LoadingBayProductService {
    pub fn admit(project_path: &Path, save_root: &Path) -> Result<Self, LoadingBayServiceError> {
        let decoded = ProjectStore::default()
            .load(project_path)
            .map_err(|error| LoadingBayServiceError {
                code: "projectLoadFailed",
                message: format!("could not load {}: {error}", project_path.display()),
            })?;
        let canonical_path =
            project_path
                .canonicalize()
                .map_err(|error| LoadingBayServiceError {
                    code: "projectPathUnavailable",
                    message: format!(
                        "loaded project {} could not be resolved: {error}",
                        project_path.display()
                    ),
                })?;
        let canonical_project =
            encode_project_document(&decoded.project).map_err(|error| LoadingBayServiceError {
                code: "projectEncodingFailed",
                message: format!("could not encode admitted Loading Bay project: {error}"),
            })?;
        let project = LoadingBayProjectReadout {
            project_id: decoded.project.project_id.clone(),
            source_schema_version: decoded.source_schema_version,
            current_schema_version: decoded.project.schema_version,
            entry_scene: decoded.project.entry_scene.clone(),
            asset_count: decoded.project.assets.len(),
            scene_count: decoded.project.scenes.len(),
            entity_count: decoded
                .project
                .scenes
                .iter()
                .map(|scene| scene.entities.len())
                .sum(),
            content_hash: rusty_engine::voxel_convert::source_sha256(canonical_project.as_bytes()),
        };
        // The same canonical package TypeScript materialized is admitted before
        // runtime construction. Its payload is a named Doom extension; the
        // standard actor/destructible mechanics themselves remain public Engine
        // preset fragments composed by gameplay admission.
        let standard_vitality = if decoded.project.project_id == "doom-e1m1" {
            let root = canonical_path
                .parent()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .ok_or_else(|| LoadingBayServiceError {
                    code: "standardVitalityPathUnavailable",
                    message: format!(
                        "could not resolve standard vitality artifact from {}",
                        canonical_path.display()
                    ),
                })?;
            let artifact =
                root.join("data/gameplay/loading-bay-e1m1-standard-vitality.package.json");
            let bytes = std::fs::read(&artifact).map_err(|error| LoadingBayServiceError {
                code: "standardVitalityLoadFailed",
                message: format!("could not read {}: {error}", artifact.display()),
            })?;
            Some(crate::admit_doom_vitality_policy(&bytes).map_err(|error| {
                LoadingBayServiceError {
                    code: "standardVitalityAdmissionFailed",
                    message: error.to_string(),
                }
            })?)
        } else {
            None
        };
        let vitality_policy = standard_vitality
            .as_ref()
            .map(AdmittedDoomVitalityPolicy::policy)
            .unwrap_or_else(DoomVitalityPolicy::doom_compatibility);
        let (authored, admitted) = admit_stored_project_with_document_and_vitality_policy(
            decoded.project,
            vitality_policy,
        )
        .map_err(|error| LoadingBayServiceError {
            code: "projectAdmissionFailed",
            message: format!("project admission failed: {error}"),
        })?;
        let runtime = LoadingBayGameLoop::new(
            GameRuntime::from_admitted_project(admitted),
            LOADING_BAY_PLAYER,
        )
        .map_err(|error| LoadingBayServiceError {
            code: "runtimeInitializationFailed",
            message: format!("could not create Loading Bay game loop: {error}"),
        })?;
        let save_identity =
            SaveProjectIdentity::from_project(authored.document(), LOADING_BAY_PLAYER).map_err(
                |error| LoadingBayServiceError {
                    code: "saveIdentityFailed",
                    message: format!("could not identify authored project for saves: {error}"),
                },
            )?;
        let save_store = SaveGameStore::new(save_root);
        let save_slots = save_store.inspect_all(&save_identity);
        let developer_bindings = create_bindings(
            developer_runtime_identity(&project),
            runtime.input_session().connection_generation,
        )?;
        Ok(Self {
            runtime,
            authored,
            project_path: canonical_path,
            project,
            standard_vitality,
            save_store,
            save_root: save_root.to_path_buf(),
            save_identity,
            save_slots,
            pending_saves: BTreeMap::new(),
            pending_loads: BTreeMap::new(),
            pending_restarts: BTreeMap::new(),
            pending_commands: BTreeMap::new(),
            pending_outcomes: VecDeque::new(),
            dropped_outcome_count: 0,
            developer_generation: None,
            developer_bindings: Some(developer_bindings),
            pending_developer_commands: VecDeque::new(),
            pending_developer_plays: BTreeMap::new(),
            developer_results: VecDeque::new(),
        })
    }

    pub fn project(&self) -> &LoadingBayProjectReadout {
        &self.project
    }

    /// Read-only proof that this E1M1 instance admitted the generated standard
    /// authoring artifact. It is not live gameplay state or a mutation route.
    pub fn standard_vitality(&self) -> Option<&AdmittedDoomVitalityPolicy> {
        self.standard_vitality.as_ref()
    }

    /// Read-only access for product projections. Runtime mutation is confined
    /// to the typed commands and fixed-step methods on this service.
    pub fn runtime(&self) -> &LoadingBayGameLoop {
        &self.runtime
    }

    /// Read-only admitted program catalog for ordinary product/tooling readout.
    pub fn gameplay_programs(&self) -> GameplayProgramReadout {
        self.runtime.runtime().session().gameplay_programs()
    }

    /// Read-only admitted pickup program catalog and placement bindings.
    pub fn pickup_programs(&self) -> PickupProgramReadout {
        self.runtime.runtime().session().pickup_programs()
    }

    /// Read-only admitted player setup catalog and player-to-program binding.
    pub fn player_setup_programs(&self) -> PlayerSetupProgramReadout {
        self.runtime.runtime().session().player_setup_programs()
    }

    /// Read-only admitted enemy program catalogs and bindings for product tooling.
    pub fn enemy_programs(&self) -> EnemyProgramReadout {
        self.runtime.runtime().session().enemy_programs()
    }

    /// Read-only admitted hazard program catalog and placed-trigger bindings.
    pub fn hazard_programs(&self) -> HazardProgramReadout {
        self.runtime.runtime().session().hazard_programs()
    }

    /// Read-only admitted explosive-prop catalog and placed-prop bindings.
    pub fn explosive_prop_programs(&self) -> ExplosivePropProgramReadout {
        self.runtime.runtime().session().explosive_prop_programs()
    }

    /// Read-only admitted encounter lifecycle catalog and placement bindings.
    pub fn encounter_programs(&self) -> EncounterProgramReadout {
        self.runtime.runtime().session().encounter_programs()
    }

    /// Read-only admitted switch program catalog and placement bindings.
    pub fn switch_programs(&self) -> SwitchProgramReadout {
        self.runtime.runtime().session().switch_programs()
    }

    pub fn floor_action_programs(&self) -> FloorActionProgramReadout {
        self.runtime.runtime().session().floor_action_programs()
    }

    pub fn lift_programs(&self) -> LiftProgramReadout {
        self.runtime.runtime().session().lift_programs()
    }

    /// Read-only admitted secret-discovery programs and region bindings.
    pub fn secret_programs(&self) -> SecretProgramReadout {
        self.runtime.runtime().session().secret_programs()
    }

    /// Read-only admitted level-exit completion programs and exit bindings.
    pub fn level_exit_programs(&self) -> LevelExitProgramReadout {
        self.runtime.runtime().session().level_exit_programs()
    }

    /// Latest-value selected-program result, if the current session has one.
    pub fn gameplay_outcome(&self) -> Option<&GameplayProgramOutcome> {
        self.runtime.runtime().session().gameplay_outcome()
    }

    pub fn authored_project(&self) -> &AdmittedStoredProject {
        &self.authored
    }

    pub fn project_path(&self) -> &Path {
        &self.project_path
    }

    /// Drains fixed-tick command outcomes. This is intentionally bounded so a
    /// disconnected transport cannot retain unbounded product history.
    pub fn drain_outcomes(&mut self) -> Vec<LoadingBayServiceOutcome> {
        self.pending_outcomes.drain(..).collect()
    }

    /// Drains authoritative gameplay facts after projection. Transport
    /// adapters may format them, but may not manufacture or consume gameplay
    /// facts independently.
    pub fn drain_game_loop_facts(&mut self) -> Vec<crate::GameLoopFact> {
        self.runtime.drain_pending_facts()
    }

    pub fn dropped_outcome_count(&self) -> u64 {
        self.dropped_outcome_count
    }

    pub fn start_session(&mut self) -> u64 {
        self.clear_pending_operations();
        self.pending_outcomes.clear();
        self.runtime.drain_pending_facts();
        self.runtime.runtime_mut().clear_gameplay_outcome();
        self.retire_developer_commands(
            "developer request was retired by a gameplay session replacement",
        );
        let generation = self.runtime.start_connection().connection_generation;
        self.developer_generation = Some(generation);
        self.developer_bindings = Some(
            create_bindings(developer_runtime_identity(&self.project), generation)
                .expect("admitted product always rebuilds its fixed developer bindings"),
        );
        generation
    }

    pub fn disconnect_session(&mut self, connection_generation: u64) {
        if self.developer_generation == Some(connection_generation) {
            self.retire_developer_commands(
                "developer request was retired because the gameplay session disconnected",
            );
        }
        self.runtime.disconnect(connection_generation);
        self.clear_pending_operations();
        self.pending_outcomes.clear();
        self.runtime.drain_pending_facts();
        if self.developer_generation == Some(connection_generation) {
            self.developer_generation = None;
        }
    }

    pub fn submit(
        &mut self,
        command: LoadingBayServiceCommand,
    ) -> Result<LoadingBayServiceReceipt, LoadingBayServiceError> {
        let (connection_generation, sequence, operation) = command_identity(&command);
        let result = match command {
            LoadingBayServiceCommand::SetInputIntent {
                connection_generation,
                sequence,
                movement,
                look_delta,
                jump_held,
                primary_fire_held,
            } => self
                .runtime
                .submit_input(PlayerInputCommand {
                    connection_generation,
                    sequence,
                    intent: PlayerInputIntent {
                        movement,
                        look_delta,
                        jump_held,
                        primary_fire_held,
                    },
                })
                .map(input_receipt)
                .map_err(input_error),
            LoadingBayServiceCommand::Edge {
                connection_generation,
                sequence,
                command,
            } => self
                .runtime
                .submit_edge_command(GameLoopEdgeCommand {
                    connection_generation,
                    sequence,
                    command,
                })
                .map(edge_receipt)
                .map_err(input_error),
            LoadingBayServiceCommand::SaveGame {
                connection_generation,
                sequence,
                slot,
                overwrite,
                expected_storage_revision,
            } => self.submit_operation(
                connection_generation,
                sequence,
                GameLoopEdgeCommandKind::SaveGame { slot },
                PendingOperation::Save(PendingSave {
                    connection_generation,
                    slot,
                    overwrite,
                    expected_storage_revision,
                }),
            ),
            LoadingBayServiceCommand::LoadGame {
                connection_generation,
                sequence,
                slot,
                expected_storage_revision,
            } => self.submit_operation(
                connection_generation,
                sequence,
                GameLoopEdgeCommandKind::LoadGame { slot },
                PendingOperation::Load(PendingLoad {
                    connection_generation,
                    slot,
                    expected_storage_revision,
                }),
            ),
            LoadingBayServiceCommand::Restart {
                connection_generation,
                sequence,
                mode,
            } => self.submit_operation(
                connection_generation,
                sequence,
                match mode {
                    crate::GameRestartMode::AuthoredBaseline => {
                        GameLoopEdgeCommandKind::RestartAuthoredBaseline
                    }
                    crate::GameRestartMode::Checkpoint => {
                        GameLoopEdgeCommandKind::RestartCheckpoint
                    }
                },
                PendingOperation::Restart(PendingRestart {
                    connection_generation,
                    mode,
                }),
            ),
        };
        if result.is_ok() {
            self.pending_commands
                .entry(sequence)
                .or_insert(PendingCommand {
                    connection_generation,
                    operation,
                });
        }
        result
    }

    pub fn advance(
        &mut self,
        elapsed: std::time::Duration,
    ) -> Result<GameLoopAdvanceReceipt, LoadingBayServiceError> {
        let receipt =
            self.runtime
                .advance_elapsed(elapsed)
                .map_err(|error| LoadingBayServiceError {
                    code: "runtimeAdvanceFailed",
                    message: error.to_string(),
                })?;
        self.apply_consumed_operations(&receipt);
        self.record_consumed_commands(&receipt);
        self.resolve_developer_plays();
        self.consume_developer_commands();
        if self.developer_generation.is_some() {
            self.refresh_developer_facts();
        }
        Ok(receipt)
    }

    pub fn save_slots(&self) -> &[SaveSlotSummary] {
        &self.save_slots
    }

    pub fn save_now(
        &mut self,
        slot: SaveSlotId,
        overwrite: bool,
        expected_storage_revision: Option<String>,
        saved_at_unix_milliseconds: u64,
    ) -> Result<(), LoadingBayServiceError> {
        self.save_store
            .save(
                &self.save_identity,
                SaveWriteRequest {
                    slot,
                    overwrite,
                    expected_storage_revision,
                    saved_at_unix_milliseconds,
                },
                self.runtime.runtime(),
            )
            .map_err(save_error)?;
        self.save_slots = self.save_store.inspect_all(&self.save_identity);
        Ok(())
    }

    /// Rebuilds the product authority around a validated saved runtime while
    /// retaining its admitted content and save-store identity.
    pub fn replacement_from_runtime(
        &self,
        runtime: GameRuntime,
    ) -> Result<Self, LoadingBayServiceError> {
        let runtime = LoadingBayGameLoop::new(runtime, LOADING_BAY_PLAYER).map_err(|error| {
            LoadingBayServiceError {
                code: "runtimeRestoreFailed",
                message: format!("could not restore Loading Bay game loop: {error}"),
            }
        })?;
        let developer_bindings = create_bindings(
            developer_runtime_identity(&self.project),
            runtime.input_session().connection_generation,
        )?;
        Ok(Self {
            runtime,
            authored: self.authored.clone(),
            project_path: self.project_path.clone(),
            project: self.project.clone(),
            standard_vitality: self.standard_vitality.clone(),
            save_store: self.save_store.clone(),
            save_root: self.save_root.clone(),
            save_identity: self.save_identity.clone(),
            save_slots: self.save_store.inspect_all(&self.save_identity),
            pending_saves: BTreeMap::new(),
            pending_loads: BTreeMap::new(),
            pending_restarts: BTreeMap::new(),
            pending_commands: BTreeMap::new(),
            pending_outcomes: VecDeque::new(),
            dropped_outcome_count: 0,
            developer_generation: None,
            developer_bindings: Some(developer_bindings),
            pending_developer_commands: VecDeque::new(),
            pending_developer_plays: BTreeMap::new(),
            developer_results: VecDeque::new(),
        })
    }

    fn submit_operation(
        &mut self,
        connection_generation: u64,
        sequence: u64,
        command: GameLoopEdgeCommandKind,
        pending: PendingOperation,
    ) -> Result<LoadingBayServiceReceipt, LoadingBayServiceError> {
        let receipt = self
            .runtime
            .submit_edge_command(GameLoopEdgeCommand {
                connection_generation,
                sequence,
                command,
            })
            .map(edge_receipt)
            .map_err(input_error)?;
        match pending {
            PendingOperation::Save(pending) => {
                self.pending_saves.insert(sequence, pending);
            }
            PendingOperation::Load(pending) => {
                self.pending_loads.insert(sequence, pending);
            }
            PendingOperation::Restart(pending) => {
                self.pending_restarts.insert(sequence, pending);
            }
        }
        Ok(receipt)
    }

    fn apply_consumed_operations(&mut self, receipt: &GameLoopAdvanceReceipt) {
        let facts = receipt
            .fixed_ticks
            .iter()
            .flat_map(|tick| tick.facts.iter())
            .cloned()
            .collect::<Vec<_>>();
        for fact in &facts {
            if let crate::GameLoopFact::EdgeCommandRejected { sequence, reason } = fact {
                if let Some(command) = self.pending_commands.remove(sequence) {
                    self.pending_saves.remove(sequence);
                    self.pending_loads.remove(sequence);
                    self.pending_restarts.remove(sequence);
                    self.push_outcome(LoadingBayServiceOutcome {
                        kind: edge_rejection_code(reason).to_owned(),
                        connection_generation: command.connection_generation,
                        command_sequence: Some(*sequence),
                        message: Some(format!("command rejected: {reason:?}")),
                        session_replaced: false,
                    });
                }
            }
        }
        for fact in facts {
            match fact {
                crate::GameLoopFact::SaveRequested { sequence, slot } => {
                    let Some(pending) = self.pending_saves.remove(&sequence) else {
                        continue;
                    };
                    if pending.connection_generation
                        != self.runtime.input_session().connection_generation
                        || pending.slot != slot
                    {
                        continue;
                    }
                    let result = self.save_now(
                        slot,
                        pending.overwrite,
                        pending.expected_storage_revision,
                        unix_time_milliseconds(),
                    );
                    self.record_operation_result(
                        pending.connection_generation,
                        sequence,
                        if slot == SaveSlotId::Checkpoint {
                            "CheckpointSaved"
                        } else {
                            "GameSaved"
                        },
                        result,
                        false,
                    );
                }
                crate::GameLoopFact::LoadRequested { sequence, slot } => {
                    let Some(pending) = self.pending_loads.remove(&sequence) else {
                        continue;
                    };
                    if pending.connection_generation
                        != self.runtime.input_session().connection_generation
                        || pending.slot != slot
                    {
                        continue;
                    }
                    let loaded = self
                        .save_store
                        .load(
                            &self.save_identity,
                            SaveLoadRequest {
                                slot,
                                expected_storage_revision: pending.expected_storage_revision,
                            },
                        )
                        .map_err(save_error)
                        .and_then(|mut loaded| {
                            loaded
                                .runtime
                                .reattach_authored_gameplay_programs(self.authored.document())
                                .map_err(|error| LoadingBayServiceError {
                                    code: "runtimeProgramRestoreFailed",
                                    message: format!(
                                        "could not reattach authored gameplay programs: {error}"
                                    ),
                                })?;
                            self.replace_runtime(loaded.runtime)
                        });
                    self.record_operation_result(
                        pending.connection_generation,
                        sequence,
                        if slot == SaveSlotId::Checkpoint {
                            "CheckpointRestored"
                        } else {
                            "GameLoaded"
                        },
                        loaded,
                        true,
                    );
                }
                crate::GameLoopFact::RestartRequested { sequence, mode } => {
                    let Some(pending) = self.pending_restarts.remove(&sequence) else {
                        continue;
                    };
                    if pending.mode != mode {
                        continue;
                    }
                    let result = match mode {
                        crate::GameRestartMode::AuthoredBaseline => self.replace_authored_runtime(),
                        crate::GameRestartMode::Checkpoint => Err(LoadingBayServiceError {
                            code: "checkpointUnavailable",
                            message: "the current Loading Bay product has no checkpoint snapshot"
                                .to_owned(),
                        }),
                    };
                    self.record_operation_result(
                        pending.connection_generation,
                        sequence,
                        "Restarted",
                        result,
                        true,
                    );
                }
                _ => {}
            }
        }
    }

    fn record_operation_result(
        &mut self,
        connection_generation: u64,
        sequence: u64,
        success_kind: &'static str,
        result: Result<(), LoadingBayServiceError>,
        session_replaced: bool,
    ) {
        self.pending_commands.remove(&sequence);
        match result {
            Ok(()) => self.push_outcome(LoadingBayServiceOutcome {
                kind: success_kind.to_owned(),
                connection_generation,
                command_sequence: Some(sequence),
                message: None,
                session_replaced,
            }),
            Err(error) => self.push_outcome(LoadingBayServiceOutcome {
                kind: error.code.to_owned(),
                connection_generation,
                command_sequence: Some(sequence),
                message: Some(error.message),
                session_replaced: false,
            }),
        }
    }

    fn push_outcome(&mut self, outcome: LoadingBayServiceOutcome) {
        if self.pending_outcomes.len() == crate::MAX_PENDING_GAME_LOOP_FACTS {
            self.pending_outcomes.pop_front();
            self.dropped_outcome_count = self.dropped_outcome_count.saturating_add(1);
        }
        self.pending_outcomes.push_back(outcome);
    }

    fn record_consumed_commands(&mut self, receipt: &GameLoopAdvanceReceipt) {
        for tick in &receipt.fixed_ticks {
            let consumed = tick.consumed_sequence;
            let settled = self
                .pending_commands
                .range(..=consumed)
                .filter(|(_, command)| !command.operation)
                .map(|(sequence, command)| (*sequence, command.connection_generation))
                .collect::<Vec<_>>();
            for (sequence, connection_generation) in settled {
                self.pending_commands.remove(&sequence);
                self.push_outcome(LoadingBayServiceOutcome {
                    kind: "CommandConsumed".to_owned(),
                    connection_generation,
                    command_sequence: Some(sequence),
                    message: None,
                    session_replaced: false,
                });
            }
        }
    }

    fn replace_runtime(&mut self, runtime: GameRuntime) -> Result<(), LoadingBayServiceError> {
        let previous_generation = self.runtime.input_session().connection_generation;
        self.runtime = LoadingBayGameLoop::new(runtime, LOADING_BAY_PLAYER).map_err(|error| {
            LoadingBayServiceError {
                code: "runtimeRestoreFailed",
                message: format!("could not restore Loading Bay game loop: {error}"),
            }
        })?;
        self.runtime.start_connection_after(previous_generation);
        self.save_slots = self.save_store.inspect_all(&self.save_identity);
        self.clear_pending_operations();
        Ok(())
    }

    fn replace_authored_runtime(&mut self) -> Result<(), LoadingBayServiceError> {
        let previous_generation = self.runtime.input_session().connection_generation;
        let replacement = Self::admit(&self.project_path, &self.save_root)?;
        self.runtime = replacement.runtime;
        self.runtime.start_connection_after(previous_generation);
        self.save_slots = replacement.save_slots;
        self.clear_pending_operations();
        Ok(())
    }

    fn clear_pending_operations(&mut self) {
        self.pending_saves.clear();
        self.pending_loads.clear();
        self.pending_restarts.clear();
        self.pending_commands.clear();
    }
}

enum PendingOperation {
    Save(PendingSave),
    Load(PendingLoad),
    Restart(PendingRestart),
}

pub(crate) fn command_identity(command: &LoadingBayServiceCommand) -> (u64, u64, bool) {
    match command {
        LoadingBayServiceCommand::SetInputIntent {
            connection_generation,
            sequence,
            ..
        }
        | LoadingBayServiceCommand::Edge {
            connection_generation,
            sequence,
            ..
        } => (*connection_generation, *sequence, false),
        LoadingBayServiceCommand::SaveGame {
            connection_generation,
            sequence,
            ..
        }
        | LoadingBayServiceCommand::LoadGame {
            connection_generation,
            sequence,
            ..
        }
        | LoadingBayServiceCommand::Restart {
            connection_generation,
            sequence,
            ..
        } => (*connection_generation, *sequence, true),
    }
}

fn unix_time_milliseconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn save_error(error: crate::SaveGameError) -> LoadingBayServiceError {
    LoadingBayServiceError {
        code: match &error {
            crate::SaveGameError::OverwriteRequired { .. } => "saveOverwriteRequired",
            crate::SaveGameError::Stale { .. } => "saveStale",
            crate::SaveGameError::Corrupt { .. } | crate::SaveGameError::TooLarge { .. } => {
                "snapshotCorrupt"
            }
            crate::SaveGameError::Incompatible { .. } => "snapshotIncompatible",
            crate::SaveGameError::Empty { .. }
            | crate::SaveGameError::Io { .. }
            | crate::SaveGameError::Encode(_) => "saveUnavailable",
        },
        message: error.to_string(),
    }
}

fn input_receipt(receipt: InputCommandReceipt) -> LoadingBayServiceReceipt {
    let repeated = matches!(
        receipt.disposition,
        crate::InputCommandDisposition::Repeated
    );
    LoadingBayServiceReceipt::Input {
        connection_generation: receipt.connection_generation,
        acknowledged_sequence: receipt.acknowledged_sequence,
        consumed_sequence: receipt.consumed_sequence,
        repeated,
    }
}

fn edge_receipt(receipt: InputCommandReceipt) -> LoadingBayServiceReceipt {
    let repeated = matches!(
        receipt.disposition,
        crate::InputCommandDisposition::Repeated
    );
    LoadingBayServiceReceipt::Edge {
        connection_generation: receipt.connection_generation,
        acknowledged_sequence: receipt.acknowledged_sequence,
        consumed_sequence: receipt.consumed_sequence,
        repeated,
    }
}

fn input_error(error: impl std::fmt::Display) -> LoadingBayServiceError {
    LoadingBayServiceError {
        code: "commandRejected",
        message: error.to_string(),
    }
}

fn edge_rejection_code(rejection: &crate::EdgeCommandRejection) -> &'static str {
    match rejection {
        crate::EdgeCommandRejection::Paused => "paused",
        crate::EdgeCommandRejection::UnknownTarget => "unknownTarget",
        crate::EdgeCommandRejection::NotInteractable
        | crate::EdgeCommandRejection::SwitchOutOfRange
        | crate::EdgeCommandRejection::SwitchUnavailable => "notInteractable",
        crate::EdgeCommandRejection::PickupRejected
        | crate::EdgeCommandRejection::InventoryRejected => "internalDefect",
        crate::EdgeCommandRejection::InvalidWeaponSlot => "invalidWeaponSlot",
        crate::EdgeCommandRejection::WeaponNotOwned => "weaponNotOwned",
        crate::EdgeCommandRejection::WeaponAlreadySelected => "weaponAlreadySelected",
        crate::EdgeCommandRejection::PlayerDefeated => "playerDefeated",
        crate::EdgeCommandRejection::ItemNotOwned => "itemNotOwned",
        crate::EdgeCommandRejection::ItemNotUsable => "itemNotUsable",
        crate::EdgeCommandRejection::HealthFull => "healthFull",
        crate::EdgeCommandRejection::CheckpointUnavailable => "checkpointUnavailable",
        crate::EdgeCommandRejection::DoorLocked
        | crate::EdgeCommandRejection::LevelExitUnavailable
        | crate::EdgeCommandRejection::LevelComplete => "notInteractable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::developer_command::{
        LoadingBayDeveloperCommandRequest, MAX_PENDING_DEVELOPER_COMMANDS,
    };
    use serde_json::Value;

    fn developer_request(
        service: &LoadingBayProductService,
        command: &str,
        correlation: &str,
        payload: Value,
    ) -> LoadingBayDeveloperCommandRequest {
        let discovery = service.discover_developer_commands().unwrap();
        LoadingBayDeveloperCommandRequest {
            protocol_version: discovery.protocol_version,
            command: rusty_engine::developer_command::CommandId::parse(command).unwrap(),
            correlation: rusty_engine::developer_command::CorrelationId::parse(correlation)
                .unwrap(),
            runtime: discovery.runtime,
            expected: rusty_engine::developer_command::HostExpectedFacts {
                profile: discovery.profile,
                revision: discovery.revision,
                catalog_epoch: discovery.catalog_epoch,
            },
            payload,
        }
    }

    #[test]
    fn e1m1_admits_the_generated_standard_vitality_extension_before_runtime_construction() {
        let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/doom-e1m1.project.json");
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-standard-vitality-{}",
            std::process::id()
        ));
        let service = LoadingBayProductService::admit(&project, &save_root)
            .expect("admit canonical E1M1 with standard vitality artifact");
        let policy = service.standard_vitality().expect("E1M1 standard vitality");
        assert_eq!(policy.policy().maximum_health, 1_000_000);
        assert_eq!(policy.policy().maximum_armor, 1_000_000);
        let _ = std::fs::remove_dir_all(save_root);
    }

    #[test]
    fn admitted_standard_vitality_policy_bounds_normal_gameplay_admission() {
        let project_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/doom-e1m1.project.json");
        let mut project = ProjectStore::default().load(&project_path).unwrap().project;
        let policy_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../data/gameplay/loading-bay-e1m1-standard-vitality.package.json");
        let policy = crate::admit_doom_vitality_policy(&std::fs::read(policy_path).unwrap())
            .expect("admit generated standard vitality extension");
        let health = project
            .scenes
            .iter_mut()
            .flat_map(|scene| &mut scene.entities)
            .find_map(|entity| entity.health.as_mut())
            .expect("canonical E1M1 health entity");
        health.max = policy.policy().maximum_health + 1;

        let error =
            admit_stored_project_with_document_and_vitality_policy(project, policy.policy())
                .expect_err("policy must reject oversized authored health before a runtime exists");
        assert!(error.diagnostic().path.ends_with(".health"));
    }

    #[test]
    fn normal_gameplay_admission_rejects_destructible_health_above_the_standard_integrity_capacity()
    {
        let project_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/doom-e1m1.project.json");
        let mut project = ProjectStore::default().load(&project_path).unwrap().project;
        let policy_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../data/gameplay/loading-bay-e1m1-standard-vitality.package.json");
        let policy = crate::admit_doom_vitality_policy(&std::fs::read(policy_path).unwrap())
            .expect("admit generated standard vitality extension");
        let health = project
            .scenes
            .iter_mut()
            .flat_map(|scene| &mut scene.entities)
            .find_map(|entity| entity.explosive_prop.as_ref().and(entity.health.as_mut()))
            .expect("canonical E1M1 explosive prop health");
        health.max = 51;

        let error =
            admit_stored_project_with_document_and_vitality_policy(project, policy.policy())
                .expect_err(
                    "fixed standard destructible integrity must reject an incompatible prop",
                );
        assert!(error.diagnostic().path.ends_with(".health"));
    }

    #[test]
    fn semantic_command_serializes_without_transport_details() {
        let command = LoadingBayServiceCommand::SetInputIntent {
            connection_generation: 7,
            sequence: 9,
            movement: [1.0, 0.0],
            look_delta: [0.0, 0.0],
            jump_held: false,
            primary_fire_held: true,
        };
        let value = serde_json::to_value(command).expect("serialize semantic command");
        assert_eq!(value["kind"], "setInputIntent");
        assert!(value.get("websocket").is_none());
        assert!(value.get("tauri").is_none());

        let success = serde_json::to_value(LoadingBayServiceOutcome {
            kind: "CommandConsumed".to_owned(),
            connection_generation: 7,
            command_sequence: Some(9),
            message: None,
            session_replaced: false,
        })
        .expect("serialize successful outcome");
        assert!(success.get("message").is_none());
    }

    #[test]
    fn developer_play_waits_for_the_existing_service_command_to_complete() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-developer-command-{}-{unique}",
            std::process::id()
        ));
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/doom-e1m1.project.json");
        let mut service = LoadingBayProductService::admit(&project, &save_root).unwrap();
        let generation = service.start_session();
        let discovery = service.discover_developer_commands().unwrap();
        let request = LoadingBayDeveloperCommandRequest {
            protocol_version: discovery.protocol_version,
            command: rusty_engine::developer_command::CommandId::parse(
                "loading-bay.play.service-command",
            )
            .unwrap(),
            correlation: rusty_engine::developer_command::CorrelationId::parse("play-fixed-step")
                .unwrap(),
            runtime: discovery.runtime,
            expected: rusty_engine::developer_command::HostExpectedFacts {
                profile: discovery.profile,
                revision: discovery.revision,
                catalog_epoch: discovery.catalog_epoch,
            },
            payload: serde_json::to_value(LoadingBayServiceCommand::SetInputIntent {
                connection_generation: generation,
                sequence: 91,
                movement: [1.0, 0.0],
                look_delta: [0.0, 0.0],
                jump_held: false,
                primary_fire_held: false,
            })
            .unwrap(),
        };
        service.submit_developer_command(request).unwrap();
        assert!(service.poll_developer_command("play-fixed-step").is_none());

        // First tick reaches the developer safe point and only stages the
        // existing semantic input command; it is not a play completion yet.
        service.advance(crate::FIXED_STEP_DURATION).unwrap();
        assert!(service.poll_developer_command("play-fixed-step").is_none());

        // The following ordinary fixed step consumes the staged command and
        // produces the product result bridged back to the developer request.
        service.advance(crate::FIXED_STEP_DURATION).unwrap();
        let response = service
            .poll_developer_command("play-fixed-step")
            .expect("ordinary service completion returns developer result");
        assert!(matches!(
            response.outcome,
            rusty_engine::developer_command::HostCommandOutcome::Success { .. }
        ));
        let _ = std::fs::remove_dir_all(save_root);
    }

    #[test]
    fn developer_admin_mutates_only_at_the_safe_point_and_returns_owner_projection() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-developer-admin-{}-{unique}",
            std::process::id()
        ));
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/doom-e1m1.project.json");
        let mut service = LoadingBayProductService::admit(&project, &save_root).unwrap();
        service.start_session();
        let before = service
            .runtime()
            .runtime()
            .session()
            .health(LOADING_BAY_PLAYER)
            .unwrap()
            .current;
        let after = before.saturating_sub(1);
        let request = developer_request(
            &service,
            "standard.admin.track.set",
            "admin-safe-point",
            serde_json::json!({
                "operation": "loading-bay.developer.track-set",
                "source": {
                    "kind": "request",
                    "operation": "loading-bay.developer.track-set",
                    "instance": "loading-bay.developer"
                },
                "entity": LOADING_BAY_PLAYER.raw().to_string(),
                "track": rusty_engine::gameplay_standard::ActionActorPreset::VITALITY_TRACK,
                "value": after,
                "policy": "rejectOutOfBounds",
                "expectedRevision": null
            }),
        );
        service.submit_developer_command(request).unwrap();
        assert_eq!(
            service
                .runtime()
                .runtime()
                .session()
                .health(LOADING_BAY_PLAYER)
                .unwrap()
                .current,
            before,
            "queue admission must not mutate gameplay"
        );
        assert!(service.poll_developer_command("admin-safe-point").is_none());

        service.advance(crate::FIXED_STEP_DURATION).unwrap();
        assert_eq!(
            service
                .runtime()
                .runtime()
                .session()
                .health(LOADING_BAY_PLAYER)
                .unwrap()
                .current,
            after
        );
        let response = service
            .poll_developer_command("admin-safe-point")
            .expect("safe point publishes admin result");
        match response.outcome {
            rusty_engine::developer_command::HostCommandOutcome::Success { value, .. } => {
                assert_eq!(value["after"], after);
                assert!(value["catalogVersion"].is_string());
                assert!(value["observedRevisions"].is_array());
                assert!(value["sourceCost"].is_object());
                assert!(value["committedTracksRevision"].is_string());
            }
            outcome => panic!("unexpected developer outcome: {outcome:?}"),
        }
        let _ = std::fs::remove_dir_all(save_root);
    }

    #[test]
    fn developer_admin_rejects_invalid_stale_and_absent_entities_without_mutation() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-developer-admin-rejections-{}-{unique}",
            std::process::id()
        ));
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/doom-e1m1.project.json");
        let mut service = LoadingBayProductService::admit(&project, &save_root).unwrap();
        service.start_session();
        let before = service
            .runtime()
            .runtime()
            .session()
            .health(LOADING_BAY_PLAYER)
            .unwrap()
            .current;
        let player_entity = LOADING_BAY_PLAYER.raw().to_string();

        for (correlation, entity, expected_revision) in [
            ("invalid-decimal", "1.0", None),
            ("absent-entity", "18446744073709551615", None),
            (
                "stale-component",
                player_entity.as_str(),
                Some("18446744073709551615"),
            ),
        ] {
            let request = developer_request(
                &service,
                "standard.admin.track.set",
                correlation,
                serde_json::json!({
                    "operation": "loading-bay.developer.track-set",
                    "source": {
                        "kind": "request",
                        "operation": "loading-bay.developer.track-set",
                        "instance": "loading-bay.developer"
                    },
                    "entity": entity,
                    "track": rusty_engine::gameplay_standard::ActionActorPreset::VITALITY_TRACK,
                    "value": before.saturating_sub(1),
                    "policy": "rejectOutOfBounds",
                    "expectedRevision": expected_revision
                }),
            );
            service.submit_developer_command(request).unwrap();
            service.advance(crate::FIXED_STEP_DURATION).unwrap();
            let response = service
                .poll_developer_command(correlation)
                .expect("invalid host request must produce a terminal result");
            assert!(matches!(
                response.outcome,
                rusty_engine::developer_command::HostCommandOutcome::Error { .. }
            ));
            assert_eq!(
                service
                    .runtime()
                    .runtime()
                    .session()
                    .health(LOADING_BAY_PLAYER)
                    .unwrap()
                    .current,
                before,
                "{correlation} must not mutate gameplay",
            );
        }
        let _ = std::fs::remove_dir_all(save_root);
    }

    #[test]
    fn developer_queue_honors_cancel_saturation_and_engine_stale_context() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-developer-queue-{}-{unique}",
            std::process::id()
        ));
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/doom-e1m1.project.json");
        let mut service = LoadingBayProductService::admit(&project, &save_root).unwrap();
        service.start_session();

        let cancelled = developer_request(
            &service,
            "standard.inspect.entity",
            "cancel-before-safe-point",
            serde_json::json!({ "entity": "1" }),
        );
        service.submit_developer_command(cancelled).unwrap();
        assert!(service.cancel_developer_command("cancel-before-safe-point"));
        service.advance(crate::FIXED_STEP_DURATION).unwrap();
        assert!(service
            .poll_developer_command("cancel-before-safe-point")
            .is_none());

        for index in 0..MAX_PENDING_DEVELOPER_COMMANDS {
            let request = developer_request(
                &service,
                "standard.inspect.entity",
                &format!("saturated-{index}"),
                serde_json::json!({ "entity": "1" }),
            );
            service.submit_developer_command(request).unwrap();
        }
        let overflow = developer_request(
            &service,
            "standard.inspect.entity",
            "saturated-overflow",
            serde_json::json!({ "entity": "1" }),
        );
        assert_eq!(
            service.submit_developer_command(overflow).unwrap_err().code,
            "queueSaturated"
        );
        service.advance(crate::FIXED_STEP_DURATION).unwrap();

        let mut stale = developer_request(
            &service,
            "standard.inspect.entity",
            "stale-context",
            serde_json::json!({ "entity": "1" }),
        );
        stale.expected.revision = rusty_engine::developer_command::HostDecimalU64::new(u64::MAX);
        service.submit_developer_command(stale).unwrap();
        service.advance(crate::FIXED_STEP_DURATION).unwrap();
        let response = service
            .poll_developer_command("stale-context")
            .expect("Engine returns stale pre-dispatch rejection");
        assert!(matches!(
            response.outcome,
            rusty_engine::developer_command::HostCommandOutcome::Error { .. }
        ));
        let _ = std::fs::remove_dir_all(save_root);
    }

    #[test]
    fn developer_discovery_rotates_with_the_gameplay_generation() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-developer-generation-{}-{unique}",
            std::process::id()
        ));
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/doom-e1m1.project.json");
        let mut service = LoadingBayProductService::admit(&project, &save_root).unwrap();
        service.start_session();
        let first = service.discover_developer_commands().unwrap();
        let pending = developer_request(
            &service,
            "standard.inspect.entity",
            "retired-with-generation",
            serde_json::json!({ "entity": "1" }),
        );
        service.submit_developer_command(pending).unwrap();
        service.start_session();
        let second = service.discover_developer_commands().unwrap();
        assert_eq!(first.runtime, second.runtime);
        assert!(second.revision.get() > first.revision.get());
        let retired = service
            .poll_developer_command("retired-with-generation")
            .expect("replacement returns an immediate terminal response");
        match retired.outcome {
            rusty_engine::developer_command::HostCommandOutcome::Error { code, .. } => {
                assert_eq!(code.as_str(), "retired-generation");
            }
            outcome => panic!("unexpected replacement outcome: {outcome:?}"),
        }
        let _ = std::fs::remove_dir_all(save_root);
    }

    #[test]
    fn developer_disconnect_retires_an_in_flight_play_without_transport_timeout() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-developer-disconnect-{}-{unique}",
            std::process::id()
        ));
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/doom-e1m1.project.json");
        let mut service = LoadingBayProductService::admit(&project, &save_root).unwrap();
        let generation = service.start_session();
        let request = developer_request(
            &service,
            "loading-bay.play.service-command",
            "retired-on-disconnect",
            serde_json::to_value(LoadingBayServiceCommand::SetInputIntent {
                connection_generation: generation,
                sequence: 92,
                movement: [1.0, 0.0],
                look_delta: [0.0, 0.0],
                jump_held: false,
                primary_fire_held: false,
            })
            .unwrap(),
        );
        service.submit_developer_command(request).unwrap();
        service.advance(crate::FIXED_STEP_DURATION).unwrap();
        assert!(service
            .poll_developer_command("retired-on-disconnect")
            .is_none());

        service.disconnect_session(generation);
        let retired = service
            .poll_developer_command("retired-on-disconnect")
            .expect("disconnect returns an immediate terminal response");
        match retired.outcome {
            rusty_engine::developer_command::HostCommandOutcome::Error { code, .. } => {
                assert_eq!(code.as_str(), "retired-generation");
            }
            outcome => panic!("unexpected disconnect outcome: {outcome:?}"),
        }
        assert_eq!(
            service.discover_developer_commands().unwrap_err().code,
            "gameplayUnavailable"
        );
        let _ = std::fs::remove_dir_all(save_root);
    }

    #[test]
    fn e1m1_save_load_and_authored_restart_roundtrip_after_fixed_tick_consumption() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-product-service-{}-{unique}",
            std::process::id()
        ));
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/doom-e1m1.project.json");
        let mut service = LoadingBayProductService::admit(&project, &save_root)
            .expect("admit E1M1 product service");
        let generation = service.start_session();

        service
            .submit(LoadingBayServiceCommand::SaveGame {
                connection_generation: generation,
                sequence: 1,
                slot: SaveSlotId::Slot1,
                overwrite: false,
                expected_storage_revision: None,
            })
            .expect("stage save");
        assert_eq!(
            service.save_slots()[1].compatibility,
            crate::SaveSlotCompatibility::Empty
        );
        service
            .advance(crate::FIXED_STEP_DURATION)
            .expect("consume save");
        assert!(service
            .drain_outcomes()
            .iter()
            .any(|outcome| outcome.kind == "GameSaved" && outcome.command_sequence == Some(1)));
        let saved_tick = service.runtime.runtime().tick();
        let saved_revision = service.save_slots()[1]
            .storage_revision
            .clone()
            .expect("saved revision");

        service
            .advance(crate::FIXED_STEP_DURATION)
            .expect("advance after save");
        assert!(service.runtime.runtime().tick() > saved_tick);
        service
            .submit(LoadingBayServiceCommand::LoadGame {
                connection_generation: generation,
                sequence: 2,
                slot: SaveSlotId::Slot1,
                expected_storage_revision: Some(saved_revision),
            })
            .expect("stage load");
        service
            .advance(crate::FIXED_STEP_DURATION)
            .expect("consume load");
        assert!(service
            .drain_outcomes()
            .iter()
            .any(|outcome| outcome.kind == "GameLoaded" && outcome.session_replaced));
        assert_eq!(service.runtime.runtime().tick(), saved_tick);
        service
            .runtime
            .runtime_mut()
            .attack(LOADING_BAY_PLAYER, crate::ResolvedAttackAction::Attack)
            .expect("loaded runtime reattaches the authored weapon program catalog");

        let restarted_generation = service.runtime.input_session().connection_generation;
        assert!(restarted_generation > generation);
        service
            .submit(LoadingBayServiceCommand::Restart {
                connection_generation: restarted_generation,
                sequence: 1,
                mode: crate::GameRestartMode::AuthoredBaseline,
            })
            .expect("stage authored restart");
        service
            .advance(crate::FIXED_STEP_DURATION)
            .expect("consume restart");
        assert!(service
            .drain_outcomes()
            .iter()
            .any(|outcome| outcome.kind == "Restarted" && outcome.session_replaced));
        assert_eq!(service.runtime.runtime().tick().raw(), 0);
        let _ = std::fs::remove_dir_all(save_root);
    }

    #[test]
    fn authored_restart_rotates_the_session_and_accepts_the_next_typed_command() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-product-restart-{}-{unique}",
            std::process::id()
        ));
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/doom-e1m1.project.json");
        let mut service = LoadingBayProductService::admit(&project, &save_root)
            .expect("admit E1M1 product service");
        let generation = service.start_session();
        service
            .submit(LoadingBayServiceCommand::Restart {
                connection_generation: generation,
                sequence: 1,
                mode: crate::GameRestartMode::AuthoredBaseline,
            })
            .expect("stage restart");
        service
            .advance(crate::FIXED_STEP_DURATION)
            .expect("consume restart");
        let replacement_generation = service.runtime.input_session().connection_generation;
        assert!(replacement_generation > generation);
        service
            .submit(LoadingBayServiceCommand::SetInputIntent {
                connection_generation: replacement_generation,
                sequence: 1,
                movement: [0.0, 1.0],
                look_delta: [0.0, 0.0],
                jump_held: false,
                primary_fire_held: false,
            })
            .expect("new session command");
        let _ = std::fs::remove_dir_all(save_root);
    }

    #[test]
    fn disconnect_and_rebegin_discard_queued_gameplay_facts() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-product-session-facts-{}-{unique}",
            std::process::id()
        ));
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/doom-e1m1.project.json");
        let mut service = LoadingBayProductService::admit(&project, &save_root)
            .expect("admit E1M1 product service");
        let first_generation = service.start_session();
        service
            .submit(LoadingBayServiceCommand::SetInputIntent {
                connection_generation: first_generation,
                sequence: 1,
                movement: [0.0, 1.0],
                look_delta: [0.0, 0.0],
                jump_held: false,
                primary_fire_held: false,
            })
            .expect("queue movement in first session");
        service
            .advance(crate::FIXED_STEP_DURATION)
            .expect("produce first-session facts");

        service.disconnect_session(first_generation);
        let replacement_generation = service.start_session();
        assert!(replacement_generation > first_generation);
        assert!(service.drain_game_loop_facts().is_empty());

        service
            .submit(LoadingBayServiceCommand::SetInputIntent {
                connection_generation: replacement_generation,
                sequence: 1,
                movement: [0.0, 1.0],
                look_delta: [0.0, 0.0],
                jump_held: false,
                primary_fire_held: false,
            })
            .expect("queue movement in replacement session");
        service
            .advance(crate::FIXED_STEP_DURATION)
            .expect("produce replacement-session facts");
        assert!(!service.drain_game_loop_facts().is_empty());

        let _ = std::fs::remove_dir_all(save_root);
    }

    #[test]
    fn reconnect_clears_the_latest_gameplay_program_outcome() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-program-outcome-reconnect-{}-{unique}",
            std::process::id()
        ));
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/doom-e1m1.project.json");
        let mut service = LoadingBayProductService::admit(&project, &save_root)
            .expect("admit E1M1 product service");
        let first_generation = service.start_session();
        service
            .runtime
            .runtime_mut()
            .attack(LOADING_BAY_PLAYER, crate::ResolvedAttackAction::Attack)
            .expect("record a live gameplay-program outcome");
        assert!(service.gameplay_outcome().is_some());

        service.disconnect_session(first_generation);
        let replacement_generation = service.start_session();

        assert!(replacement_generation > first_generation);
        assert!(service.gameplay_outcome().is_none());
        let _ = std::fs::remove_dir_all(save_root);
    }

    #[test]
    fn rejected_ordinary_edge_emits_a_typed_fixed_tick_outcome() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-product-rejection-{}-{unique}",
            std::process::id()
        ));
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/doom-e1m1.project.json");
        let mut service = LoadingBayProductService::admit(&project, &save_root)
            .expect("admit E1M1 product service");
        let generation = service.start_session();
        service
            .submit(LoadingBayServiceCommand::Edge {
                connection_generation: generation,
                sequence: 1,
                command: GameLoopEdgeCommandKind::SelectWeaponSlot { slot: u8::MAX },
            })
            .expect("admit edge before fixed-tick adjudication");
        service
            .advance(crate::FIXED_STEP_DURATION)
            .expect("adjudicate rejected edge");
        let outcome = service
            .drain_outcomes()
            .into_iter()
            .find(|outcome| outcome.command_sequence == Some(1))
            .expect("rejected edge outcome");
        assert_eq!(outcome.kind, "invalidWeaponSlot");
        assert!(outcome.message.is_some());
        assert!(!outcome.session_replaced);
        let _ = std::fs::remove_dir_all(save_root);
    }
}
