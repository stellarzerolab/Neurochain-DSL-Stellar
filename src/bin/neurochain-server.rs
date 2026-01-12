use std::{
    env,
    net::SocketAddr,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, OnceLock},
};

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use neurochain::{engine, interpreter};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::Semaphore,
    task,
    time::{timeout, Duration},
};
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
struct AppState {
    inference_sem: Arc<Semaphore>,
}

#[derive(Deserialize, Debug)]
struct AnalyzeReq {
    #[serde(default)]
    model: String,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Serialize)]
struct AnalyzeResp {
    ok: bool,
    output: String,
    logs: Vec<String>,
}

static REQUIRED_API_KEY: OnceLock<Option<String>> = OnceLock::new();

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

    // Support: Authorization: Bearer <token>
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

fn models_base() -> String {
    env::var("NC_MODELS_DIR").unwrap_or_else(|_| "/opt/neurochain/models".to_string())
}

fn resolve_model_path(id: &str) -> Option<String> {
    let base = models_base();
    let path = match id {
        "sst2" => format!("{base}/distilbert-sst2/model.onnx"),
        "factcheck" => format!("{base}/factcheck/model.onnx"),
        "intent" => format!("{base}/intent/model.onnx"),
        "toxic" => format!("{base}/toxic_quantized/model.onnx"),
        "macro" | "intent_macro" | "macro_intent" | "gpt2" | "generator" => {
            format!("{base}/intent_macro/model.onnx")
        }
        _ => return None,
    };
    Some(path)
}

fn normalize(s: &str) -> String {
    s.replace('\u{FEFF}', "")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "    ")
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::main]
async fn main() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("PANIC: {info}");
        if std::env::var("RUST_BACKTRACE").as_deref() != Ok("0") {
            eprintln!("(enable RUST_BACKTRACE=1 for backtrace)");
        }
    }));

    let max_infer: usize = env::var("NC_MAX_INFER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);

    let state = Arc::new(AppState {
        inference_sem: Arc::new(Semaphore::new(max_infer)),
    });

    let api = Router::new()
        .route("/analyze", post(api_analyze))
        .with_state(state);

    let app = Router::new().nest("/api", api).layer(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
    );

    let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8081);
    let addr: SocketAddr = format!("{host}:{port}").parse().expect("Invalid HOST/PORT");

    println!("NeuroChain API listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("ERROR: failed to bind to {addr}: {e}");
            eprintln!("Hint: is the port already in use?");
            eprintln!("  Linux:   `ss -tulpn | grep :{port}`");
            eprintln!("  Windows: `netstat -ano | findstr :{port}`");
            std::process::exit(1);
        });

    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("ERROR: server error: {e}");
        std::process::exit(1);
    }
}

async fn api_analyze(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<AnalyzeReq>,
) -> impl IntoResponse {
    let mut logs: Vec<String> = Vec::new();
    if !req.model.is_empty() {
        logs.push(format!("model={}", req.model));
    }

    if let Some(required) = required_api_key() {
        let ok = provided_api_key(&headers)
            .map(|got| secure_eq(got, required))
            .unwrap_or(false);
        if !ok {
            logs.push("auth: missing or invalid api key".into());
            return (
                StatusCode::UNAUTHORIZED,
                Json(AnalyzeResp {
                    ok: false,
                    output: "ERROR: unauthorized".into(),
                    logs,
                }),
            );
        }
    }

    let mut code = req.code.or(req.content).unwrap_or_default();
    if code.trim().is_empty() {
        logs.push("warn: empty input".into());
        return (
            StatusCode::OK,
            Json(AnalyzeResp {
                ok: false,
                output: "ERROR: empty input".into(),
                logs,
            }),
        );
    }

    if let Some(path) = resolve_model_path(&req.model) {
        let has_ai = code.lines().any(|l| l.trim_start().starts_with("AI:"));
        if !has_ai {
            code = format!("AI: \"{path}\"\n{code}");
            logs.push(format!("auto: injected AI model path {}", path));
        }
    } else if !req.model.is_empty() {
        logs.push(format!("warn: unknown model id '{}'", req.model));
    }

    let code = normalize(&code);

    let permit = match state.inference_sem.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            let maybe = timeout(
                Duration::from_millis(50),
                state.inference_sem.clone().acquire_owned(),
            )
            .await;
            match maybe {
                Ok(Ok(p)) => p,
                _ => {
                    logs.push("busy: inference slots full".into());
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        Json(AnalyzeResp {
                            ok: false,
                            output: "BUSY: inference slots full; please retry shortly.".into(),
                            logs,
                        }),
                    );
                }
            }
        }
    };

    let task_res = task::spawn_blocking(move || {
        catch_unwind(AssertUnwindSafe(|| {
            let mut interpreter = interpreter::Interpreter::new();
            engine::analyze(&code, &mut interpreter)
        }))
    })
    .await;

    drop(permit);

    let res = match task_res {
        Ok(inner) => inner,
        Err(e) => {
            logs.push(format!("join error: {e}"));
            return (
                StatusCode::OK,
                Json(AnalyzeResp {
                    ok: false,
                    output: "ERROR: internal join error in analyze()".into(),
                    logs,
                }),
            );
        }
    };

    match res {
        Ok(Ok(out)) => (
            StatusCode::OK,
            Json(AnalyzeResp {
                ok: true,
                output: out,
                logs,
            }),
        ),
        Ok(Err(e)) => (
            StatusCode::OK,
            Json(AnalyzeResp {
                ok: false,
                output: format!("ERROR: {e}"),
                logs,
            }),
        ),
        Err(panic) => {
            let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic.downcast_ref::<String>() {
                s.clone()
            } else {
                "internal panic in analyze()".to_string()
            };
            (
                StatusCode::OK,
                Json(AnalyzeResp {
                    ok: false,
                    output: format!("ERROR: {msg}"),
                    logs,
                }),
            )
        }
    }
}
