use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Default, Serialize)]
pub struct StellarDemoState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub balances: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub friendbot_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_tx_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_tx_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explorer_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StellarDemoResp {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub state: StellarDemoState,
    pub logs: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct StellarDemoWorkspaceCreateReq {
    #[serde(default)]
    pub alias_prefix: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StellarDemoWorkspaceReq {
    pub alias: String,
    #[serde(default)]
    pub contract_id: Option<String>,
    #[serde(default)]
    pub tx_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StellarDemoDeployReq {
    pub alias: String,
}

#[derive(Debug, Deserialize)]
pub struct StellarDemoInvokeReq {
    pub alias: String,
    pub contract_id: String,
}

#[derive(Debug, Deserialize)]
pub struct StellarDemoTxStatusReq {
    pub hash: String,
}

const STELLAR_DEMO_NETWORK: &str = "testnet";
const STELLAR_DEMO_INCREMENT_FUNCTION: &str = "increment";

#[derive(Debug, Clone)]
struct StellarDemoConfig {
    cli_bin: String,
    cli_timeout_secs: u64,
    config_dir: PathBuf,
    cargo_target_dir: PathBuf,
    horizon_url: String,
    friendbot_url: String,
    explorer_tx_base: String,
    http_timeout_secs: u64,
    http_connect_timeout_secs: u64,
    manifest_path: PathBuf,
    build_dir: PathBuf,
    wasm_override: Option<PathBuf>,
}

#[derive(Debug)]
struct FriendbotFundResult {
    status: String,
    tx_hash: Option<String>,
}

fn default_demo_horizon_url() -> String {
    "https://horizon-testnet.stellar.org".to_string()
}

fn default_demo_friendbot_url() -> String {
    "https://friendbot.stellar.org".to_string()
}

fn load_stellar_demo_config() -> StellarDemoConfig {
    let temp_root = env::temp_dir().join("neurochain_stellar_demo");
    let manifest_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("demo_contracts")
        .join("stellar_demo_contract")
        .join("Cargo.toml");

    StellarDemoConfig {
        cli_bin: env::var("NC_STELLAR_CLI").unwrap_or_else(|_| "stellar".to_string()),
        cli_timeout_secs: env::var("NC_STELLAR_DEMO_CLI_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(20),
        config_dir: env::var("NC_STELLAR_DEMO_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| temp_root.join("config")),
        cargo_target_dir: env::var("NC_STELLAR_DEMO_CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| temp_root.join("cargo_target")),
        horizon_url: env::var("NC_STELLAR_HORIZON_URL")
            .unwrap_or_else(|_| default_demo_horizon_url()),
        friendbot_url: env::var("NC_FRIENDBOT_URL")
            .unwrap_or_else(|_| default_demo_friendbot_url()),
        explorer_tx_base: env::var("NC_STELLAR_DEMO_EXPLORER_TX_BASE")
            .unwrap_or_else(|_| "https://stellar.expert/explorer/testnet/tx".to_string()),
        http_timeout_secs: env::var("NC_STELLAR_DEMO_HTTP_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(10),
        http_connect_timeout_secs: env::var("NC_STELLAR_DEMO_HTTP_CONNECT_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(5),
        manifest_path: env::var("NC_STELLAR_DEMO_MANIFEST_PATH")
            .map(PathBuf::from)
            .unwrap_or(manifest_root),
        build_dir: env::var("NC_STELLAR_DEMO_BUILD_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| temp_root.join("build")),
        wasm_override: env::var("NC_STELLAR_DEMO_WASM_PATH")
            .ok()
            .map(PathBuf::from),
    }
}

fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("failed to create directory {}", path.display()))
}

fn normalize_cli_output(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stdout.is_empty() && !stderr.is_empty() {
        return stderr;
    }
    stdout
}

fn run_stellar_cli(
    cfg: &StellarDemoConfig,
    args: &[String],
    cwd: Option<&Path>,
) -> Result<std::process::Output> {
    ensure_dir(&cfg.config_dir)?;
    ensure_dir(&cfg.cargo_target_dir)?;

    let mut cmd = Command::new(&cfg.cli_bin);
    cmd.arg("--config-dir")
        .arg(&cfg.config_dir)
        .args(args)
        .env("CARGO_TARGET_DIR", &cfg.cargo_target_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }

    let mut child = cmd
        .spawn()
        .map_err(anyhow::Error::from)
        .with_context(|| format!("failed to run {}", cfg.cli_bin))?;

    let start = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(anyhow::Error::from)
            .with_context(|| format!("failed waiting for {}", cfg.cli_bin))?
        {
            return collect_child_output(&mut child, status);
        }

        if start.elapsed() >= Duration::from_secs(cfg.cli_timeout_secs) {
            let _ = child.kill();
            let status = child
                .wait()
                .map_err(anyhow::Error::from)
                .with_context(|| format!("failed to collect timed out {}", cfg.cli_bin))?;
            let out = collect_child_output(&mut child, status)?;
            return Err(anyhow::anyhow!(
                "stellar CLI timeout after {}s: {}",
                cfg.cli_timeout_secs,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }

        thread::sleep(Duration::from_millis(40));
    }
}

fn collect_child_output(
    child: &mut std::process::Child,
    status: std::process::ExitStatus,
) -> Result<Output> {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    if let Some(mut out) = child.stdout.take() {
        out.read_to_end(&mut stdout)
            .map_err(anyhow::Error::from)
            .context("failed to read CLI stdout")?;
    }
    if let Some(mut err) = child.stderr.take() {
        err.read_to_end(&mut stderr)
            .map_err(anyhow::Error::from)
            .context("failed to read CLI stderr")?;
    }

    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn make_demo_http_client(cfg: &StellarDemoConfig) -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(cfg.http_timeout_secs))
        .connect_timeout(Duration::from_secs(cfg.http_connect_timeout_secs))
        .build()
        .map_err(anyhow::Error::from)
        .context("failed to build demo HTTP client")
}

fn run_stellar_cli_ok(
    cfg: &StellarDemoConfig,
    args: &[String],
    cwd: Option<&Path>,
) -> Result<String> {
    let output = run_stellar_cli(cfg, args, cwd)?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "stellar CLI error: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(normalize_cli_output(&output))
}

fn is_base32_char(c: char) -> bool {
    matches!(c, 'A'..='Z' | '2'..='7')
}

fn is_strkey(value: &str) -> bool {
    if value.len() != 56 {
        return false;
    }
    let first = value.chars().next().unwrap_or('\0');
    if first != 'G' && first != 'C' {
        return false;
    }
    value.chars().all(is_base32_char)
}

fn extract_tx_hash(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(json) = serde_json::from_str::<Value>(trimmed) {
        for key in ["hash", "tx_hash", "transaction_hash", "envelope_hash"] {
            if let Some(hash) = json.get(key).and_then(|v| v.as_str()) {
                if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Some(hash.to_string());
                }
            }
        }
    }

    let mut candidate = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_hexdigit() {
            candidate.push(ch);
        } else {
            if candidate.len() == 64 {
                return Some(candidate);
            }
            candidate.clear();
        }
    }
    if candidate.len() == 64 {
        return Some(candidate);
    }
    None
}

fn extract_contract_id(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(json) = serde_json::from_str::<Value>(trimmed) {
        for key in ["contract_id", "id"] {
            if let Some(contract_id) = json.get(key).and_then(|v| v.as_str()) {
                if is_strkey(contract_id) && contract_id.starts_with('C') {
                    return Some(contract_id.to_string());
                }
            }
        }
    }

    let mut candidate = String::new();
    for ch in trimmed.chars() {
        if is_base32_char(ch) || ch == 'C' {
            candidate.push(ch);
        } else {
            if candidate.len() == 56 && candidate.starts_with('C') && is_strkey(&candidate) {
                return Some(candidate);
            }
            candidate.clear();
        }
    }
    if candidate.len() == 56 && candidate.starts_with('C') && is_strkey(&candidate) {
        return Some(candidate);
    }
    None
}

fn normalize_return(output: &str) -> Option<String> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.replace('\n', "\\n"))
}

fn sanitize_alias_prefix(raw: Option<&str>) -> String {
    let prefix = raw.unwrap_or("demo").trim();
    let mut cleaned = prefix
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect::<String>()
        .to_ascii_lowercase();
    if cleaned.is_empty() {
        cleaned = "demo".to_string();
    }
    cleaned.truncate(18);
    cleaned
}

fn unique_workspace_alias(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{prefix}-{millis}")
}

fn workspace_account(cfg: &StellarDemoConfig, alias: &str) -> Result<String> {
    let output = run_stellar_cli_ok(
        cfg,
        &[
            "keys".to_string(),
            "public-key".to_string(),
            alias.to_string(),
        ],
        None,
    )?;
    let account = output.trim().to_string();
    if !is_strkey(&account) || !account.starts_with('G') {
        return Err(anyhow::anyhow!(
            "invalid public key returned for alias {alias}"
        ));
    }
    Ok(account)
}

fn create_workspace_alias(
    cfg: &StellarDemoConfig,
    alias_prefix: Option<&str>,
) -> Result<(String, String)> {
    let alias = unique_workspace_alias(&sanitize_alias_prefix(alias_prefix));
    let _ = run_stellar_cli_ok(
        cfg,
        &["keys".to_string(), "generate".to_string(), alias.clone()],
        None,
    )?;
    let account = workspace_account(cfg, &alias)?;
    Ok((alias, account))
}

fn fetch_account(client: &Client, horizon_url: &str, account: &str) -> Result<Value> {
    let url = format!("{}/accounts/{}", horizon_url.trim_end_matches('/'), account);
    let resp = client.get(url).send().context("horizon request failed")?;
    if resp.status().as_u16() == 404 {
        return Err(anyhow::anyhow!("account not found"));
    }
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("horizon error: {}", resp.status()));
    }
    resp.json::<Value>()
        .map_err(anyhow::Error::from)
        .context("failed to parse horizon account response")
}

fn fetch_balances(client: &Client, horizon_url: &str, account: &str) -> Result<Vec<String>> {
    let json = fetch_account(client, horizon_url, account)?;
    let balances = json
        .get("balances")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing balances"))?;

    let mut out = Vec::new();
    for entry in balances {
        let asset_type = entry
            .get("asset_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let balance = entry.get("balance").and_then(|v| v.as_str()).unwrap_or("");
        let label = if asset_type == "native" {
            "XLM".to_string()
        } else {
            let code = entry
                .get("asset_code")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let issuer = entry
                .get("asset_issuer")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!("{code}:{issuer}")
        };
        out.push(format!("{label} = {balance}"));
    }
    Ok(out)
}

fn fetch_latest_tx_hash(client: &Client, horizon_url: &str, account: &str) -> Result<String> {
    let url = format!(
        "{}/accounts/{}/transactions?limit=1&order=desc",
        horizon_url.trim_end_matches('/'),
        account
    );
    let resp = client.get(url).send().context("horizon request failed")?;
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("horizon error: {}", resp.status()));
    }
    let json = resp.json::<Value>()?;
    let record = json
        .get("_embedded")
        .and_then(|v| v.get("records"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .ok_or_else(|| anyhow::anyhow!("no transactions found"))?;
    record
        .get("hash")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .ok_or_else(|| anyhow::anyhow!("missing tx hash"))
}

fn fetch_tx_status(client: &Client, horizon_url: &str, hash: &str) -> Result<String> {
    let url = format!(
        "{}/transactions/{}",
        horizon_url.trim_end_matches('/'),
        hash
    );
    let resp = client
        .get(url)
        .send()
        .context("horizon tx request failed")?;
    if resp.status().as_u16() == 404 {
        return Err(anyhow::anyhow!("transaction not found"));
    }
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("horizon tx error: {}", resp.status()));
    }
    let json: Value = resp
        .json()
        .map_err(anyhow::Error::from)
        .context("failed to parse horizon tx response")?;
    let successful = json
        .get("successful")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let ledger = json
        .get("ledger")
        .and_then(|v| v.as_i64())
        .map(|v| v.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    Ok(format!(
        "tx {hash}: successful={successful} ledger={ledger}"
    ))
}

fn friendbot_fund(
    client: &Client,
    friendbot_url: &str,
    account: &str,
) -> Result<FriendbotFundResult> {
    let url = format!("{}?addr={}", friendbot_url.trim_end_matches('/'), account);
    let resp = client
        .get(&url)
        .send()
        .context("friendbot request failed")?;
    let status = resp.status();
    let body = resp.text().unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow::anyhow!("friendbot error: {} {}", status, body));
    }

    let tx_hash = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|json| {
            json.get("hash")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string())
        })
        .or_else(|| extract_tx_hash(&body));

    Ok(FriendbotFundResult {
        status: "friendbot funded account".to_string(),
        tx_hash,
    })
}

fn ensure_demo_contract_wasm(cfg: &StellarDemoConfig) -> Result<PathBuf> {
    if let Some(path) = &cfg.wasm_override {
        if path.exists() {
            return Ok(path.clone());
        }
        return Err(anyhow::anyhow!(
            "NC_STELLAR_DEMO_WASM_PATH does not exist: {}",
            path.display()
        ));
    }

    if !cfg.manifest_path.exists() {
        return Err(anyhow::anyhow!(
            "demo contract manifest not found: {}",
            cfg.manifest_path.display()
        ));
    }

    ensure_dir(&cfg.build_dir)?;

    let _ = run_stellar_cli_ok(
        cfg,
        &[
            "contract".to_string(),
            "build".to_string(),
            "--manifest-path".to_string(),
            cfg.manifest_path.display().to_string(),
            "--package".to_string(),
            "stellar_demo_contract".to_string(),
            "--out-dir".to_string(),
            cfg.build_dir.display().to_string(),
        ],
        None,
    )?;

    let preferred = cfg.build_dir.join("stellar_demo_contract.wasm");
    if preferred.exists() {
        return Ok(preferred);
    }

    fs::read_dir(&cfg.build_dir)
        .map_err(anyhow::Error::from)?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("wasm"))
        .ok_or_else(|| anyhow::anyhow!("demo contract build completed but no wasm was found"))
}

fn deploy_demo_contract(
    cfg: &StellarDemoConfig,
    alias: &str,
) -> Result<(String, String, Option<String>)> {
    let wasm_path = ensure_demo_contract_wasm(cfg)?;
    let contract_alias = format!("{alias}-demo");
    let output = run_stellar_cli_ok(
        cfg,
        &[
            "contract".to_string(),
            "deploy".to_string(),
            "--source-account".to_string(),
            alias.to_string(),
            "--network".to_string(),
            STELLAR_DEMO_NETWORK.to_string(),
            "--alias".to_string(),
            contract_alias.clone(),
            "--wasm".to_string(),
            wasm_path.display().to_string(),
        ],
        None,
    )?;
    let tx_hash_from_cli = extract_tx_hash(&output);
    let contract_id = extract_contract_id(&output)
        .or_else(|| {
            let trimmed = output.trim();
            if is_strkey(trimmed) && trimmed.starts_with('C') {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow::anyhow!("failed to parse deployed contract id from CLI output"))?;

    let account = workspace_account(cfg, alias)?;
    let client = make_demo_http_client(cfg)?;
    let tx_hash =
        tx_hash_from_cli.or_else(|| fetch_latest_tx_hash(&client, &cfg.horizon_url, &account).ok());

    Ok((contract_id, contract_alias, tx_hash))
}

fn invoke_demo_contract(
    cfg: &StellarDemoConfig,
    alias: &str,
    contract_id: &str,
) -> Result<(Option<String>, Option<String>)> {
    let output = run_stellar_cli_ok(
        cfg,
        &[
            "contract".to_string(),
            "invoke".to_string(),
            "--id".to_string(),
            contract_id.to_string(),
            "--source".to_string(),
            alias.to_string(),
            "--network".to_string(),
            STELLAR_DEMO_NETWORK.to_string(),
            "--".to_string(),
            STELLAR_DEMO_INCREMENT_FUNCTION.to_string(),
        ],
        None,
    )?;

    let tx_hash_from_cli = extract_tx_hash(&output);
    let account = workspace_account(cfg, alias)?;
    let client = make_demo_http_client(cfg)?;
    let tx_hash =
        tx_hash_from_cli.or_else(|| fetch_latest_tx_hash(&client, &cfg.horizon_url, &account).ok());

    Ok((tx_hash, normalize_return(&output)))
}

fn explorer_tx_url(cfg: &StellarDemoConfig, hash: &str) -> String {
    format!("{}/{}", cfg.explorer_tx_base.trim_end_matches('/'), hash)
}

fn build_demo_state(
    cfg: &StellarDemoConfig,
    alias: Option<String>,
    account: Option<String>,
    contract_id: Option<String>,
    contract_alias: Option<String>,
    tx_hash: Option<String>,
    friendbot_status: Option<String>,
    last_result: Option<String>,
) -> StellarDemoState {
    let client = match make_demo_http_client(cfg) {
        Ok(c) => c,
        Err(_) => {
            return state_without_enriched_network_data(
                alias,
                account,
                contract_id,
                contract_alias,
                tx_hash,
                friendbot_status,
                last_result,
            );
        }
    };
    let mut state = StellarDemoState {
        alias,
        account,
        balances: Vec::new(),
        friendbot_status,
        contract_id,
        contract_alias,
        last_tx_hash: tx_hash,
        last_tx_status: None,
        last_result,
        explorer_url: None,
    };

    if let Some(account) = state.account.as_deref() {
        if let Ok(balances) = fetch_balances(&client, &cfg.horizon_url, account) {
            state.balances = balances;
        }
    }

    if let Some(hash) = state.last_tx_hash.as_deref() {
        state.explorer_url = Some(explorer_tx_url(cfg, hash));
        if let Ok(status) = fetch_tx_status(&client, &cfg.horizon_url, hash) {
            state.last_tx_status = Some(status);
        }
    }

    state
}

fn state_without_enriched_network_data(
    alias: Option<String>,
    account: Option<String>,
    contract_id: Option<String>,
    contract_alias: Option<String>,
    tx_hash: Option<String>,
    friendbot_status: Option<String>,
    last_result: Option<String>,
) -> StellarDemoState {
    StellarDemoState {
        alias,
        account,
        balances: Vec::new(),
        friendbot_status,
        contract_id,
        contract_alias,
        last_tx_hash: tx_hash,
        last_tx_status: None,
        last_result,
        explorer_url: None,
    }
}

pub fn handle_workspace_create(
    alias_prefix: Option<String>,
) -> Result<(StellarDemoState, Vec<String>)> {
    let cfg = load_stellar_demo_config();
    let (alias, account) = create_workspace_alias(&cfg, alias_prefix.as_deref())?;
    let state = build_demo_state(
        &cfg,
        Some(alias.clone()),
        Some(account.clone()),
        None,
        None,
        None,
        Some("workspace created".to_string()),
        None,
    );
    let logs = vec![
        format!("network={STELLAR_DEMO_NETWORK}"),
        format!("alias={alias}"),
        format!("account={account}"),
        format!("config_dir={}", cfg.config_dir.display()),
    ];
    Ok((state, logs))
}

pub fn handle_workspace_fund(
    alias: String,
    contract_id: Option<String>,
    tx_hash: Option<String>,
) -> Result<(StellarDemoState, Vec<String>)> {
    let cfg = load_stellar_demo_config();
    let account = workspace_account(&cfg, &alias)?;
    let client = make_demo_http_client(&cfg)?;
    let fund = friendbot_fund(&client, &cfg.friendbot_url, &account)?;
    let tx_hash = fund
        .tx_hash
        .or_else(|| fetch_latest_tx_hash(&client, &cfg.horizon_url, &account).ok())
        .or(tx_hash);
    let state = build_demo_state(
        &cfg,
        Some(alias.clone()),
        Some(account.clone()),
        contract_id,
        None,
        tx_hash,
        Some(fund.status.clone()),
        None,
    );
    let logs = vec![
        format!("network={STELLAR_DEMO_NETWORK}"),
        format!("alias={alias}"),
        format!("account={account}"),
        format!("friendbot_url={}", cfg.friendbot_url),
    ];
    Ok((state, logs))
}

pub fn handle_workspace_status(
    alias: String,
    contract_id: Option<String>,
    tx_hash: Option<String>,
) -> Result<(StellarDemoState, Vec<String>)> {
    let cfg = load_stellar_demo_config();
    let account = workspace_account(&cfg, &alias)?;
    let state = build_demo_state(
        &cfg,
        Some(alias.clone()),
        Some(account.clone()),
        contract_id,
        None,
        tx_hash,
        None,
        None,
    );
    let logs = vec![
        format!("network={STELLAR_DEMO_NETWORK}"),
        format!("alias={alias}"),
        format!("account={account}"),
    ];
    Ok((state, logs))
}

pub fn handle_contract_deploy(alias: String) -> Result<(StellarDemoState, Vec<String>)> {
    let cfg = load_stellar_demo_config();
    let account = workspace_account(&cfg, &alias)?;
    let (contract_id, contract_alias, tx_hash) = deploy_demo_contract(&cfg, &alias)?;
    let state = build_demo_state(
        &cfg,
        Some(alias.clone()),
        Some(account.clone()),
        Some(contract_id.clone()),
        Some(contract_alias.clone()),
        tx_hash,
        None,
        Some("contract deployed".to_string()),
    );
    let logs = vec![
        format!("network={STELLAR_DEMO_NETWORK}"),
        format!("alias={alias}"),
        format!("account={account}"),
        format!("contract_alias={contract_alias}"),
        format!("contract_id={contract_id}"),
    ];
    Ok((state, logs))
}

pub fn handle_contract_invoke(
    alias: String,
    contract_id: String,
) -> Result<(StellarDemoState, Vec<String>)> {
    let cfg = load_stellar_demo_config();
    let account = workspace_account(&cfg, &alias)?;
    let (tx_hash, last_result) = invoke_demo_contract(&cfg, &alias, &contract_id)?;
    let state = build_demo_state(
        &cfg,
        Some(alias.clone()),
        Some(account.clone()),
        Some(contract_id.clone()),
        None,
        tx_hash,
        None,
        last_result,
    );
    let logs = vec![
        format!("network={STELLAR_DEMO_NETWORK}"),
        format!("alias={alias}"),
        format!("account={account}"),
        format!("contract_id={contract_id}"),
        format!("function={STELLAR_DEMO_INCREMENT_FUNCTION}"),
    ];
    Ok((state, logs))
}

pub fn handle_tx_status(hash: String) -> Result<(StellarDemoState, Vec<String>)> {
    let cfg = load_stellar_demo_config();
    let client = make_demo_http_client(&cfg)?;
    let status = fetch_tx_status(&client, &cfg.horizon_url, &hash)?;
    let state = StellarDemoState {
        last_tx_hash: Some(hash.clone()),
        last_tx_status: Some(status.clone()),
        explorer_url: Some(explorer_tx_url(&cfg, &hash)),
        ..StellarDemoState::default()
    };
    let logs = vec![
        format!("network={STELLAR_DEMO_NETWORK}"),
        format!("tx_hash={hash}"),
        format!("tx_status={status}"),
    ];
    Ok((state, logs))
}
