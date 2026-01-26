use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

fn default_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionPlan {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub actions: Vec<Action>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl Default for ActionPlan {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            actions: Vec::new(),
            warnings: Vec::new(),
            source: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    StellarAccountBalance {
        account: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        asset: Option<String>,
    },
    StellarAccountCreate {
        destination: String,
        starting_balance: String,
    },
    StellarAccountFundTestnet {
        account: String,
    },
    StellarChangeTrust {
        asset_code: String,
        asset_issuer: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<String>,
    },
    StellarPayment {
        to: String,
        amount: String,
        asset_code: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        asset_issuer: Option<String>,
    },
    StellarTxStatus {
        hash: String,
    },
    SorobanContractInvoke {
        contract_id: String,
        function: String,
        #[serde(default)]
        args: serde_json::Value,
    },
    Unknown {
        reason: String,
    },
}

impl Action {
    pub fn kind(&self) -> &'static str {
        match self {
            Action::StellarAccountBalance { .. } => "stellar.account.balance",
            Action::StellarAccountCreate { .. } => "stellar.account.create",
            Action::StellarAccountFundTestnet { .. } => "stellar.account.fund_testnet",
            Action::StellarChangeTrust { .. } => "stellar.change_trust",
            Action::StellarPayment { .. } => "stellar.payment",
            Action::StellarTxStatus { .. } => "stellar.tx.status",
            Action::SorobanContractInvoke { .. } => "soroban.contract.invoke",
            Action::Unknown { .. } => "unknown",
        }
    }
}

#[derive(Debug, Default)]
pub struct Allowlist {
    assets: HashSet<String>,
    contracts: HashSet<String>,
}

impl Allowlist {
    pub fn from_env() -> Self {
        let assets = parse_allowlist(std::env::var("NC_ASSET_ALLOWLIST").unwrap_or_default());
        let contracts = parse_allowlist(std::env::var("NC_SOROBAN_ALLOWLIST").unwrap_or_default());
        Self { assets, contracts }
    }

    fn is_asset_allowed(&self, code: &str, issuer: Option<&str>) -> bool {
        if self.assets.is_empty() {
            return true;
        }
        if code.eq_ignore_ascii_case("XLM") {
            return self.assets.contains("XLM");
        }
        let issuer = issuer.unwrap_or("");
        let full = format!("{code}:{issuer}");
        self.assets.contains(&full) || self.assets.contains(code)
    }

    fn is_contract_allowed(&self, contract_id: &str, function: &str) -> bool {
        if self.contracts.is_empty() {
            return true;
        }
        let full = format!("{contract_id}:{function}");
        self.contracts.contains(&full) || self.contracts.contains(contract_id)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AllowlistViolation {
    pub index: usize,
    pub action: String,
    pub reason: String,
}

pub fn validate_plan(plan: &ActionPlan, allowlist: &Allowlist) -> Vec<AllowlistViolation> {
    let mut violations = Vec::new();

    for (idx, action) in plan.actions.iter().enumerate() {
        match action {
            Action::SorobanContractInvoke {
                contract_id,
                function,
                ..
            } => {
                if !allowlist.is_contract_allowed(contract_id, function) {
                    violations.push(AllowlistViolation {
                        index: idx,
                        action: action.kind().to_string(),
                        reason: format!("contract {contract_id}:{function} not in allowlist"),
                    });
                }
            }
            Action::StellarPayment {
                asset_code,
                asset_issuer,
                ..
            } => {
                if !allowlist.is_asset_allowed(asset_code, asset_issuer.as_deref()) {
                    violations.push(AllowlistViolation {
                        index: idx,
                        action: action.kind().to_string(),
                        reason: format!(
                            "asset {asset_code}:{} not in allowlist",
                            asset_issuer.as_deref().unwrap_or("")
                        ),
                    });
                }
            }
            Action::StellarChangeTrust {
                asset_code,
                asset_issuer,
                ..
            } => {
                if !allowlist.is_asset_allowed(asset_code, Some(asset_issuer)) {
                    violations.push(AllowlistViolation {
                        index: idx,
                        action: action.kind().to_string(),
                        reason: format!("asset {asset_code}:{asset_issuer} not in allowlist"),
                    });
                }
            }
            _ => {}
        }
    }

    violations
}

pub fn enforce_allowlist(
    plan: &ActionPlan,
    allowlist: &Allowlist,
) -> Result<(), Vec<AllowlistViolation>> {
    let violations = validate_plan(plan, allowlist);
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

fn parse_allowlist(raw: String) -> HashSet<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

fn strip_inline_comment(line: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if let Some(active) = quote {
            if ch == '\\' {
                out.push(ch);
                if let Some(next) = chars.next() {
                    out.push(next);
                }
                continue;
            }
            if ch == active {
                quote = None;
            }
            out.push(ch);
            continue;
        }

        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            out.push(ch);
            continue;
        }

        if ch == '#' {
            break;
        }
        if ch == '/' && chars.peek() == Some(&'/') {
            break;
        }

        out.push(ch);
    }

    out.trim_end().to_string()
}

pub fn parse_action_plan_from_nc(contents: &str) -> ActionPlan {
    let mut plan = ActionPlan::default();

    for (idx, raw_line) in contents.lines().enumerate() {
        let mut line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        if let Some(stripped) = line.strip_prefix("action ") {
            line = stripped.trim_start();
        }

        let line = strip_inline_comment(line);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if !(line.starts_with("stellar.") || line.starts_with("soroban.")) {
            continue;
        }

        let (line_no_args, args_raw) = split_args_tail(line);
        let tokens = split_tokens(line_no_args);
        if tokens.is_empty() {
            continue;
        }
        let kind = tokens[0].as_str();
        let kv = parse_key_values(&tokens[1..]);

        let action = match kind {
            "stellar.account.balance" => {
                if let Some(account) = kv.get("account") {
                    Action::StellarAccountBalance {
                        account: account.clone(),
                        asset: kv.get("asset").cloned(),
                    }
                } else {
                    Action::Unknown {
                        reason: format!("line {}: missing account", idx + 1),
                    }
                }
            }
            "stellar.account.create" => {
                let destination = kv.get("destination");
                let starting_balance = kv.get("starting_balance");
                match (destination, starting_balance) {
                    (Some(destination), Some(starting_balance)) => Action::StellarAccountCreate {
                        destination: destination.clone(),
                        starting_balance: starting_balance.clone(),
                    },
                    _ => Action::Unknown {
                        reason: format!("line {}: missing destination/starting_balance", idx + 1),
                    },
                }
            }
            "stellar.account.fund_testnet" => {
                if let Some(account) = kv.get("account") {
                    Action::StellarAccountFundTestnet {
                        account: account.clone(),
                    }
                } else {
                    Action::Unknown {
                        reason: format!("line {}: missing account", idx + 1),
                    }
                }
            }
            "stellar.change_trust" => {
                let asset_code = kv.get("asset_code");
                let asset_issuer = kv.get("asset_issuer");
                match (asset_code, asset_issuer) {
                    (Some(asset_code), Some(asset_issuer)) => Action::StellarChangeTrust {
                        asset_code: asset_code.clone(),
                        asset_issuer: asset_issuer.clone(),
                        limit: kv.get("limit").cloned(),
                    },
                    _ => Action::Unknown {
                        reason: format!("line {}: missing asset_code/asset_issuer", idx + 1),
                    },
                }
            }
            "stellar.payment" => {
                let to = kv.get("to");
                let amount = kv.get("amount");
                let asset_code = kv.get("asset_code");
                match (to, amount, asset_code) {
                    (Some(to), Some(amount), Some(asset_code)) => Action::StellarPayment {
                        to: to.clone(),
                        amount: amount.clone(),
                        asset_code: asset_code.clone(),
                        asset_issuer: kv.get("asset_issuer").cloned(),
                    },
                    _ => Action::Unknown {
                        reason: format!("line {}: missing to/amount/asset_code", idx + 1),
                    },
                }
            }
            "stellar.tx.status" => {
                if let Some(hash) = kv.get("hash") {
                    Action::StellarTxStatus { hash: hash.clone() }
                } else {
                    Action::Unknown {
                        reason: format!("line {}: missing hash", idx + 1),
                    }
                }
            }
            "soroban.contract.invoke" => {
                let contract_id = kv.get("contract_id");
                let function = kv.get("function");
                match (contract_id, function) {
                    (Some(contract_id), Some(function)) => Action::SorobanContractInvoke {
                        contract_id: contract_id.clone(),
                        function: function.clone(),
                        args: parse_args_json(args_raw).unwrap_or(serde_json::Value::Null),
                    },
                    _ => Action::Unknown {
                        reason: format!("line {}: missing contract_id/function", idx + 1),
                    },
                }
            }
            _ => Action::Unknown {
                reason: format!("line {}: unknown action '{kind}'", idx + 1),
            },
        };

        if let Action::Unknown { reason } = &action {
            plan.warnings.push(reason.to_string());
        }
        plan.actions.push(action);
    }

    if plan.actions.is_empty() {
        plan.warnings
            .push("no actions detected in .nc input".to_string());
    }

    plan
}

fn split_args_tail(line: &str) -> (&str, Option<&str>) {
    if let Some(pos) = line.find(" args=") {
        let head = line[..pos].trim();
        let tail = line[pos + 6..].trim();
        return (head, if tail.is_empty() { None } else { Some(tail) });
    }
    (line, None)
}

fn parse_args_json(raw: Option<&str>) -> Option<serde_json::Value> {
    let raw = raw?;
    if raw.starts_with('{') || raw.starts_with('[') {
        return serde_json::from_str(raw).ok();
    }
    if raw.starts_with('"') || raw.starts_with('\'') {
        let unquoted = unquote(raw);
        return serde_json::from_str(&unquoted)
            .ok()
            .or(Some(serde_json::Value::String(unquoted)));
    }
    Some(serde_json::Value::String(raw.to_string()))
}

fn split_tokens(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if let Some(active) = quote {
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
                continue;
            }
            if ch == active {
                quote = None;
                continue;
            }
            current.push(ch);
            continue;
        }

        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }

        if ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            continue;
        }

        current.push(ch);
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn parse_key_values(tokens: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for token in tokens {
        if let Some((key, value)) = token.split_once('=') {
            let value = unquote(value);
            map.insert(key.to_string(), value);
        }
    }
    map
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        let first = bytes[0] as char;
        let last = bytes[bytes.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}
