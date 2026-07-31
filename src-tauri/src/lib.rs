use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use url::Url;

const MANIFEST_FILE: &str = "desktop-package-manifest.json";
const SIDECAR_NAME: &str = "loading-bay-browser-host";
const READY_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageManifest {
    pub schema_version: u32,
    pub source_revision: String,
    pub app_version: String,
    pub target_triple: String,
    pub files: Vec<PackageFile>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageFile {
    pub path: String,
    pub byte_len: u64,
    pub sha256: String,
    pub kind: PackageFileKind,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PackageFileKind {
    Resource,
    Sidecar,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostReady {
    schema_version: u32,
    address: SocketAddr,
    pid: u32,
    project_content_hash: String,
}

#[derive(Debug)]
struct HostState {
    child: Mutex<Option<CommandChild>>,
    expected_shutdown: Arc<Mutex<bool>>,
}

impl HostState {
    fn stop(&self) {
        *self.expected_shutdown.lock().expect("shutdown lock") = true;
        if let Some(child) = self.child.lock().expect("child lock").take() {
            let _ = child.kill();
        }
    }
}

impl Drop for HostState {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_shell::init());
    let app = builder
        .setup(|app| {
            let resource_root = app.path().resource_dir()?;
            let sidecar_path =
                sidecar_path_from_current_executable().map_err(std::io::Error::other)?;
            let manifest = load_and_verify_manifest(&resource_root, &sidecar_path)
                .map_err(std::io::Error::other)?;

            let data_root = app.path().app_data_dir()?;
            let cache_root = app.path().app_cache_dir()?;
            let log_root = app.path().app_log_dir()?;
            for directory in [&data_root, &cache_root, &log_root] {
                fs::create_dir_all(directory)?;
            }
            let save_root = data_root.join("saves");
            fs::create_dir_all(&save_root)?;
            let ready_file = cache_root.join(format!("host-ready-{}.json", std::process::id()));
            let _ = fs::remove_file(&ready_file);
            let log_file = log_root.join("browser-host.log");

            let project = resource_root.join("content/projects/loading-bay.project.json");
            let dist = resource_root.join("web");
            let parent_pid = std::process::id().to_string();
            let arguments = vec![
                "--addr".into(),
                "127.0.0.1:0".into(),
                "--dist".into(),
                dist.into_os_string(),
                "--project".into(),
                project.into_os_string(),
                "--save-root".into(),
                save_root.clone().into_os_string(),
                "--ready-file".into(),
                ready_file.clone().into_os_string(),
                "--parent-pid".into(),
                parent_pid.into(),
                "--require-loopback".into(),
            ];
            let (mut events, child) = app
                .shell()
                .sidecar(SIDECAR_NAME)?
                .args(arguments)
                .current_dir(&data_root)
                .env_clear()
                .env("RUST_BACKTRACE", "1")
                .spawn()?;
            let child_pid = child.pid();
            let expected_shutdown = Arc::new(Mutex::new(false));
            let host_state = HostState {
                child: Mutex::new(Some(child)),
                expected_shutdown: Arc::clone(&expected_shutdown),
            };
            app.manage(host_state);

            let event_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(event) = events.recv().await {
                    append_host_event(&log_file, &event);
                    if matches!(event, CommandEvent::Terminated(_) | CommandEvent::Error(_))
                        && !*expected_shutdown.lock().expect("shutdown lock")
                    {
                        event_app.exit(1);
                        break;
                    }
                }
            });

            let ready = wait_for_ready(&ready_file, child_pid, READY_TIMEOUT)
                .map_err(std::io::Error::other)?;
            let product_url = product_url(ready.address).map_err(std::io::Error::other)?;
            wait_for_health(ready.address, READY_TIMEOUT).map_err(std::io::Error::other)?;
            let expected_origin = product_url.origin().ascii_serialization();

            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(product_url))
                .title("Rusty Engine — Loading Bay")
                .inner_size(1280.0, 720.0)
                .min_inner_size(960.0, 540.0)
                .resizable(true)
                .fullscreen(false)
                .on_navigation(move |url| {
                    url.origin().ascii_serialization() == expected_origin
                        || matches!(url.scheme(), "about" | "data")
                })
                .build()?;

            println!(
                "loading-bay-desktop ready sourceRevision={} host={} saves={} projectHash={}",
                manifest.source_revision,
                ready.address,
                save_root.display(),
                ready.project_content_hash
            );
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("could not build Loading Bay desktop application");

    app.run(|handle, event| match event {
        tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::Destroyed,
            ..
        } if label == "main" => {
            if let Some(state) = handle.try_state::<HostState>() {
                state.stop();
            }
            handle.exit(0);
        }
        tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit => {
            if let Some(state) = handle.try_state::<HostState>() {
                state.stop();
            }
        }
        _ => {}
    });
}

fn product_url(address: SocketAddr) -> Result<Url, String> {
    if !address.ip().is_loopback() || address.port() == 0 {
        return Err(format!(
            "desktop host advertised unsafe address {address}; expected loopback with a bound port"
        ));
    }
    Url::parse(&format!("http://{address}/"))
        .map_err(|error| format!("desktop host URL is invalid: {error}"))
}

fn sidecar_path_from_current_executable() -> Result<PathBuf, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not resolve desktop executable: {error}"))?;
    let directory = executable.parent().ok_or_else(|| {
        format!(
            "desktop executable {} has no parent directory",
            executable.display()
        )
    })?;
    Ok(directory.join(SIDECAR_NAME))
}

pub fn load_and_verify_manifest(
    resource_root: &Path,
    sidecar_path: &Path,
) -> Result<PackageManifest, String> {
    let manifest_path = resource_root.join(MANIFEST_FILE);
    let source = fs::read_to_string(&manifest_path).map_err(|error| {
        format!(
            "required desktop package manifest {} is unavailable: {error}",
            manifest_path.display()
        )
    })?;
    let manifest: PackageManifest = serde_json::from_str(&source)
        .map_err(|error| format!("desktop package manifest is invalid: {error}"))?;
    if manifest.schema_version != 1 {
        return Err(format!(
            "unsupported desktop package manifest schema {}",
            manifest.schema_version
        ));
    }
    if manifest.files.is_empty() {
        return Err("desktop package manifest has no files".to_owned());
    }
    if manifest.source_revision.len() != 40
        || !manifest
            .source_revision
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("desktop package manifest source revision is not an exact SHA".to_owned());
    }
    let mut paths = BTreeSet::new();
    let mut sidecar_count = 0;
    for file in &manifest.files {
        validate_relative_path(&file.path)?;
        if !paths.insert(file.path.as_str()) {
            return Err(format!("duplicate desktop package path {}", file.path));
        }
        if file.sha256.len() != 64
            || !file
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(format!(
                "package file {} has an invalid SHA-256 digest",
                file.path
            ));
        }
        let path = match file.kind {
            PackageFileKind::Resource => resource_root.join(&file.path),
            PackageFileKind::Sidecar => {
                sidecar_count += 1;
                if file.path != SIDECAR_NAME {
                    return Err(format!("unexpected desktop sidecar path {}", file.path));
                }
                sidecar_path.to_path_buf()
            }
        };
        let metadata = fs::metadata(&path).map_err(|error| {
            format!(
                "required package file {} is unavailable: {error}",
                path.display()
            )
        })?;
        if metadata.len() != file.byte_len {
            return Err(format!(
                "package file {} has {} bytes; expected {}",
                file.path,
                metadata.len(),
                file.byte_len
            ));
        }
        let bytes = fs::read(&path).map_err(|error| {
            format!("could not verify package file {}: {error}", path.display())
        })?;
        let actual = hex::encode(Sha256::digest(bytes));
        if actual != file.sha256 {
            return Err(format!(
                "package file {} hash mismatch: got {actual}, expected {}",
                file.path, file.sha256
            ));
        }
    }
    if sidecar_count != 1 {
        return Err(format!(
            "desktop package manifest has {sidecar_count} sidecars; expected exactly one"
        ));
    }
    Ok(manifest)
}

fn validate_relative_path(path: &str) -> Result<(), String> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("unsafe package manifest path {}", path.display()));
    }
    Ok(())
}

fn wait_for_ready(path: &Path, child_pid: u32, timeout: Duration) -> Result<HostReady, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match fs::read_to_string(path) {
            Ok(source) => {
                let ready: HostReady = serde_json::from_str(&source)
                    .map_err(|error| format!("desktop host readiness is invalid: {error}"))?;
                if ready.schema_version != 1 {
                    return Err(format!(
                        "unsupported desktop host readiness schema {}",
                        ready.schema_version
                    ));
                }
                if ready.pid != child_pid {
                    return Err(format!(
                        "desktop host readiness pid {} does not match child {child_pid}",
                        ready.pid
                    ));
                }
                product_url(ready.address)?;
                return Ok(ready);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "could not read desktop host readiness {}: {error}",
                    path.display()
                ));
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "desktop host did not become ready within {} ms",
                timeout.as_millis()
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn wait_for_health(address: SocketAddr, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(250)) {
            let _ = stream
                .write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
            let mut response = String::new();
            if std::io::Read::read_to_string(&mut stream, &mut response).is_ok()
                && response.starts_with("HTTP/1.1 200")
                && response.contains("\"status\":\"ok\"")
            {
                return Ok(());
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err(format!(
        "desktop host at {address} did not pass health within {} ms",
        timeout.as_millis()
    ))
}

fn append_host_event(path: &Path, event: &CommandEvent) {
    let line = match event {
        CommandEvent::Stdout(bytes) => format!("stdout {}", String::from_utf8_lossy(bytes)),
        CommandEvent::Stderr(bytes) => format!("stderr {}", String::from_utf8_lossy(bytes)),
        CommandEvent::Error(message) => format!("error {message}"),
        CommandEvent::Terminated(payload) => {
            format!(
                "terminated code={:?} signal={:?}",
                payload.code, payload.signal
            )
        }
        _ => "unknown sidecar event".to_owned(),
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "loading-bay-desktop-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn sha256(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    #[test]
    fn package_manifest_verifies_resources_and_sidecar() {
        let root = temporary_directory("manifest-ok");
        fs::create_dir_all(root.join("web")).unwrap();
        fs::write(root.join("web/index.html"), b"desktop").unwrap();
        let sidecar = root.join("sidecar");
        fs::write(&sidecar, b"host").unwrap();
        fs::write(
            root.join(MANIFEST_FILE),
            serde_json::to_vec(&serde_json::json!({
                "schemaVersion": 1,
                "sourceRevision": "0123456789012345678901234567890123456789",
                "appVersion": "0.1.0",
                "targetTriple": "x86_64-unknown-linux-gnu",
                "files": [
                    {
                        "path": "web/index.html",
                        "byteLen": 7,
                        "sha256": sha256(b"desktop"),
                        "kind": "resource"
                    },
                    {
                        "path": SIDECAR_NAME,
                        "byteLen": 4,
                        "sha256": sha256(b"host"),
                        "kind": "sidecar"
                    }
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        let manifest = load_and_verify_manifest(&root, &sidecar).unwrap();
        assert_eq!(manifest.files.len(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_manifest_rejects_changed_and_escaping_files() {
        let root = temporary_directory("manifest-reject");
        fs::write(root.join("asset"), b"changed").unwrap();
        let sidecar = root.join("sidecar");
        fs::write(&sidecar, b"host").unwrap();
        fs::write(
            root.join(MANIFEST_FILE),
            serde_json::to_vec(&serde_json::json!({
                "schemaVersion": 1,
                "sourceRevision": "0123456789012345678901234567890123456789",
                "appVersion": "0.1.0",
                "targetTriple": "x86_64-unknown-linux-gnu",
                "files": [{
                    "path": "asset",
                    "byteLen": 7,
                    "sha256": sha256(b"original"),
                    "kind": "resource"
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(load_and_verify_manifest(&root, &sidecar)
            .unwrap_err()
            .contains("hash mismatch"));
        assert!(validate_relative_path("../outside").is_err());
        assert!(validate_relative_path("/outside").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn product_url_accepts_only_bound_loopback_addresses() {
        assert!(product_url("127.0.0.1:49152".parse().unwrap()).is_ok());
        assert!(product_url("[::1]:49152".parse().unwrap()).is_ok());
        assert!(product_url("0.0.0.0:49152".parse().unwrap()).is_err());
        assert!(product_url("127.0.0.1:0".parse().unwrap()).is_err());
    }

    #[test]
    fn readiness_is_bounded_and_child_specific() {
        let root = temporary_directory("readiness");
        let path = root.join("ready.json");
        let error = wait_for_ready(&path, 42, Duration::from_millis(1)).unwrap_err();
        assert!(error.contains("did not become ready"));

        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "schemaVersion": 1,
                "address": "127.0.0.1:49152",
                "pid": 43,
                "projectContentHash": "sha256:abc"
            }))
            .unwrap(),
        )
        .unwrap();
        let error = wait_for_ready(&path, 42, Duration::from_millis(10)).unwrap_err();
        assert!(error.contains("does not match child"));
        fs::remove_dir_all(root).unwrap();
    }
}
