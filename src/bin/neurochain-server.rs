use std::{
    env, fs,
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
use neurochain::{
    actions::{validate_plan, ActionPlan, Allowlist},
    banner, engine,
    intent_stellar::{
        build_action_plan as build_intent_action_plan, classify as classify_intent_stellar,
        has_intent_blocking_issue, resolve_model_path as resolve_intent_model_path,
        threshold_from_env as intent_threshold_from_env, DEFAULT_INTENT_STELLAR_THRESHOLD,
    },
    interpreter, soroban_deep,
    soroban_deep::ContractPolicy,
};
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

#[derive(Deserialize, Debug)]
struct StellarIntentPlanReq {
    prompt: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    model_path: Option<String>,
    #[serde(default)]
    threshold: Option<f32>,
    #[serde(default)]
    allowlist_assets: Option<String>,
    #[serde(default)]
    allowlist_contracts: Option<String>,
    #[serde(default)]
    allowlist_enforce: Option<bool>,
    #[serde(default)]
    contract_policy_enforce: Option<bool>,
}

#[derive(Serialize)]
struct StellarIntentPlanResp {
    ok: bool,
    blocked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    plan: ActionPlan,
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
        "intent_stellar" | "stellar_intent" => format!("{base}/intent_stellar/model.onnx"),
        "toxic" => format!("{base}/toxic_quantized/model.onnx"),
        "macro" | "intent_macro" | "macro_intent" | "gpt2" | "generator" => {
            format!("{base}/intent_macro/model.onnx")
        }
        _ => return None,
    };
    Some(path)
}

fn parse_bool_value(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn allowlist_enforced(override_value: Option<bool>) -> bool {
    if let Some(value) = override_value {
        return value;
    }
    parse_bool_value(&env::var("NC_ALLOWLIST_ENFORCE").unwrap_or_default()).unwrap_or(false)
}

fn policy_enforced(override_value: Option<bool>) -> bool {
    if let Some(value) = override_value {
        return value;
    }
    matches!(
        env::var("NC_CONTRACT_POLICY_ENFORCE")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn load_contract_policies() -> Vec<ContractPolicy> {
    let mut policies = Vec::new();

    if let Ok(path) = env::var("NC_CONTRACT_POLICY") {
        if let Ok(data) = fs::read_to_string(&path) {
            match serde_json::from_str::<ContractPolicy>(&data) {
                Ok(policy) => policies.push(policy),
                Err(err) => eprintln!("Policy parse failed for {path}: {err}"),
            }
        } else {
            eprintln!("Policy file not found: {path}");
        }
    }

    let policy_dir = env::var("NC_CONTRACT_POLICY_DIR").unwrap_or_else(|_| "contracts".to_string());
    if let Ok(entries) = fs::read_dir(&policy_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let policy_path = path.join("policy.json");
                if let Ok(data) = fs::read_to_string(&policy_path) {
                    match serde_json::from_str::<ContractPolicy>(&data) {
                        Ok(policy) => policies.push(policy),
                        Err(err) => {
                            eprintln!("Policy parse failed for {}: {err}", policy_path.display())
                        }
                    }
                }
            }
        }
    }

    policies
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
    banner::print_banner();
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
        .route("/stellar/intent-plan", post(api_stellar_intent_plan))
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

async fn api_stellar_intent_plan(
    _state: State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<StellarIntentPlanReq>,
) -> impl IntoResponse {
    let mut logs: Vec<String> = Vec::new();

    if let Some(required) = required_api_key() {
        let ok = provided_api_key(&headers)
            .map(|got| secure_eq(got, required))
            .unwrap_or(false);
        if !ok {
            logs.push("auth: missing or invalid api key".into());
            return (
                StatusCode::UNAUTHORIZED,
                Json(StellarIntentPlanResp {
                    ok: false,
                    blocked: true,
                    exit_code: Some(1),
                    error: Some("unauthorized".to_string()),
                    plan: ActionPlan::default(),
                    logs,
                }),
            );
        }
    }

    let prompt = req.prompt.trim().to_string();
    if prompt.is_empty() {
        logs.push("warn: empty prompt".into());
        return (
            StatusCode::OK,
            Json(StellarIntentPlanResp {
                ok: false,
                blocked: true,
                exit_code: Some(2),
                error: Some("empty prompt".to_string()),
                plan: ActionPlan::default(),
                logs,
            }),
        );
    }

    let model_path = req
        .model_path
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .or_else(|| {
            req.model
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .and_then(resolve_model_path)
        })
        .unwrap_or_else(resolve_intent_model_path);
    logs.push(format!("model_path={model_path}"));

    let threshold = match req.threshold {
        Some(v) => v,
        None => match intent_threshold_from_env() {
            Ok(Some(v)) => v,
            Ok(None) => DEFAULT_INTENT_STELLAR_THRESHOLD,
            Err(err) => {
                return (
                    StatusCode::OK,
                    Json(StellarIntentPlanResp {
                        ok: false,
                        blocked: true,
                        exit_code: Some(2),
                        error: Some(format!("invalid intent threshold env: {err}")),
                        plan: ActionPlan::default(),
                        logs,
                    }),
                )
            }
        },
    };
    logs.push(format!("threshold={threshold:.2}"));

    let decision = match classify_intent_stellar(&prompt, &model_path, threshold) {
        Ok(decision) => decision,
        Err(err) => {
            return (
                StatusCode::OK,
                Json(StellarIntentPlanResp {
                    ok: false,
                    blocked: true,
                    exit_code: Some(1),
                    error: Some(format!("{err:#}")),
                    plan: ActionPlan::default(),
                    logs,
                }),
            );
        }
    };

    let policies = load_contract_policies();
    let mut plan = build_intent_action_plan(&prompt, &decision);
    plan.warnings
        .push(format!("intent_model: path={model_path}"));
    let template_report =
        soroban_deep::apply_contract_intent_templates(&prompt, &mut plan, &policies);
    logs.push(format!(
        "soroban_deep_template: expanded={} template={} contract_id={} function={} reason={}",
        template_report.expanded,
        template_report.template_name.as_deref().unwrap_or("(none)"),
        template_report.contract_id.as_deref().unwrap_or("(none)"),
        template_report.function.as_deref().unwrap_or("(none)"),
        template_report.reason.as_deref().unwrap_or("(none)")
    ));
    let typed_v2_report = soroban_deep::apply_policy_typed_templates_v2(&mut plan, &policies);
    logs.push(format!(
        "typed_template_v2: policy_slot_type_converted={} normalized_args={}",
        typed_v2_report.converted, typed_v2_report.normalized_args
    ));

    let assets_raw = req
        .allowlist_assets
        .clone()
        .unwrap_or_else(|| env::var("NC_ASSET_ALLOWLIST").unwrap_or_default());
    let contracts_raw = req
        .allowlist_contracts
        .clone()
        .unwrap_or_else(|| env::var("NC_SOROBAN_ALLOWLIST").unwrap_or_default());
    let allowlist = Allowlist::from_raw(&assets_raw, &contracts_raw);
    let allowlist_violations = validate_plan(&plan, &allowlist);
    let allowlist_is_enforced = allowlist_enforced(req.allowlist_enforce);
    logs.push(format!(
        "allowlist: violations={} enforced={allowlist_is_enforced}",
        allowlist_violations.len()
    ));

    for violation in &allowlist_violations {
        plan.warnings.push(format!(
            "allowlist warning: #{} {} ({})",
            violation.index, violation.action, violation.reason
        ));
    }

    let (policy_warnings, policy_errors) =
        soroban_deep::validate_contract_policies(&plan, &policies);
    let policy_is_enforced = policy_enforced(req.contract_policy_enforce);
    logs.push(format!(
        "policy: warnings={} errors={} enforced={policy_is_enforced}",
        policy_warnings.len(),
        policy_errors.len()
    ));

    for warning in &policy_warnings {
        plan.warnings.push(format!("policy warning: {warning}"));
    }
    for err in &policy_errors {
        plan.warnings.push(format!("policy error: {err}"));
    }

    let mut blocked = false;
    let mut exit_code = None;

    if allowlist_is_enforced && !allowlist_violations.is_empty() {
        blocked = true;
        exit_code = Some(3);
        logs.push("block: allowlist_enforced".to_string());
    }
    if policy_is_enforced && !policy_errors.is_empty() {
        blocked = true;
        if exit_code.is_none() {
            exit_code = Some(4);
        }
        logs.push("block: contract_policy_enforced".to_string());
    }
    if exit_code.is_none() && has_intent_blocking_issue(&plan) {
        blocked = true;
        exit_code = Some(5);
        logs.push("block: intent_safety".to_string());
    }

    (
        StatusCode::OK,
        Json(StellarIntentPlanResp {
            ok: !blocked,
            blocked,
            exit_code,
            error: None,
            plan,
            logs,
        }),
    )
}
