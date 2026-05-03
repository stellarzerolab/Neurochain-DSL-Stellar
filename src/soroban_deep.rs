use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::actions::{Action, ActionPlan};

#[derive(Debug, Clone, Deserialize)]
pub struct ArgSchema {
    #[serde(default)]
    pub required: HashMap<String, String>,
    #[serde(default)]
    pub optional: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContractPolicy {
    pub contract_id: String,
    #[serde(default)]
    pub allowed_functions: Vec<String>,
    #[serde(default)]
    pub args_schema: HashMap<String, ArgSchema>,
    #[serde(default)]
    pub max_fee_stroops: Option<u64>,
    #[serde(default)]
    pub resource_limits: Option<Value>,
    #[serde(default)]
    pub intent_templates: HashMap<String, ContractIntentTemplate>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContractIntentTemplate {
    pub function: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub args: HashMap<String, TemplateArg>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateArg {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(rename = "type", default)]
    pub ty: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TemplateExpansionReport {
    pub expanded: bool,
    pub template_name: Option<String>,
    pub contract_id: Option<String>,
    pub function: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolicyTypedV2Report {
    pub converted: usize,
    pub normalized_args: usize,
}

#[derive(Default)]
struct PolicyTypedV2Outcome {
    errors: Vec<String>,
    normalized_args: usize,
}

pub fn apply_contract_intent_templates(
    prompt: &str,
    plan: &mut ActionPlan,
    policies: &[ContractPolicy],
) -> TemplateExpansionReport {
    if !plan_is_template_expandable(plan) {
        return TemplateExpansionReport {
            reason: Some("plan_is_not_template_expandable".to_string()),
            ..TemplateExpansionReport::default()
        };
    }

    let Some((template_name, policy, template)) = find_matching_template(prompt, policies) else {
        return TemplateExpansionReport {
            reason: Some("no_template_match".to_string()),
            ..TemplateExpansionReport::default()
        };
    };

    let function = template.function.trim();
    if function.is_empty() {
        return TemplateExpansionReport {
            template_name: Some(template_name),
            contract_id: Some(policy.contract_id.clone()),
            reason: Some("template_function_missing".to_string()),
            ..TemplateExpansionReport::default()
        };
    }

    if !policy.allowed_functions.is_empty()
        && !policy
            .allowed_functions
            .iter()
            .any(|allowed| allowed == function)
    {
        return TemplateExpansionReport {
            template_name: Some(template_name),
            contract_id: Some(policy.contract_id.clone()),
            function: Some(function.to_string()),
            reason: Some("template_function_not_allowed_by_policy".to_string()),
            ..TemplateExpansionReport::default()
        };
    }

    let args = match build_template_args(prompt, &template_name, template, policy) {
        Ok(args) => args,
        Err(reason) => {
            plan.warnings.push(format!("intent_error: {reason}"));
            return TemplateExpansionReport {
                template_name: Some(template_name),
                contract_id: Some(policy.contract_id.clone()),
                function: Some(function.to_string()),
                reason: Some(reason),
                ..TemplateExpansionReport::default()
            };
        }
    };

    plan.actions.clear();
    plan.actions.push(Action::SorobanContractInvoke {
        contract_id: policy.contract_id.clone(),
        function: function.to_string(),
        args: Value::Object(args),
    });
    plan.warnings
        .retain(|warning| !is_template_expandable_intent_warning(warning));
    plan.warnings.push(format!(
        "soroban_deep_template: template={template_name} contract_id={} function={function}",
        policy.contract_id
    ));

    TemplateExpansionReport {
        expanded: true,
        template_name: Some(template_name),
        contract_id: Some(policy.contract_id.clone()),
        function: Some(function.to_string()),
        reason: None,
    }
}

pub fn apply_policy_typed_templates_v2(
    plan: &mut ActionPlan,
    policies: &[ContractPolicy],
) -> PolicyTypedV2Report {
    if policies.is_empty() {
        return PolicyTypedV2Report::default();
    }

    let mut policy_map: HashMap<&str, &ContractPolicy> = HashMap::new();
    for policy in policies {
        policy_map.insert(policy.contract_id.as_str(), policy);
    }

    let mut report = PolicyTypedV2Report::default();
    for action in &mut plan.actions {
        let outcome = match action {
            Action::SorobanContractInvoke {
                contract_id,
                function,
                args,
            } => {
                let Some(policy) = policy_map.get(contract_id.as_str()) else {
                    continue;
                };
                let Some(schema) = policy.args_schema.get(function) else {
                    continue;
                };

                apply_policy_typed_schema_to_args(contract_id, function, args, schema)
            }
            _ => PolicyTypedV2Outcome::default(),
        };
        report.normalized_args += outcome.normalized_args;

        if let Some(reason) = outcome.errors.first().cloned() {
            *action = Action::Unknown {
                reason: reason.clone(),
            };
            for err in outcome.errors {
                plan.warnings.push(format!("intent_error: {err}"));
            }
            report.converted += 1;
        }
    }

    report
}

pub fn validate_contract_policies(
    plan: &ActionPlan,
    policies: &[ContractPolicy],
) -> (Vec<String>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    if policies.is_empty() {
        return (warnings, errors);
    }

    let mut map: HashMap<&str, &ContractPolicy> = HashMap::new();
    for policy in policies {
        map.insert(policy.contract_id.as_str(), policy);
    }

    for action in &plan.actions {
        if let Action::SorobanContractInvoke {
            contract_id,
            function,
            args,
        } = action
        {
            let Some(policy) = map.get(contract_id.as_str()) else {
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
                let args_obj = args_obj.expect("checked is_some above");

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

fn plan_is_template_expandable(plan: &ActionPlan) -> bool {
    let only_unknown_actions = !plan.actions.is_empty()
        && plan
            .actions
            .iter()
            .all(|action| matches!(action, Action::Unknown { .. }));
    only_unknown_actions
        && plan
            .warnings
            .iter()
            .any(|warning| is_template_expandable_intent_warning(warning))
}

fn is_template_expandable_intent_warning(warning: &str) -> bool {
    warning.starts_with("intent_error: slot_missing: ContractInvoke missing ")
        || warning == "intent_error: slot_missing: Unknown intent has no action mapping"
}

fn find_matching_template<'a>(
    prompt: &str,
    policies: &'a [ContractPolicy],
) -> Option<(String, &'a ContractPolicy, &'a ContractIntentTemplate)> {
    let lower_prompt = prompt.to_ascii_lowercase();
    for policy in policies {
        for (name, template) in &policy.intent_templates {
            if template_matches_prompt(name, template, &lower_prompt) {
                return Some((name.clone(), policy, template));
            }
        }
    }
    None
}

fn template_matches_prompt(
    name: &str,
    template: &ContractIntentTemplate,
    lower_prompt: &str,
) -> bool {
    let name = name.trim().to_ascii_lowercase();
    if !name.is_empty() && lower_prompt.contains(&name) {
        return true;
    }
    template.aliases.iter().any(|alias| {
        let alias = alias.trim().to_ascii_lowercase();
        !alias.is_empty() && lower_prompt.contains(&alias)
    })
}

fn build_template_args(
    prompt: &str,
    template_name: &str,
    template: &ContractIntentTemplate,
    policy: &ContractPolicy,
) -> Result<Map<String, Value>, String> {
    let mut args = Map::new();
    for (key, arg) in &template.args {
        if let Some(value) = resolve_template_arg(prompt, arg) {
            args.insert(key.clone(), value);
        }
    }

    if let Some(schema) = policy.args_schema.get(template.function.trim()) {
        for key in schema.required.keys() {
            if !args.contains_key(key) {
                return Err(format!(
                    "slot_missing: ContractInvoke template {template_name} missing arg {key}"
                ));
            }
        }
    }

    Ok(args)
}

fn apply_policy_typed_schema_to_args(
    contract_id: &str,
    function: &str,
    args: &mut Value,
    schema: &ArgSchema,
) -> PolicyTypedV2Outcome {
    let Some(args_obj) = args.as_object() else {
        return PolicyTypedV2Outcome::default();
    };
    let mut outcome = PolicyTypedV2Outcome::default();
    let mut updates: Vec<(String, Value)> = Vec::new();

    for (key, ty_raw) in &schema.required {
        let ty = ty_raw.trim().to_ascii_lowercase();
        if !is_typed_template_v2_type(ty.as_str()) {
            continue;
        }
        if let Some(value) = args_obj.get(key) {
            let mut normalized = value.clone();
            match normalize_typed_slot_value(&mut normalized, ty.as_str()) {
                Ok(changed) => {
                    if !validate_arg_type(&normalized, ty.as_str()) {
                        outcome.errors.push(format!(
                            "slot_type_error: ContractInvoke {key} expected {ty} (policy {contract_id}:{function})"
                        ));
                        continue;
                    }
                    if changed && &normalized != value {
                        updates.push((key.clone(), normalized));
                    }
                }
                Err(detail) => outcome.errors.push(format!(
                    "slot_type_error: ContractInvoke {key} {detail} (policy {contract_id}:{function})"
                )),
            }
        }
    }

    for (key, ty_raw) in &schema.optional {
        let ty = ty_raw.trim().to_ascii_lowercase();
        if !is_typed_template_v2_type(ty.as_str()) {
            continue;
        }
        if let Some(value) = args_obj.get(key) {
            let mut normalized = value.clone();
            match normalize_typed_slot_value(&mut normalized, ty.as_str()) {
                Ok(changed) => {
                    if !validate_arg_type(&normalized, ty.as_str()) {
                        outcome.errors.push(format!(
                            "slot_type_error: ContractInvoke {key} expected {ty} (policy {contract_id}:{function})"
                        ));
                        continue;
                    }
                    if changed && &normalized != value {
                        updates.push((key.clone(), normalized));
                    }
                }
                Err(detail) => outcome.errors.push(format!(
                    "slot_type_error: ContractInvoke {key} {detail} (policy {contract_id}:{function})"
                )),
            }
        }
    }

    if let Some(args_obj_mut) = args.as_object_mut() {
        for (key, value) in updates {
            args_obj_mut.insert(key, value);
            outcome.normalized_args += 1;
        }
    }

    outcome
}

fn resolve_template_arg(prompt: &str, arg: &TemplateArg) -> Option<Value> {
    if let Some(value) = &arg.value {
        return Some(value.clone());
    }

    arg.source
        .as_deref()
        .and_then(|source| extract_arg_source(prompt, source))
        .or_else(|| arg.default.clone())
}

fn extract_arg_source(prompt: &str, source: &str) -> Option<Value> {
    match source.trim().to_ascii_lowercase().as_str() {
        "after_to" => extract_after_keyword(prompt, "to").map(Value::String),
        "after_for" => extract_after_keyword(prompt, "for").map(Value::String),
        "quoted" | "first_quoted" => extract_first_quoted(prompt).map(Value::String),
        "first_account" => first_account_re()
            .find(prompt)
            .map(|m| Value::String(m.as_str().to_string())),
        "first_contract" => first_contract_re()
            .find(prompt)
            .map(|m| Value::String(m.as_str().to_string())),
        "first_number" => first_number_re()
            .find(prompt)
            .map(|m| Value::String(m.as_str().to_string())),
        _ => None,
    }
}

fn extract_after_keyword(prompt: &str, keyword: &str) -> Option<String> {
    let pattern = format!(
        r#"(?i)\b{}\b\s+(?:"([^"]+)"|'([^']+)'|([A-Za-z0-9_.:-]+))"#,
        regex::escape(keyword)
    );
    let re = Regex::new(&pattern).ok()?;
    let captures = re.captures(prompt)?;
    captures
        .get(1)
        .or_else(|| captures.get(2))
        .or_else(|| captures.get(3))
        .map(|m| m.as_str().trim().to_string())
        .filter(|value| !value.is_empty())
}

fn extract_first_quoted(prompt: &str) -> Option<String> {
    let captures = first_quoted_re().captures(prompt)?;
    captures
        .get(1)
        .or_else(|| captures.get(2))
        .map(|m| m.as_str().trim().to_string())
        .filter(|value| !value.is_empty())
}

fn first_quoted_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#""([^"]+)"|'([^']+)'"#).expect("quoted regex"))
}

fn first_account_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bG[A-Z2-7]{55}\b").expect("account regex"))
}

fn first_contract_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bC[A-Z2-7]{55}\b").expect("contract regex"))
}

fn first_number_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d+(?:\.\d+)?\b").expect("number regex"))
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

fn is_u64_value(value: &Value) -> bool {
    if value.as_u64().is_some() {
        return true;
    }
    value
        .as_str()
        .map(|s| s.trim().parse::<u64>().is_ok())
        .unwrap_or(false)
}

fn typed_value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(n) => {
            if n.is_i64() {
                "i64"
            } else if n.is_u64() {
                "u64"
            } else {
                "number"
            }
        }
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn typed_value_preview(value: &Value) -> String {
    let mut rendered = value.to_string();
    if rendered.len() > 96 {
        rendered.truncate(93);
        rendered.push_str("...");
    }
    rendered
}

fn validate_arg_type(value: &Value, ty: &str) -> bool {
    match ty {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "bool" => value.is_boolean(),
        "address" => value.as_str().map(is_strkey).unwrap_or(false),
        "symbol" => value.as_str().map(is_symbol).unwrap_or(false),
        "bytes" => value.as_str().map(is_hex_bytes).unwrap_or(false),
        "u64" => is_u64_value(value),
        _ => false,
    }
}

fn is_typed_template_v2_type(ty: &str) -> bool {
    matches!(ty, "address" | "bytes" | "symbol" | "u64")
}

fn normalize_typed_slot_value(value: &mut Value, ty: &str) -> Result<bool, String> {
    match ty {
        "address" => {
            let Some(raw) = value.as_str() else {
                return Err(format!(
                    "expected address got {} value={}",
                    typed_value_kind(value),
                    typed_value_preview(value)
                ));
            };
            let normalized = raw.trim().to_ascii_uppercase();
            if !is_strkey(&normalized) {
                return Err(format!(
                    "expected address got string value={}",
                    typed_value_preview(&Value::String(raw.to_string()))
                ));
            }
            let changed = normalized != raw;
            if changed {
                *value = Value::String(normalized);
            }
            Ok(changed)
        }
        "bytes" => {
            let Some(raw) = value.as_str() else {
                return Err(format!(
                    "expected bytes got {} value={}",
                    typed_value_kind(value),
                    typed_value_preview(value)
                ));
            };
            let trimmed = raw.trim();
            let (had_prefix, body) = if let Some(rest) = trimmed.strip_prefix("0x") {
                (true, rest)
            } else if let Some(rest) = trimmed.strip_prefix("0X") {
                (true, rest)
            } else {
                (false, trimmed)
            };
            let compact: String = body
                .chars()
                .filter(|c| !(c.is_ascii_whitespace() || matches!(c, '_' | '-')))
                .collect();
            let mut normalized = if had_prefix {
                format!("0x{compact}")
            } else {
                compact.clone()
            };
            if !had_prefix
                && !compact.is_empty()
                && compact.len().is_multiple_of(2)
                && compact.chars().all(|c| c.is_ascii_hexdigit())
            {
                normalized = format!("0x{compact}");
            }
            if normalized.starts_with("0x") {
                let lower_hex = normalized[2..].to_ascii_lowercase();
                normalized = format!("0x{lower_hex}");
            }
            if !is_hex_bytes(&normalized) {
                return Err(format!(
                    "expected bytes got string value={}",
                    typed_value_preview(&Value::String(raw.to_string()))
                ));
            }
            let changed = normalized != raw;
            if changed {
                *value = Value::String(normalized);
            }
            Ok(changed)
        }
        "symbol" => {
            let Some(raw) = value.as_str() else {
                return Err(format!(
                    "expected symbol got {} value={}",
                    typed_value_kind(value),
                    typed_value_preview(value)
                ));
            };
            let normalized = raw.trim().to_string();
            if !is_symbol(&normalized) {
                return Err(format!(
                    "expected symbol got string value={}",
                    typed_value_preview(&Value::String(raw.to_string()))
                ));
            }
            let changed = normalized != raw;
            if changed {
                *value = Value::String(normalized);
            }
            Ok(changed)
        }
        "u64" => {
            if value.as_u64().is_some() {
                return Ok(false);
            }
            if let Some(raw) = value.as_str() {
                let trimmed = raw.trim();
                let compact: String = trimmed
                    .chars()
                    .filter(|c| !matches!(c, '_' | ','))
                    .collect();
                let parsed = compact.parse::<u64>().map_err(|_| {
                    format!(
                        "expected u64 got string value={}",
                        typed_value_preview(&Value::String(raw.to_string()))
                    )
                })?;
                let new_value = Value::Number(parsed.into());
                let changed = *value != new_value;
                *value = new_value;
                return Ok(changed);
            }
            Err(format!(
                "expected u64 got {} value={}",
                typed_value_kind(value),
                typed_value_preview(value)
            ))
        }
        _ => Ok(false),
    }
}
