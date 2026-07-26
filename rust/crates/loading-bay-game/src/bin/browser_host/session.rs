//! One bounded, versioned WebSocket lifecycle for Loading Bay play.

use std::io::ErrorKind;
use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

use loading_bay_game::{
    GameLoopEdgeCommand, GameLoopEdgeCommandKind, InputCommandRejection, PlayerInputCommand,
    PlayerInputIntent,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tungstenite::handshake::server::{Request, Response};
use tungstenite::http::header::{HeaderValue, SEC_WEBSOCKET_PROTOCOL};
use tungstenite::http::StatusCode;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{accept_hdr_with_config, Error as WebSocketError, Message, WebSocket};

use super::state::{
    browser_dynamic_state, browser_state, browser_static_resources, browser_static_revision,
};
use super::{
    drain_game_loop_feedback, BrowserFeedbackProjection, BrowserRuntime, SharedBrowserRuntime,
};

const PROTOCOL_VERSION: u16 = 1;
const SESSION_UPDATE_INTERVAL: Duration = Duration::from_micros(16_667);
const SESSION_READ_TIMEOUT: Duration = Duration::from_millis(4);
const SESSION_WRITE_TIMEOUT: Duration = Duration::from_millis(100);
const MAX_COMMAND_BYTES: usize = 16 * 1024;
const MAX_COMMANDS_PER_POLL: usize = 32;
const MAX_OUTBOUND_BUFFER_BYTES: usize = 256 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClientCommandEnvelope {
    protocol_version: u16,
    session_id: String,
    sequence: u64,
    observed_snapshot_sequence: Option<u64>,
    observed_static_revision: Option<String>,
    command: BrowserGameCommand,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum BrowserGameCommand {
    SetInputIntent {
        movement: [f32; 2],
        look_delta: [f32; 2],
        primary_fire_held: bool,
    },
    Interact {
        target: u64,
    },
    SelectWeaponSlot {
        slot: u8,
    },
    SetPaused {
        paused: bool,
    },
    Restart {
        mode: RestartMode,
    },
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
enum RestartMode {
    AuthoredBaseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // The closed wire family includes loss/resync identities emitted by clients.
pub(super) enum RejectionCode {
    ProtocolMismatch,
    SessionClosed,
    TransportLost,
    StaleSequence,
    EdgeQueueSaturated,
    DeltaBaseUnavailable,
    InvalidInput,
    UnknownTarget,
    NotInteractable,
    Cooldown,
    NoAmmo,
    NoEquippedWeapon,
    InvalidWeaponSlot,
    WeaponNotOwned,
    WeaponAlreadySelected,
    PlayerDefeated,
    Paused,
    InternalDefect,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Resync is reserved for a retained-delta rejection in this protocol version.
pub(super) enum RetryDisposition {
    Never,
    Reconnect,
    Resync,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandRejection {
    protocol_version: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command_sequence: Option<u64>,
    acknowledged_command_sequence: u64,
    code: RejectionCode,
    retry: RetryDisposition,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum StateUpdate {
    Full {
        state: Value,
    },
    Delta {
        base_snapshot_sequence: u64,
        changes: Map<String, Value>,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionFact {
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<RejectionCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command_sequence: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionMetrics {
    inbound_command_count: u64,
    outbound_update_count: u64,
    rejected_command_count: u64,
    last_inbound_bytes: usize,
    last_outbound_bytes: usize,
    legacy_whole_state_bytes: usize,
    bootstrap_outbound_bytes: usize,
    static_resource_update_count: u64,
    static_resource_last_bytes: usize,
    static_resource_max_bytes: usize,
    steady_state_last_bytes: usize,
    steady_state_max_bytes: usize,
    steady_state_update_count: u64,
    maximum_pending_outbound_updates: usize,
    dropped_fact_count: u64,
    last_update_build_microseconds: u64,
    maximum_update_build_microseconds: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerUpdateEnvelope {
    protocol_version: u16,
    session_id: String,
    connection_generation: u64,
    server_tick: u64,
    snapshot_sequence: u64,
    acknowledged_command_sequence: u64,
    static_revision: String,
    update: StateUpdate,
    #[serde(skip_serializing_if = "Option::is_none")]
    resources: Option<Value>,
    facts: Vec<SessionFact>,
    metrics: SessionMetrics,
}

#[derive(Debug)]
struct SessionContext {
    connection_generation: u64,
    session_id: String,
    snapshot_sequence: u64,
    previous_dynamic: Option<Value>,
    previous_static_revision: Option<String>,
    force_full: bool,
    force_static_resources: bool,
    acknowledged_command_sequence: u64,
    pending_restart_sequence: Option<u64>,
    metrics: SessionMetrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateDisposition {
    Sent,
    SessionClosed,
}

impl SessionContext {
    fn new(connection_generation: u64) -> Self {
        Self {
            connection_generation,
            session_id: session_id(connection_generation),
            snapshot_sequence: 0,
            previous_dynamic: None,
            previous_static_revision: None,
            force_full: true,
            force_static_resources: false,
            acknowledged_command_sequence: 0,
            pending_restart_sequence: None,
            metrics: SessionMetrics::default(),
        }
    }

    fn replace_connection(&mut self, connection_generation: u64) {
        self.connection_generation = connection_generation;
        self.session_id = session_id(connection_generation);
        self.snapshot_sequence = 0;
        self.previous_dynamic = None;
        self.force_full = true;
        self.force_static_resources = false;
        self.acknowledged_command_sequence = 0;
        self.pending_restart_sequence = None;
    }
}

pub(super) fn session_upgrade_requested(stream: &TcpStream) -> bool {
    let mut prefix = [0_u8; 512];
    stream
        .peek(&mut prefix)
        .ok()
        .and_then(|length| std::str::from_utf8(&prefix[..length]).ok())
        .is_some_and(|request| request.starts_with("GET /api/session "))
}

pub(super) fn run_game_session(stream: TcpStream, runtime: Arc<SharedBrowserRuntime>) {
    let config = WebSocketConfig::default()
        .read_buffer_size(MAX_COMMAND_BYTES)
        .write_buffer_size(0)
        .max_write_buffer_size(MAX_OUTBOUND_BUFFER_BYTES)
        .max_message_size(Some(MAX_COMMAND_BYTES))
        .max_frame_size(Some(MAX_COMMAND_BYTES));
    let websocket = match accept_hdr_with_config(stream, select_protocol, Some(config)) {
        Ok(websocket) => websocket,
        Err(error) => {
            eprintln!("browser-host WebSocket handshake failed: {error}");
            return;
        }
    };
    let _ = websocket
        .get_ref()
        .set_read_timeout(Some(SESSION_READ_TIMEOUT));
    let _ = websocket
        .get_ref()
        .set_write_timeout(Some(SESSION_WRITE_TIMEOUT));

    let connection_generation = runtime
        .lock()
        .expect("runtime lock")
        .start_browser_connection();
    let mut context = SessionContext::new(connection_generation);
    serve_session(websocket, &runtime, &mut context);

    let mut host = runtime.lock().expect("runtime lock");
    host.disconnect_browser_session(
        context.connection_generation,
        context.pending_restart_sequence,
    );
}

#[allow(clippy::result_large_err)] // Tungstenite requires this exact handshake callback result.
fn select_protocol(
    request: &Request,
    mut response: Response,
) -> Result<Response, tungstenite::handshake::server::ErrorResponse> {
    let protocol_supported = request
        .headers()
        .get(SEC_WEBSOCKET_PROTOCOL)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|candidate| candidate.trim() == "loading-bay.v1")
        });
    if !protocol_supported {
        return Err(tungstenite::http::Response::builder()
            .status(StatusCode::UPGRADE_REQUIRED)
            .header(SEC_WEBSOCKET_PROTOCOL, "loading-bay.v1")
            .body(Some(
                "Loading Bay requires the loading-bay.v1 WebSocket subprotocol".to_owned(),
            ))
            .expect("valid protocol rejection"));
    }
    response.headers_mut().insert(
        SEC_WEBSOCKET_PROTOCOL,
        HeaderValue::from_static("loading-bay.v1"),
    );
    Ok(response)
}

fn serve_session(
    mut websocket: WebSocket<TcpStream>,
    runtime: &Arc<SharedBrowserRuntime>,
    context: &mut SessionContext,
) {
    if !matches!(
        send_latest_update(&mut websocket, runtime, context),
        Ok(UpdateDisposition::Sent)
    ) {
        return;
    }
    let mut next_update = Instant::now() + SESSION_UPDATE_INTERVAL;
    loop {
        for _ in 0..MAX_COMMANDS_PER_POLL {
            match websocket.read() {
                Ok(Message::Text(text)) => {
                    context.metrics.last_inbound_bytes = text.len();
                    context.metrics.inbound_command_count =
                        context.metrics.inbound_command_count.saturating_add(1);
                    if text.len() > MAX_COMMAND_BYTES {
                        if send_rejection(
                            &mut websocket,
                            context,
                            None,
                            RejectionCode::InvalidInput,
                            RetryDisposition::Never,
                            "command exceeds the 16 KiB session limit",
                        )
                        .is_err()
                        {
                            return;
                        }
                        continue;
                    }
                    match serde_json::from_str::<ClientCommandEnvelope>(&text) {
                        Ok(command) => {
                            if process_command(&mut websocket, runtime, context, command).is_err() {
                                return;
                            }
                        }
                        Err(error) => {
                            if send_rejection(
                                &mut websocket,
                                context,
                                None,
                                RejectionCode::InvalidInput,
                                RetryDisposition::Never,
                                &format!("invalid command envelope: {error}"),
                            )
                            .is_err()
                            {
                                return;
                            }
                        }
                    }
                }
                Ok(Message::Close(frame)) => {
                    let _ = websocket.close(frame);
                    return;
                }
                Ok(Message::Ping(payload)) => {
                    if websocket.send(Message::Pong(payload)).is_err() {
                        return;
                    }
                }
                Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => {}
                Ok(Message::Binary(_)) => {
                    if send_rejection(
                        &mut websocket,
                        context,
                        None,
                        RejectionCode::InvalidInput,
                        RetryDisposition::Never,
                        "Loading Bay session commands must be JSON text",
                    )
                    .is_err()
                    {
                        return;
                    }
                }
                Err(error) if websocket_would_block(&error) => break,
                Err(WebSocketError::Capacity(_)) => {
                    let _ = send_rejection(
                        &mut websocket,
                        context,
                        None,
                        RejectionCode::InvalidInput,
                        RetryDisposition::Never,
                        "command exceeds the 16 KiB session limit",
                    );
                    return;
                }
                Err(WebSocketError::ConnectionClosed | WebSocketError::AlreadyClosed) => return,
                Err(error) => {
                    eprintln!("browser-host WebSocket read failed: {error}");
                    return;
                }
            }
        }

        if Instant::now() >= next_update {
            if !matches!(
                send_latest_update(&mut websocket, runtime, context),
                Ok(UpdateDisposition::Sent)
            ) {
                return;
            }
            next_update = Instant::now() + SESSION_UPDATE_INTERVAL;
        }
    }
}

fn process_command(
    websocket: &mut WebSocket<TcpStream>,
    runtime: &Arc<SharedBrowserRuntime>,
    context: &mut SessionContext,
    envelope: ClientCommandEnvelope,
) -> Result<(), WebSocketError> {
    if envelope.protocol_version != PROTOCOL_VERSION {
        return send_rejection(
            websocket,
            context,
            Some(envelope.sequence),
            RejectionCode::ProtocolMismatch,
            RetryDisposition::Never,
            "unsupported Loading Bay session protocol version",
        );
    }
    if envelope.session_id != context.session_id {
        return send_rejection(
            websocket,
            context,
            Some(envelope.sequence),
            RejectionCode::SessionClosed,
            RetryDisposition::Reconnect,
            "command belongs to a replaced session",
        );
    }
    if envelope
        .observed_snapshot_sequence
        .is_some_and(|observed| observed != context.snapshot_sequence)
    {
        context.force_full = true;
    }
    if envelope
        .observed_static_revision
        .as_ref()
        .is_some_and(|observed| context.previous_static_revision.as_ref() != Some(observed))
    {
        context.force_full = true;
        context.force_static_resources = true;
    }

    let mut host = runtime.lock().expect("runtime lock");
    let result = match envelope.command {
        BrowserGameCommand::SetInputIntent {
            movement,
            look_delta,
            primary_fire_held,
        } => host.runtime.submit_input(PlayerInputCommand {
            connection_generation: context.connection_generation,
            sequence: envelope.sequence,
            intent: PlayerInputIntent {
                movement,
                look_delta,
                primary_fire_held,
            },
        }),
        BrowserGameCommand::Interact { target } => {
            host.runtime.submit_edge_command(GameLoopEdgeCommand {
                connection_generation: context.connection_generation,
                sequence: envelope.sequence,
                command: GameLoopEdgeCommandKind::Interact { target },
            })
        }
        BrowserGameCommand::SelectWeaponSlot { slot } => {
            host.runtime.submit_edge_command(GameLoopEdgeCommand {
                connection_generation: context.connection_generation,
                sequence: envelope.sequence,
                command: GameLoopEdgeCommandKind::SelectWeaponSlot { slot },
            })
        }
        BrowserGameCommand::SetPaused { paused } => {
            host.runtime.submit_edge_command(GameLoopEdgeCommand {
                connection_generation: context.connection_generation,
                sequence: envelope.sequence,
                command: GameLoopEdgeCommandKind::SetPaused { paused },
            })
        }
        BrowserGameCommand::Restart {
            mode: RestartMode::AuthoredBaseline,
        } => {
            if let Some(pending) = host.pending_restart.as_ref() {
                if pending.identity.connection_generation == context.connection_generation
                    && pending.identity.sequence == envelope.sequence
                {
                    host.runtime.submit_edge_command(GameLoopEdgeCommand {
                        connection_generation: context.connection_generation,
                        sequence: envelope.sequence,
                        command: GameLoopEdgeCommandKind::RestartAuthoredBaseline,
                    })
                } else {
                    Err(InputCommandRejection::EdgeQueueSaturated { capacity: 1 })
                }
            } else {
                let project_path = host.project_path.clone();
                let replacement = match BrowserRuntime::load(&project_path) {
                    Ok(replacement) => replacement,
                    Err(error) => {
                        drop(host);
                        return send_rejection(
                            websocket,
                            context,
                            Some(envelope.sequence),
                            RejectionCode::InternalDefect,
                            RetryDisposition::Never,
                            &format!("restart failed before mutation: {error}"),
                        );
                    }
                };
                let receipt = host.runtime.submit_edge_command(GameLoopEdgeCommand {
                    connection_generation: context.connection_generation,
                    sequence: envelope.sequence,
                    command: GameLoopEdgeCommandKind::RestartAuthoredBaseline,
                });
                if receipt.is_ok() {
                    host.stage_restart(
                        context.connection_generation,
                        envelope.sequence,
                        replacement,
                    );
                    context.pending_restart_sequence = Some(envelope.sequence);
                }
                receipt
            }
        }
    };
    drop(host);

    match result {
        Ok(receipt) => {
            context.acknowledged_command_sequence = receipt.acknowledged_sequence;
            Ok(())
        }
        Err(rejection) => {
            let (code, retry) = rejection_identity(rejection);
            send_rejection(
                websocket,
                context,
                Some(envelope.sequence),
                code,
                retry,
                &rejection.to_string(),
            )
        }
    }
}

fn send_latest_update(
    websocket: &mut WebSocket<TcpStream>,
    runtime: &Arc<SharedBrowserRuntime>,
    context: &mut SessionContext,
) -> Result<UpdateDisposition, WebSocketError> {
    let build_started = Instant::now();
    let mut host = runtime.lock().expect("runtime lock");
    let mut active = host.runtime.input_session();
    if active.connection_generation != context.connection_generation || !active.connected {
        let adopted_generation = context.pending_restart_sequence.and_then(|sequence| {
            host.adopt_consumed_restart(context.connection_generation, sequence)
        });
        if let Some(connection_generation) = adopted_generation {
            context.replace_connection(connection_generation);
            active = host.runtime.input_session();
        } else {
            drop(host);
            send_rejection(
                websocket,
                context,
                None,
                RejectionCode::SessionClosed,
                RetryDisposition::Reconnect,
                "the authoritative input session was replaced",
            )?;
            return Ok(UpdateDisposition::SessionClosed);
        }
    }

    let dropped_facts = host.runtime.dropped_fact_count();
    context.acknowledged_command_sequence = active.acknowledged_sequence;
    if dropped_facts != context.metrics.dropped_fact_count {
        context.force_full = true;
        context.metrics.dropped_fact_count = dropped_facts;
    }
    let (fact_projection, feedback) = drain_game_loop_feedback(&mut host.runtime);
    if let Some(sequence) = context.pending_restart_sequence {
        let restart_rejected = fact_projection.iter().any(|(kind, command_sequence)| {
            *command_sequence == Some(sequence) && kind == "InputEdgeRejectedPaused"
        });
        if restart_rejected {
            host.cancel_staged_restart(context.connection_generation, sequence);
            context.pending_restart_sequence = None;
        }
    }
    let fact_names = fact_projection
        .iter()
        .map(|(kind, _)| kind.clone())
        .collect::<Vec<_>>();
    if context.metrics.legacy_whole_state_bytes == 0 {
        context.metrics.legacy_whole_state_bytes = serde_json::to_vec(&browser_state(
            &host,
            Vec::new(),
            BrowserFeedbackProjection::default(),
        ))
        .expect("serialize legacy whole-state baseline")
        .len();
    }
    let dynamic = serde_json::to_value(browser_dynamic_state(&host, fact_names.clone(), feedback))
        .expect("serialize dynamic browser state");
    let static_revision = browser_static_revision(&host);
    let static_changed = context.force_static_resources
        || context.previous_static_revision.as_ref() != Some(&static_revision);
    let full = context.force_full || context.previous_dynamic.is_none() || static_changed;
    let next_snapshot_sequence = context.snapshot_sequence.saturating_add(1);
    let update = if full {
        StateUpdate::Full {
            state: dynamic.clone(),
        }
    } else {
        let changes = dynamic_diff(
            context
                .previous_dynamic
                .as_ref()
                .expect("delta has a previous state"),
            &dynamic,
        );
        StateUpdate::Delta {
            base_snapshot_sequence: context.snapshot_sequence,
            changes,
        }
    };
    let resources = if static_changed {
        Some(
            serde_json::to_value(browser_static_resources(&host))
                .expect("serialize static browser resources"),
        )
    } else {
        None
    };
    let included_static_resources = resources.is_some();
    let facts = fact_projection
        .into_iter()
        .map(|(kind, command_sequence)| SessionFact {
            code: fact_rejection_code(&kind),
            kind,
            command_sequence,
        })
        .collect();
    let envelope = ServerUpdateEnvelope {
        protocol_version: PROTOCOL_VERSION,
        session_id: context.session_id.clone(),
        connection_generation: context.connection_generation,
        server_tick: host.runtime.runtime().tick().raw(),
        snapshot_sequence: next_snapshot_sequence,
        acknowledged_command_sequence: context.acknowledged_command_sequence,
        static_revision: static_revision.clone(),
        update,
        resources,
        facts,
        metrics: SessionMetrics {
            outbound_update_count: context.metrics.outbound_update_count.saturating_add(1),
            maximum_pending_outbound_updates: 1,
            ..context.metrics
        },
    };
    drop(host);

    let encoded = serde_json::to_string(&envelope).expect("encode session update");
    websocket.send(Message::Text(encoded.clone().into()))?;
    let update_build_microseconds =
        u64::try_from(build_started.elapsed().as_micros()).unwrap_or(u64::MAX);
    context.metrics.outbound_update_count = context.metrics.outbound_update_count.saturating_add(1);
    context.metrics.last_outbound_bytes = encoded.len();
    if context.metrics.outbound_update_count == 1 {
        context.metrics.bootstrap_outbound_bytes = encoded.len();
    } else if included_static_resources {
        context.metrics.static_resource_update_count = context
            .metrics
            .static_resource_update_count
            .saturating_add(1);
        context.metrics.static_resource_last_bytes = encoded.len();
        context.metrics.static_resource_max_bytes =
            context.metrics.static_resource_max_bytes.max(encoded.len());
    } else {
        context.metrics.steady_state_last_bytes = encoded.len();
        context.metrics.steady_state_max_bytes =
            context.metrics.steady_state_max_bytes.max(encoded.len());
        context.metrics.steady_state_update_count =
            context.metrics.steady_state_update_count.saturating_add(1);
    }
    context.metrics.maximum_pending_outbound_updates = 1;
    context.metrics.last_update_build_microseconds = update_build_microseconds;
    context.metrics.maximum_update_build_microseconds = context
        .metrics
        .maximum_update_build_microseconds
        .max(update_build_microseconds);
    context.snapshot_sequence = next_snapshot_sequence;
    context.previous_dynamic = Some(dynamic);
    context.previous_static_revision = Some(static_revision);
    context.force_full = false;
    context.force_static_resources = false;
    Ok(UpdateDisposition::Sent)
}

fn send_rejection(
    websocket: &mut WebSocket<TcpStream>,
    context: &mut SessionContext,
    command_sequence: Option<u64>,
    code: RejectionCode,
    retry: RetryDisposition,
    message: &str,
) -> Result<(), WebSocketError> {
    context.metrics.rejected_command_count =
        context.metrics.rejected_command_count.saturating_add(1);
    let rejection = CommandRejection {
        protocol_version: PROTOCOL_VERSION,
        session_id: Some(context.session_id.clone()),
        command_sequence,
        acknowledged_command_sequence: context.acknowledged_command_sequence,
        code,
        retry,
        message: message.to_owned(),
    };
    websocket.send(Message::Text(
        serde_json::to_string(&rejection)
            .expect("encode command rejection")
            .into(),
    ))
}

fn rejection_identity(rejection: InputCommandRejection) -> (RejectionCode, RetryDisposition) {
    match rejection {
        InputCommandRejection::SessionDisconnected
        | InputCommandRejection::WrongConnectionGeneration { .. } => {
            (RejectionCode::SessionClosed, RetryDisposition::Reconnect)
        }
        InputCommandRejection::StaleSequence { .. } => {
            (RejectionCode::StaleSequence, RetryDisposition::Never)
        }
        InputCommandRejection::InvalidInput => {
            (RejectionCode::InvalidInput, RetryDisposition::Never)
        }
        InputCommandRejection::EdgeQueueSaturated { .. } => {
            (RejectionCode::EdgeQueueSaturated, RetryDisposition::Never)
        }
    }
}

fn fact_rejection_code(kind: &str) -> Option<RejectionCode> {
    match kind {
        "InputEdgeRejectedUnknownTarget" => Some(RejectionCode::UnknownTarget),
        "InputEdgeRejectedNotInteractable" => Some(RejectionCode::NotInteractable),
        "InputEdgeRejectedPaused" => Some(RejectionCode::Paused),
        "CombatRejectedCooldown" => Some(RejectionCode::Cooldown),
        "CombatRejectedNoAmmo" => Some(RejectionCode::NoAmmo),
        "CombatRejectedNoEquippedWeapon" => Some(RejectionCode::NoEquippedWeapon),
        "CombatRejectedPlayerDefeated" => Some(RejectionCode::PlayerDefeated),
        "InputEdgeRejectedInvalidWeaponSlot" => Some(RejectionCode::InvalidWeaponSlot),
        "InputEdgeRejectedWeaponNotOwned" => Some(RejectionCode::WeaponNotOwned),
        "InputEdgeRejectedWeaponAlreadySelected" => Some(RejectionCode::WeaponAlreadySelected),
        "InputEdgeRejectedPlayerDefeated" => Some(RejectionCode::PlayerDefeated),
        "InputEdgeRejectedInventory" => Some(RejectionCode::InternalDefect),
        _ => None,
    }
}

fn dynamic_diff(previous: &Value, current: &Value) -> Map<String, Value> {
    let Some(current) = current.as_object() else {
        return Map::new();
    };
    let previous = previous.as_object();
    current
        .iter()
        .filter(|(key, value)| previous.and_then(|state| state.get(*key)) != Some(*value))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn session_id(connection_generation: u64) -> String {
    format!("loading-bay-{connection_generation:016x}")
}

fn websocket_would_block(error: &WebSocketError) -> bool {
    matches!(
        error,
        WebSocketError::Io(io_error)
            if matches!(io_error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;

    #[test]
    fn dynamic_delta_contains_only_changed_owners() {
        let previous = json!({
            "tick": 1,
            "player": { "position": [1, 2, 3] },
            "weapon": { "ammoRemaining": 8 }
        });
        let current = json!({
            "tick": 2,
            "player": { "position": [1, 2, 4] },
            "weapon": { "ammoRemaining": 8 }
        });

        assert_eq!(
            dynamic_diff(&previous, &current),
            BTreeMap::from([
                ("player".to_owned(), json!({ "position": [1, 2, 4] })),
                ("tick".to_owned(), json!(2)),
            ])
            .into_iter()
            .collect()
        );
    }

    #[test]
    fn session_replacement_keeps_unchanged_static_resource_identity() {
        let mut context = SessionContext::new(1);
        context.previous_static_revision = Some("7:content-hash".to_owned());
        context.replace_connection(2);

        assert_eq!(
            context.previous_static_revision.as_deref(),
            Some("7:content-hash")
        );
        assert!(context.force_full);
        assert!(context.previous_dynamic.is_none());
        assert_eq!(context.snapshot_sequence, 0);
    }
}
