//! Tauri's in-process adapter over the Loading Bay product service.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use loading_bay_game::browser_adapter::{
    BrowserDynamicState, InProcessLoadingBayAdapter, InProcessProjection,
};
use loading_bay_game::{
    LoadingBayDeveloperCommandRequest, LoadingBayDeveloperCommandResponse,
    LoadingBayDeveloperDiscovery, LoadingBayProjectReadout, LoadingBayServiceCommand,
    LoadingBayServiceOutcome, LoadingBayServiceReceipt,
};
use serde::Serialize;
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

struct DesktopProductService {
    adapter: Arc<Mutex<InProcessLoadingBayAdapter>>,
    ticking: Arc<AtomicBool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopReadout {
    project: LoadingBayProjectReadout,
    #[serde(skip_serializing_if = "Option::is_none")]
    connection_generation: Option<u64>,
    dynamic: BrowserDynamicState,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopSessionReadout {
    connection_generation: u64,
    projection: InProcessProjection,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopCommandReadout {
    #[serde(skip_serializing_if = "Option::is_none")]
    connection_generation: Option<u64>,
    receipt: LoadingBayServiceReceipt,
    dynamic: BrowserDynamicState,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopMenuReadout {
    project: LoadingBayProjectReadout,
    save_slots: Vec<loading_bay_game::SaveSlotSummary>,
}

#[tauri::command]
fn loading_bay_service_readout(
    service: State<'_, DesktopProductService>,
) -> Result<DesktopReadout, String> {
    let mut service = service
        .adapter
        .lock()
        .map_err(|_| "Loading Bay service lock poisoned")?;
    Ok(DesktopReadout {
        project: service.project().clone(),
        connection_generation: service.active_connection_generation(),
        dynamic: service.dynamic_projection()?,
    })
}

#[tauri::command]
fn loading_bay_service_menu_readout(
    service: State<'_, DesktopProductService>,
) -> Result<DesktopMenuReadout, String> {
    let service = service
        .adapter
        .lock()
        .map_err(|_| "Loading Bay service lock poisoned")?;
    Ok(DesktopMenuReadout {
        project: service.project().clone(),
        save_slots: service.save_slots(),
    })
}

#[tauri::command]
fn loading_bay_service_begin_session(
    service: State<'_, DesktopProductService>,
) -> Result<DesktopSessionReadout, String> {
    let mut service = service
        .adapter
        .lock()
        .map_err(|_| "Loading Bay service lock poisoned")?;
    let connection_generation = service.begin_session();
    Ok(DesktopSessionReadout {
        connection_generation,
        projection: service.projection()?,
    })
}

#[tauri::command]
fn loading_bay_service_disconnect_session(
    connection_generation: u64,
    service: State<'_, DesktopProductService>,
) -> Result<(), String> {
    let mut service = service
        .adapter
        .lock()
        .map_err(|_| "Loading Bay service lock poisoned")?;
    service.disconnect_session(connection_generation);
    Ok(())
}

#[tauri::command]
fn loading_bay_service_submit(
    command: LoadingBayServiceCommand,
    service: State<'_, DesktopProductService>,
) -> Result<DesktopCommandReadout, String> {
    let mut service = service
        .adapter
        .lock()
        .map_err(|_| "Loading Bay service lock poisoned")?;
    let receipt = service.submit(command)?;
    Ok(DesktopCommandReadout {
        connection_generation: service.active_connection_generation(),
        receipt,
        dynamic: service.dynamic_projection()?,
    })
}

#[tauri::command]
fn loading_bay_service_command_outcome(
    connection_generation: u64,
    sequence: u64,
    service: State<'_, DesktopProductService>,
) -> Result<Option<LoadingBayServiceOutcome>, String> {
    Ok(service
        .adapter
        .lock()
        .map_err(|_| "Loading Bay service lock poisoned")?
        .command_outcome(connection_generation, sequence))
}

#[tauri::command]
fn loading_bay_service_application_resource(
    index: usize,
    service: State<'_, DesktopProductService>,
) -> Result<Vec<u8>, String> {
    service
        .adapter
        .lock()
        .map_err(|_| "Loading Bay service lock poisoned")?
        .application_resource(index)
}

#[tauri::command]
fn loading_bay_developer_discover(
    service: State<'_, DesktopProductService>,
) -> Result<LoadingBayDeveloperDiscovery, String> {
    service
        .adapter
        .lock()
        .map_err(|_| "Loading Bay service lock poisoned")?
        .discover_developer_commands()
}

#[tauri::command]
fn loading_bay_developer_submit(
    request: LoadingBayDeveloperCommandRequest,
    service: State<'_, DesktopProductService>,
) -> Result<(), String> {
    service
        .adapter
        .lock()
        .map_err(|_| "Loading Bay service lock poisoned")?
        .submit_developer_command(request)
}

#[tauri::command]
fn loading_bay_developer_poll(
    correlation: String,
    service: State<'_, DesktopProductService>,
) -> Result<Option<LoadingBayDeveloperCommandResponse>, String> {
    Ok(service
        .adapter
        .lock()
        .map_err(|_| "Loading Bay service lock poisoned")?
        .poll_developer_command(&correlation))
}

#[tauri::command]
fn loading_bay_developer_cancel(
    correlation: String,
    service: State<'_, DesktopProductService>,
) -> Result<bool, String> {
    Ok(service
        .adapter
        .lock()
        .map_err(|_| "Loading Bay service lock poisoned")?
        .cancel_developer_command(&correlation))
}

pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let resource_root = app.path().resource_dir()?;
            let project = resource_root.join("content/projects/doom-e1m1.project.json");
            let save_root = app.path().app_data_dir()?.join("saves");
            std::fs::create_dir_all(&save_root)?;
            let service = InProcessLoadingBayAdapter::admit(&project, &save_root)
                .map_err(std::io::Error::other)?;
            let service = Arc::new(Mutex::new(service));
            let tick_service = Arc::clone(&service);
            let ticking = Arc::new(AtomicBool::new(true));
            let tick_running = Arc::clone(&ticking);
            std::thread::spawn(move || {
                while tick_running.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(16));
                    if let Ok(mut service) = tick_service.lock() {
                        if let Err(error) =
                            service.tick_if_session_active(Duration::from_millis(16))
                        {
                            eprintln!("Loading Bay desktop tick failed: {error}");
                        }
                    }
                }
            });
            app.manage(DesktopProductService {
                adapter: service,
                ticking,
            });
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Loading Bay — E1M1")
                .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            loading_bay_service_readout,
            loading_bay_service_menu_readout,
            loading_bay_service_begin_session,
            loading_bay_service_disconnect_session,
            loading_bay_service_submit,
            loading_bay_service_command_outcome,
            loading_bay_service_application_resource,
            loading_bay_developer_discover,
            loading_bay_developer_submit,
            loading_bay_developer_poll,
            loading_bay_developer_cancel,
        ])
        .build(tauri::generate_context!())
        .expect("could not build Loading Bay desktop application");
    app.run(|handle, event| {
        if matches!(
            event,
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
        ) {
            handle
                .state::<DesktopProductService>()
                .ticking
                .store(false, Ordering::Relaxed);
            handle.exit(0);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_adapter_uses_typed_product_commands() {
        let command = LoadingBayServiceCommand::SetInputIntent {
            connection_generation: 3,
            sequence: 5,
            movement: [0.0, 1.0],
            look_delta: [0.0, 0.0],
            jump_held: false,
            primary_fire_held: false,
        };
        assert_eq!(
            serde_json::to_value(command).unwrap()["kind"],
            "setInputIntent"
        );
    }

    #[test]
    fn desktop_save_command_preserves_storage_concurrency_fields() {
        let command = LoadingBayServiceCommand::SaveGame {
            connection_generation: 3,
            sequence: 5,
            slot: loading_bay_game::SaveSlotId::Slot1,
            overwrite: true,
            expected_storage_revision: Some("saved-revision".to_owned()),
        };
        let value = serde_json::to_value(command).expect("serialize save command");
        assert_eq!(value["kind"], "saveGame");
        assert_eq!(value["expectedStorageRevision"], "saved-revision");
        assert_eq!(value["overwrite"], true);
    }

    #[test]
    fn desktop_replies_omit_absent_connection_generations() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-desktop-wire-shape-{}-{unique}",
            std::process::id()
        ));
        let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../content/projects/doom-e1m1.project.json");
        let mut adapter =
            InProcessLoadingBayAdapter::admit(&project, &save_root).expect("admit desktop project");

        let readout = DesktopReadout {
            project: adapter.project().clone(),
            connection_generation: None,
            dynamic: adapter.dynamic_projection().expect("dynamic readout"),
        };
        let readout = serde_json::to_value(readout).expect("serialize desktop readout");
        assert!(readout.get("connectionGeneration").is_none());

        let command = DesktopCommandReadout {
            connection_generation: None,
            receipt: LoadingBayServiceReceipt::Input {
                connection_generation: 1,
                acknowledged_sequence: 0,
                consumed_sequence: 0,
                repeated: false,
            },
            dynamic: adapter.dynamic_projection().expect("command readout"),
        };
        let command = serde_json::to_value(command).expect("serialize desktop command readout");
        assert!(command.get("connectionGeneration").is_none());

        let _ = std::fs::remove_dir_all(save_root);
    }

    #[test]
    fn desktop_developer_port_waits_for_the_existing_ticker_safe_point() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let save_root = std::env::temp_dir().join(format!(
            "loading-bay-desktop-developer-{}-{unique}",
            std::process::id()
        ));
        let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../content/projects/doom-e1m1.project.json");
        let mut adapter =
            InProcessLoadingBayAdapter::admit(&project, &save_root).expect("admit desktop project");
        adapter.begin_session();
        let discovery = adapter.discover_developer_commands().unwrap();
        let discovery = serde_json::to_value(discovery).unwrap();
        adapter
            .submit_developer_command(
                serde_json::from_value(serde_json::json!({
                    "protocolVersion": discovery["protocolVersion"],
                    "command": "standard.inspect.entity",
                    "correlation": "tauri-safe-point",
                    "runtime": discovery["runtime"],
                    "expected": {
                        "profile": discovery["profile"],
                        "revision": discovery["revision"],
                        "catalogEpoch": discovery["catalogEpoch"]
                    },
                    "payload": { "entity": "1" }
                }))
                .unwrap(),
            )
            .unwrap();
        assert!(adapter.poll_developer_command("tauri-safe-point").is_none());
        adapter
            .tick_if_session_active(loading_bay_game::FIXED_STEP_DURATION)
            .unwrap();
        assert!(adapter.poll_developer_command("tauri-safe-point").is_some());
        let _ = std::fs::remove_dir_all(save_root);
    }
}
