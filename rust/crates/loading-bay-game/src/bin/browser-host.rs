use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::{Deref, DerefMut};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Condvar, LockResult, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use core_ids::EntityId;
use loading_bay_game::{
    admit_stored_project_with_document, materialize_stored_project_voxels, AdmittedStoredProject,
    CombatFact, CombatMissReason, GameEvent, GameLoopEdgeCommand, GameLoopFact, GameRuntime,
    InputCommandRejection, LoadingBayGameLoop, NavigationFact, PlayerControlFact,
    PlayerInputCommand, ProjectSaveMode, ProjectStore, VoxelEdit, VoxelEditTransaction,
    VoxelSourceRevision,
};
use serde::{Deserialize, Serialize};

#[path = "browser_host/presentation.rs"]
mod presentation;
#[path = "browser_host/state.rs"]
mod state;

use presentation::BrowserFeedbackProjection;
use state::{browser_state, BrowserState};

const DEFAULT_ADDRESS: &str = "127.0.0.1:8787";
const DEN_PROJECT: &str = "rusty-engine-demo";
const ACTOR: EntityId = EntityId::new(1);
const BEACON: EntityId = EntityId::new(7);
const ENCOUNTER: EntityId = EntityId::new(2);
const EXIT: EntityId = EntityId::new(3);
const FIRST_ENEMY: u64 = 4;
const SECOND_ENEMY: u64 = 5;
const MOTION_PROBE: EntityId = EntityId::new(10);
const INPUT_ACK_WAIT: Duration = Duration::from_millis(100);

#[derive(Debug)]
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
    runtime: LoadingBayGameLoop,
    authored: AdmittedStoredProject,
    project_path: PathBuf,
    project: BrowserProjectSummary,
}

impl BrowserRuntime {
    fn load(path: &Path) -> Result<Self, String> {
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
        Ok(Self {
            runtime: LoadingBayGameLoop::new(GameRuntime::from_admitted_project(admitted), ACTOR)
                .map_err(|error| format!("could not create Loading Bay game loop: {error}"))?,
            authored,
            project_path,
            project,
        })
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
    advanced: Condvar,
    automatic_driver: bool,
}

impl SharedBrowserRuntime {
    fn new(runtime: BrowserRuntime, automatic_driver: bool) -> Self {
        Self {
            runtime: Mutex::new(runtime),
            advanced: Condvar::new(),
            automatic_driver,
        }
    }

    fn lock(&self) -> LockResult<MutexGuard<'_, BrowserRuntime>> {
        self.runtime.lock()
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BrowserDisconnectRequest {
    connection_generation: u64,
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
    let (address, dist, project_path) = arguments();
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

    let runtime = BrowserRuntime::load(&project_path)
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
    let runtime = Arc::new(SharedBrowserRuntime::new(runtime, true));
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
            let result = runtime
                .lock()
                .expect("runtime lock")
                .runtime
                .advance_elapsed(elapsed);
            if let Err(error) = result {
                eprintln!("browser-host fixed game loop error: {error}");
            }
            runtime.advanced.notify_all();
        }
    });
}

fn arguments() -> (String, PathBuf, PathBuf) {
    let default_dist =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../dist/apps/loading-bay/browser");
    let mut address = DEFAULT_ADDRESS.to_owned();
    let mut dist = default_dist;
    let mut project = default_project_path();
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--addr" => address = args.next().expect("--addr needs a value"),
            "--dist" => dist = PathBuf::from(args.next().expect("--dist needs a value")),
            "--project" => {
                project = PathBuf::from(args.next().expect("--project needs a value"));
            }
            _ => panic!("unknown browser-host argument {argument}"),
        }
    }
    (address, dist, project)
}

fn default_project_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../content/projects/loading-bay.project.json")
}

fn handle_connection(mut stream: TcpStream, runtime: &Arc<SharedBrowserRuntime>, dist: &Path) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
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
        ("POST", "/api/input/connect") => {
            let mut runtime = runtime.lock().expect("runtime lock");
            runtime.runtime.start_connection();
            json_response(
                200,
                browser_state(&runtime, Vec::new(), BrowserFeedbackProjection::default()),
            )
        }
        ("POST", "/api/input/disconnect") => {
            let request: BrowserDisconnectRequest = match serde_json::from_slice(body) {
                Ok(request) => request,
                Err(error) => {
                    return error_json(400, &format!("invalid disconnect request: {error}"));
                }
            };
            let mut runtime = runtime.lock().expect("runtime lock");
            if !runtime.runtime.disconnect(request.connection_generation) {
                return error_json(409, "sessionClosed: connection generation is not active");
            }
            json_response(
                200,
                browser_state(&runtime, Vec::new(), BrowserFeedbackProjection::default()),
            )
        }
        ("POST", "/api/input-intent") => {
            let command: PlayerInputCommand = match serde_json::from_slice(body) {
                Ok(command) => command,
                Err(error) => return error_json(400, &format!("invalid input intent: {error}")),
            };
            {
                let mut runtime = runtime.lock().expect("runtime lock");
                if let Err(rejection) = runtime.runtime.submit_input(command) {
                    return input_rejection_json(rejection);
                }
            }
            wait_for_input_consumption(runtime, command.connection_generation, command.sequence);
            let mut runtime = runtime.lock().expect("runtime lock");
            let (facts, feedback) = drain_game_loop_feedback(&mut runtime.runtime);
            json_response(200, browser_state(&runtime, facts, feedback))
        }
        ("POST", "/api/input-edge") => {
            let command: GameLoopEdgeCommand = match serde_json::from_slice(body) {
                Ok(command) => command,
                Err(error) => return error_json(400, &format!("invalid input edge: {error}")),
            };
            {
                let mut runtime = runtime.lock().expect("runtime lock");
                if let Err(rejection) = runtime.runtime.submit_edge_command(command) {
                    return input_rejection_json(rejection);
                }
            }
            wait_for_input_consumption(runtime, command.connection_generation, command.sequence);
            let mut runtime = runtime.lock().expect("runtime lock");
            let (facts, feedback) = drain_game_loop_feedback(&mut runtime.runtime);
            json_response(200, browser_state(&runtime, facts, feedback))
        }
        ("POST", "/api/reset") => {
            let mut runtime = runtime.lock().expect("runtime lock");
            let project_path = runtime.project_path.clone();
            let previous_generation = runtime.runtime.input_session().connection_generation;
            let mut replacement =
                BrowserRuntime::load(&project_path).expect("reset stored browser project");
            replacement
                .runtime
                .start_connection_after(previous_generation);
            *runtime = replacement;
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

fn wait_for_input_consumption(
    runtime: &Arc<SharedBrowserRuntime>,
    connection_generation: u64,
    sequence: u64,
) {
    if !runtime.automatic_driver {
        return;
    }
    let guard = runtime.lock().expect("runtime lock");
    let _ = runtime
        .advanced
        .wait_timeout_while(guard, INPUT_ACK_WAIT, |runtime| {
            let input = runtime.runtime.input_session();
            input.connection_generation == connection_generation
                && input.connected
                && input.consumed_sequence < sequence
        })
        .expect("runtime input wait");
}

fn input_rejection_json(rejection: InputCommandRejection) -> (u16, &'static str, Vec<u8>) {
    let (status, code) = match rejection {
        InputCommandRejection::InvalidInput => (400, "invalidInput"),
        InputCommandRejection::SessionDisconnected
        | InputCommandRejection::WrongConnectionGeneration { .. } => (409, "sessionClosed"),
        InputCommandRejection::StaleSequence { .. } => (409, "staleSequence"),
        InputCommandRejection::EdgeQueueSaturated { .. } => (409, "edgeQueueSaturated"),
    };
    error_json(status, &format!("{code}: {rejection}"))
}

fn drain_game_loop_feedback(
    game_loop: &mut LoadingBayGameLoop,
) -> (Vec<String>, BrowserFeedbackProjection) {
    let mut names = Vec::new();
    let mut feedback = BrowserFeedbackProjection::default();
    for fact in game_loop.drain_pending_facts() {
        match fact {
            GameLoopFact::PlayerControl(fact) => {
                names.push(player_fact_name(&fact).to_owned());
                feedback.extend_player_control(std::slice::from_ref(&fact));
            }
            GameLoopFact::Navigation(fact) => {
                names.push(navigation_fact_name(&fact).to_owned());
                feedback.extend_navigation(std::slice::from_ref(&fact));
            }
            GameLoopFact::Combat(fact) => {
                names.push(combat_fact_name(&fact).to_owned());
                feedback.extend_combat(std::slice::from_ref(&fact));
            }
            GameLoopFact::ExtractionBeacon(fact) => {
                names.push("ExtractionBeaconActivated".to_owned());
                feedback.extend_extraction_beacon(fact);
            }
            GameLoopFact::Event(event) => {
                names.push(event_name(&event).to_owned());
                feedback.extend_events(std::slice::from_ref(&event));
            }
            GameLoopFact::CombatRejected { reason } => names.push(
                match reason {
                    loading_bay_game::CombatRejectionReason::Cooldown => "CombatRejectedCooldown",
                    loading_bay_game::CombatRejectionReason::NoAmmo => "CombatRejectedNoAmmo",
                }
                .to_owned(),
            ),
            GameLoopFact::EdgeCommandRejected { reason, .. } => names.push(
                match reason {
                    loading_bay_game::EdgeCommandRejection::Paused => "InputEdgeRejectedPaused",
                    loading_bay_game::EdgeCommandRejection::UnknownTarget => {
                        "InputEdgeRejectedUnknownTarget"
                    }
                    loading_bay_game::EdgeCommandRejection::NotInteractable => {
                        "InputEdgeRejectedNotInteractable"
                    }
                }
                .to_owned(),
            ),
            GameLoopFact::InputExpired { .. } => names.push("InputExpired".to_owned()),
        }
    }
    (names, feedback)
}

fn combat_fact_name(fact: &CombatFact) -> &'static str {
    match fact {
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
        CombatFact::DamageApplied { .. } => "DamageApplied",
        CombatFact::EnemyDefeated { .. } => "CombatEnemyDefeated",
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
        Arc::new(SharedBrowserRuntime::new(stored_browser_runtime(), false))
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
    fn input_routes_preserve_generation_sequence_and_disconnect_identity() {
        let runtime = shared_browser_runtime();
        let connected = response_json(route(
            "POST",
            "/api/input/connect",
            &[],
            &runtime,
            Path::new("."),
        ));
        let generation = connected["input"]["connectionGeneration"]
            .as_u64()
            .expect("connection generation");
        let command = serde_json::to_vec(&PlayerInputCommand {
            connection_generation: generation,
            sequence: 1,
            intent: loading_bay_game::PlayerInputIntent {
                movement: [1.0, 0.0],
                look_delta: [0.25, 0.0],
                primary_fire_held: false,
            },
        })
        .unwrap();
        let accepted = response_json(route(
            "POST",
            "/api/input-intent",
            &command,
            &runtime,
            Path::new("."),
        ));
        assert_eq!(accepted["input"]["acknowledgedSequence"], 1);
        assert_eq!(accepted["input"]["consumedSequence"], 0);

        runtime.lock().unwrap().runtime.run_fixed_tick().unwrap();
        let advanced = response_json(route("GET", "/api/state", &[], &runtime, Path::new(".")));
        assert_eq!(advanced["input"]["consumedSequence"], 1);
        assert_ne!(
            advanced["player"]["position"],
            connected["player"]["position"]
        );
        assert_ne!(
            advanced["player"]["yawDegrees"],
            connected["player"]["yawDegrees"]
        );

        let disconnect = serde_json::to_vec(&BrowserDisconnectRequest {
            connection_generation: generation,
        })
        .unwrap();
        assert_eq!(
            route(
                "POST",
                "/api/input/disconnect",
                &disconnect,
                &runtime,
                Path::new(".")
            )
            .0,
            200
        );
        assert_eq!(
            route(
                "POST",
                "/api/input-intent",
                &command,
                &runtime,
                Path::new(".")
            )
            .0,
            409
        );
    }

    #[test]
    fn host_gameplay_edges_wait_for_the_fixed_tick_and_legacy_phase_routes_are_inert() {
        let runtime = shared_browser_runtime();
        let connected = response_json(route(
            "POST",
            "/api/input/connect",
            &[],
            &runtime,
            Path::new("."),
        ));
        let generation = connected["input"]["connectionGeneration"]
            .as_u64()
            .expect("connection generation");
        let tick_before = connected["tick"].as_u64().expect("tick");

        for path in [
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

        let pause = serde_json::to_vec(&GameLoopEdgeCommand {
            connection_generation: generation,
            sequence: 1,
            command: loading_bay_game::GameLoopEdgeCommandKind::SetPaused { paused: true },
        })
        .unwrap();
        let queued = response_json(route(
            "POST",
            "/api/input-edge",
            &pause,
            &runtime,
            Path::new("."),
        ));
        assert_eq!(queued["tick"], tick_before);
        assert_eq!(queued["input"]["queuedEdgeCommands"], 1);
        assert_eq!(queued["input"]["consumedSequence"], 0);
        assert_eq!(queued["input"]["paused"], false);

        let receipt = runtime.lock().unwrap().runtime.run_fixed_tick().unwrap();
        assert!(!receipt.simulation_advanced);
        let consumed = response_json(route("GET", "/api/state", &[], &runtime, Path::new(".")));
        assert_eq!(consumed["tick"], tick_before);
        assert_eq!(consumed["input"]["queuedEdgeCommands"], 0);
        assert_eq!(consumed["input"]["consumedSequence"], 1);
        assert_eq!(consumed["input"]["paused"], true);
    }

    #[test]
    fn voxel_edit_route_reports_only_after_coherent_rebuild_and_rejects_atomically() {
        let runtime = shared_browser_runtime();
        let before = response_json(route("GET", "/api/state", &[], &runtime, Path::new(".")));
        let stale = serde_json::to_vec(&serde_json::json!({
            "expectedRevision": 1,
            "persistToProject": false,
            "edits": [{ "kind": "clear", "address": [4, 1, 6] }]
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
            "expectedRevision": 0,
            "persistToProject": false,
            "edits": [{ "kind": "clear", "address": [4, 1, 6] }]
        }))
        .unwrap();
        let edited = response_json(route(
            "POST",
            "/api/voxel-edit",
            &clear,
            &runtime,
            Path::new("."),
        ));
        assert_eq!(edited["voxelRevision"], 1);
        assert_eq!(edited["voxelEditReceipt"]["acceptedRevision"], 1);
        assert_eq!(edited["voxelEditReceipt"]["changedVoxels"], 1);
        assert_eq!(edited["voxelEditReceipt"]["persistedToProject"], false);
        assert_eq!(edited["generatedEnvironment"], serde_json::Value::Null);
        assert_eq!(
            edited["voxelSolidCount"].as_u64(),
            before["voxelSolidCount"].as_u64().map(|count| count - 1)
        );
        assert_ne!(edited["voxelAuthorityHash"], before["voxelAuthorityHash"]);
        assert_ne!(edited["voxelNavigationHash"], before["voxelNavigationHash"]);
        assert!(
            edited["voxelProbePathLength"].as_u64().unwrap()
                < before["voxelProbePathLength"].as_u64().unwrap()
        );
        assert_ne!(edited["voxelMeshes"], before["voxelMeshes"]);

        let reset = response_json(route("POST", "/api/reset", &[], &runtime, Path::new(".")));
        assert_eq!(reset["voxelRevision"], before["voxelRevision"]);
        assert_eq!(reset["voxelAuthorityHash"], before["voxelAuthorityHash"]);
        assert_eq!(reset["voxelNavigationHash"], before["voxelNavigationHash"]);
        assert_eq!(reset["voxelMeshes"], before["voxelMeshes"]);
    }

    #[test]
    fn state_and_reset_rebuild_posture_without_replaying_transient_cues() {
        let runtime = shared_browser_runtime();

        for response in [
            route("GET", "/api/state", &[], &runtime, Path::new(".")),
            route("POST", "/api/reset", &[], &runtime, Path::new(".")),
        ] {
            let value = response_json(response);
            assert_eq!(value["presentation"]["cues"], serde_json::json!([]));
            assert_eq!(
                value["presentation"]["animationStates"]
                    .as_array()
                    .expect("animation states")
                    .len(),
                5
            );
            assert!(value["presentation"]["animationStates"]
                .as_array()
                .expect("animation states")
                .iter()
                .any(|state| state["entity"] == BEACON.raw()));
        }
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

        assert_eq!(state.last_events, ["DoorOpened"]);
        assert_eq!(
            loading_bay_game::encode_game_snapshot(&stored).expect("snapshot after projection"),
            before
        );
    }

    #[test]
    fn dropped_response_feedback_is_not_replayed_and_does_not_change_outcome() {
        let first = shared_browser_runtime();
        let second = shared_browser_runtime();
        for runtime in [&first, &second] {
            response_json(route(
                "POST",
                "/api/input/connect",
                &[],
                runtime,
                Path::new("."),
            ));
            let movement = serde_json::to_vec(&PlayerInputCommand {
                connection_generation: 1,
                sequence: 1,
                intent: loading_bay_game::PlayerInputIntent {
                    movement: [1.0, 0.0],
                    look_delta: [0.0, 0.0],
                    primary_fire_held: false,
                },
            })
            .unwrap();
            response_json(route(
                "POST",
                "/api/input-intent",
                &movement,
                runtime,
                Path::new("."),
            ));
            runtime.lock().unwrap().runtime.run_fixed_tick().unwrap();
        }
        let neutral = serde_json::to_vec(&PlayerInputCommand {
            connection_generation: 1,
            sequence: 2,
            intent: loading_bay_game::PlayerInputIntent::NEUTRAL,
        })
        .unwrap();
        let delivered = response_json(route(
            "POST",
            "/api/input-intent",
            &neutral,
            &first,
            Path::new("."),
        ));
        let dropped = route(
            "POST",
            "/api/input-intent",
            &neutral,
            &second,
            Path::new("."),
        );
        assert_eq!(dropped.0, 200);
        assert_eq!(delivered["presentation"]["cues"][0]["kind"], "movement");

        let refreshed = response_json(route("GET", "/api/state", &[], &second, Path::new(".")));
        assert_eq!(refreshed["presentation"]["cues"], serde_json::json!([]));
        assert_eq!(
            loading_bay_game::encode_game_snapshot(&first.lock().expect("first runtime"))
                .expect("first snapshot"),
            loading_bay_game::encode_game_snapshot(&second.lock().expect("second runtime"))
                .expect("second snapshot")
        );
    }
}
