use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::{Deref, DerefMut};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LockResult, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use core_ids::EntityId;
use loading_bay_game::{
    admit_stored_project_with_document, materialize_stored_project_voxels, AdmittedStoredProject,
    CombatFact, CombatMissReason, GameEvent, GameLoopFact, GameRuntime, LoadingBayGameLoop,
    NavigationFact, PlayerControlFact, ProjectSaveMode, ProjectStore, SaveGameError, SaveGameStore,
    SaveProjectIdentity, SaveSlotId, SaveSlotSummary, SaveWriteRequest, VoxelEdit,
    VoxelEditTransaction, VoxelSourceRevision, MAX_PENDING_GAME_LOOP_FACTS,
};
use serde::{Deserialize, Serialize};

#[path = "browser_host/presentation.rs"]
mod presentation;
#[path = "browser_host/session.rs"]
mod session;
#[path = "browser_host/state.rs"]
mod state;

use presentation::BrowserFeedbackProjection;
use session::{run_game_session, session_upgrade_requested};
use state::{browser_state, BrowserState};

const DEFAULT_ADDRESS: &str = "127.0.0.1:8787";
const DEN_PROJECT: &str = "rusty-engine-demo";
const ACTOR: EntityId = EntityId::new(1);
const BEACON: EntityId = EntityId::new(7);
const ENCOUNTER: EntityId = EntityId::new(2);
const EXIT: EntityId = EntityId::new(3);
const FIRST_ENEMY: u64 = 4;
const MOTION_PROBE: EntityId = EntityId::new(10);

#[derive(Debug, Clone)]
struct BrowserProjectSummary {
    project_id: String,
    source_schema_version: u32,
    current_schema_version: u32,
    entry_scene: String,
    asset_count: usize,
    scene_count: usize,
    entity_count: usize,
}

#[derive(Debug)]
struct BrowserRuntime {
    host_session_id: String,
    runtime: LoadingBayGameLoop,
    authored: AdmittedStoredProject,
    project_path: PathBuf,
    project: BrowserProjectSummary,
    pending_restart: Option<PendingRestart>,
    replacement_origin: Option<RestartIdentity>,
    save_store: SaveGameStore,
    save_identity: SaveProjectIdentity,
    save_slots: Vec<SaveSlotSummary>,
    pending_saves: BTreeMap<u64, PendingSave>,
    pending_session_facts: VecDeque<(String, Option<u64>)>,
    dropped_session_fact_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RestartIdentity {
    connection_generation: u64,
    sequence: u64,
}

#[derive(Debug)]
struct PendingRestart {
    identity: RestartIdentity,
    replacement: Box<BrowserRuntime>,
}

#[derive(Debug)]
struct PendingSave {
    identity: RestartIdentity,
    slot: SaveSlotId,
    overwrite: bool,
    expected_storage_revision: Option<String>,
}

impl BrowserRuntime {
    #[cfg(test)]
    fn load(path: &Path) -> Result<Self, String> {
        Self::load_with_save_root(path, &default_save_root())
    }

    fn load_with_save_root(path: &Path, save_root: &Path) -> Result<Self, String> {
        let decoded = ProjectStore::default()
            .load(path)
            .map_err(|error| format!("could not load {}: {error}", path.display()))?;
        let project_path = path.canonicalize().map_err(|error| {
            format!(
                "loaded project {} could not be resolved: {error}",
                path.display()
            )
        })?;
        let project = BrowserProjectSummary {
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
        };
        let (authored, admitted) = admit_stored_project_with_document(decoded.project)
            .map_err(|error| format!("project admission failed: {error}"))?;
        let save_identity = SaveProjectIdentity::from_project(authored.document(), ACTOR)
            .map_err(|error| format!("could not identify authored project for saves: {error}"))?;
        let save_store = SaveGameStore::new(save_root);
        let save_slots = save_store.inspect_all(&save_identity);
        Ok(Self {
            host_session_id: new_host_session_id(),
            runtime: LoadingBayGameLoop::new(GameRuntime::from_admitted_project(admitted), ACTOR)
                .map_err(|error| format!("could not create Loading Bay game loop: {error}"))?,
            authored,
            project_path,
            project,
            pending_restart: None,
            replacement_origin: None,
            save_store,
            save_identity,
            save_slots,
            pending_saves: BTreeMap::new(),
            pending_session_facts: VecDeque::new(),
            dropped_session_fact_count: 0,
        })
    }

    fn start_browser_connection(&mut self) -> u64 {
        self.pending_restart = None;
        self.replacement_origin = None;
        self.pending_saves.clear();
        self.runtime.start_connection().connection_generation
    }

    fn stage_restart(
        &mut self,
        connection_generation: u64,
        sequence: u64,
        mut replacement: BrowserRuntime,
    ) {
        replacement
            .host_session_id
            .clone_from(&self.host_session_id);
        self.pending_restart = Some(PendingRestart {
            identity: RestartIdentity {
                connection_generation,
                sequence,
            },
            replacement: Box::new(replacement),
        });
    }

    fn stage_save(
        &mut self,
        connection_generation: u64,
        sequence: u64,
        slot: SaveSlotId,
        overwrite: bool,
        expected_storage_revision: Option<String>,
    ) {
        self.pending_saves.insert(
            sequence,
            PendingSave {
                identity: RestartIdentity {
                    connection_generation,
                    sequence,
                },
                slot,
                overwrite,
                expected_storage_revision,
            },
        );
    }

    fn cancel_staged_restart(&mut self, connection_generation: u64, sequence: u64) {
        if self.pending_restart.as_ref().is_some_and(|pending| {
            pending.identity
                == (RestartIdentity {
                    connection_generation,
                    sequence,
                })
        }) {
            self.pending_restart = None;
        }
    }

    fn staged_restart_matches(&self, connection_generation: u64, sequence: u64) -> bool {
        self.pending_restart.as_ref().is_some_and(|pending| {
            pending.identity
                == (RestartIdentity {
                    connection_generation,
                    sequence,
                })
        })
    }

    fn apply_consumed_restart(&mut self, sequence: u64) -> bool {
        let Some(pending) = self.pending_restart.take() else {
            return false;
        };
        if pending.identity.sequence != sequence
            || self.runtime.input_session().connection_generation
                != pending.identity.connection_generation
        {
            self.pending_restart = Some(pending);
            return false;
        }

        let mut replacement = *pending.replacement;
        replacement
            .runtime
            .start_connection_after(pending.identity.connection_generation);
        replacement.replacement_origin = Some(pending.identity);
        *self = replacement;
        true
    }

    fn apply_consumed_save(&mut self, sequence: u64, slot: SaveSlotId) -> bool {
        let Some(pending) = self.pending_saves.remove(&sequence) else {
            return false;
        };
        if pending.identity.sequence != sequence
            || pending.identity.connection_generation
                != self.runtime.input_session().connection_generation
            || pending.slot != slot
        {
            self.pending_saves.insert(sequence, pending);
            return false;
        }
        let result = self.save_store.save(
            &self.save_identity,
            SaveWriteRequest {
                slot,
                overwrite: pending.overwrite,
                expected_storage_revision: pending.expected_storage_revision,
                saved_at_unix_milliseconds: unix_time_milliseconds(),
            },
            self.runtime.runtime(),
        );
        self.save_slots = self.save_store.inspect_all(&self.save_identity);
        match result {
            Ok(_) => self.push_session_fact("GameSaved", Some(sequence)),
            Err(error) => {
                eprintln!("browser-host save {} failed: {error}", slot.display_name());
                self.push_session_fact(save_error_fact_name(&error), Some(sequence));
            }
        }
        true
    }

    fn adopt_consumed_restart(&mut self, connection_generation: u64, sequence: u64) -> Option<u64> {
        let identity = RestartIdentity {
            connection_generation,
            sequence,
        };
        if self.replacement_origin != Some(identity) {
            return None;
        }
        self.replacement_origin = None;
        Some(self.runtime.input_session().connection_generation)
    }

    fn disconnect_browser_session(
        &mut self,
        connection_generation: u64,
        pending_restart_sequence: Option<u64>,
    ) {
        if let Some(sequence) = pending_restart_sequence {
            self.cancel_staged_restart(connection_generation, sequence);
            if self.replacement_origin
                == Some(RestartIdentity {
                    connection_generation,
                    sequence,
                })
            {
                self.replacement_origin = None;
                let active_generation = self.runtime.input_session().connection_generation;
                self.runtime.disconnect(active_generation);
                return;
            }
        }
        self.runtime.disconnect(connection_generation);
    }

    fn replacement_from_runtime(&self, runtime: GameRuntime) -> Result<Self, String> {
        Ok(Self {
            host_session_id: self.host_session_id.clone(),
            runtime: LoadingBayGameLoop::new(runtime, ACTOR)
                .map_err(|error| format!("could not restore Loading Bay game loop: {error}"))?,
            authored: self.authored.clone(),
            project_path: self.project_path.clone(),
            project: self.project.clone(),
            pending_restart: None,
            replacement_origin: None,
            save_store: self.save_store.clone(),
            save_identity: self.save_identity.clone(),
            save_slots: self.save_store.inspect_all(&self.save_identity),
            pending_saves: BTreeMap::new(),
            pending_session_facts: VecDeque::new(),
            dropped_session_fact_count: self.dropped_session_fact_count,
        })
    }

    fn push_session_fact(&mut self, kind: impl Into<String>, sequence: Option<u64>) {
        if self.pending_session_facts.len() == MAX_PENDING_GAME_LOOP_FACTS {
            self.pending_session_facts.pop_front();
            self.dropped_session_fact_count = self.dropped_session_fact_count.saturating_add(1);
        }
        self.pending_session_facts
            .push_back((kind.into(), sequence));
    }

    fn drain_session_facts(&mut self) -> Vec<(String, Option<u64>)> {
        self.pending_session_facts.drain(..).collect()
    }

    fn dropped_fact_count(&self) -> u64 {
        self.runtime
            .dropped_fact_count()
            .saturating_add(self.dropped_session_fact_count)
    }
}

fn new_host_session_id() -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(1);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}-{sequence:x}", std::process::id(), time)
}

fn unix_time_milliseconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn save_error_fact_name(error: &SaveGameError) -> &'static str {
    match error {
        SaveGameError::OverwriteRequired { .. } => "SaveRejectedOverwriteRequired",
        SaveGameError::Stale { .. } => "SaveRejectedStale",
        SaveGameError::Corrupt { .. } | SaveGameError::TooLarge { .. } => {
            "SaveRejectedSnapshotCorrupt"
        }
        SaveGameError::Incompatible { .. } => "SaveRejectedSnapshotIncompatible",
        SaveGameError::Empty { .. } | SaveGameError::Io { .. } | SaveGameError::Encode(_) => {
            "SaveRejectedUnavailable"
        }
    }
}

impl Deref for BrowserRuntime {
    type Target = GameRuntime;

    fn deref(&self) -> &Self::Target {
        self.runtime.runtime()
    }
}

impl DerefMut for BrowserRuntime {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.runtime.runtime_mut()
    }
}

#[derive(Debug)]
struct SharedBrowserRuntime {
    runtime: Mutex<BrowserRuntime>,
}

impl SharedBrowserRuntime {
    fn new(runtime: BrowserRuntime) -> Self {
        Self {
            runtime: Mutex::new(runtime),
        }
    }

    fn lock(&self) -> LockResult<MutexGuard<'_, BrowserRuntime>> {
        self.runtime.lock()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct BrowserVoxelEditRequest {
    expected_revision: u64,
    #[serde(default)]
    persist_to_project: bool,
    edits: Vec<BrowserVoxelEdit>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum BrowserVoxelEdit {
    Set {
        address: [i64; 3],
        material_slot: u16,
    },
    Clear {
        address: [i64; 3],
    },
}

impl BrowserVoxelEdit {
    const fn into_edit(self) -> VoxelEdit {
        match self {
            Self::Set {
                address,
                material_slot,
            } => VoxelEdit::Set {
                address,
                material_slot,
            },
            Self::Clear { address } => VoxelEdit::Clear { address },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserVoxelEditReceipt {
    revision_before: u64,
    accepted_revision: u64,
    changed_voxels: usize,
    changed_min: [i64; 3],
    changed_max_inclusive: [i64; 3],
    authority_hash: String,
    persisted_to_project: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserVoxelEditResponse {
    #[serde(flatten)]
    state: BrowserState,
    voxel_edit_receipt: BrowserVoxelEditReceipt,
}

fn main() {
    let (address, dist, project_path, save_root) = arguments();
    let dist = dist.canonicalize().unwrap_or_else(|error| {
        panic!(
            "browser shell dist {} is unavailable: {error}",
            dist.display()
        )
    });
    assert!(
        dist.join("index.html").is_file(),
        "browser shell is not built"
    );

    let runtime = BrowserRuntime::load_with_save_root(&project_path, &save_root)
        .unwrap_or_else(|error| panic!("could not start browser project: {error}"));
    println!(
        "browser-host project id={} sourceSchema={} currentSchema={} entryScene={} assets={} scenes={} entities={} path={}",
        runtime.project.project_id,
        runtime.project.source_schema_version,
        runtime.project.current_schema_version,
        runtime.project.entry_scene,
        runtime.project.asset_count,
        runtime.project.scene_count,
        runtime.project.entity_count,
        runtime.project_path.display()
    );
    let runtime = Arc::new(SharedBrowserRuntime::new(runtime));
    start_game_loop_driver(&runtime);
    let listener = TcpListener::bind(&address)
        .unwrap_or_else(|error| panic!("cannot bind browser host at {address}: {error}"));
    println!("browser-host listening at http://{address}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let runtime = Arc::clone(&runtime);
                let dist = dist.clone();
                std::thread::spawn(move || handle_connection(stream, &runtime, &dist));
            }
            Err(error) => eprintln!("browser-host accept error: {error}"),
        }
    }
}

fn start_game_loop_driver(runtime: &Arc<SharedBrowserRuntime>) {
    let runtime = Arc::clone(runtime);
    std::thread::spawn(move || {
        let mut previous = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(4));
            let now = Instant::now();
            let elapsed = now.saturating_duration_since(previous);
            previous = now;
            let mut host = runtime.lock().expect("runtime lock");
            match host.runtime.advance_elapsed(elapsed) {
                Ok(receipt) => {
                    for fact in receipt
                        .fixed_ticks
                        .iter()
                        .flat_map(|tick| tick.facts.iter())
                    {
                        match fact {
                            GameLoopFact::SaveRequested { sequence, slot } => {
                                if !host.apply_consumed_save(*sequence, *slot) {
                                    eprintln!(
                                        "browser-host ignored save {sequence} without matching staged storage intent"
                                    );
                                }
                            }
                            GameLoopFact::LoadRequested { sequence, slot } => {
                                if host.apply_consumed_restart(*sequence) {
                                    host.push_session_fact(
                                        if *slot == SaveSlotId::Checkpoint {
                                            "CheckpointRestored"
                                        } else {
                                            "GameLoaded"
                                        },
                                        Some(*sequence),
                                    );
                                } else {
                                    eprintln!(
                                        "browser-host ignored load {sequence} without a matching staged runtime"
                                    );
                                }
                            }
                            GameLoopFact::RestartRequested { sequence, mode } => {
                                if host.apply_consumed_restart(*sequence) {
                                    if *mode == loading_bay_game::GameRestartMode::Checkpoint {
                                        host.push_session_fact(
                                            "CheckpointRestored",
                                            Some(*sequence),
                                        );
                                    }
                                } else {
                                    eprintln!(
                                        "browser-host ignored restart {sequence} without a matching staged runtime"
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Err(error) => {
                    eprintln!("browser-host fixed game loop error: {error}");
                }
            }
        }
    });
}

fn arguments() -> (String, PathBuf, PathBuf, PathBuf) {
    let default_dist =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../dist/apps/loading-bay/browser");
    let mut address = DEFAULT_ADDRESS.to_owned();
    let mut dist = default_dist;
    let mut project = default_project_path();
    let mut save_root = default_save_root();
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--addr" => address = args.next().expect("--addr needs a value"),
            "--dist" => dist = PathBuf::from(args.next().expect("--dist needs a value")),
            "--project" => {
                project = PathBuf::from(args.next().expect("--project needs a value"));
            }
            "--save-root" => {
                save_root = PathBuf::from(args.next().expect("--save-root needs a value"));
            }
            _ => panic!("unknown browser-host argument {argument}"),
        }
    }
    (address, dist, project, save_root)
}

fn default_project_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../content/projects/loading-bay.project.json")
}

fn default_save_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.runtime/saves")
}

fn handle_connection(mut stream: TcpStream, runtime: &Arc<SharedBrowserRuntime>, dist: &Path) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    if session_upgrade_requested(&stream) {
        run_game_session(stream, Arc::clone(runtime));
        return;
    }
    let request = match read_request(&mut stream) {
        Ok(request) => request,
        Err(message) => {
            let _ = write_response(
                &mut stream,
                400,
                "text/plain; charset=utf-8",
                message.into(),
            );
            return;
        }
    };
    let path = request.path.split('?').next().unwrap_or(&request.path);
    let response = route(&request.method, path, &request.body, runtime, dist);
    let _ = write_response(&mut stream, response.0, response.1, response.2);
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut request = Vec::new();
    let mut buffer = [0u8; 2_048];
    let header_end = loop {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("request ended before its headers".to_owned());
        }
        request.extend_from_slice(&buffer[..read]);
        if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
        if request.len() > 16_384 {
            return Err("request headers are too large".to_owned());
        }
    };
    let head = String::from_utf8(request[..header_end].to_vec())
        .map_err(|_| "request headers are not UTF-8".to_owned())?;
    let content_length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>())
        })
        .transpose()
        .map_err(|_| "content-length must be an integer".to_owned())?
        .unwrap_or(0);
    if content_length > 16_384 {
        return Err("request body is too large".to_owned());
    }
    while request.len() < header_end + content_length {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("request ended before its declared body".to_owned());
        }
        request.extend_from_slice(&buffer[..read]);
    }
    let mut parts = head.lines().next().unwrap_or_default().split_whitespace();
    let method = parts.next().ok_or("request method is missing")?.to_owned();
    let path = parts.next().ok_or("request path is missing")?.to_owned();
    Ok(HttpRequest {
        method,
        path,
        body: request[header_end..header_end + content_length].to_vec(),
    })
}

fn route(
    method: &str,
    path: &str,
    body: &[u8],
    runtime: &Arc<SharedBrowserRuntime>,
    dist: &Path,
) -> (u16, &'static str, Vec<u8>) {
    match (method, path) {
        ("GET", "/health") => json_response(
            200,
            serde_json::json!({ "project": DEN_PROJECT, "status": "ok" }),
        ),
        ("GET", "/api/state") => {
            let runtime = runtime.lock().expect("runtime lock");
            json_response(
                200,
                browser_state(&runtime, Vec::new(), BrowserFeedbackProjection::default()),
            )
        }
        ("POST", "/api/voxel-edit") => {
            let request: BrowserVoxelEditRequest = match serde_json::from_slice(body) {
                Ok(request) => request,
                Err(error) => return error_json(400, &format!("invalid voxel edit: {error}")),
            };
            let edits: Vec<_> = request
                .edits
                .iter()
                .copied()
                .map(BrowserVoxelEdit::into_edit)
                .collect();
            let mut runtime = runtime.lock().expect("runtime lock");
            let before = runtime.snapshot();
            let receipt = match runtime.apply_voxel_edits(VoxelEditTransaction {
                expected_revision: VoxelSourceRevision::new(request.expected_revision),
                edits: &edits,
            }) {
                Ok(receipt) => receipt,
                Err(error) => return error_json(409, &format!("{error}")),
            };
            if request.persist_to_project {
                let candidate = match materialize_stored_project_voxels(
                    &runtime.authored,
                    runtime
                        .collision_scene()
                        .expect("edited browser collision scene"),
                ) {
                    Ok(candidate) => candidate,
                    Err(error) => {
                        *runtime.runtime.runtime_mut() = GameRuntime::from_snapshot(before)
                            .expect("pre-edit browser snapshot remains valid");
                        return error_json(
                            409,
                            &format!("project materialization failed: {error}"),
                        );
                    }
                };
                if let Err(error) = ProjectStore::default().save(
                    &runtime.project_path,
                    &candidate,
                    ProjectSaveMode::ReplaceExisting,
                ) {
                    *runtime.runtime.runtime_mut() = GameRuntime::from_snapshot(before)
                        .expect("pre-edit browser snapshot remains valid");
                    return error_json(500, &format!("project save failed: {error}"));
                }
                runtime.authored = candidate;
            }
            json_response(
                200,
                BrowserVoxelEditResponse {
                    state: browser_state(
                        &runtime,
                        vec!["VoxelEdited".to_owned()],
                        BrowserFeedbackProjection::default(),
                    ),
                    voxel_edit_receipt: BrowserVoxelEditReceipt {
                        revision_before: receipt.revision_before.raw(),
                        accepted_revision: receipt.accepted_revision.raw(),
                        changed_voxels: receipt.fact.changed_voxels,
                        changed_min: receipt.fact.changed_min,
                        changed_max_inclusive: receipt.fact.changed_max_inclusive,
                        authority_hash: format!("{:016x}", receipt.authority_hash),
                        persisted_to_project: request.persist_to_project,
                    },
                },
            )
        }
        ("GET", _) | ("HEAD", _) => serve_static(method, path, dist),
        _ => error_json(405, "method not allowed"),
    }
}

fn drain_game_loop_feedback(
    game_loop: &mut LoadingBayGameLoop,
) -> (Vec<(String, Option<u64>)>, BrowserFeedbackProjection) {
    let mut facts = Vec::new();
    let mut feedback = BrowserFeedbackProjection::default();
    let presentation_tick = game_loop.runtime().tick().raw();
    for fact in game_loop.drain_pending_facts() {
        match fact {
            GameLoopFact::PlayerControl(fact) => {
                facts.push((player_fact_name(&fact).to_owned(), None));
                // Accepted pose/posture is projected every update. Footstep and
                // blocked pulses are disposable accents, so sample them from
                // authoritative ticks instead of creating shared-host work at
                // the 60 Hz simulation cadence.
                if emits_locomotion_feedback(presentation_tick) {
                    feedback.extend_player_control(std::slice::from_ref(&fact));
                }
            }
            GameLoopFact::Navigation(fact) => {
                facts.push((navigation_fact_name(&fact).to_owned(), None));
                // Enemy movement remains visible through retained transforms
                // and durable posture. Per-tick pathfinding facts must not
                // mint particles/audio/billboards.
            }
            GameLoopFact::EnemyCombat(fact) => {
                facts.push((enemy_combat_fact_name(&fact).to_owned(), None));
                feedback.extend_enemy_combat(std::slice::from_ref(&fact));
            }
            GameLoopFact::Combat(fact) => {
                facts.push((combat_fact_name(&fact).to_owned(), None));
                feedback.extend_combat(std::slice::from_ref(&fact));
            }
            GameLoopFact::ExtractionBeacon(fact) => {
                facts.push(("ExtractionBeaconActivated".to_owned(), None));
                feedback.extend_extraction_beacon(fact);
            }
            GameLoopFact::Pickup(fact) => {
                facts.push(("PickupCollected".to_owned(), None));
                feedback.extend_pickup(&fact);
            }
            GameLoopFact::Inventory(_) => {
                facts.push(("InventoryWeaponSelected".to_owned(), None));
            }
            GameLoopFact::Vitality(fact) => {
                facts.push((vitality_fact_name(&fact).to_owned(), None));
                feedback.extend_vitality(std::slice::from_ref(&fact));
            }
            GameLoopFact::Hazard(loading_bay_game::HazardFact::Damage(fact)) => {
                facts.push((vitality_fact_name(&fact).to_owned(), None));
                feedback.extend_vitality(std::slice::from_ref(&fact));
            }
            GameLoopFact::Progression(fact) => {
                facts.push((
                    match fact {
                        loading_bay_game::ProgressionFact::DoorAccessGranted { .. } => {
                            "DoorAccessGranted"
                        }
                        loading_bay_game::ProgressionFact::SecretDiscovered { .. } => {
                            "SecretDiscovered"
                        }
                        loading_bay_game::ProgressionFact::LevelCompleted { .. } => {
                            "LevelCompleted"
                        }
                    }
                    .to_owned(),
                    None,
                ));
                feedback.extend_progression(&fact);
            }
            GameLoopFact::DoorAccessRejected {
                sequence,
                door,
                required_key,
                presentation,
            } => {
                facts.push(("DoorAccessRejectedLocked".to_owned(), Some(sequence)));
                feedback.extend_door_access_denied(door, &required_key, &presentation);
            }
            GameLoopFact::PickupRejected { reason, .. } => {
                facts.push((pickup_rejection_name(&reason).to_owned(), None));
            }
            GameLoopFact::Event(event) => {
                facts.push((event_name(&event).to_owned(), None));
                feedback.extend_events(std::slice::from_ref(&event));
            }
            GameLoopFact::CombatRejected {
                attacker,
                weapon,
                presentation,
                reason,
            } => {
                if reason == loading_bay_game::CombatRejectionReason::NoAmmo {
                    if let (Some(weapon), Some(presentation)) = (&weapon, &presentation) {
                        feedback.extend_dry_fire(attacker, weapon, presentation);
                    }
                }
                facts.push((
                    match reason {
                        loading_bay_game::CombatRejectionReason::Cooldown => {
                            "CombatRejectedCooldown"
                        }
                        loading_bay_game::CombatRejectionReason::NoAmmo => "CombatRejectedNoAmmo",
                        loading_bay_game::CombatRejectionReason::NoEquippedWeapon => {
                            "CombatRejectedNoEquippedWeapon"
                        }
                        loading_bay_game::CombatRejectionReason::AttackerDefeated => {
                            "CombatRejectedPlayerDefeated"
                        }
                    }
                    .to_owned(),
                    None,
                ))
            }
            GameLoopFact::EdgeCommandRejected { sequence, reason } => facts.push((
                match reason {
                    loading_bay_game::EdgeCommandRejection::Paused => "InputEdgeRejectedPaused",
                    loading_bay_game::EdgeCommandRejection::UnknownTarget => {
                        "InputEdgeRejectedUnknownTarget"
                    }
                    loading_bay_game::EdgeCommandRejection::NotInteractable => {
                        "InputEdgeRejectedNotInteractable"
                    }
                    loading_bay_game::EdgeCommandRejection::PickupRejected => {
                        "InputEdgeRejectedPickup"
                    }
                    loading_bay_game::EdgeCommandRejection::InvalidWeaponSlot => {
                        "InputEdgeRejectedInvalidWeaponSlot"
                    }
                    loading_bay_game::EdgeCommandRejection::WeaponNotOwned => {
                        "InputEdgeRejectedWeaponNotOwned"
                    }
                    loading_bay_game::EdgeCommandRejection::WeaponAlreadySelected => {
                        "InputEdgeRejectedWeaponAlreadySelected"
                    }
                    loading_bay_game::EdgeCommandRejection::PlayerDefeated => {
                        "InputEdgeRejectedPlayerDefeated"
                    }
                    loading_bay_game::EdgeCommandRejection::InventoryRejected => {
                        "InputEdgeRejectedInventory"
                    }
                    loading_bay_game::EdgeCommandRejection::ItemNotOwned => {
                        "InputEdgeRejectedItemNotOwned"
                    }
                    loading_bay_game::EdgeCommandRejection::ItemNotUsable => {
                        "InputEdgeRejectedItemNotUsable"
                    }
                    loading_bay_game::EdgeCommandRejection::HealthFull => {
                        "InputEdgeRejectedHealthFull"
                    }
                    loading_bay_game::EdgeCommandRejection::CheckpointUnavailable => {
                        "InputEdgeRejectedCheckpointUnavailable"
                    }
                    loading_bay_game::EdgeCommandRejection::DoorLocked => {
                        "InputEdgeRejectedDoorLocked"
                    }
                    loading_bay_game::EdgeCommandRejection::LevelExitUnavailable => {
                        "InputEdgeRejectedLevelExitUnavailable"
                    }
                    loading_bay_game::EdgeCommandRejection::LevelComplete => {
                        "InputEdgeRejectedLevelComplete"
                    }
                }
                .to_owned(),
                Some(sequence),
            )),
            GameLoopFact::InputExpired { .. } => {
                facts.push(("InputExpired".to_owned(), None));
            }
            GameLoopFact::RestartRequested { .. } => {
                facts.push(("RestartRequested".to_owned(), None));
            }
            GameLoopFact::SaveRequested { sequence, .. } => {
                facts.push(("SaveRequested".to_owned(), Some(sequence)));
            }
            GameLoopFact::LoadRequested { sequence, .. } => {
                facts.push(("LoadRequested".to_owned(), Some(sequence)));
            }
        }
    }
    (facts, feedback)
}

fn emits_locomotion_feedback(tick: u64) -> bool {
    tick % 6 == 0
}

fn enemy_combat_fact_name(fact: &loading_bay_game::EnemyCombatFact) -> &'static str {
    match fact {
        loading_bay_game::EnemyCombatFact::Alerted { .. } => "EnemyAlerted",
        loading_bay_game::EnemyCombatFact::PostureChanged { .. } => "EnemyPostureChanged",
        loading_bay_game::EnemyCombatFact::AttackFired { .. } => "EnemyAttackFired",
        loading_bay_game::EnemyCombatFact::AttackHit { .. } => "EnemyAttackHit",
        loading_bay_game::EnemyCombatFact::AttackMissed { .. } => "EnemyAttackMissed",
        loading_bay_game::EnemyCombatFact::Vitality(fact) => vitality_fact_name(fact),
    }
}

fn pickup_rejection_name(reason: &loading_bay_game::PickupRejection) -> &'static str {
    match reason {
        loading_bay_game::PickupRejection::Inventory(
            loading_bay_game::InventoryRejection::QuantityOverflow { .. },
        ) => "PickupRejectedQuantityOverflow",
        loading_bay_game::PickupRejection::Inventory(
            loading_bay_game::InventoryRejection::InventoryFull { .. },
        ) => "PickupRejectedInventoryFull",
        loading_bay_game::PickupRejection::Inventory(_) => "PickupRejectedInventory",
        loading_bay_game::PickupRejection::NotOverlapping { .. } => "PickupRejectedNotOverlapping",
        loading_bay_game::PickupRejection::UnknownPickup { .. } => "PickupRejectedUnknown",
        loading_bay_game::PickupRejection::NotMaterialized { .. } => {
            "PickupRejectedNotMaterialized"
        }
        loading_bay_game::PickupRejection::PlayerDefeated { .. } => "PickupRejectedPlayerDefeated",
        loading_bay_game::PickupRejection::InventorySequenceOverflow { .. } => {
            "PickupRejectedSequenceOverflow"
        }
        loading_bay_game::PickupRejection::WorldMutationFailed { .. } => {
            "PickupRejectedWorldMutation"
        }
        loading_bay_game::PickupRejection::Trigger { .. } => "PickupRejectedTrigger",
        loading_bay_game::PickupRejection::Vitality(_) => "PickupRejectedVitality",
    }
}

fn combat_fact_name(fact: &CombatFact) -> &'static str {
    match fact {
        CombatFact::Inventory(_) => "CombatAmmunitionConsumed",
        CombatFact::AttackFired { .. } => "CombatFired",
        CombatFact::AttackHit { .. } => "CombatHit",
        CombatFact::AttackMissed {
            reason: CombatMissReason::NoTarget,
            ..
        } => "CombatMissedNoTarget",
        CombatFact::AttackMissed {
            reason: CombatMissReason::WorldBlocked,
            ..
        } => "CombatMissedWorldBlocked",
        CombatFact::Vitality(loading_bay_game::VitalityFact::DamageApplied { .. }) => {
            "DamageApplied"
        }
        CombatFact::Vitality(loading_bay_game::VitalityFact::Died { .. }) => "EntityDied",
        CombatFact::Vitality(loading_bay_game::VitalityFact::ArmorGranted { .. }) => "ArmorGranted",
        CombatFact::Vitality(loading_bay_game::VitalityFact::HealthRestored { .. }) => {
            "HealthRestored"
        }
        CombatFact::EnemyDefeated { .. } => "CombatEnemyDefeated",
        CombatFact::EnemyDrop(_) => "EnemyDropMaterialized",
    }
}

fn vitality_fact_name(fact: &loading_bay_game::VitalityFact) -> &'static str {
    match fact {
        loading_bay_game::VitalityFact::DamageApplied { .. } => "DamageApplied",
        loading_bay_game::VitalityFact::Died { .. } => "EntityDied",
        loading_bay_game::VitalityFact::ArmorGranted { .. } => "ArmorGranted",
        loading_bay_game::VitalityFact::HealthRestored { .. } => "HealthRestored",
    }
}

fn navigation_fact_name(fact: &NavigationFact) -> &'static str {
    match fact {
        NavigationFact::Advanced { .. } => "NavigationAdvanced",
        NavigationFact::Arrived { .. } => "NavigationArrived",
        NavigationFact::Blocked { .. } => "NavigationBlocked",
        NavigationFact::Unreachable { .. } => "NavigationUnreachable",
    }
}

fn player_fact_name(fact: &PlayerControlFact) -> &'static str {
    match fact {
        PlayerControlFact::Moved { .. } => "PlayerMoved",
        PlayerControlFact::Blocked { .. } => "PlayerBlocked",
        PlayerControlFact::LookChanged { .. } => "PlayerLookChanged",
    }
}

fn event_name(event: &GameEvent) -> &'static str {
    match event {
        GameEvent::SwitchActivated { .. } => "SwitchActivated",
        GameEvent::DoorOpened { .. } => "DoorOpened",
        GameEvent::DoorClosed { .. } => "DoorClosed",
        GameEvent::EnemyDefeated { .. } => "EnemyDefeated",
        GameEvent::PlayerDied { .. } => "PlayerDied",
        GameEvent::EncounterActivated { .. } => "EncounterActivated",
        GameEvent::EncounterCleared { .. } => "EncounterCleared",
    }
}

fn json_response(value_status: u16, value: impl Serialize) -> (u16, &'static str, Vec<u8>) {
    (
        value_status,
        "application/json; charset=utf-8",
        serde_json::to_vec(&value).expect("encode browser response"),
    )
}

fn error_json(status: u16, message: &str) -> (u16, &'static str, Vec<u8>) {
    json_response(status, serde_json::json!({ "error": message }))
}

fn serve_static(method: &str, path: &str, dist: &Path) -> (u16, &'static str, Vec<u8>) {
    let relative = if path == "/" {
        PathBuf::from("index.html")
    } else {
        PathBuf::from(path.trim_start_matches('/'))
    };
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return (403, "text/plain; charset=utf-8", b"forbidden\n".to_vec());
    }
    let file = dist.join(&relative);
    if !file.is_file() {
        return (404, "text/plain; charset=utf-8", b"not found\n".to_vec());
    }
    let content_type = content_type(&file);
    let body = if method == "HEAD" {
        Vec::new()
    } else {
        match fs::read(&file) {
            Ok(body) => body,
            Err(_) => return (500, "text/plain; charset=utf-8", b"read error\n".to_vec()),
        }
    };
    (200, content_type, body)
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: Vec<u8>,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Den-Project: {DEN_PROJECT}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored_browser_runtime() -> BrowserRuntime {
        BrowserRuntime::load(&default_project_path()).expect("admit stored browser project")
    }

    fn shared_browser_runtime() -> Arc<SharedBrowserRuntime> {
        Arc::new(SharedBrowserRuntime::new(stored_browser_runtime()))
    }

    #[test]
    fn health_identifies_the_managed_demo_host() {
        let runtime = shared_browser_runtime();
        let response = route("GET", "/health", &[], &runtime, Path::new("."));
        assert_eq!(response.0, 200);
        assert_eq!(response.1, "application/json; charset=utf-8");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&response.2).unwrap(),
            serde_json::json!({ "project": DEN_PROJECT, "status": "ok" })
        );
    }

    fn response_json(response: (u16, &'static str, Vec<u8>)) -> serde_json::Value {
        assert_eq!(response.0, 200);
        serde_json::from_slice(&response.2).expect("browser response JSON")
    }

    #[test]
    fn browser_load_recovers_a_complete_pending_project_before_resolving_its_path() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "rusty-engine-browser-recovery-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let target = directory.join("recovered.project.json");
        let pending = ProjectStore::pending_path(&target).unwrap();
        let source = fs::read_to_string(default_project_path()).unwrap();
        let document = loading_bay_game::decode_project_document(&source)
            .unwrap()
            .project;
        let canonical = loading_bay_game::encode_project_document(&document).unwrap();
        fs::write(&pending, &canonical).unwrap();

        let runtime = BrowserRuntime::load(&target).expect("recover browser project");

        assert_eq!(runtime.project_path, target.canonicalize().unwrap());
        assert_eq!(fs::read_to_string(&target).unwrap(), canonical);
        assert!(!pending.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn legacy_gameplay_mutation_routes_are_inert() {
        let runtime = shared_browser_runtime();
        let tick_before = response_json(route("GET", "/api/state", &[], &runtime, Path::new(".")))
            ["tick"]
            .as_u64()
            .expect("tick");

        for path in [
            "/api/input/connect",
            "/api/input/disconnect",
            "/api/input-intent",
            "/api/input-edge",
            "/api/reset",
            "/api/motion-phase",
            "/api/navigation-step",
            "/api/navigation-phase",
            "/api/extraction-beacon/activate",
        ] {
            assert_eq!(
                route("POST", path, &[], &runtime, Path::new(".")).0,
                405,
                "{path} must not bypass the game loop"
            );
        }
        assert_eq!(
            response_json(route("GET", "/api/state", &[], &runtime, Path::new(".")))["tick"],
            tick_before
        );
    }

    #[test]
    fn authored_restart_replaces_the_runtime_only_after_fixed_tick_consumption() {
        let mut host = stored_browser_runtime();
        let host_session_id = host.host_session_id.clone();
        let mut defeated_snapshot: serde_json::Value = serde_json::from_str(
            &loading_bay_game::encode_game_snapshot(host.runtime.runtime()).unwrap(),
        )
        .unwrap();
        let player_health = defeated_snapshot["health"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|health| health["entity"] == ACTOR.raw())
            .unwrap();
        player_health["current"] = 0.into();
        player_health["armor"] = 50.into();
        player_health["armorItem"] = "armor/impact-vest".into();
        player_health["state"] = "dead".into();
        host.runtime = LoadingBayGameLoop::new(
            loading_bay_game::decode_game_snapshot(&defeated_snapshot.to_string()).unwrap(),
            ACTOR,
        )
        .unwrap();
        let generation = host.start_browser_connection();
        host.runtime
            .submit_edge_command(loading_bay_game::GameLoopEdgeCommand {
                connection_generation: generation,
                sequence: 1,
                command: loading_bay_game::GameLoopEdgeCommandKind::RestartAuthoredBaseline,
            })
            .unwrap();
        host.stage_restart(generation, 1, stored_browser_runtime());

        assert_eq!(
            host.runtime.input_session().connection_generation,
            generation
        );
        assert!(host.replacement_origin.is_none());

        let tick = host.runtime.run_fixed_tick().unwrap();
        assert!(tick.facts.contains(&GameLoopFact::RestartRequested {
            sequence: 1,
            mode: loading_bay_game::GameRestartMode::AuthoredBaseline,
        }));
        assert_eq!(
            host.runtime.input_session().connection_generation,
            generation
        );

        assert!(host.apply_consumed_restart(1));
        assert_eq!(host.host_session_id, host_session_id);
        let replacement_generation = host.runtime.input_session().connection_generation;
        assert!(replacement_generation > generation);
        let restarted_health = host.session().health(ACTOR).unwrap();
        assert_eq!(restarted_health.current, restarted_health.config.max);
        assert_eq!(restarted_health.armor, 0);
        assert_eq!(
            restarted_health.state,
            loading_bay_game::VitalityState::Alive
        );
        assert!(host
            .session()
            .hazards()
            .all(|hazard| hazard.ready_at_tick == core_time::Tick::ZERO));
        assert_eq!(
            host.adopt_consumed_restart(generation, 1),
            Some(replacement_generation)
        );
        assert!(host.replacement_origin.is_none());
    }

    #[test]
    fn independent_host_loads_have_distinct_continuity_identities() {
        let first = stored_browser_runtime();
        let second = stored_browser_runtime();

        assert!(!first.host_session_id.is_empty());
        assert_ne!(first.host_session_id, second.host_session_id);
    }

    #[test]
    fn save_survives_a_fresh_host_and_loads_only_after_fixed_tick_consumption() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-browser-save-{}-{unique}",
            std::process::id()
        ));
        let project_path = default_project_path();
        let mut first = BrowserRuntime::load_with_save_root(&project_path, &save_root).unwrap();
        let first_host_session = first.host_session_id.clone();
        let generation = first.start_browser_connection();
        first
            .runtime
            .submit_edge_command(loading_bay_game::GameLoopEdgeCommand {
                connection_generation: generation,
                sequence: 1,
                command: loading_bay_game::GameLoopEdgeCommandKind::SaveGame {
                    slot: SaveSlotId::Slot1,
                },
            })
            .unwrap();
        first.stage_save(generation, 1, SaveSlotId::Slot1, false, None);
        let tick = first.runtime.run_fixed_tick().unwrap();
        assert!(tick.facts.contains(&GameLoopFact::SaveRequested {
            sequence: 1,
            slot: SaveSlotId::Slot1,
        }));
        assert!(first.apply_consumed_save(1, SaveSlotId::Slot1));
        let expected = loading_bay_game::encode_game_snapshot(first.runtime.runtime()).unwrap();
        assert_eq!(
            first
                .save_slots
                .iter()
                .find(|slot| slot.slot == SaveSlotId::Slot1)
                .unwrap()
                .compatibility,
            loading_bay_game::SaveSlotCompatibility::Available
        );
        drop(first);

        let mut fresh = BrowserRuntime::load_with_save_root(&project_path, &save_root).unwrap();
        assert_ne!(fresh.host_session_id, first_host_session);
        let saved = fresh
            .save_slots
            .iter()
            .find(|slot| slot.slot == SaveSlotId::Slot1)
            .unwrap()
            .clone();
        let loaded = fresh
            .save_store
            .load(
                &fresh.save_identity,
                loading_bay_game::SaveLoadRequest {
                    slot: SaveSlotId::Slot1,
                    expected_storage_revision: saved.storage_revision,
                },
            )
            .unwrap();
        assert_eq!(
            loading_bay_game::encode_game_snapshot(&loaded.runtime).unwrap(),
            expected
        );
        let replacement = fresh.replacement_from_runtime(loaded.runtime).unwrap();
        let generation = fresh.start_browser_connection();
        fresh
            .runtime
            .submit_edge_command(loading_bay_game::GameLoopEdgeCommand {
                connection_generation: generation,
                sequence: 1,
                command: loading_bay_game::GameLoopEdgeCommandKind::LoadGame {
                    slot: SaveSlotId::Slot1,
                },
            })
            .unwrap();
        fresh.stage_restart(generation, 1, replacement);
        let tick = fresh.runtime.run_fixed_tick().unwrap();
        assert!(tick.facts.contains(&GameLoopFact::LoadRequested {
            sequence: 1,
            slot: SaveSlotId::Slot1,
        }));
        assert!(fresh.apply_consumed_restart(1));
        assert_eq!(
            loading_bay_game::encode_game_snapshot(fresh.runtime.runtime()).unwrap(),
            expected
        );
        assert!(fresh.adopt_consumed_restart(generation, 1).is_some());
        fs::remove_dir_all(save_root).unwrap();
    }

    #[test]
    fn elapsed_catch_up_saves_the_exact_command_consumption_tick() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-browser-save-tick-{}-{unique}",
            std::process::id()
        ));
        let project_path = default_project_path();
        let mut host = BrowserRuntime::load_with_save_root(&project_path, &save_root).unwrap();
        let generation = host.start_browser_connection();
        host.runtime
            .submit_edge_command(loading_bay_game::GameLoopEdgeCommand {
                connection_generation: generation,
                sequence: 1,
                command: loading_bay_game::GameLoopEdgeCommandKind::SaveGame {
                    slot: SaveSlotId::Slot1,
                },
            })
            .unwrap();
        host.stage_save(generation, 1, SaveSlotId::Slot1, false, None);

        let receipt = host
            .runtime
            .advance_elapsed(loading_bay_game::FIXED_STEP_DURATION.saturating_mul(3))
            .unwrap();
        assert_eq!(receipt.fixed_ticks.len(), 1);
        assert_eq!(receipt.fixed_ticks[0].simulation_tick, 1);
        assert!(receipt.fixed_ticks[0]
            .facts
            .contains(&GameLoopFact::SaveRequested {
                sequence: 1,
                slot: SaveSlotId::Slot1,
            }));
        assert!(host.apply_consumed_save(1, SaveSlotId::Slot1));

        let saved = host
            .save_store
            .load(
                &host.save_identity,
                loading_bay_game::SaveLoadRequest {
                    slot: SaveSlotId::Slot1,
                    expected_storage_revision: None,
                },
            )
            .unwrap();
        assert_eq!(saved.summary.metadata.as_ref().unwrap().tick, 1);
        assert_eq!(saved.runtime.tick().raw(), 1);

        let remainder = host.runtime.advance_elapsed(Duration::ZERO).unwrap();
        assert_eq!(remainder.fixed_ticks.len(), 2);
        assert_eq!(host.runtime.runtime().tick().raw(), 3);
        assert_eq!(
            host.save_store
                .load(
                    &host.save_identity,
                    loading_bay_game::SaveLoadRequest {
                        slot: SaveSlotId::Slot1,
                        expected_storage_revision: None,
                    },
                )
                .unwrap()
                .runtime
                .tick()
                .raw(),
            1
        );
        fs::remove_dir_all(save_root).unwrap();
    }

    #[test]
    fn voxel_edit_route_reports_only_after_coherent_rebuild_and_rejects_atomically() {
        let runtime = shared_browser_runtime();
        let before = response_json(route("GET", "/api/state", &[], &runtime, Path::new(".")));
        let before_revision = before["voxelRevision"].as_u64().unwrap();
        let stale = serde_json::to_vec(&serde_json::json!({
            "expectedRevision": before_revision + 1,
            "persistToProject": false,
            "edits": [{ "kind": "clear", "address": [2, 1, 6] }]
        }))
        .unwrap();
        assert_eq!(
            route("POST", "/api/voxel-edit", &stale, &runtime, Path::new(".")).0,
            409
        );
        let after_rejection =
            response_json(route("GET", "/api/state", &[], &runtime, Path::new(".")));
        for field in [
            "voxelRevision",
            "voxelAuthorityHash",
            "voxelSolidCount",
            "voxelNavigationHash",
            "voxelProbePathLength",
            "voxelMeshes",
        ] {
            assert_eq!(after_rejection[field], before[field], "changed {field}");
        }

        let clear = serde_json::to_vec(&serde_json::json!({
            "expectedRevision": before_revision,
            "persistToProject": false,
            "edits": [{ "kind": "clear", "address": [2, 1, 6] }]
        }))
        .unwrap();
        let edited = response_json(route(
            "POST",
            "/api/voxel-edit",
            &clear,
            &runtime,
            Path::new("."),
        ));
        assert_eq!(edited["voxelRevision"], before_revision + 1);
        assert_eq!(
            edited["voxelEditReceipt"]["acceptedRevision"],
            before_revision + 1
        );
        assert_eq!(edited["voxelEditReceipt"]["changedVoxels"], 1);
        assert_eq!(edited["voxelEditReceipt"]["persistedToProject"], false);
        assert_eq!(edited["generatedEnvironment"], serde_json::Value::Null);
        assert_eq!(
            edited["voxelSolidCount"].as_u64(),
            before["voxelSolidCount"].as_u64().map(|count| count - 1)
        );
        assert_ne!(edited["voxelAuthorityHash"], before["voxelAuthorityHash"]);
        assert_ne!(edited["voxelNavigationHash"], before["voxelNavigationHash"]);
        assert_ne!(edited["voxelMeshes"], before["voxelMeshes"]);
    }

    #[test]
    fn state_rebuilds_posture_without_replaying_transient_cues() {
        let runtime = shared_browser_runtime();

        let value = response_json(route("GET", "/api/state", &[], &runtime, Path::new(".")));
        assert_eq!(value["presentation"]["cues"], serde_json::json!([]));
        assert_eq!(
            value["presentation"]["animationStates"]
                .as_array()
                .expect("animation states")
                .len(),
            11
        );
        assert!(value["presentation"]["animationStates"]
            .as_array()
            .expect("animation states")
            .iter()
            .any(|state| state["entity"] == BEACON.raw()));
    }

    #[test]
    fn state_projects_progression_and_rust_owned_interaction_prompt() {
        let project_path = default_project_path();
        let mut project: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(project_path).unwrap()).unwrap();
        let player = project["scenes"][0]["entities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entity| entity["id"] == ACTOR.raw())
            .unwrap();
        player["translation"] = serde_json::json!([11.5, 1.5, 15.5]);
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("loading-bay-progression-{unique}.project.json"));
        fs::write(&path, serde_json::to_vec_pretty(&project).unwrap()).unwrap();

        let without_key = BrowserRuntime::load(&path).unwrap();
        let state = serde_json::to_value(browser_state(
            &without_key,
            Vec::new(),
            BrowserFeedbackProjection::default(),
        ))
        .unwrap();
        assert_eq!(state["doorAccess"].as_array().unwrap().len(), 1);
        assert_eq!(state["secretRegions"].as_array().unwrap().len(), 1);
        assert_eq!(state["levelExits"].as_array().unwrap().len(), 1);
        assert_eq!(state["levelComplete"], false);
        assert_eq!(state["interaction"]["target"], 30);
        assert_eq!(state["interaction"]["prompt"], "Maintenance pass required");

        project["scenes"][0]["entities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entity| entity["id"] == ACTOR.raw())
            .unwrap()["inventory"]["startingStacks"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "item": "key/maintenance-pass",
                "quantity": 1
            }));
        fs::write(&path, serde_json::to_vec_pretty(&project).unwrap()).unwrap();
        let with_key = BrowserRuntime::load(&path).unwrap();
        let state = serde_json::to_value(browser_state(
            &with_key,
            Vec::new(),
            BrowserFeedbackProjection::default(),
        ))
        .unwrap();
        assert_eq!(state["interaction"]["target"], 30);
        assert_eq!(state["interaction"]["prompt"], "Open maintenance bulkhead");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn legacy_project_without_player_vitality_keeps_a_neutral_browser_projection() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/converted-wall.project.json");
        let runtime = BrowserRuntime::load(&project).expect("admit legacy converted project");

        assert!(runtime.session().health(ACTOR).is_none());
        let state = serde_json::to_value(browser_state(
            &runtime,
            Vec::new(),
            BrowserFeedbackProjection::default(),
        ))
        .unwrap();
        assert_eq!(state["player"]["currentHealth"], 0);
        assert_eq!(state["player"]["maxHealth"], 0);
        assert_eq!(state["player"]["armor"], 0);
        assert_eq!(state["player"]["maxArmor"], 0);
        assert_eq!(state["player"]["vitalityState"], "alive");
        assert_eq!(
            state["enemies"][0]["combatPosture"],
            serde_json::Value::Null
        );
        assert_eq!(state["enemies"][0]["attackKind"], serde_json::Value::Null);
        assert!(runtime.session().health(ACTOR).is_none());
    }

    #[test]
    fn presentation_projection_cannot_change_authoritative_snapshot() {
        let stored = stored_browser_runtime();
        let before =
            loading_bay_game::encode_game_snapshot(&stored).expect("snapshot before projection");
        let mut feedback = BrowserFeedbackProjection::default();
        feedback.extend_events(&[GameEvent::DoorOpened {
            door: EXIT,
            entity_facts: Vec::new(),
        }]);

        let state = browser_state(&stored, vec!["DoorOpened".to_owned()], feedback);

        assert_eq!(state.dynamic.last_events, ["DoorOpened"]);
        assert_eq!(
            loading_bay_game::encode_game_snapshot(&stored).expect("snapshot after projection"),
            before
        );
    }

    #[test]
    fn disposable_locomotion_feedback_is_sampled_from_authoritative_ticks() {
        assert!(emits_locomotion_feedback(0));
        assert!(!emits_locomotion_feedback(1));
        assert!(!emits_locomotion_feedback(5));
        assert!(emits_locomotion_feedback(6));
        assert!(emits_locomotion_feedback(60));
    }
}
