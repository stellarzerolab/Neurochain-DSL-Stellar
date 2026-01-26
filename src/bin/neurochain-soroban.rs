use std::env;
use std::fs;
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use neurochain::actions::{parse_action_plan_from_nc, validate_plan, ActionPlan, Allowlist};
use reqwest::blocking::Client;
use serde_json::Value;

fn print_usage() {
    eprintln!("Usage: neurochain-soroban <file.nc|plan.json> [--flow] [--yes]");
    eprintln!("If input is JSON, it is treated as an ActionPlan.");
    eprintln!(
        "Manual .nc lines can start with 'stellar.' or 'soroban.' (comment lines are ignored)."
    );
    eprintln!("Set NC_ALLOWLIST_ENFORCE=1 to hard-fail on allowlist violations.");
    eprintln!("--flow enables simulate → preview → confirm → submit.");
    eprintln!("--yes auto-confirms submit in --flow mode.");
    eprintln!("Env: NC_STELLAR_NETWORK / NC_SOROBAN_NETWORK (default: testnet)");
    eprintln!("Env: NC_STELLAR_HORIZON_URL (default: testnet Horizon)");
    eprintln!("Env: NC_FRIENDBOT_URL (default: testnet Friendbot)");
    eprintln!("Env: NC_SOROBAN_SOURCE or NC_STELLAR_SOURCE (for soroban invoke)");
    eprintln!("Env: NC_STELLAR_CLI (default: stellar)");
    eprintln!("Env: NC_SOROBAN_SIMULATE_FLAG (default: \"--send no\")");
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

fn load_network_config() -> NetworkConfig {
    let network = env::var("NC_STELLAR_NETWORK")
        .or_else(|_| env::var("NC_SOROBAN_NETWORK"))
        .unwrap_or_else(|_| "testnet".to_string());

    let horizon_url =
        env::var("NC_STELLAR_HORIZON_URL").unwrap_or_else(|_| match network.as_str() {
            "public" | "pubnet" | "mainnet" => "https://horizon.stellar.org".to_string(),
            _ => "https://horizon-testnet.stellar.org".to_string(),
        });

    let friendbot_url = env::var("NC_FRIENDBOT_URL")
        .ok()
        .or_else(|| match network.as_str() {
            "testnet" => Some("https://friendbot.stellar.org".to_string()),
            _ => None,
        });

    let soroban_source = env::var("NC_SOROBAN_SOURCE")
        .or_else(|_| env::var("NC_STELLAR_SOURCE"))
        .ok();

    let soroban_cli = env::var("NC_STELLAR_CLI").unwrap_or_else(|_| "stellar".to_string());
    let soroban_simulate_raw =
        env::var("NC_SOROBAN_SIMULATE_FLAG").unwrap_or_else(|_| "--send no".to_string());
    let soroban_simulate_args = parse_simulate_args(&soroban_simulate_raw);

    NetworkConfig {
        horizon_url,
        friendbot_url,
        soroban_network: network,
        soroban_source,
        soroban_cli,
        soroban_simulate_args,
    }
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

fn format_submit_line(label: &str, output: &str, hash: Option<String>) -> String {
    if let Some(hash) = hash {
        return format!("{label} -> tx-hash {hash}");
    }
    if output.trim().is_empty() {
        return format!("{label} -> submit ok");
    }
    format!("{label} -> {output}")
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
                        warnings.push(format!("balance simulate failed for {account}: {err}"))
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
            }
            neurochain::actions::Action::StellarTxStatus { hash } => {
                match fetch_tx_status(&client, &cfg.horizon_url, hash) {
                    Ok(status) => effects.push(status),
                    Err(err) => warnings.push(format!("tx status simulate failed: {err}")),
                }
            }
            neurochain::actions::Action::SorobanContractInvoke {
                contract_id,
                function,
                args,
            } => match soroban_cli_invoke(cfg, contract_id, function, args, true) {
                Ok(output) => effects.push(format!(
                    "soroban simulate {contract_id}:{function} -> {output}"
                )),
                Err(err) => warnings.push(format!(
                    "soroban simulate failed for {contract_id}:{function}: {err}"
                )),
            },
            other => warnings.push(format!("simulate not implemented for {}", other.kind())),
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
                    let hash = extract_tx_hash(&output);
                    outputs.push(format_submit_line(
                        &format!("soroban submit {contract_id}:{function}"),
                        &output,
                        hash,
                    ));
                }
                Err(err) => outputs.push(format!(
                    "soroban submit failed for {contract_id}:{function}: {err}"
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
                    outputs.push(format_submit_line(
                        &format!("create-account {destination}"),
                        &output,
                        hash,
                    ));
                }
                Err(err) => outputs.push(format!("create-account failed for {destination}: {err}")),
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
                            outputs.push(format!("change-trust failed for {line}: {err}"));
                            continue;
                        }
                    }
                }
                match stellar_tx_new(cfg, &args) {
                    Ok(output) => {
                        let hash =
                            extract_tx_hash(&output).or_else(|| try_hash_via_cli(cfg, &output));
                        outputs.push(format_submit_line(
                            &format!("change-trust {line}"),
                            &output,
                            hash,
                        ));
                    }
                    Err(err) => outputs.push(format!("change-trust failed for {line}: {err}")),
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
                    outputs.push(format!(
                        "payment failed for {to}: missing asset_issuer for {asset_code}"
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
                        outputs.push(format_submit_line(
                            &format!("payment {amount} {asset} -> {to}"),
                            &output,
                            hash,
                        ));
                    }
                    Err(err) => outputs.push(format!("payment failed for {to}: {err}")),
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

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        std::process::exit(2);
    }

    let mut path: Option<String> = None;
    let mut flow = false;
    let mut auto_yes = false;
    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--flow" => flow = true,
            "--yes" | "-y" => auto_yes = true,
            _ => {
                if path.is_none() {
                    path = Some(arg.clone());
                }
            }
        }
    }

    let Some(path) = path else {
        print_usage();
        std::process::exit(2);
    };

    let input = match fs::read_to_string(path.clone()) {
        Ok(contents) => contents,
        Err(err) => {
            eprintln!("Error reading {path}: {err}");
            std::process::exit(1);
        }
    };

    let mut plan: ActionPlan = match serde_json::from_str(&input) {
        Ok(plan) => plan,
        Err(_) => parse_action_plan_from_nc(&input),
    };
    if plan.source.is_none() {
        plan.source = Some(path.to_string());
    }

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
            std::process::exit(3);
        }
        eprintln!("Allowlist warnings (stub, not enforced):");
        for violation in &violations {
            eprintln!(
                "- #{} {}: {}",
                violation.index, violation.action, violation.reason
            );
        }
    }

    match serde_json::to_string_pretty(&plan) {
        Ok(json) => println!("{json}"),
        Err(err) => {
            eprintln!("Error serializing action plan: {err}");
            std::process::exit(1);
        }
    }

    if flow {
        let cfg = load_network_config();
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
}
