use std::{
    env,
    net::SocketAddr,
    process::Stdio,
    sync::{Arc, OnceLock},
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use neurochain::banner;
use serde::Deserialize;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::{mpsc, Semaphore},
    time::{timeout, Duration},
};
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
struct AppState {
    repl_sem: Arc<Semaphore>,
    allow_flow: bool,
}

#[derive(Deserialize, Debug, Default)]
struct StellarReplWsReq {
    #[serde(default)]
    debug: Option<String>,
}

static REQUIRED_API_KEY: OnceLock<Option<String>> = OnceLock::new();
static ALLOWED_ORIGINS: OnceLock<Vec<String>> = OnceLock::new();

fn required_api_key() -> Option<&'static str> {
    REQUIRED_API_KEY
        .get_or_init(|| {
            env::var("NC_API_KEY")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .as_deref()
}

fn provided_api_key(headers: &HeaderMap) -> Option<&str> {
    let from_x_api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if from_x_api_key.is_some() {
        return from_x_api_key;
    }

    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())?;

    const PREFIX: &str = "Bearer ";
    if auth.len() > PREFIX.len() && auth[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
        return Some(auth[PREFIX.len()..].trim());
    }

    None
}

fn secure_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.as_bytes().iter().zip(b.as_bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn normalize_origin(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_ascii_lowercase())
}

fn allowed_origins() -> &'static [String] {
    ALLOWED_ORIGINS.get_or_init(|| {
        let from_env = env::var("NC_STELLAR_DEMO_ALLOWED_ORIGINS")
            .ok()
            .map(|raw| {
                raw.split(',')
                    .filter_map(normalize_origin)
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        if !from_env.is_empty() {
            return from_env;
        }

        vec![
            "https://stellarzerolab.art".to_string(),
            "https://www.stellarzerolab.art".to_string(),
        ]
    })
}

fn origin_allowed(headers: &HeaderMap) -> bool {
    let allowlist = allowed_origins();
    if allowlist.iter().any(|v| v == "*") {
        return true;
    }

    let provided = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .and_then(normalize_origin);

    // Browser WS includes Origin. For non-browser clients, missing origin is allowed.
    let Some(origin) = provided else {
        return true;
    };

    allowlist.iter().any(|allowed| allowed == &origin)
}

fn parse_bool_value(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn demo_allow_flow() -> bool {
    let primary = env::var("NC_DEMO_ALLOW_FLOW")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let fallback = env::var("NC_STELLAR_DEMO_ALLOW_FLOW")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(raw) = primary.as_deref().or(fallback.as_deref()) {
        return parse_bool_value(raw).unwrap_or(false);
    }
    false
}

fn default_repl_bin_path() -> String {
    if let Some(path) = env::var("NC_STELLAR_REPL_BIN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        return path;
    }

    let fallback_name = if cfg!(windows) {
        "neurochain-stellar.exe"
    } else {
        "neurochain-stellar"
    };
    if let Ok(current) = env::current_exe() {
        if let Some(dir) = current.parent() {
            let sibling = dir.join(fallback_name);
            if sibling.exists() {
                return sibling.to_string_lossy().to_string();
            }
        }
    }

    "neurochain-stellar".to_string()
}

fn ws_text_message(text: impl Into<String>) -> Message {
    Message::Text(text.into().into())
}

async fn stream_child_output<R>(mut reader: R, tx: mpsc::UnboundedSender<String>)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
                if tx.send(chunk).is_err() {
                    break;
                }
            }
            Err(err) => {
                let _ = tx.send(format!("\r\n[repl] stream read error: {err}\r\n"));
                break;
            }
        }
    }
}

async fn handle_stellar_repl_socket(mut socket: WebSocket, state: Arc<AppState>, debug: bool) {
    let _permit = match state.repl_sem.clone().acquire_owned().await {
        Ok(permit) => permit,
        Err(_) => {
            let _ = socket
                .send(ws_text_message(
                    "[repl] server is shutting down, cannot open REPL session\r\n",
                ))
                .await;
            return;
        }
    };

    let repl_bin = default_repl_bin_path();
    let mut cmd = Command::new(&repl_bin);
    cmd.arg("--repl");
    if !state.allow_flow {
        cmd.arg("--no-flow");
    }
    if debug {
        cmd.arg("--debug");
    }
    cmd.env_remove("NO_COLOR");
    cmd.env("CLICOLOR_FORCE", "1");
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            let _ = socket
                .send(ws_text_message(format!(
                    "[repl] failed to start `{repl_bin}`: {err}\r\n"
                )))
                .await;
            return;
        }
    };

    let mut child_stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            let _ = socket
                .send(ws_text_message(
                    "[repl] failed: child stdin not available\r\n",
                ))
                .await;
            let _ = child.start_kill();
            let _ = child.wait().await;
            return;
        }
    };
    let child_stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = socket
                .send(ws_text_message(
                    "[repl] failed: child stdout not available\r\n",
                ))
                .await;
            let _ = child.start_kill();
            let _ = child.wait().await;
            return;
        }
    };
    let child_stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            let _ = socket
                .send(ws_text_message(
                    "[repl] failed: child stderr not available\r\n",
                ))
                .await;
            let _ = child.start_kill();
            let _ = child.wait().await;
            return;
        }
    };

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
    tokio::spawn(stream_child_output(child_stdout, out_tx.clone()));
    tokio::spawn(stream_child_output(child_stderr, out_tx));

    loop {
        tokio::select! {
            maybe_chunk = out_rx.recv() => {
                if let Some(chunk) = maybe_chunk {
                    if socket.send(ws_text_message(chunk)).await.is_err() {
                        break;
                    }
                }
            }
            child_status = child.wait() => {
                match child_status {
                    Ok(status) => {
                        let _ = socket
                            .send(ws_text_message(format!("\r\n[repl] process exited ({status})\r\n")))
                            .await;
                    }
                    Err(err) => {
                        let _ = socket
                            .send(ws_text_message(format!("\r\n[repl] process wait error: {err}\r\n")))
                            .await;
                    }
                }
                break;
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        let payload = text.to_string();
                        if payload.is_empty() {
                            continue;
                        }
                        if child_stdin.write_all(payload.as_bytes()).await.is_err() {
                            break;
                        }
                        let _ = child_stdin.flush().await;
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        if child_stdin.write_all(&bytes).await.is_err() {
                            break;
                        }
                        let _ = child_stdin.flush().await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        let _ = socket
                            .send(ws_text_message(format!("\r\n[repl] websocket error: {err}\r\n")))
                            .await;
                        break;
                    }
                }
            }
        }
    }

    let _ = child.start_kill();
    let _ = timeout(Duration::from_secs(1), child.wait()).await;
}

async fn api_stellar_repl_ws(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(req): Query<StellarReplWsReq>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if !origin_allowed(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }

    if let Some(required) = required_api_key() {
        let ok = provided_api_key(&headers)
            .map(|got| secure_eq(got, required))
            .unwrap_or(false);
        if !ok {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }

    let debug = req
        .debug
        .as_deref()
        .and_then(parse_bool_value)
        .unwrap_or(false);
    ws.on_upgrade(move |socket| handle_stellar_repl_socket(socket, state, debug))
}

#[tokio::main]
async fn main() {
    banner::print_banner();

    if required_api_key().is_none() {
        eprintln!(
            "NC_API_KEY is required for neurochain-stellar-demo-server (refusing to start without auth)"
        );
        std::process::exit(2);
    }

    let host = env::var("NC_STELLAR_DEMO_HOST")
        .or_else(|_| env::var("HOST"))
        .unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("NC_STELLAR_DEMO_PORT")
        .ok()
        .or_else(|| env::var("PORT").ok())
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(8082);
    let max_repl_sessions: usize = env::var("NC_MAX_REPL_SESSIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);

    let state = Arc::new(AppState {
        repl_sem: Arc::new(Semaphore::new(max_repl_sessions)),
        allow_flow: demo_allow_flow(),
    });
    let allow_flow = state.allow_flow;

    let api = Router::new()
        .route("/stellar/repl/ws", get(api_stellar_repl_ws))
        .with_state(state);

    let app = Router::new().nest("/api", api).layer(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
    );

    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("invalid demo server bind address");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind demo server listener");

    println!(
        "NeuroChain Stellar demo server listening on http://{addr} (demo_flow={})",
        if allow_flow { "on" } else { "off" }
    );
    println!("Allowed WS origins: {}", allowed_origins().join(", "));

    axum::serve(listener, app).await.expect("serve demo server");
}
