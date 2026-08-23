use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LockResult, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use loading_bay_game::{
    browser_adapter::{browser_state, BrowserFeedbackProjection, BrowserRuntime},
    SaveSlotSummary, StoredAsset, StoredImportSource, StoredProject,
};
use rusty_engine::core_ids::EntityId;
use serde::Serialize;

#[cfg(test)]
use loading_bay_game::{
    browser_adapter::{drain_projection_feedback, emits_locomotion_feedback},
    GameEvent, GameLoopFact, ProjectStore,
};

#[path = "browser_host/developer_command.rs"]
mod developer_command;
#[path = "browser_host/session.rs"]
mod session;

use developer_command::{developer_upgrade_requested, run_developer_session};
use session::{run_game_session, session_upgrade_requested};

const DEFAULT_ADDRESS: &str = "127.0.0.1:8787";
const DEN_PROJECT: &str = "rusty-engine-demo";
const ACTOR: EntityId = EntityId::new(1);
#[cfg(test)]
const EXIT: EntityId = EntityId::new(3);

#[derive(Debug)]
struct SharedBrowserRuntime {
    runtime: Mutex<BrowserRuntime>,
    consumed_command_sequence: AtomicU64,
    projection_sequence: AtomicU64,
}

impl SharedBrowserRuntime {
    fn new(runtime: BrowserRuntime) -> Self {
        Self {
            runtime: Mutex::new(runtime),
            consumed_command_sequence: AtomicU64::new(0),
            projection_sequence: AtomicU64::new(0),
        }
    }

    fn lock(&self) -> LockResult<MutexGuard<'_, BrowserRuntime>> {
        self.runtime.lock()
    }

    fn projection_sequence(&self) -> u64 {
        self.projection_sequence.load(Ordering::Relaxed)
    }

    fn mark_projection_changed(&self) {
        self.projection_sequence.fetch_add(1, Ordering::Relaxed);
    }

    fn consumed_command_sequence(&self) -> u64 {
        self.consumed_command_sequence.load(Ordering::Relaxed)
    }

    fn set_consumed_command_sequence(&self, sequence: u64) {
        self.consumed_command_sequence
            .store(sequence, Ordering::Relaxed);
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserMenuState {
    host_session_id: String,
    project_id: String,
    save_slots: Vec<SaveSlotSummary>,
}

fn browser_menu_state(runtime: &BrowserRuntime) -> BrowserMenuState {
    BrowserMenuState {
        host_session_id: runtime.host_session_id().to_owned(),
        project_id: runtime.project().project_id.clone(),
        save_slots: runtime.save_slots().to_vec(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BrowserHostArguments {
    address: SocketAddr,
    dist: PathBuf,
    project: PathBuf,
    save_root: PathBuf,
}

fn main() {
    let arguments = arguments().unwrap_or_else(|error| panic!("{error}"));
    let dist = arguments.dist.canonicalize().unwrap_or_else(|error| {
        panic!(
            "browser shell dist {} is unavailable: {error}",
            arguments.dist.display()
        )
    });
    assert!(
        dist.join("index.html").is_file(),
        "browser shell is not built"
    );

    let runtime = BrowserRuntime::load(&arguments.project, &arguments.save_root)
        .unwrap_or_else(|error| panic!("could not start browser project: {error}"));
    println!(
        "browser-host project id={} sourceSchema={} currentSchema={} entryScene={} assets={} scenes={} entities={} path={}",
        runtime.project().project_id,
        runtime.project().source_schema_version,
        runtime.project().current_schema_version,
        runtime.project().entry_scene,
        runtime.project().asset_count,
        runtime.project().scene_count,
        runtime.project().entity_count,
        runtime.project_path().display()
    );
    let runtime = Arc::new(SharedBrowserRuntime::new(runtime));
    start_game_loop_driver(&runtime);
    let listener = TcpListener::bind(arguments.address).unwrap_or_else(|error| {
        panic!("cannot bind browser host at {}: {error}", arguments.address)
    });
    let address = listener
        .local_addr()
        .expect("bound browser-host listener has a local address");
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
            std::thread::sleep(Duration::from_millis(1));
            let now = Instant::now();
            let elapsed = now.saturating_duration_since(previous);
            previous = now;
            let mut host = runtime.lock().expect("runtime lock");
            // A browser product session owns whether this live simulation is in use.
            // Do not let enemies, hazards, or fact queues run ahead while no client is
            // connected, then dump that stale work into the next browser bootstrap.
            if !host.runtime().input_session().connected {
                continue;
            }
            match host.advance(elapsed) {
                Ok(receipt) => {
                    // Autonomous retained presentation needs no second clock:
                    // sample the authoritative 60 Hz driver at 30 Hz, while
                    // the session publisher separately wakes on every newly
                    // consumed command so acknowledgement is never throttled.
                    let projection_changed = receipt
                        .fixed_ticks
                        .iter()
                        .any(|tick| background_projection_due(tick.driver_tick));
                    if let Some(tick) = receipt.fixed_ticks.last() {
                        runtime.set_consumed_command_sequence(tick.consumed_sequence);
                    }
                    if projection_changed {
                        runtime.mark_projection_changed();
                    }
                }
                Err(error) => {
                    eprintln!("browser-host fixed game loop error: {error}");
                }
            }
        }
    });
}

fn arguments() -> Result<BrowserHostArguments, String> {
    parse_arguments(std::env::args().skip(1))
}

fn parse_arguments(
    arguments: impl IntoIterator<Item = impl Into<String>>,
) -> Result<BrowserHostArguments, String> {
    let default_dist =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../dist/apps/loading-bay/browser");
    let mut address = DEFAULT_ADDRESS
        .parse::<SocketAddr>()
        .expect("default browser-host address");
    let mut dist = default_dist;
    let mut project = default_project_path();
    let mut save_root = default_save_root();
    let mut args = arguments.into_iter().map(Into::into);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--addr" => {
                address = args
                    .next()
                    .ok_or_else(|| "--addr needs a value".to_owned())?
                    .parse()
                    .map_err(|error| format!("--addr must be a socket address: {error}"))?;
            }
            "--dist" => {
                dist = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--dist needs a value".to_owned())?,
                );
            }
            "--project" => {
                project = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--project needs a value".to_owned())?,
                );
            }
            "--save-root" => {
                save_root = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--save-root needs a value".to_owned())?,
                );
            }
            _ => return Err(format!("unknown browser-host argument {argument}")),
        }
    }
    Ok(BrowserHostArguments {
        address,
        dist,
        project,
        save_root,
    })
}

fn default_project_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../content/projects/doom-e1m1.project.json")
}

fn default_save_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.runtime/saves")
}

fn handle_connection(mut stream: TcpStream, runtime: &Arc<SharedBrowserRuntime>, dist: &Path) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    if developer_upgrade_requested(&stream) {
        run_developer_session(stream, Arc::clone(runtime));
        return;
    }
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
    _body: &[u8],
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
        ("GET", "/api/menu-state") => {
            let runtime = runtime.lock().expect("runtime lock");
            json_response(200, browser_menu_state(&runtime))
        }
        ("GET", path) if path.starts_with("/api/animated-mesh/") => {
            let Some(index) = path
                .strip_prefix("/api/animated-mesh/")
                .and_then(|value| value.parse::<usize>().ok())
            else {
                return error_json(404, "animated mesh resource not found");
            };
            let runtime = runtime.lock().expect("runtime lock");
            serve_animated_mesh_resource(&runtime, index)
        }
        ("GET", path) if path.starts_with("/api/application-resource/") => {
            let Some(index) = path
                .strip_prefix("/api/application-resource/")
                .and_then(|value| value.parse::<usize>().ok())
            else {
                return error_json(404, "application resource not found");
            };
            let runtime = runtime.lock().expect("runtime lock");
            serve_application_resource(&runtime, index)
        }
        ("GET", _) | ("HEAD", _) => serve_static(method, path, dist),
        _ => error_json(405, "method not allowed"),
    }
}

fn serve_application_resource(
    runtime: &BrowserRuntime,
    index: usize,
) -> (u16, &'static str, Vec<u8>) {
    let Some(resource) = runtime
        .application_content()
        .and_then(|content| content.resources.get(index))
    else {
        return error_json(404, "application resource not found");
    };
    let media_type = if resource.identity.starts_with("texture-resource/") {
        "image/png"
    } else {
        "application/octet-stream"
    };
    (200, media_type, resource.bytes.clone())
}

fn serve_animated_mesh_resource(
    runtime: &BrowserRuntime,
    index: usize,
) -> (u16, &'static str, Vec<u8>) {
    let Some(asset) = browser_animated_mesh_assets(runtime.authored_project().document())
        .get(index)
        .copied()
    else {
        return error_json(404, "animated mesh resource not found");
    };
    let Some(import) = asset.import.as_ref() else {
        return error_json(404, "animated mesh has no durable import source");
    };
    let StoredImportSource::Project { path } = &import.source else {
        return error_json(
            403,
            "host-local animated mesh sources are not browser-readable",
        );
    };
    let Some(source) = resolve_project_asset_source(runtime.project_path(), Path::new(path)) else {
        return error_json(404, "animated mesh project source is unavailable");
    };
    match fs::read(source) {
        Ok(bytes) => (200, "model/gltf-binary", bytes),
        Err(_) => error_json(500, "animated mesh project source could not be read"),
    }
}

fn browser_animated_mesh_assets(document: &StoredProject) -> Vec<&StoredAsset> {
    // Renderable asset identities live on Engine scene nodes.
    let referenced_assets = document
        .scenes
        .iter()
        .enumerate()
        .find(|(_, scene)| scene.id == document.entry_scene)
        .into_iter()
        .filter_map(|(scene_index, scene)| {
            loading_bay_game::decoded_authored_scene(scene, scene_index).ok()
        })
        .flat_map(|authored| authored.nodes)
        .filter_map(|node| match &node.kind {
            rusty_engine::authored_scene::SceneNodeKind::StaticMesh(asset)
            | rusty_engine::authored_scene::SceneNodeKind::AnimatedMesh(asset)
            | rusty_engine::authored_scene::SceneNodeKind::Sprite(asset) => {
                Some(asset.id().as_str().to_owned())
            }
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    document
        .assets
        .iter()
        .filter(|asset| {
            asset.animated_mesh.is_some() && referenced_assets.contains(asset.id.as_str())
        })
        .collect()
}

fn resolve_project_asset_source(project_file: &Path, source: &Path) -> Option<PathBuf> {
    if source.is_absolute()
        || source.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return None;
    }
    for ancestor in project_file.parent()?.ancestors() {
        let root = ancestor.canonicalize().ok()?;
        let candidate = root.join(source);
        if !candidate.is_file() {
            continue;
        }
        let canonical = candidate.canonicalize().ok()?;
        if canonical.starts_with(&root) {
            return Some(canonical);
        }
    }
    None
}

fn background_projection_due(driver_tick: u64) -> bool {
    driver_tick.is_multiple_of(2)
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
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'self'; script-src 'self'; script-src-attr 'unsafe-hashes' 'sha256-MhtPZXr7+LpJUY5qtMutB+qWfQtMaPccfe7QXtCcEYc='; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ws://127.0.0.1:*; font-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; worker-src 'self' blob:\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nX-Den-Project: {DEN_PROJECT}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_exit_encounter_clear_keeps_the_browser_hud_fact_name() {
        let projection = drain_projection_feedback(
            vec![GameLoopFact::Event(GameEvent::EncounterCleared {
                encounter: EntityId::new(2),
                exit: None,
            })],
            0,
        );
        assert_eq!(projection.facts, [("EncounterCleared".to_owned(), None)]);
    }

    fn stored_browser_runtime() -> BrowserRuntime {
        BrowserRuntime::load(&default_project_path(), &default_save_root())
            .expect("admit stored browser project")
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

    #[test]
    fn menu_state_exposes_only_rust_owned_continue_identity_and_slots() {
        let runtime = shared_browser_runtime();
        let response = route("GET", "/api/menu-state", &[], &runtime, Path::new("."));
        assert_eq!(response.0, 200);
        assert_eq!(response.1, "application/json; charset=utf-8");
        assert!(
            response.2.len() < 16 * 1_024,
            "menu state unexpectedly expanded to {} bytes",
            response.2.len()
        );
        let value: serde_json::Value = serde_json::from_slice(&response.2).unwrap();
        assert_eq!(
            value["hostSessionId"],
            runtime.lock().unwrap().host_session_id()
        );
        assert_eq!(value["saveSlots"].as_array().unwrap().len(), 4);
        assert_eq!(
            value.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["hostSessionId", "projectId", "saveSlots"]
        );
        assert!(value.get("voxelMeshes").is_none());
        assert!(value.get("projection").is_none());
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
        let target = directory.join("content/projects/recovered.project.json");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        copy_directory(
            &directory_for_test_resources().join("content/doom-e1m1/textures"),
            &directory.join("content/doom-e1m1/textures"),
        );
        copy_directory(
            &directory_for_test_resources().join("content/doom-e1m1/sprites"),
            &directory.join("content/doom-e1m1/sprites"),
        );
        let vitality =
            directory.join("data/gameplay/loading-bay-e1m1-standard-vitality.package.json");
        fs::create_dir_all(vitality.parent().unwrap()).unwrap();
        fs::copy(
            directory_for_test_resources()
                .join("data/gameplay/loading-bay-e1m1-standard-vitality.package.json"),
            vitality,
        )
        .unwrap();
        let pending = ProjectStore::pending_path(&target).unwrap();
        let source = fs::read_to_string(default_project_path()).unwrap();
        let document = loading_bay_game::decode_project_document(&source)
            .unwrap()
            .project;
        let canonical = loading_bay_game::encode_project_document(&document).unwrap();
        fs::write(&pending, &canonical).unwrap();

        let runtime =
            BrowserRuntime::load(&target, &default_save_root()).expect("recover browser project");

        assert_eq!(runtime.project_path(), target.canonicalize().unwrap());
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
            "/api/developer-command",
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
    fn independent_host_loads_have_distinct_continuity_identities() {
        let first = stored_browser_runtime();
        let second = stored_browser_runtime();

        assert!(!first.host_session_id().is_empty());
        assert_ne!(first.host_session_id(), second.host_session_id());
    }

    #[test]
    fn presentation_projection_cannot_change_authoritative_snapshot() {
        let stored = stored_browser_runtime();
        let before = loading_bay_game::encode_game_snapshot(stored.runtime().runtime())
            .expect("snapshot before projection");
        let mut feedback = BrowserFeedbackProjection::default();
        feedback.extend_events(&[GameEvent::DoorOpened {
            door: EXIT,
            entity_facts: Vec::new(),
        }]);

        let state = browser_state(&stored, vec!["DoorOpened".to_owned()], feedback);

        let state = serde_json::to_value(state).expect("serialize projected browser state");
        assert_eq!(state["lastEvents"], serde_json::json!(["DoorOpened"]));
        assert_eq!(
            loading_bay_game::encode_game_snapshot(stored.runtime().runtime())
                .expect("snapshot after projection"),
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

    #[test]
    fn background_projection_is_tick_owned_and_sampled_at_thirty_hertz() {
        assert!(background_projection_due(0));
        assert!(!background_projection_due(1));
        assert!(background_projection_due(2));
        assert!(!background_projection_due(3));
    }

    fn directory_for_test_resources() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    fn copy_directory(source: &Path, destination: &Path) {
        fs::create_dir_all(destination).expect("create packaged test directory");
        for entry in fs::read_dir(source).expect("read packaged source directory") {
            let entry = entry.expect("source directory entry");
            let source_path = entry.path();
            let destination_path = destination.join(entry.file_name());
            if source_path.is_dir() {
                copy_directory(&source_path, &destination_path);
            } else {
                fs::copy(&source_path, &destination_path).expect("copy packaged source file");
            }
        }
    }
}
