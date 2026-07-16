use std::collections::BTreeSet;

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::actions::ActionPlan;
use crate::intent_stellar::{
    build_action_plan, classify, resolve_model_path, IntentBuildConfig, IntentDecision,
};
use crate::mcp_v0_fixture::{
    self, validate_no_secret_like_fields, validate_no_submit_value, EXCLUDED_TOOLS,
};

const PLAN_TOOL: &str = "plan_stellar_action";
const PLAN_HASH_DOMAIN: &[u8] = b"neurochain:mcp-v0:action-plan-json:v1\0";
const MAX_INTENT_TEXT_BYTES: usize = 4096;
const MAX_SOURCE_HINT_BYTES: usize = 64;

pub fn tool_value_by_call_value(value: &Value) -> Result<Value, String> {
    validate_no_secret_like_fields("call", value)?;

    if value.get("fixture").is_some() && value.get("name").is_none() {
        return mcp_v0_fixture::fixture_value_by_call_value(value);
    }

    let tool = value
        .get("tool")
        .or_else(|| value.get("name"))
        .and_then(Value::as_str)
        .ok_or_else(|| "call JSON must include tool/name or fixture".to_string())?;
    if EXCLUDED_TOOLS.contains(&tool) {
        return Err(format!("tool {tool} is excluded from default MCP v0"));
    }
    if tool != PLAN_TOOL {
        return mcp_v0_fixture::fixture_value_by_call_value(value);
    }

    let arguments = value
        .get("arguments")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            "plan_stellar_action requires object arguments with intent_text or an explicit fixture/scenario"
                .to_string()
        })?;
    if arguments.contains_key("fixture") || arguments.contains_key("scenario") {
        return mcp_v0_fixture::fixture_value_by_call_value(value);
    }

    plan_stellar_action_value(arguments)
}

pub fn plan_stellar_action_value(arguments: &Map<String, Value>) -> Result<Value, String> {
    plan_stellar_action_with_classifier(arguments, |intent_text| {
        let config = IntentBuildConfig::from_env()
            .map_err(|_| "invalid server-side intent classifier configuration".to_string())?;
        let model_path = resolve_model_path();
        classify(intent_text, &model_path, config.threshold).map_err(|_| {
            "local intent_stellar runtime unavailable; configure NC_INTENT_STELLAR_MODEL"
                .to_string()
        })
    })
}

fn plan_stellar_action_with_classifier<F>(
    arguments: &Map<String, Value>,
    classifier: F,
) -> Result<Value, String>
where
    F: FnOnce(&str) -> Result<IntentDecision, String>,
{
    validate_plan_arguments(arguments)?;

    let intent_text = required_trimmed_string(arguments, "intent_text")?;
    if intent_text.len() > MAX_INTENT_TEXT_BYTES {
        return Err(format!(
            "intent_text exceeds {MAX_INTENT_TEXT_BYTES} UTF-8 bytes"
        ));
    }

    let network = optional_trimmed_string(arguments, "network")?.unwrap_or("testnet");
    if network != "testnet" {
        return Err("plan_stellar_action v0 accepts only network=testnet".to_string());
    }

    let plan_mode = optional_trimmed_string(arguments, "plan_mode")?.unwrap_or("preview_only");
    if plan_mode != "preview_only" {
        return Err("plan_stellar_action v0 accepts only plan_mode=preview_only".to_string());
    }

    let source_hint = optional_trimmed_string(arguments, "source_hint")?;
    if let Some(alias) = source_hint {
        validate_source_hint(alias)?;
    }

    let decision = classifier(intent_text)?;
    let plan = build_action_plan(intent_text, &decision);
    build_plan_response(plan, decision, network, source_hint)
}

fn build_plan_response(
    plan: ActionPlan,
    decision: IntentDecision,
    network: &str,
    source_hint: Option<&str>,
) -> Result<Value, String> {
    let action_plan_hash = canonical_action_plan_hash(&plan)?;
    let response = json!({
        "schema_version": 1,
        "tool": PLAN_TOOL,
        "mode": "read_only",
        "runtime_source": "neurochain_intent_stellar",
        "status": "ok",
        "decision": "not_evaluated",
        "exit_code": null,
        "reason_code": "plan_preview_only",
        "action_plan_hash": action_plan_hash,
        "policy_commitment": null,
        "policy_version": null,
        "stellar_verification": "not_requested",
        "attestation_submitted": false,
        "verification_transaction_submitted": false,
        "transaction_hash": null,
        "nullifier_consumed": false,
        "underlying_action_submit_allowed": false,
        "network": network,
        "source_hint": source_hint,
        "intent_decision": {
            "label": decision.label.as_str(),
            "confidence_bps": basis_points(decision.score),
            "threshold_bps": basis_points(decision.threshold),
            "downgraded_to_unknown": decision.downgraded_to_unknown
        },
        "action_plan": plan,
        "next_recommended_tool": "evaluate_guardrails",
        "logs": [
            "intent classified by the local intent_stellar model",
            "typed ActionPlan preview created by the NeuroChain runtime",
            "preview only: no policy evaluation, simulation, signing, broadcast, or submit"
        ]
    });
    validate_no_submit_value("plan_stellar_action runtime", &response)?;
    Ok(response)
}

fn canonical_action_plan_hash(plan: &ActionPlan) -> Result<String, String> {
    let encoded = serde_json::to_vec(plan)
        .map_err(|err| format!("failed to serialize canonical ActionPlan: {err}"))?;
    let mut hasher = Sha256::new();
    hasher.update(PLAN_HASH_DOMAIN);
    hasher.update(encoded);
    Ok(hex::encode(hasher.finalize()))
}

fn validate_plan_arguments(arguments: &Map<String, Value>) -> Result<(), String> {
    let allowed = BTreeSet::from(["intent_text", "network", "source_hint", "plan_mode"]);
    if let Some(field) = arguments
        .keys()
        .find(|field| !allowed.contains(field.as_str()))
    {
        return Err(format!(
            "plan_stellar_action does not accept argument {field}"
        ));
    }
    Ok(())
}

fn required_trimmed_string<'a>(
    arguments: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, String> {
    let value = arguments
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("plan_stellar_action requires non-empty {field}"))?;
    Ok(value)
}

fn optional_trimmed_string<'a>(
    arguments: &'a Map<String, Value>,
    field: &str,
) -> Result<Option<&'a str>, String> {
    let Some(value) = arguments.get(field) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .ok_or_else(|| format!("plan_stellar_action argument {field} must be a string"))?
        .trim();
    if value.is_empty() {
        return Err(format!(
            "plan_stellar_action argument {field} must not be empty"
        ));
    }
    Ok(Some(value))
}

fn validate_source_hint(alias: &str) -> Result<(), String> {
    let valid = alias.len() <= MAX_SOURCE_HINT_BYTES
        && alias
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    if !valid {
        return Err(format!(
            "source_hint must be an alias of at most {MAX_SOURCE_HINT_BYTES} ASCII characters using letters, digits, '.', '_' or '-'"
        ));
    }
    Ok(())
}

fn basis_points(value: f32) -> u16 {
    (value.clamp(0.0, 1.0) * 10_000.0).round() as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent_stellar::IntentStellarLabel;

    const CONTRACT_ID: &str = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";

    fn decision(label: IntentStellarLabel) -> IntentDecision {
        IntentDecision {
            label,
            score: 0.99,
            threshold: 0.55,
            downgraded_to_unknown: false,
        }
    }

    #[test]
    fn real_plan_adapter_uses_action_plan_builder_and_preserves_no_submit() {
        let arguments = json!({
            "intent_text": format!(
                "Invoke contract {CONTRACT_ID} function purchase_credits args={{\"amount\":100}}"
            ),
            "network": "testnet",
            "source_hint": "demo-wallet",
            "plan_mode": "preview_only"
        });
        let arguments = arguments.as_object().expect("arguments object");

        let response = plan_stellar_action_with_classifier(arguments, |_| {
            Ok(decision(IntentStellarLabel::ContractInvoke))
        })
        .expect("runtime plan response");

        assert_eq!(response["runtime_source"], "neurochain_intent_stellar");
        assert_eq!(
            response["action_plan"]["actions"][0]["kind"],
            "soroban_contract_invoke"
        );
        assert_eq!(response["intent_decision"]["label"], "ContractInvoke");
        assert_eq!(response["underlying_action_submit_allowed"], false);
        assert_eq!(response["attestation_submitted"], false);
        assert_eq!(response["verification_transaction_submitted"], false);
        assert_eq!(response["nullifier_consumed"], false);
        assert!(response["transaction_hash"].is_null());
        assert_eq!(
            response["action_plan_hash"]
                .as_str()
                .expect("hash string")
                .len(),
            64
        );
    }

    #[test]
    fn real_plan_adapter_hash_is_deterministic() {
        let arguments = json!({
            "intent_text": format!(
                "Invoke contract {CONTRACT_ID} function hello args={{\"to\":\"world\"}}"
            )
        });
        let arguments = arguments.as_object().expect("arguments object");
        let build = || {
            plan_stellar_action_with_classifier(arguments, |_| {
                Ok(decision(IntentStellarLabel::ContractInvoke))
            })
            .expect("runtime plan response")
        };

        assert_eq!(build()["action_plan_hash"], build()["action_plan_hash"]);
    }

    #[test]
    fn real_plan_adapter_rejects_non_testnet_and_unknown_arguments() {
        let mainnet = json!({"intent_text": "balance", "network": "mainnet"});
        let error = plan_stellar_action_with_classifier(
            mainnet.as_object().expect("arguments object"),
            |_| Ok(decision(IntentStellarLabel::BalanceQuery)),
        )
        .expect_err("mainnet must fail closed");
        assert!(error.contains("only network=testnet"));

        let threshold = json!({"intent_text": "balance", "threshold": 0.1});
        let error = plan_stellar_action_with_classifier(
            threshold.as_object().expect("arguments object"),
            |_| Ok(decision(IntentStellarLabel::BalanceQuery)),
        )
        .expect_err("client threshold must fail closed");
        assert!(error.contains("does not accept argument threshold"));
    }

    #[test]
    fn real_plan_adapter_keeps_unknown_plan_for_guardrail_evaluation() {
        let arguments = json!({"intent_text": "do something"});
        let arguments = arguments.as_object().expect("arguments object");
        let response = plan_stellar_action_with_classifier(arguments, |_| {
            Ok(decision(IntentStellarLabel::Unknown))
        })
        .expect("unknown preview remains inspectable");

        assert_eq!(response["decision"], "not_evaluated");
        assert!(response["exit_code"].is_null());
        assert_eq!(response["action_plan"]["actions"][0]["kind"], "unknown");
        assert_eq!(response["next_recommended_tool"], "evaluate_guardrails");
        assert_eq!(response["underlying_action_submit_allowed"], false);
    }
}
