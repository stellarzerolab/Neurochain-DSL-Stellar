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
