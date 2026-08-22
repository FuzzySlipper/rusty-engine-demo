//! Separate bounded WebSocket adapter for the public developer-command port.
//!
//! This connection never starts or disconnects gameplay. It submits to the
//! already-active product generation and waits for the ordinary game-loop
//! driver to reach Loading Bay's safe point.

use std::io::ErrorKind;
use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

use loading_bay_game::LoadingBayDeveloperCommandRequest;
use serde::{Deserialize, Serialize};
use tungstenite::handshake::server::{Request, Response};
use tungstenite::http::{header::SEC_WEBSOCKET_PROTOCOL, HeaderValue, StatusCode};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{accept_hdr_with_config, Error as WebSocketError, Message, WebSocket};

use super::SharedBrowserRuntime;

const DEVELOPER_PROTOCOL: &str = "loading-bay.developer-command.v1";
const MAX_DEVELOPER_MESSAGE_BYTES: usize = 64 * 1024;
const DEVELOPER_RESULT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum DeveloperSocketRequest {
    Discover,
    Execute {
        request: LoadingBayDeveloperCommandRequest,
    },
    Cancel {
        correlation: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum DeveloperSocketResponse<T: Serialize> {
    Success { value: T },
    Error { message: String },
}

pub(super) fn developer_upgrade_requested(stream: &TcpStream) -> bool {
    let mut prefix = [0_u8; 512];
    stream
        .peek(&mut prefix)
        .ok()
        .and_then(|length| std::str::from_utf8(&prefix[..length]).ok())
        .is_some_and(|request| request.starts_with("GET /api/developer-command "))
}

pub(super) fn run_developer_session(stream: TcpStream, runtime: Arc<SharedBrowserRuntime>) {
    let config = WebSocketConfig::default()
        .read_buffer_size(MAX_DEVELOPER_MESSAGE_BYTES)
        .write_buffer_size(0)
        .max_write_buffer_size(MAX_DEVELOPER_MESSAGE_BYTES * 2)
        .max_message_size(Some(MAX_DEVELOPER_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_DEVELOPER_MESSAGE_BYTES));
    let mut socket = match accept_hdr_with_config(stream, select_protocol, Some(config)) {
        Ok(socket) => socket,
        Err(error) => {
            eprintln!("developer-command WebSocket handshake failed: {error}");
            return;
        }
    };
    let _ = socket
        .get_ref()
        .set_read_timeout(Some(Duration::from_millis(20)));
    let _ = socket
        .get_ref()
        .set_write_timeout(Some(Duration::from_secs(2)));
    let request = match socket.read() {
        Ok(Message::Text(text)) => serde_json::from_str::<DeveloperSocketRequest>(&text)
            .map_err(|error| format!("invalid developer request: {error}")),
        Ok(_) => Err("developer request must be one text frame".to_owned()),
        Err(error) => Err(format!("could not read developer request: {error}")),
    };
    match request {
        Ok(DeveloperSocketRequest::Discover) => {
            let result = runtime
                .lock()
                .map_err(|_| "runtime lock poisoned".to_owned())
                .and_then(|host| host.discover_developer_commands());
            send_result(&mut socket, result);
        }
        Ok(DeveloperSocketRequest::Execute { request }) => {
            let correlation = request.correlation.to_string();
            let submitted = runtime
                .lock()
                .map_err(|_| "runtime lock poisoned".to_owned())
                .and_then(|mut host| host.submit_developer_command(request));
            if let Err(message) = submitted {
                send_result::<()>(&mut socket, Err(message));
                return;
            }
            await_result(&mut socket, &runtime, &correlation);
        }
        Ok(DeveloperSocketRequest::Cancel { correlation }) => {
            let cancelled = runtime
                .lock()
                .map(|mut host| host.cancel_developer_command(&correlation))
                .unwrap_or(false);
            send_result(&mut socket, Ok(cancelled));
        }
        Err(message) => send_result::<()>(&mut socket, Err(message)),
    }
}

fn await_result(
    socket: &mut WebSocket<TcpStream>,
    runtime: &Arc<SharedBrowserRuntime>,
    correlation: &str,
) {
    let deadline = Instant::now() + DEVELOPER_RESULT_TIMEOUT;
    loop {
        if let Some(response) = runtime
            .lock()
            .ok()
            .and_then(|mut host| host.poll_developer_command(correlation))
        {
            send_result(socket, Ok(response));
            return;
        }
        if Instant::now() >= deadline {
            let _ = runtime
                .lock()
                .map(|mut host| host.cancel_developer_command(correlation));
            send_result::<()>(socket, Err("developer command timed out".to_owned()));
            return;
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                if matches!(
                    serde_json::from_str::<DeveloperSocketRequest>(&text),
                    Ok(DeveloperSocketRequest::Cancel { correlation: requested })
                        if requested == correlation
                ) {
                    let _ = runtime
                        .lock()
                        .map(|mut host| host.cancel_developer_command(correlation));
                    send_result::<()>(socket, Err("developer command cancelled".to_owned()));
                    return;
                }
            }
            Ok(Message::Close(_))
            | Err(WebSocketError::ConnectionClosed | WebSocketError::AlreadyClosed) => {
                let _ = runtime
                    .lock()
                    .map(|mut host| host.cancel_developer_command(correlation));
                return;
            }
            Err(WebSocketError::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(error) => {
                eprintln!("developer-command WebSocket read failed: {error}");
                return;
            }
            _ => {}
        }
    }
}

fn send_result<T: Serialize>(socket: &mut WebSocket<TcpStream>, result: Result<T, String>) {
    let response = match result {
        Ok(value) => DeveloperSocketResponse::Success { value },
        Err(message) => DeveloperSocketResponse::Error { message },
    };
    if let Ok(value) = serde_json::to_string(&response) {
        let _ = socket.send(Message::Text(value.into()));
    }
}

#[allow(clippy::result_large_err)]
fn select_protocol(
    request: &Request,
    mut response: Response,
) -> Result<Response, tungstenite::handshake::server::ErrorResponse> {
    let supported = request
        .headers()
        .get(SEC_WEBSOCKET_PROTOCOL)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|candidate| candidate.trim() == DEVELOPER_PROTOCOL)
        });
    if !supported {
        return Err(tungstenite::http::Response::builder()
            .status(StatusCode::UPGRADE_REQUIRED)
            .header(SEC_WEBSOCKET_PROTOCOL, DEVELOPER_PROTOCOL)
            .body(Some("developer-command subprotocol is required".to_owned()))
            .expect("valid developer protocol rejection"));
    }
    response.headers_mut().insert(
        SEC_WEBSOCKET_PROTOCOL,
        HeaderValue::from_static(DEVELOPER_PROTOCOL),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{SocketAddr, TcpListener};
    use tungstenite::client::IntoClientRequest;

    fn connect(address: SocketAddr) -> WebSocket<TcpStream> {
        let stream = TcpStream::connect(address).expect("connect developer test socket");
        let mut request = format!("ws://{address}/api/developer-command")
            .into_client_request()
            .expect("developer client request");
        request.headers_mut().insert(
            SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_static(DEVELOPER_PROTOCOL),
        );
        tungstenite::client(request, stream)
            .expect("developer WebSocket handshake")
            .0
    }

    fn serve_one(
        listener: &TcpListener,
        runtime: &Arc<SharedBrowserRuntime>,
    ) -> std::thread::JoinHandle<()> {
        let listener = listener.try_clone().unwrap();
        let runtime = Arc::clone(runtime);
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept developer client");
            run_developer_session(stream, runtime);
        })
    }

    #[test]
    fn websocket_discovers_and_executes_only_through_the_active_safe_point() {
        let runtime = Arc::new(SharedBrowserRuntime::new(
            loading_bay_game::browser_adapter::BrowserRuntime::load(
                &super::super::default_project_path(),
                &super::super::default_save_root(),
            )
            .expect("admit developer WebSocket test runtime"),
        ));
        runtime.lock().unwrap().start_session();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = serve_one(&listener, &runtime);
        let mut socket = connect(address);
        socket
            .send(Message::Text(r#"{"kind":"discover"}"#.into()))
            .unwrap();
        let discovery_message = socket.read().unwrap().into_text().unwrap();
        let discovery: serde_json::Value = serde_json::from_str(&discovery_message).unwrap();
        assert_eq!(discovery["kind"], "success");
        assert!(discovery["value"]["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|command| command["id"] == "standard.inspect.entity"));
        socket.close(None).ok();
        server.join().unwrap();

        let server = serve_one(&listener, &runtime);
        let mut socket = connect(address);
        let request = serde_json::json!({
            "kind": "execute",
            "request": {
                "protocolVersion": discovery["value"]["protocolVersion"],
                "command": "standard.inspect.entity",
                "correlation": "browser-safe-point",
                "runtime": discovery["value"]["runtime"],
                "expected": {
                    "profile": discovery["value"]["profile"],
                    "revision": discovery["value"]["revision"],
                    "catalogEpoch": discovery["value"]["catalogEpoch"]
                },
                "payload": { "entity": "1" }
            }
        });
        socket
            .send(Message::Text(request.to_string().into()))
            .unwrap();
        std::thread::sleep(Duration::from_millis(25));
        assert!(runtime
            .lock()
            .unwrap()
            .poll_developer_command("browser-safe-point")
            .is_none());
        runtime
            .lock()
            .unwrap()
            .advance(loading_bay_game::FIXED_STEP_DURATION)
            .unwrap();
        let response: serde_json::Value =
            serde_json::from_str(&socket.read().unwrap().into_text().unwrap()).unwrap();
        assert_eq!(response["kind"], "success");
        assert_eq!(response["value"]["correlation"], "browser-safe-point");
        assert_eq!(response["value"]["outcome"]["kind"], "success");
        socket.close(None).ok();
        server.join().unwrap();
    }
}
