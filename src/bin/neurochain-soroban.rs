use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use neurochain::actions::{
    parse_action_plan_from_nc, validate_plan, Action, ActionPlan, Allowlist,
};
use neurochain::banner;
use neurochain::intent_stellar::{
    build_action_plan as build_intent_action_plan, classify as classify_intent_stellar,
    has_intent_blocking_issue, resolve_model_path as resolve_intent_model_path,
    threshold_from_env as intent_threshold_from_env, DEFAULT_INTENT_STELLAR_THRESHOLD,
};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;

fn print_usage() {
    eprintln!(
        "Usage: neurochain-soroban [<file.nc|plan.json>] [--flow] [--yes] [--intent-text \"...\"] [--intent-model <path>] [--intent-threshold <f32>]"
    );
    eprintln!("Usage: neurochain-soroban --repl");
    eprintln!("If no args are provided, REPL mode is started.");
    eprintln!("If input is JSON, it is treated as an ActionPlan.");
    eprintln!(
        "Manual .nc lines can start with 'stellar.' or 'soroban.' (comment lines are ignored)."
    );
    eprintln!(
        ".nc files also support: AI: \"...\", network: testnet|mainnet|public, wallet/source: <alias>."
    );
    eprintln!("--intent-text enables IntentStellar -> ActionPlan mode.");
    eprintln!("--intent-model overrides the intent_stellar model path.");
    eprintln!("--intent-threshold overrides confidence threshold (default: 0.55).");
    eprintln!("Set NC_ALLOWLIST_ENFORCE=1 to hard-fail on allowlist violations.");
    eprintln!("--flow enables simulate → preview → confirm → submit.");
    eprintln!("--yes auto-confirms submit in --flow mode.");
    eprintln!("Flow in intent mode is blocked when plan has Unknown/intent_error (exit code 5).");
    eprintln!("Env: NC_STELLAR_NETWORK / NC_SOROBAN_NETWORK (default: testnet)");
    eprintln!("Env: NC_STELLAR_HORIZON_URL (default: testnet Horizon)");
    eprintln!("Env: NC_FRIENDBOT_URL (default: testnet Friendbot)");
    eprintln!("Env: NC_SOROBAN_SOURCE or NC_STELLAR_SOURCE (for soroban invoke)");
    eprintln!("Env: NC_STELLAR_CLI (default: stellar)");
    eprintln!("Env: NC_SOROBAN_SIMULATE_FLAG (default: \"--send no\")");
    eprintln!("Env: NC_TXREP_PREVIEW=1 (include txrep in preview output)");
    eprintln!("Env: NC_INTENT_STELLAR_MODEL (default: models/intent_stellar/model.onnx)");
    eprintln!("Env: NC_INTENT_STELLAR_THRESHOLD (default: 0.55)");
    eprintln!("Env: NC_CONTRACT_POLICY=path/to/policy.json");
    eprintln!("Env: NC_CONTRACT_POLICY_DIR=contracts");
    eprintln!("Env: NC_CONTRACT_POLICY_ENFORCE=1 (hard-fail on policy violations)");
}

#[derive(Debug, Default)]
struct CliArgs {
    repl: bool,
    path: Option<String>,
    flow: bool,
    auto_yes: bool,
    intent_text: Option<String>,
    intent_model: Option<String>,
    intent_threshold: Option<f32>,
}

fn parse_cli_args(args: &[String]) -> Result<CliArgs> {
    let mut out = CliArgs::default();
    if args.len() <= 1 {
        out.repl = true;
        return Ok(out);
    }

    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--repl" => out.repl = true,
            "--flow" => out.flow = true,
            "--yes" | "-y" => out.auto_yes = true,
            "--intent-text" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| anyhow!("missing value for --intent-text"))?;
                out.intent_text = Some(value.clone());
            }
            "--intent-model" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| anyhow!("missing value for --intent-model"))?;
                out.intent_model = Some(value.clone());
            }
            "--intent-threshold" => {
                i += 1;
                let raw = args
                    .get(i)
                    .ok_or_else(|| anyhow!("missing value for --intent-threshold"))?;
                let value = raw
                    .parse::<f32>()
                    .with_context(|| format!("invalid --intent-threshold value: {raw}"))?;
                out.intent_threshold = Some(value);
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other if other.starts_with('-') => {
                return Err(anyhow!("unknown flag: {other}"));
            }
            other => {
                if out.path.is_none() {
                    out.path = Some(other.to_string());
                } else {
                    return Err(anyhow!("multiple input paths are not supported"));
                }
            }
        }
        i += 1;
    }

    if out.repl {
        if out.path.is_some() || out.intent_text.is_some() {
            return Err(anyhow!(
                "--repl cannot be combined with <file> or --intent-text"
            ));
        }
        return Ok(out);
    }

    if out.path.is_some() && out.intent_text.is_some() {
        return Err(anyhow!("use either <file> or --intent-text, not both"));
    }
    if out.path.is_none() && out.intent_text.is_none() {
        return Err(anyhow!("missing input path or --intent-text"));
    }

    Ok(out)
}

fn allowlist_enforced() -> bool {
    matches!(
        std::env::var("NC_ALLOWLIST_ENFORCE")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn policy_enforced() -> bool {
    matches!(
        std::env::var("NC_CONTRACT_POLICY_ENFORCE")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[derive(Debug, Clone, Deserialize)]
struct ArgSchema {
    #[serde(default)]
    required: HashMap<String, String>,
    #[serde(default)]
    optional: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ContractPolicy {
    contract_id: String,
    #[serde(default)]
    allowed_functions: Vec<String>,
    #[serde(default)]
    args_schema: HashMap<String, ArgSchema>,
    #[serde(default)]
    max_fee_stroops: Option<u64>,
    #[serde(default)]
    resource_limits: Option<Value>,
}

#[derive(Debug)]
struct Preview {
    fee_estimate: String,
    effects: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct NetworkConfig {
    horizon_url: String,
    friendbot_url: Option<String>,
    soroban_network: String,
    soroban_source: Option<String>,
    soroban_cli: String,
    soroban_simulate_args: Vec<String>,
    txrep_preview: bool,
}

fn parse_simulate_args(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut parts: Vec<String> = trimmed
        .split_whitespace()
        .map(|part| part.to_string())
        .collect();
    if parts.len() == 1 && parts[0] == "--send" {
        parts.push("no".to_string());
    }
    parts
}

fn default_horizon_url(network: &str) -> String {
    match network {
        "public" | "pubnet" | "mainnet" => "https://horizon.stellar.org".to_string(),
        _ => "https://horizon-testnet.stellar.org".to_string(),
    }
}

fn default_friendbot_url(network: &str) -> Option<String> {
    match network {
        "testnet" => Some("https://friendbot.stellar.org".to_string()),
        _ => None,
    }
}

fn load_network_config() -> NetworkConfig {
    let network = env::var("NC_STELLAR_NETWORK")
        .or_else(|_| env::var("NC_SOROBAN_NETWORK"))
        .unwrap_or_else(|_| "testnet".to_string());

    let horizon_url =
        env::var("NC_STELLAR_HORIZON_URL").unwrap_or_else(|_| default_horizon_url(&network));

    let friendbot_url = env::var("NC_FRIENDBOT_URL")
        .ok()
        .or_else(|| default_friendbot_url(&network));

    let soroban_source = env::var("NC_SOROBAN_SOURCE")
        .or_else(|_| env::var("NC_STELLAR_SOURCE"))
        .ok();

    let soroban_cli = env::var("NC_STELLAR_CLI").unwrap_or_else(|_| "stellar".to_string());
    let soroban_simulate_raw =
        env::var("NC_SOROBAN_SIMULATE_FLAG").unwrap_or_else(|_| "--send no".to_string());
    let soroban_simulate_args = parse_simulate_args(&soroban_simulate_raw);
    let txrep_preview = matches!(
        env::var("NC_TXREP_PREVIEW")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    );

    NetworkConfig {
        horizon_url,
        friendbot_url,
        soroban_network: network,
        soroban_source,
        soroban_cli,
        soroban_simulate_args,
        txrep_preview,
    }
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

fn is_symbol(value: &str) -> bool {
    let len = value.len();
    if len == 0 || len > 32 {
        return false;
    }
    value
        .chars()
        .all(|c| c.is_ascii() && !c.is_control() && !c.is_whitespace())
}

fn is_hex_bytes(value: &str) -> bool {
    if !value.starts_with("0x") {
        return false;
    }
    let hex = &value[2..];
    if hex.is_empty() || !hex.len().is_multiple_of(2) {
        return false;
    }
    hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn validate_arg_type(value: &Value, ty: &str) -> bool {
    match ty {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "bool" => value.is_boolean(),
        "address" => value.as_str().map(is_strkey).unwrap_or(false),
        "symbol" => value.as_str().map(is_symbol).unwrap_or(false),
        "bytes" => value.as_str().map(is_hex_bytes).unwrap_or(false),
        _ => false,
    }
}

fn validate_contract_policies(
    plan: &ActionPlan,
    policies: &[ContractPolicy],
) -> (Vec<String>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    if policies.is_empty() {
        return (warnings, errors);
    }

    let mut map: HashMap<String, ContractPolicy> = HashMap::new();
    for policy in policies {
        map.insert(policy.contract_id.clone(), policy.clone());
    }

    for action in &plan.actions {
        if let neurochain::actions::Action::SorobanContractInvoke {
            contract_id,
            function,
            args,
        } = action
        {
            let Some(policy) = map.get(contract_id) else {
                errors.push(format!(
                    "policy_missing: no policy for contract_id {contract_id}"
                ));
                continue;
            };
            if !policy.allowed_functions.is_empty()
                && !policy.allowed_functions.iter().any(|f| f == function)
            {
                errors.push(format!(
                    "policy_function_denied: {contract_id}:{function} not allowed"
                ));
                continue;
            }

            if let Some(schema) = policy.args_schema.get(function) {
                let args_obj = args.as_object();
                if args_obj.is_none() {
                    errors.push(format!(
                        "policy_args_invalid: {contract_id}:{function} args must be object"
                    ));
                    continue;
                }
                let args_obj = args_obj.unwrap();

                for (key, ty) in &schema.required {
                    match args_obj.get(key) {
                        Some(val) => {
                            if !validate_arg_type(val, ty) {
                                errors.push(format!(
                                    "policy_args_type: {contract_id}:{function} {key} expected {ty}"
                                ));
                            }
                        }
                        None => errors.push(format!(
                            "policy_args_missing: {contract_id}:{function} missing {key}"
                        )),
                    }
                }

                for (key, ty) in &schema.optional {
                    if let Some(val) = args_obj.get(key) {
                        if !validate_arg_type(val, ty) {
                            errors.push(format!(
                                "policy_args_type: {contract_id}:{function} {key} expected {ty}"
                            ));
                        }
                    }
                }

                for key in args_obj.keys() {
                    if !schema.required.contains_key(key) && !schema.optional.contains_key(key) {
                        warnings.push(format!(
                            "policy_args_unknown: {contract_id}:{function} unexpected arg {key}"
                        ));
                    }
                }
            }

            if let Some(limits) = &policy.resource_limits {
                if !limits.is_object() {
                    warnings.push(format!(
                        "policy_resource_limits_invalid: {contract_id} resource_limits must be object"
                    ));
                }
            }

            if let Some(max_fee) = policy.max_fee_stroops {
                warnings.push(format!(
                    "policy_hint: {contract_id}:{function} max_fee_stroops={max_fee}"
                ));
            }
        }
    }

    (warnings, errors)
}

fn estimate_op_count(plan: &ActionPlan) -> usize {
    plan.actions
        .iter()
        .filter(|action| {
            matches!(
                action.kind(),
                "stellar.account.create"
                    | "stellar.change_trust"
                    | "stellar.payment"
                    | "soroban.contract.invoke"
            )
        })
        .count()
}

fn fetch_base_fee(client: &Client, horizon_url: &str) -> Option<u64> {
    let url = format!("{}/fee_stats", horizon_url.trim_end_matches('/'));
    let resp = client.get(url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: Value = resp.json().ok()?;
    json.get("last_ledger_base_fee")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<u64>().ok())
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
        return Err(anyhow!("transaction not found"));
    }
    if !resp.status().is_success() {
        return Err(anyhow!("horizon tx error: {}", resp.status()));
    }
    let json: Value = resp.json().context("failed to parse horizon tx response")?;
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

fn parse_amount_to_stroops(raw: &str) -> Result<String> {
    let cleaned = raw.trim().replace('_', "");
    if cleaned.is_empty() {
        return Err(anyhow!("amount is empty"));
    }
    if !cleaned.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Err(anyhow!("amount must be numeric"));
    }
    let mut parts = cleaned.splitn(2, '.');
    let whole = parts.next().unwrap_or("0");
    let frac = parts.next().unwrap_or("");
    if frac.len() > 7 {
        return Err(anyhow!("amount has more than 7 decimal places"));
    }
    let mut frac_padded = frac.to_string();
    while frac_padded.len() < 7 {
        frac_padded.push('0');
    }
    let whole_val: u128 = whole.parse().unwrap_or(0);
    let frac_val: u128 = if frac_padded.is_empty() {
        0
    } else {
        frac_padded.parse().unwrap_or(0)
    };
    let stroops = whole_val
        .saturating_mul(10_000_000u128)
        .saturating_add(frac_val);
    Ok(stroops.to_string())
}

fn normalize_cli_output(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stdout.is_empty() && !stderr.is_empty() {
        return stderr;
    }
    stdout
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

fn try_hash_via_cli(cfg: &NetworkConfig, xdr: &str) -> Option<String> {
    if xdr.trim().is_empty() {
        return None;
    }
    let output = Command::new(&cfg.soroban_cli)
        .args([
            "tx",
            "hash",
            "--xdr",
            xdr,
            "--network",
            &cfg.soroban_network,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    extract_tx_hash(&stdout).or_else(|| {
        if stdout.len() == 64 && stdout.chars().all(|c| c.is_ascii_hexdigit()) {
            Some(stdout)
        } else {
            None
        }
    })
}

fn normalize_return(output: &str) -> Option<String> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return None;
    }
    let single_line = trimmed.replace('\n', "\\n");
    Some(single_line)
}

fn format_submit_ok(label: &str, hash: Option<String>, output: &str, note: Option<&str>) -> String {
    let hash_text = hash.unwrap_or_else(|| "-".to_string());
    let mut return_text = normalize_return(output).unwrap_or_else(|| "-".to_string());
    if let Some(note) = note {
        return_text = format!("{return_text} ({note})");
    }
    format!("{label} | status=ok | tx_hash={hash_text} | return={return_text}")
}

fn format_submit_error(label: &str, stage: &str, err: &str) -> String {
    let err_text = err.trim().replace('\n', "\\n");
    format!("{label} | status=error | stage={stage} | error={err_text}")
}

fn stellar_tx_new(cfg: &NetworkConfig, args: &[String]) -> Result<String> {
    let source = cfg
        .soroban_source
        .as_deref()
        .ok_or_else(|| anyhow!("NC_SOROBAN_SOURCE/NC_STELLAR_SOURCE not set"))?;
    let mut cmd = Command::new(&cfg.soroban_cli);
    cmd.arg("tx")
        .arg("new")
        .args(args)
        .arg("--source-account")
        .arg(source)
        .arg("--network")
        .arg(&cfg.soroban_network);
    let output = cmd
        .output()
        .with_context(|| format!("failed to run {}", cfg.soroban_cli))?;
    if !output.status.success() {
        return Err(anyhow!(
            "stellar CLI error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(normalize_cli_output(&output))
}

fn stellar_tx_build_only(cfg: &NetworkConfig, args: &[String]) -> Result<String> {
    let source = cfg
        .soroban_source
        .as_deref()
        .ok_or_else(|| anyhow!("NC_SOROBAN_SOURCE/NC_STELLAR_SOURCE not set"))?;
    let mut cmd = Command::new(&cfg.soroban_cli);
    cmd.arg("tx")
        .arg("new")
        .args(args)
        .arg("--source-account")
        .arg(source)
        .arg("--network")
        .arg(&cfg.soroban_network)
        .arg("--build-only");
    let output = cmd
        .output()
        .with_context(|| format!("failed to run {}", cfg.soroban_cli))?;
    if !output.status.success() {
        return Err(anyhow!(
            "stellar CLI error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(normalize_cli_output(&output))
}

fn soroban_cli_build(
    cfg: &NetworkConfig,
    contract_id: &str,
    function: &str,
    args: &Value,
) -> Result<String> {
    let source = cfg
        .soroban_source
        .as_ref()
        .ok_or_else(|| anyhow!("NC_SOROBAN_SOURCE is not set"))?;

    let mut cmd = Command::new(&cfg.soroban_cli);
    cmd.args([
        "contract",
        "invoke",
        "--id",
        contract_id,
        "--source",
        source,
        "--network",
        &cfg.soroban_network,
        "--build-only",
    ]);
    cmd.arg("--");
    cmd.arg(function);
    for (key, value) in args_to_cli(args) {
        cmd.arg(format!("--{key}")).arg(value);
    }
    let output = cmd.output().context("failed to run stellar CLI")?;
    if !output.status.success() {
        return Err(anyhow!(
            "stellar CLI error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(normalize_cli_output(&output))
}

fn xdr_to_txrep(cfg: &NetworkConfig, xdr: &str) -> Result<String> {
    if xdr.trim().is_empty() {
        return Err(anyhow!("empty xdr"));
    }
    let output = Command::new(&cfg.soroban_cli)
        .args(["tx", "to-rep", "--xdr", xdr])
        .output()
        .with_context(|| format!("failed to run {}", cfg.soroban_cli))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    // Fallback for CLI versions without `tx to-rep`.
    let fallback = Command::new(&cfg.soroban_cli)
        .args(["tx", "decode", "--output", "json-formatted", xdr])
        .output()
        .with_context(|| format!("failed to run {}", cfg.soroban_cli))?;
    if !fallback.status.success() {
        return Err(anyhow!(
            "stellar CLI error: {}",
            String::from_utf8_lossy(&fallback.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&fallback.stdout).trim().to_string())
}

fn fetch_account(client: &Client, horizon_url: &str, account: &str) -> Result<Value> {
    let url = format!("{}/accounts/{}", horizon_url.trim_end_matches('/'), account);
    let resp = client.get(url).send().context("horizon request failed")?;
    if resp.status().as_u16() == 404 {
        return Err(anyhow!("account not found"));
    }
    if !resp.status().is_success() {
        return Err(anyhow!("horizon error: {}", resp.status()));
    }
    Ok(resp.json::<Value>()?)
}

fn fetch_latest_tx_hash(client: &Client, horizon_url: &str, account: &str) -> Result<String> {
    let url = format!(
        "{}/accounts/{}/transactions?limit=1&order=desc",
        horizon_url.trim_end_matches('/'),
        account
    );
    let resp = client.get(url).send().context("horizon request failed")?;
    if !resp.status().is_success() {
        return Err(anyhow!("horizon error: {}", resp.status()));
    }
    let json = resp.json::<Value>()?;
    let record = json
        .get("_embedded")
        .and_then(|v| v.get("records"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .ok_or_else(|| anyhow!("no transactions found"))?;
    record
        .get("hash")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .ok_or_else(|| anyhow!("missing tx hash"))
}

fn fetch_balances(client: &Client, horizon_url: &str, account: &str) -> Result<Vec<String>> {
    let json = fetch_account(client, horizon_url, account)?;
    let balances = json
        .get("balances")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("missing balances"))?;

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

fn friendbot_fund(client: &Client, friendbot_url: &str, account: &str) -> Result<String> {
    let url = format!("{}?addr={}", friendbot_url.trim_end_matches('/'), account);
    let resp = client.get(url).send().context("friendbot request failed")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(anyhow!("friendbot error: {} {}", status, text));
    }
    Ok("friendbot funded account".to_string())
}

fn args_to_cli(args: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(map) = args.as_object() {
        let mut keys: Vec<_> = map.keys().collect();
        keys.sort();
        for key in keys {
            let value = map.get(key).unwrap();
            let val = if let Some(s) = value.as_str() {
                s.to_string()
            } else {
                value.to_string()
            };
            out.push((key.to_string(), val));
        }
    }
    out
}

fn soroban_cli_invoke(
    cfg: &NetworkConfig,
    contract_id: &str,
    function: &str,
    args: &Value,
    simulate: bool,
) -> Result<String> {
    let source = cfg
        .soroban_source
        .as_ref()
        .ok_or_else(|| anyhow!("NC_SOROBAN_SOURCE is not set"))?;

    let mut cmd = Command::new(&cfg.soroban_cli);
    cmd.args([
        "contract",
        "invoke",
        "--id",
        contract_id,
        "--source",
        source,
        "--network",
        &cfg.soroban_network,
    ]);
    if simulate {
        cmd.args(&cfg.soroban_simulate_args);
    }
    cmd.arg("--");
    cmd.arg(function);
    for (key, value) in args_to_cli(args) {
        cmd.arg(format!("--{key}")).arg(value);
    }
    let output = cmd.output().context("failed to run stellar CLI")?;
    if !output.status.success() {
        return Err(anyhow!(
            "stellar CLI error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(normalize_cli_output(&output))
}

fn simulate_plan(plan: &ActionPlan, cfg: &NetworkConfig) -> Preview {
    let client = Client::new();
    let base_fee = fetch_base_fee(&client, &cfg.horizon_url).unwrap_or(100);
    let op_count = estimate_op_count(plan);
    let total_fee = base_fee.saturating_mul(op_count as u64);

    let mut effects = Vec::new();
    let mut warnings = Vec::new();

    for action in &plan.actions {
        match action {
            neurochain::actions::Action::StellarAccountBalance { account, asset } => {
                match fetch_balances(&client, &cfg.horizon_url, account) {
                    Ok(balances) => {
                        if let Some(asset) = asset {
                            let line = balances
                                .iter()
                                .find(|b| b.starts_with(asset))
                                .cloned()
                                .unwrap_or_else(|| format!("{asset} = (not found)"));
                            effects.push(format!("balance {account}: {line}"));
                        } else {
                            for line in balances {
                                effects.push(format!("balance {account}: {line}"));
                            }
                        }
                    }
                    Err(err) => {
                        warnings.push(format!("simulate_error: balance {account} failed: {err}"))
                    }
                }
            }
            neurochain::actions::Action::StellarAccountFundTestnet { account } => {
                let exists = fetch_account(&client, &cfg.horizon_url, account).is_ok();
                let msg = if exists {
                    format!("friendbot will top up existing account {account}")
                } else {
                    format!("friendbot will create and fund account {account}")
                };
                effects.push(msg);
            }
            neurochain::actions::Action::StellarAccountCreate {
                destination,
                starting_balance,
            } => {
                effects.push(format!(
                    "create account {destination} with starting_balance {starting_balance} XLM"
                ));
                if cfg.txrep_preview {
                    match parse_amount_to_stroops(starting_balance).and_then(|amount| {
                        stellar_tx_build_only(
                            cfg,
                            &[
                                "create-account".to_string(),
                                "--destination".to_string(),
                                destination.clone(),
                                "--starting-balance".to_string(),
                                amount,
                            ],
                        )
                    }) {
                        Ok(xdr) => match xdr_to_txrep(cfg, &xdr) {
                            Ok(txrep) => effects
                                .push(format!("txrep create-account {destination}:\n{txrep}")),
                            Err(err) => warnings.push(format!(
                                "preview_error: txrep create-account {destination} failed: {err}"
                            )),
                        },
                        Err(err) => warnings.push(format!(
                            "preview_error: txrep create-account {destination} failed: {err}"
                        )),
                    }
                }
            }
            neurochain::actions::Action::StellarChangeTrust {
                asset_code,
                asset_issuer,
                limit,
            } => {
                let mut line = format!("change trust {}:{}", asset_code, asset_issuer);
                if let Some(limit) = limit {
                    line.push_str(&format!(" limit {limit}"));
                }
                effects.push(line);
                if cfg.txrep_preview {
                    let line = format!("{asset_code}:{asset_issuer}");
                    let mut args = vec![
                        "change-trust".to_string(),
                        "--line".to_string(),
                        line.clone(),
                    ];
                    if let Some(limit) = limit {
                        match parse_amount_to_stroops(limit) {
                            Ok(limit_stroops) => {
                                args.push("--limit".to_string());
                                args.push(limit_stroops);
                            }
                            Err(err) => {
                                warnings.push(format!(
                                    "preview_error: txrep change-trust {line} failed: {err}"
                                ));
                                continue;
                            }
                        }
                    }
                    match stellar_tx_build_only(cfg, &args) {
                        Ok(xdr) => match xdr_to_txrep(cfg, &xdr) {
                            Ok(txrep) => {
                                effects.push(format!("txrep change-trust {line}:\n{txrep}"))
                            }
                            Err(err) => warnings.push(format!(
                                "preview_error: txrep change-trust {line} failed: {err}"
                            )),
                        },
                        Err(err) => warnings.push(format!(
                            "preview_error: txrep change-trust {line} failed: {err}"
                        )),
                    }
                }
            }
            neurochain::actions::Action::StellarPayment {
                to,
                amount,
                asset_code,
                asset_issuer,
            } => {
                let asset = if asset_code.eq_ignore_ascii_case("XLM") && asset_issuer.is_none() {
                    "native".to_string()
                } else if let Some(issuer) = asset_issuer {
                    format!("{}:{}", asset_code, issuer)
                } else {
                    asset_code.clone()
                };
                effects.push(format!("payment {amount} {asset} -> {to}"));
                if cfg.txrep_preview {
                    match parse_amount_to_stroops(amount).and_then(|amount_stroops| {
                        stellar_tx_build_only(
                            cfg,
                            &[
                                "payment".to_string(),
                                "--destination".to_string(),
                                to.clone(),
                                "--asset".to_string(),
                                asset.clone(),
                                "--amount".to_string(),
                                amount_stroops,
                            ],
                        )
                    }) {
                        Ok(xdr) => match xdr_to_txrep(cfg, &xdr) {
                            Ok(txrep) => effects
                                .push(format!("txrep payment {amount} {asset} -> {to}:\n{txrep}")),
                            Err(err) => warnings.push(format!(
                                "preview_error: txrep payment {amount} {asset} -> {to} failed: {err}"
                            )),
                        },
                        Err(err) => warnings.push(format!(
                            "preview_error: txrep payment {amount} {asset} -> {to} failed: {err}"
                        )),
                    }
                }
            }
            neurochain::actions::Action::StellarTxStatus { hash } => {
                match fetch_tx_status(&client, &cfg.horizon_url, hash) {
                    Ok(status) => effects.push(status),
                    Err(err) => warnings.push(format!("simulate_error: tx status failed: {err}")),
                }
            }
            neurochain::actions::Action::SorobanContractInvoke {
                contract_id,
                function,
                args,
            } => match soroban_cli_invoke(cfg, contract_id, function, args, true) {
                Ok(output) => {
                    if output.trim().is_empty() {
                        effects.push(format!("soroban simulate {contract_id}:{function} -> ok"));
                    } else {
                        effects.push(format!(
                            "soroban simulate {contract_id}:{function} -> {output}"
                        ));
                    }
                    if cfg.txrep_preview {
                        match soroban_cli_build(cfg, contract_id, function, args) {
                            Ok(xdr) => match xdr_to_txrep(cfg, &xdr) {
                                Ok(txrep) => effects.push(format!(
                                    "txrep soroban {contract_id}:{function}:\n{txrep}"
                                )),
                                Err(err) => warnings.push(format!(
                                    "preview_error: txrep soroban {contract_id}:{function} failed: {err}"
                                )),
                            },
                            Err(err) => warnings.push(format!(
                                "preview_error: txrep soroban {contract_id}:{function} failed: {err}"
                            )),
                        }
                    }
                }
                Err(err) => warnings.push(format!(
                    "simulate_error: soroban {contract_id}:{function} failed: {err}"
                )),
            },
            other => warnings.push(format!(
                "simulate_skip: not implemented for {}",
                other.kind()
            )),
        }
    }

    Preview {
        fee_estimate: format!("{base_fee} stroops x {op_count} ops = {total_fee} stroops"),
        effects,
        warnings,
    }
}

fn print_preview(preview: &Preview) {
    eprintln!("=== Preview ===");
    eprintln!("Estimated fee: {}", preview.fee_estimate);
    if preview.effects.is_empty() {
        eprintln!("Effects: (none)");
    } else {
        eprintln!("Effects:");
        for effect in &preview.effects {
            eprintln!("  - {effect}");
        }
    }
    if !preview.warnings.is_empty() {
        eprintln!("Warnings:");
        for warning in &preview.warnings {
            eprintln!("  - {warning}");
        }
    }
}

fn confirm_submit(auto_yes: bool) -> bool {
    if auto_yes {
        return true;
    }
    eprint!("Confirm submit? [y/N]: ");
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        return false;
    }
    matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn submit_plan(plan: &ActionPlan, cfg: &NetworkConfig) -> Vec<String> {
    let client = Client::new();
    let mut outputs = Vec::new();

    for action in &plan.actions {
        match action {
            neurochain::actions::Action::StellarAccountFundTestnet { account } => {
                if let Some(friendbot_url) = cfg.friendbot_url.as_deref() {
                    match friendbot_fund(&client, friendbot_url, account) {
                        Ok(msg) => outputs.push(format!("{account}: {msg}")),
                        Err(err) => outputs.push(format!("{account}: friendbot failed: {err}")),
                    }
                } else {
                    outputs.push(format!("{account}: friendbot unavailable (not testnet)"));
                }
            }
            neurochain::actions::Action::StellarAccountBalance { account, asset } => {
                match fetch_balances(&client, &cfg.horizon_url, account) {
                    Ok(balances) => {
                        if let Some(asset) = asset {
                            let line = balances
                                .iter()
                                .find(|b| b.starts_with(asset))
                                .cloned()
                                .unwrap_or_else(|| format!("{asset} = (not found)"));
                            outputs.push(format!("balance {account}: {line}"));
                        } else {
                            for line in balances {
                                outputs.push(format!("balance {account}: {line}"));
                            }
                        }
                    }
                    Err(err) => outputs.push(format!("balance submit failed for {account}: {err}")),
                }
            }
            neurochain::actions::Action::SorobanContractInvoke {
                contract_id,
                function,
                args,
            } => match soroban_cli_invoke(cfg, contract_id, function, args, false) {
                Ok(output) => {
                    let mut hash = extract_tx_hash(&output);
                    let mut note = None;
                    if hash.is_none() {
                        if let Some(source) = cfg.soroban_source.as_deref() {
                            if let Ok(latest) =
                                fetch_latest_tx_hash(&client, &cfg.horizon_url, source)
                            {
                                hash = Some(latest);
                                note = Some("latest");
                            }
                        }
                    }
                    outputs.push(format_submit_ok(
                        &format!("soroban submit {contract_id}:{function}"),
                        hash,
                        &output,
                        note,
                    ));
                }
                Err(err) => outputs.push(format_submit_error(
                    &format!("soroban submit {contract_id}:{function}"),
                    "submit",
                    &err.to_string(),
                )),
            },
            neurochain::actions::Action::StellarAccountCreate {
                destination,
                starting_balance,
            } => match parse_amount_to_stroops(starting_balance).and_then(|amount| {
                stellar_tx_new(
                    cfg,
                    &[
                        "create-account".to_string(),
                        "--destination".to_string(),
                        destination.clone(),
                        "--starting-balance".to_string(),
                        amount,
                    ],
                )
            }) {
                Ok(output) => {
                    let hash = extract_tx_hash(&output).or_else(|| try_hash_via_cli(cfg, &output));
                    outputs.push(format_submit_ok(
                        &format!("create-account {destination}"),
                        hash,
                        &output,
                        None,
                    ));
                }
                Err(err) => outputs.push(format_submit_error(
                    &format!("create-account {destination}"),
                    "submit",
                    &err.to_string(),
                )),
            },
            neurochain::actions::Action::StellarChangeTrust {
                asset_code,
                asset_issuer,
                limit,
            } => {
                let line = format!("{asset_code}:{asset_issuer}");
                let mut args = vec![
                    "change-trust".to_string(),
                    "--line".to_string(),
                    line.clone(),
                ];
                if let Some(limit) = limit {
                    match parse_amount_to_stroops(limit) {
                        Ok(limit_stroops) => {
                            args.push("--limit".to_string());
                            args.push(limit_stroops);
                        }
                        Err(err) => {
                            outputs.push(format_submit_error(
                                &format!("change-trust {line}"),
                                "submit",
                                &err.to_string(),
                            ));
                            continue;
                        }
                    }
                }
                match stellar_tx_new(cfg, &args) {
                    Ok(output) => {
                        let hash =
                            extract_tx_hash(&output).or_else(|| try_hash_via_cli(cfg, &output));
                        outputs.push(format_submit_ok(
                            &format!("change-trust {line}"),
                            hash,
                            &output,
                            None,
                        ));
                    }
                    Err(err) => outputs.push(format_submit_error(
                        &format!("change-trust {line}"),
                        "submit",
                        &err.to_string(),
                    )),
                }
            }
            neurochain::actions::Action::StellarPayment {
                to,
                amount,
                asset_code,
                asset_issuer,
            } => {
                let asset = if asset_code.eq_ignore_ascii_case("XLM") && asset_issuer.is_none() {
                    "native".to_string()
                } else if let Some(issuer) = asset_issuer {
                    format!("{asset_code}:{issuer}")
                } else {
                    outputs.push(format_submit_error(
                        &format!("payment {amount} {asset_code} -> {to}"),
                        "submit",
                        &format!("missing asset_issuer for {asset_code}"),
                    ));
                    continue;
                };
                match parse_amount_to_stroops(amount).and_then(|amount_stroops| {
                    stellar_tx_new(
                        cfg,
                        &[
                            "payment".to_string(),
                            "--destination".to_string(),
                            to.clone(),
                            "--asset".to_string(),
                            asset.clone(),
                            "--amount".to_string(),
                            amount_stroops,
                        ],
                    )
                }) {
                    Ok(output) => {
                        let hash =
                            extract_tx_hash(&output).or_else(|| try_hash_via_cli(cfg, &output));
                        outputs.push(format_submit_ok(
                            &format!("payment {amount} {asset} -> {to}"),
                            hash,
                            &output,
                            None,
                        ));
                    }
                    Err(err) => outputs.push(format_submit_error(
                        &format!("payment {amount} {asset} -> {to}"),
                        "submit",
                        &err.to_string(),
                    )),
                }
            }
            neurochain::actions::Action::StellarTxStatus { hash } => {
                match fetch_tx_status(&client, &cfg.horizon_url, hash) {
                    Ok(status) => outputs.push(status),
                    Err(err) => outputs.push(format!("tx status failed for {hash}: {err}")),
                }
            }
            other => outputs.push(format!("submit not implemented for {}", other.kind())),
        }
    }

    outputs
}

fn print_intent_block_reasons(plan: &ActionPlan) {
    for warning in &plan.warnings {
        if warning.starts_with("intent_error:") || warning.starts_with("intent_warning:") {
            eprintln!("- {warning}");
        }
    }
    for action in &plan.actions {
        if let Action::Unknown { reason } = action {
            eprintln!("- intent_block: {reason}");
        }
    }
}

fn strip_wrapping_quotes(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        return trimmed[1..trimmed.len() - 1].to_string();
    }
    trimmed.to_string()
}

fn parse_ai_model_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let (left, right) = trimmed.split_once(':')?;
    if !left.trim().eq_ignore_ascii_case("AI") {
        return None;
    }
    let model_path = strip_wrapping_quotes(right);
    if model_path.is_empty() {
        None
    } else {
        Some(model_path)
    }
}

fn extract_prompt_from_set_from_ai(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("set ") {
        return None;
    }
    let marker = " from ai:";
    let idx = lower.find(marker)?;
    let prompt = strip_wrapping_quotes(&trimmed[idx + marker.len()..]);
    if prompt.is_empty() {
        None
    } else {
        Some(prompt)
    }
}

fn extract_prompt_from_macro_from_ai(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("macro from ai:") {
        return None;
    }
    let idx = trimmed.find(':')?;
    let prompt = strip_wrapping_quotes(&trimmed[idx + 1..]);
    if prompt.is_empty() {
        None
    } else {
        Some(prompt)
    }
}

fn parse_named_value(line: &str, names: &[&str]) -> Option<String> {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    for name in names {
        let name_l = name.to_ascii_lowercase();

        let prefix = format!("{name_l}:");
        if lower.starts_with(&prefix) {
            let value = strip_wrapping_quotes(trimmed[prefix.len()..].trim_start());
            if !value.is_empty() {
                return Some(value);
            }
        }

        let prefix = format!("{name_l}=");
        if lower.starts_with(&prefix) {
            let value = strip_wrapping_quotes(trimmed[prefix.len()..].trim_start());
            if !value.is_empty() {
                return Some(value);
            }
        }

        let prefix = format!("{name_l} ");
        if lower.starts_with(&prefix) {
            let value = strip_wrapping_quotes(trimmed[prefix.len()..].trim_start());
            if !value.is_empty() {
                return Some(value);
            }
        }

        let prefix = format!("set {name_l} =");
        if lower.starts_with(&prefix) {
            let value = strip_wrapping_quotes(trimmed[prefix.len()..].trim_start());
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn parse_network_line(line: &str) -> Option<String> {
    parse_named_value(line, &["network"])
}

fn parse_source_line(line: &str) -> Option<String> {
    parse_named_value(line, &["wallet", "source", "lompakko"])
}

fn build_plan_from_intent_prompt(
    prompt: &str,
    model_path: &str,
    threshold: f32,
) -> Result<ActionPlan> {
    let decision = classify_intent_stellar(prompt, model_path, threshold)?;
    let mut plan = build_intent_action_plan(prompt, &decision);
    plan.warnings
        .push(format!("intent_model: path={model_path}"));
    Ok(plan)
}

fn resolve_threshold(override_value: Option<f32>) -> Result<f32> {
    if let Some(value) = override_value {
        return Ok(value);
    }
    Ok(intent_threshold_from_env()?.unwrap_or(DEFAULT_INTENT_STELLAR_THRESHOLD))
}

fn merge_action_plans(target: &mut ActionPlan, mut other: ActionPlan) {
    target.actions.append(&mut other.actions);
    target.warnings.append(&mut other.warnings);
}

fn build_plan_from_script(
    script: &str,
    source_path: &str,
    initial_model: Option<String>,
    initial_threshold: Option<f32>,
) -> Result<(ActionPlan, NetworkConfig, bool)> {
    let mut model_path = initial_model.unwrap_or_else(resolve_intent_model_path);
    let threshold = resolve_threshold(initial_threshold)?;
    let horizon_from_env = env::var("NC_STELLAR_HORIZON_URL")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let friendbot_from_env = env::var("NC_FRIENDBOT_URL")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let mut flow_cfg = load_network_config();
    let mut intent_mode = false;
    let mut plan = ActionPlan::default();
    let mut manual_lines = Vec::new();

    for raw in script.lines() {
        let line = raw.trim();
        if line_is_comment_or_empty(line) {
            continue;
        }

        if let Some(new_model_path) = parse_ai_model_line(line) {
            model_path = new_model_path;
            continue;
        }

        if let Some(network) = parse_network_line(line) {
            flow_cfg.soroban_network = network.to_string();
            if !horizon_from_env {
                flow_cfg.horizon_url = default_horizon_url(&network);
            }
            if !friendbot_from_env {
                flow_cfg.friendbot_url = default_friendbot_url(&network);
            }
            continue;
        }

        if let Some(source) = parse_source_line(line) {
            flow_cfg.soroban_source = Some(source);
            continue;
        }

        if line_is_manual_action(line) {
            manual_lines.push(line.to_string());
            continue;
        }

        if let Some(_msg) = line.strip_prefix("neuro ") {
            continue;
        }

        let prompt = extract_prompt_from_set_from_ai(line)
            .or_else(|| extract_prompt_from_macro_from_ai(line))
            .unwrap_or_else(|| strip_wrapping_quotes(line));

        intent_mode = true;
        let prompt_plan = build_plan_from_intent_prompt(&prompt, &model_path, threshold)?;
        merge_action_plans(&mut plan, prompt_plan);
    }

    if !manual_lines.is_empty() {
        let manual_block = manual_lines.join("\n");
        let manual_plan = parse_action_plan_from_nc(&manual_block);
        merge_action_plans(&mut plan, manual_plan);
    }

    if plan.actions.is_empty() {
        let fallback = parse_action_plan_from_nc(script);
        merge_action_plans(&mut plan, fallback);
    }

    if plan.source.is_none() {
        plan.source = Some(source_path.to_string());
    }

    Ok((plan, flow_cfg, intent_mode))
}

fn execute_plan(
    mut plan: ActionPlan,
    flow: bool,
    auto_yes: bool,
    intent_mode: bool,
    cfg_override: Option<&NetworkConfig>,
) -> i32 {
    let allowlist = Allowlist::from_env();
    let violations = validate_plan(&plan, &allowlist);
    if !violations.is_empty() {
        for violation in &violations {
            plan.warnings.push(format!(
                "allowlist warning: #{} {} ({})",
                violation.index, violation.action, violation.reason
            ));
        }
        if allowlist_enforced() {
            eprintln!("Allowlist violations (enforced):");
            for violation in &violations {
                eprintln!(
                    "- #{} {}: {}",
                    violation.index, violation.action, violation.reason
                );
            }
            eprintln!("Set NC_ALLOWLIST_ENFORCE=0 (or unset) to allow warnings only.");
            return 3;
        }
        eprintln!("Allowlist warnings (stub, not enforced):");
        for violation in &violations {
            eprintln!(
                "- #{} {}: {}",
                violation.index, violation.action, violation.reason
            );
        }
    }

    let policies = load_contract_policies();
    let (policy_warnings, policy_errors) = validate_contract_policies(&plan, &policies);
    for warning in &policy_warnings {
        plan.warnings.push(format!("policy warning: {warning}"));
    }
    if !policy_errors.is_empty() {
        if policy_enforced() {
            eprintln!("Contract policy violations (enforced):");
            for err in &policy_errors {
                eprintln!("- {err}");
            }
            eprintln!("Set NC_CONTRACT_POLICY_ENFORCE=0 (or unset) to allow warnings only.");
            return 4;
        }
        eprintln!("Contract policy warnings (not enforced):");
        for err in &policy_errors {
            eprintln!("- {err}");
            plan.warnings.push(format!("policy error: {err}"));
        }
    }

    match serde_json::to_string_pretty(&plan) {
        Ok(json) => println!("{json}"),
        Err(err) => {
            eprintln!("Error serializing action plan: {err}");
            return 1;
        }
    }

    if flow {
        if intent_mode && has_intent_blocking_issue(&plan) {
            eprintln!("Intent safety guard blocked flow. simulate/submit skipped.");
            print_intent_block_reasons(&plan);
            return 5;
        }
        let cfg = cfg_override.cloned().unwrap_or_else(load_network_config);
        let preview = simulate_plan(&plan, &cfg);
        print_preview(&preview);
        if confirm_submit(auto_yes) {
            let outputs = submit_plan(&plan, &cfg);
            if outputs.is_empty() {
                eprintln!("Submit: no actions executed.");
            } else {
                eprintln!("Submit results:");
                for line in outputs {
                    eprintln!("  - {line}");
                }
            }
        } else {
            eprintln!("Submit aborted by user.");
        }
    }

    0
}

fn line_is_manual_action(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("stellar.")
        || trimmed.starts_with("soroban.")
        || trimmed.starts_with("action stellar.")
        || trimmed.starts_with("action soroban.")
}

fn line_is_comment_or_empty(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//")
}

fn print_repl_help() {
    println!(
        "Soroban REPL commands:
- AI: \"models/intent_stellar/model.onnx\"   set intent model path
- network: testnet|mainnet|public           set active network for flow
- wallet: <stellar-key-alias>               set active source wallet alias
- set intent from AI: \"Transfer 5 XLM to G...\"   classify prompt -> ActionPlan
- macro from AI: \"Transfer 5 XLM to G...\"        alias to intent prompt
- plain text prompt                              classify prompt -> ActionPlan
- stellar.* / soroban.* lines                    manual action-plan mode
- help, exit"
    );
}

fn run_repl(
    flow: bool,
    auto_yes: bool,
    initial_model: Option<String>,
    initial_threshold: Option<f32>,
) -> i32 {
    let mut model_path = initial_model.unwrap_or_else(resolve_intent_model_path);
    let threshold = match resolve_threshold(initial_threshold) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("Error: {err}");
            return 2;
        }
    };
    let horizon_from_env = env::var("NC_STELLAR_HORIZON_URL")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let friendbot_from_env = env::var("NC_FRIENDBOT_URL")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let mut flow_cfg = load_network_config();

    println!("NeuroChain Soroban REPL (intent -> action).");
    println!("Current model: {model_path}");
    println!("Current threshold: {threshold:.2}");
    println!("Current network: {}", flow_cfg.soroban_network);
    println!(
        "Current wallet/source: {}",
        flow_cfg.soroban_source.as_deref().unwrap_or("(not set)")
    );
    println!("Type `help` for commands, `exit` to quit.");

    loop {
        println!("Enter Soroban prompt/code (finish with an empty line):");
        let mut block = String::new();
        loop {
            print!("... ");
            let _ = io::stdout().flush();
            let mut line = String::new();
            if io::stdin().read_line(&mut line).is_err() {
                eprintln!("stdin read failed");
                return 1;
            }
            if line.trim().is_empty() {
                break;
            }
            block.push_str(&line);
        }

        let trimmed = block.trim();
        if trimmed.is_empty() {
            continue;
        }
        match trimmed {
            "exit" | "quit" => {
                println!("Exiting...");
                return 0;
            }
            "help" => {
                print_repl_help();
                continue;
            }
            _ => {}
        }

        let lines: Vec<&str> = trimmed
            .lines()
            .filter(|l| !line_is_comment_or_empty(l))
            .collect();
        let all_manual_actions =
            !lines.is_empty() && lines.iter().all(|l| line_is_manual_action(l));
        if all_manual_actions {
            let mut plan = parse_action_plan_from_nc(trimmed);
            if plan.source.is_none() {
                plan.source = Some("repl.manual".to_string());
            }
            let code = execute_plan(plan, flow, auto_yes, false, Some(&flow_cfg));
            if code != 0 {
                eprintln!("repl step returned code {code}");
            }
            continue;
        }

        for line in lines {
            if let Some(new_model_path) = parse_ai_model_line(line) {
                model_path = new_model_path;
                println!("Intent model path set to: {model_path}");
                continue;
            }

            if let Some(network) = parse_network_line(line) {
                flow_cfg.soroban_network = network.to_string();
                if !horizon_from_env {
                    flow_cfg.horizon_url = default_horizon_url(&network);
                }
                if !friendbot_from_env {
                    flow_cfg.friendbot_url = default_friendbot_url(&network);
                }
                println!("Network set to: {}", flow_cfg.soroban_network);
                println!("Horizon URL: {}", flow_cfg.horizon_url);
                println!(
                    "Friendbot: {}",
                    flow_cfg.friendbot_url.as_deref().unwrap_or("(disabled)")
                );
                continue;
            }

            if let Some(source) = parse_source_line(line) {
                flow_cfg.soroban_source = Some(source.to_string());
                println!(
                    "Wallet/source set to: {}",
                    flow_cfg.soroban_source.as_deref().unwrap_or("")
                );
                continue;
            }

            if let Some(prompt) = extract_prompt_from_set_from_ai(line)
                .or_else(|| extract_prompt_from_macro_from_ai(line))
            {
                match build_plan_from_intent_prompt(&prompt, &model_path, threshold) {
                    Ok(plan) => {
                        let code = execute_plan(plan, flow, auto_yes, true, Some(&flow_cfg));
                        if code != 0 {
                            eprintln!("repl step returned code {code}");
                        }
                    }
                    Err(err) => eprintln!("intent error: {err}"),
                }
                continue;
            }

            if let Some(msg) = line.trim().strip_prefix("neuro ") {
                println!("{}", strip_wrapping_quotes(msg));
                continue;
            }

            let prompt = strip_wrapping_quotes(line);
            match build_plan_from_intent_prompt(&prompt, &model_path, threshold) {
                Ok(plan) => {
                    let code = execute_plan(plan, flow, auto_yes, true, Some(&flow_cfg));
                    if code != 0 {
                        eprintln!("repl step returned code {code}");
                    }
                }
                Err(err) => eprintln!("intent error: {err}"),
            }
        }
    }
}

fn main() {
    banner::print_banner_stderr();
    let args: Vec<String> = env::args().collect();
    let cli = match parse_cli_args(&args) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("Error: {err}");
            print_usage();
            std::process::exit(2);
        }
    };

    if cli.repl {
        let code = run_repl(
            cli.flow,
            cli.auto_yes,
            cli.intent_model,
            cli.intent_threshold,
        );
        if code != 0 {
            std::process::exit(code);
        }
        return;
    }

    let mut cfg_override: Option<NetworkConfig> = None;
    let mut intent_mode = false;
    let plan: ActionPlan = if let Some(prompt) = cli.intent_text {
        intent_mode = true;
        let threshold = match resolve_threshold(cli.intent_threshold) {
            Ok(v) => v,
            Err(err) => {
                eprintln!("Error: {err}");
                std::process::exit(2);
            }
        };
        let model_path = cli.intent_model.unwrap_or_else(resolve_intent_model_path);
        match build_plan_from_intent_prompt(&prompt, &model_path, threshold) {
            Ok(plan) => plan,
            Err(err) => {
                eprintln!("Error: {err}");
                std::process::exit(1);
            }
        }
    } else {
        let path = cli.path.expect("path must exist when not in intent mode");
        let input = match fs::read_to_string(path.clone()) {
            Ok(contents) => contents,
            Err(err) => {
                eprintln!("Error reading {path}: {err}");
                std::process::exit(1);
            }
        };

        let mut plan: ActionPlan = match serde_json::from_str(&input) {
            Ok(plan) => plan,
            Err(_) => match build_plan_from_script(
                &input,
                &path,
                cli.intent_model.clone(),
                cli.intent_threshold,
            ) {
                Ok((script_plan, script_cfg, script_intent_mode)) => {
                    cfg_override = Some(script_cfg);
                    intent_mode = script_intent_mode;
                    script_plan
                }
                Err(err) => {
                    eprintln!("Error: {err}");
                    std::process::exit(1);
                }
            },
        };
        if plan.source.is_none() {
            plan.source = Some(path.to_string());
        }
        plan
    };

    let code = execute_plan(
        plan,
        cli.flow,
        cli.auto_yes,
        intent_mode,
        cfg_override.as_ref(),
    );
    if code != 0 {
        std::process::exit(code);
    }
}
