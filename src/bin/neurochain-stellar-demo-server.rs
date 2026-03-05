use std::{env, net::SocketAddr, sync::OnceLock};

use axum::{
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use neurochain::{
    banner,
    server_stellar_demo::{
        handle_contract_deploy, handle_contract_invoke, handle_tx_status, handle_workspace_create,
        handle_workspace_fund, handle_workspace_status, StellarDemoDeployReq, StellarDemoInvokeReq,
        StellarDemoResp, StellarDemoState, StellarDemoTxStatusReq, StellarDemoWorkspaceCreateReq,
        StellarDemoWorkspaceReq,
    },
};
use tokio::task;
use tower_http::cors::{Any, CorsLayer};

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

fn demo_auth_failed(headers: &HeaderMap, logs: &mut Vec<String>) -> bool {
    if let Some(required) = required_api_key() {
        let ok = provided_api_key(headers)
            .map(|got| secure_eq(got, required))
            .unwrap_or(false);
        if !ok {
            logs.push("auth: missing or invalid api key".into());
            return true;
        }
    }
    false
}

fn demo_error_json(
    error: impl Into<String>,
    logs: Vec<String>,
    state: StellarDemoState,
) -> (StatusCode, Json<StellarDemoResp>) {
    (
        StatusCode::OK,
        Json(StellarDemoResp {
            ok: false,
            error: Some(error.into()),
            state,
            logs,
        }),
    )
}

fn demo_ok_json(state: StellarDemoState, logs: Vec<String>) -> (StatusCode, Json<StellarDemoResp>) {
    (
        StatusCode::OK,
        Json(StellarDemoResp {
            ok: true,
            error: None,
            state,
            logs,
        }),
    )
}

async fn api_stellar_demo_workspace_create(
    headers: HeaderMap,
    Json(req): Json<StellarDemoWorkspaceCreateReq>,
) -> impl IntoResponse {
    let mut logs = vec!["demo_op=workspace_create".to_string()];
    if demo_auth_failed(&headers, &mut logs) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(StellarDemoResp {
                ok: false,
                error: Some("unauthorized".to_string()),
                state: StellarDemoState::default(),
                logs,
            }),
        );
    }

    let alias_prefix = req.alias_prefix.clone();
    let task = task::spawn_blocking(move || handle_workspace_create(alias_prefix)).await;

    match task {
        Ok(Ok((state, op_logs))) => {
            logs.extend(op_logs);
            demo_ok_json(state, logs)
        }
        Ok(Err(err)) => {
            logs.push(format!("error: {err:#}"));
            demo_error_json(err.to_string(), logs, StellarDemoState::default())
        }
        Err(err) => {
            logs.push(format!("join error: {err}"));
            demo_error_json(
                "internal join error in demo workspace create",
                logs,
                StellarDemoState::default(),
            )
        }
    }
}

async fn api_stellar_demo_workspace_fund(
    headers: HeaderMap,
    Json(req): Json<StellarDemoWorkspaceReq>,
) -> impl IntoResponse {
    let alias = req.alias.trim().to_string();
    let contract_id = req.contract_id.clone();
    let tx_hash = req.tx_hash.clone();
    let mut logs = vec!["demo_op=workspace_fund".to_string()];
    if demo_auth_failed(&headers, &mut logs) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(StellarDemoResp {
                ok: false,
                error: Some("unauthorized".to_string()),
                state: StellarDemoState::default(),
                logs,
            }),
        );
    }
    if alias.is_empty() {
        return demo_error_json("missing alias", logs, StellarDemoState::default());
    }

    let task =
        task::spawn_blocking(move || handle_workspace_fund(alias, contract_id, tx_hash)).await;

    match task {
        Ok(Ok((state, op_logs))) => {
            logs.extend(op_logs);
            demo_ok_json(state, logs)
        }
        Ok(Err(err)) => {
            logs.push(format!("error: {err:#}"));
            demo_error_json(err.to_string(), logs, StellarDemoState::default())
        }
        Err(err) => {
            logs.push(format!("join error: {err}"));
            demo_error_json(
                "internal join error in demo workspace fund",
                logs,
                StellarDemoState::default(),
            )
        }
    }
}

async fn api_stellar_demo_workspace_status(
    headers: HeaderMap,
    Json(req): Json<StellarDemoWorkspaceReq>,
) -> impl IntoResponse {
    let alias = req.alias.trim().to_string();
    let contract_id = req.contract_id.clone();
    let tx_hash = req.tx_hash.clone();
    let mut logs = vec!["demo_op=workspace_status".to_string()];
    if demo_auth_failed(&headers, &mut logs) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(StellarDemoResp {
                ok: false,
                error: Some("unauthorized".to_string()),
                state: StellarDemoState::default(),
                logs,
            }),
        );
    }
    if alias.is_empty() {
        return demo_error_json("missing alias", logs, StellarDemoState::default());
    }

    let task =
        task::spawn_blocking(move || handle_workspace_status(alias, contract_id, tx_hash)).await;

    match task {
        Ok(Ok((state, op_logs))) => {
            logs.extend(op_logs);
            demo_ok_json(state, logs)
        }
        Ok(Err(err)) => {
            logs.push(format!("error: {err:#}"));
            demo_error_json(err.to_string(), logs, StellarDemoState::default())
        }
        Err(err) => {
            logs.push(format!("join error: {err}"));
            demo_error_json(
                "internal join error in demo workspace status",
                logs,
                StellarDemoState::default(),
            )
        }
    }
}

async fn api_stellar_demo_contract_deploy(
    headers: HeaderMap,
    Json(req): Json<StellarDemoDeployReq>,
) -> impl IntoResponse {
    let alias = req.alias.trim().to_string();
    let contract_alias = req
        .contract_alias
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    let wasm = req
        .wasm
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    let mut logs = vec!["demo_op=contract_deploy".to_string()];
    if demo_auth_failed(&headers, &mut logs) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(StellarDemoResp {
                ok: false,
                error: Some("unauthorized".to_string()),
                state: StellarDemoState::default(),
                logs,
            }),
        );
    }
    if alias.is_empty() {
        return demo_error_json("missing alias", logs, StellarDemoState::default());
    }

    let task =
        task::spawn_blocking(move || handle_contract_deploy(alias, contract_alias, wasm)).await;

    match task {
        Ok(Ok((state, op_logs))) => {
            logs.extend(op_logs);
            demo_ok_json(state, logs)
        }
        Ok(Err(err)) => {
            logs.push(format!("error: {err:#}"));
            demo_error_json(err.to_string(), logs, StellarDemoState::default())
        }
        Err(err) => {
            logs.push(format!("join error: {err}"));
            demo_error_json(
                "internal join error in demo contract deploy",
                logs,
                StellarDemoState::default(),
            )
        }
    }
}

async fn api_stellar_demo_contract_invoke(
    headers: HeaderMap,
    Json(req): Json<StellarDemoInvokeReq>,
) -> impl IntoResponse {
    let alias = req.alias.trim().to_string();
    let contract_id = req.contract_id.trim().to_string();
    let mut logs = vec!["demo_op=contract_invoke".to_string()];
    if demo_auth_failed(&headers, &mut logs) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(StellarDemoResp {
                ok: false,
                error: Some("unauthorized".to_string()),
                state: StellarDemoState::default(),
                logs,
            }),
        );
    }
    if alias.is_empty() || contract_id.is_empty() {
        return demo_error_json(
            "missing alias or contract_id",
            logs,
            StellarDemoState::default(),
        );
    }

    let task = task::spawn_blocking(move || handle_contract_invoke(alias, contract_id)).await;

    match task {
        Ok(Ok((state, op_logs))) => {
            logs.extend(op_logs);
            demo_ok_json(state, logs)
        }
        Ok(Err(err)) => {
            logs.push(format!("error: {err:#}"));
            demo_error_json(err.to_string(), logs, StellarDemoState::default())
        }
        Err(err) => {
            logs.push(format!("join error: {err}"));
            demo_error_json(
                "internal join error in demo contract invoke",
                logs,
                StellarDemoState::default(),
            )
        }
    }
}

async fn api_stellar_demo_tx_status(
    headers: HeaderMap,
    Json(req): Json<StellarDemoTxStatusReq>,
) -> impl IntoResponse {
    let hash = req.hash.trim().to_string();
    let mut logs = vec!["demo_op=tx_status".to_string()];
    if demo_auth_failed(&headers, &mut logs) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(StellarDemoResp {
                ok: false,
                error: Some("unauthorized".to_string()),
                state: StellarDemoState::default(),
                logs,
            }),
        );
    }
    if hash.is_empty() {
        return demo_error_json("missing hash", logs, StellarDemoState::default());
    }

    let task = task::spawn_blocking(move || handle_tx_status(hash)).await;

    match task {
        Ok(Ok((state, op_logs))) => {
            logs.extend(op_logs);
            demo_ok_json(state, logs)
        }
        Ok(Err(err)) => {
            logs.push(format!("error: {err:#}"));
            demo_error_json(err.to_string(), logs, StellarDemoState::default())
        }
        Err(err) => {
            logs.push(format!("join error: {err}"));
            demo_error_json(
                "internal join error in demo tx status",
                logs,
                StellarDemoState::default(),
            )
        }
    }
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

    let api = Router::new()
        .route(
            "/stellar/demo/workspace/create",
            post(api_stellar_demo_workspace_create),
        )
        .route(
            "/stellar/demo/workspace/fund",
            post(api_stellar_demo_workspace_fund),
        )
        .route(
            "/stellar/demo/workspace/status",
            post(api_stellar_demo_workspace_status),
        )
        .route(
            "/stellar/demo/contract/deploy",
            post(api_stellar_demo_contract_deploy),
        )
        .route(
            "/stellar/demo/contract/invoke",
            post(api_stellar_demo_contract_invoke),
        )
        .route("/stellar/demo/tx/status", post(api_stellar_demo_tx_status));

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

    println!("NeuroChain Stellar demo server listening on http://{addr}");

    axum::serve(listener, app).await.expect("serve demo server");
}
