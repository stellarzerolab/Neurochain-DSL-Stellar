use std::{collections::BTreeSet, env, fs, process::Command};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::actions::{validate_enforced_plan, validate_plan, Action, ActionPlan, Allowlist};
use crate::intent_stellar::{
    build_action_plan, classify, has_intent_blocking_issue, resolve_model_path, IntentBuildConfig,
    IntentDecision,
};
use crate::mcp_v0_fixture::{
    self, validate_no_secret_like_fields, validate_no_submit_value, EXCLUDED_TOOLS,
};
use crate::soroban_deep::{self, ContractPolicy};
use crate::zk_attestation::{
    inspect_zk_attestation, ZkAttestationViewRequest, ZkProofArtifact, ZkTypedActionPlan,
};

const PLAN_TOOL: &str = "plan_stellar_action";
const EVALUATE_TOOL: &str = "evaluate_guardrails";
const PROVE_TOOL: &str = "prove_guardrail_decision";
const VERIFY_TOOL: &str = "verify_zk_on_stellar";
const STATUS_TOOL: &str = "get_guardrail_status";
const PLAN_HASH_DOMAIN: &[u8] = b"neurochain:mcp-v0:action-plan-json:v1\0";
const MAX_INTENT_TEXT_BYTES: usize = 4096;
const MAX_SOURCE_HINT_BYTES: usize = 64;
const MAX_ACTION_PLAN_JSON_BYTES: usize = 65_536;
const MAX_ACTIONS_PER_PLAN: usize = 64;
const MAX_ZK_REQUEST_JSON_BYTES: usize = 2 * 1024 * 1024;

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
    if !matches!(
        tool,
        PLAN_TOOL | EVALUATE_TOOL | PROVE_TOOL | VERIFY_TOOL | STATUS_TOOL
    ) {
        return mcp_v0_fixture::fixture_value_by_call_value(value);
    }

    let arguments = value
        .get("arguments")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            format!("{tool} requires object arguments or an explicit fixture/scenario")
        })?;
    if arguments.contains_key("fixture") || arguments.contains_key("scenario") {
        return mcp_v0_fixture::fixture_value_by_call_value(value);
    }

    match tool {
        PLAN_TOOL => plan_stellar_action_value(arguments),
        EVALUATE_TOOL => evaluate_guardrails_value(arguments),
        PROVE_TOOL => prove_guardrail_decision_value(arguments),
        VERIFY_TOOL => verify_zk_on_stellar_value(arguments),
        STATUS_TOOL => get_guardrail_status_value(arguments),
        _ => unreachable!("tool dispatch checked above"),
    }
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

pub fn evaluate_guardrails_value(arguments: &Map<String, Value>) -> Result<Value, String> {
    let requires_approval = optional_bool(arguments, "requires_approval")?.unwrap_or(false);
    let config = GuardrailRuntimeConfig::from_env(requires_approval);
    evaluate_guardrails_with_config(arguments, config)
}

pub fn prove_guardrail_decision_value(arguments: &Map<String, Value>) -> Result<Value, String> {
    validate_prove_arguments(arguments)?;

    let proof_mode = optional_trimmed_string_for(PROVE_TOOL, arguments, "proof_mode")?
        .unwrap_or("inspect_public_artifact");
    if proof_mode != "inspect_public_artifact" {
        return Err(
            "prove_guardrail_decision v0 accepts only proof_mode=inspect_public_artifact"
                .to_string(),
        );
    }

    let serialized_size = serde_json::to_vec(arguments)
        .map_err(|err| format!("prove_guardrail_decision arguments are invalid: {err}"))?
        .len();
    if serialized_size > MAX_ZK_REQUEST_JSON_BYTES {
        return Err(format!(
            "prove_guardrail_decision arguments exceed {MAX_ZK_REQUEST_JSON_BYTES} serialized bytes"
        ));
    }

    let action_plan: ZkTypedActionPlan = serde_json::from_value(
        arguments
            .get("action_plan")
            .cloned()
            .ok_or_else(|| "prove_guardrail_decision requires action_plan".to_string())?,
    )
    .map_err(|err| format!("prove_guardrail_decision action_plan is invalid: {err}"))?;
    let proof: ZkProofArtifact = serde_json::from_value(
        arguments
            .get("proof")
            .cloned()
            .ok_or_else(|| "prove_guardrail_decision requires proof".to_string())?,
    )
    .map_err(|err| format!("prove_guardrail_decision proof is invalid: {err}"))?;
    let journal_digest = proof.journal_digest_hex.to_ascii_lowercase();

    let inspected = inspect_zk_attestation(ZkAttestationViewRequest { action_plan, proof })
        .map_err(|err| {
            format!(
                "prove_guardrail_decision rejected public artifact: {}",
                err.code()
            )
        })?;
    let attestation = inspected
        .zk_attestation
        .ok_or_else(|| "prove_guardrail_decision inspection returned no attestation".to_string())?;
    let decision = attestation.attested_decision;

    let response = json!({
        "schema_version": 1,
        "tool": PROVE_TOOL,
        "mode": "read_only",
        "runtime_source": "neurochain_zk_attestation_view",
        "status": if decision.status == "blocked" { "blocked" } else { "ok" },
        "decision": decision.status,
        "exit_code": decision.exit_code,
        "reason_code": decision.reason,
        "action_plan_hash": attestation.action_plan_hash,
        "policy_commitment": attestation.policy_commitment,
        "policy_version": attestation.policy_version,
        "stellar_verification": "required_on_stellar",
        "attestation_submitted": false,
        "verification_transaction_submitted": false,
        "transaction_hash": null,
        "nullifier_consumed": false,
        "underlying_action_submit_allowed": false,
        "proof_artifact_ref": format!("inline:{journal_digest}"),
        "proof_kind": attestation.proof_kind,
        "proof_binding": attestation.verification_state,
        "cryptographically_verified": attestation.cryptographically_verified,
        "stellar_verification_required": attestation.stellar_verification_required,
        "evaluator_image_id": attestation.evaluator_image_id,
        "verifier_selector": attestation.verifier_selector,
        "journal_digest": journal_digest,
        "audit_nullifier": attestation.audit_nullifier,
        "private_policy_revealed": attestation.private_policy_revealed,
        "requires_approval": decision.requires_approval,
        "next_recommended_tool": "verify_zk_on_stellar",
        "logs": [
            "public ZK journal and canonical typed ActionPlan bindings validated locally",
            "Groth16 seal not cryptographically verified by this tool; Stellar verification remains required",
            "private policy, proof seal, and public journal bytes were not returned",
            "read only: no signing, broadcast, attestation, nullifier consume, or ActionPlan submit"
        ]
    });
    validate_no_submit_value("prove_guardrail_decision runtime", &response)?;
    Ok(response)
}

pub fn verify_zk_on_stellar_value(arguments: &Map<String, Value>) -> Result<Value, String> {
    validate_verify_arguments(arguments)?;
    let config = ZkStellarVerifyConfig::from_env(arguments)?;
    verify_zk_on_stellar_with_runner(arguments, config, run_zk_stellar_read_only_cli)
}

pub fn get_guardrail_status_value(arguments: &Map<String, Value>) -> Result<Value, String> {
    validate_status_arguments(arguments)?;

    let Some(latest_result) = arguments.get("latest_result") else {
        let response = json!({
            "schema_version": 1,
            "tool": STATUS_TOOL,
            "mode": "read_only",
            "runtime_source": "neurochain_mcp_status_view",
            "status": "state_unavailable",
            "decision": "not_evaluated",
            "exit_code": null,
            "reason_code": "status_unavailable",
            "action_plan_hash": null,
            "policy_commitment": null,
            "policy_version": null,
            "stellar_verification": "not_requested",
            "attestation_submitted": false,
            "verification_transaction_submitted": false,
            "transaction_hash": null,
            "nullifier_consumed": false,
            "underlying_action_submit_allowed": false,
            "local_binding": null,
            "verification_mode": "read_only",
            "status_source": "no_latest_result",
            "session_id": arguments.get("session_id").cloned().unwrap_or(Value::Null),
            "proof_artifact_ref": arguments
                .get("proof_artifact_ref")
                .cloned()
                .unwrap_or(Value::Null),
            "logs": [
                "no latest_result supplied; status lookup is unavailable in the stateless MCP v0 adapter",
                "status is observational and did not trigger verification, attestation, nullifier consume, or submit"
            ]
        });
        validate_no_submit_value("get_guardrail_status runtime", &response)?;
        return Ok(response);
    };

    let latest_result = latest_result
        .as_object()
        .ok_or_else(|| "get_guardrail_status latest_result must be an object".to_string())?;
    let latest_value = Value::Object(latest_result.clone());
    validate_no_submit_value("get_guardrail_status latest_result", &latest_value)?;

    let latest_tool = required_status_string(latest_result, "tool")?;
    if latest_tool == STATUS_TOOL {
        return Err(
            "get_guardrail_status latest_result must come from a prior non-status tool".to_string(),
        );
    }
    if !matches!(
        latest_tool,
        PLAN_TOOL | EVALUATE_TOOL | PROVE_TOOL | VERIFY_TOOL
    ) {
        return Err(format!(
            "get_guardrail_status latest_result uses unsupported tool {latest_tool}"
        ));
    }

    let decision = status_string_or(latest_result, "decision", "not_evaluated")?;
    validate_status_decision(decision)?;
    let status = status_string_or(latest_result, "status", "ok")?;
    validate_status_value_name(status)?;
    let exit_code = optional_status_exit_code(latest_result)?;
    let reason_code = status_string_or(latest_result, "reason_code", "status")?;
    let stellar_verification =
        status_string_or(latest_result, "stellar_verification", "not_requested")?;
    validate_stellar_verification(stellar_verification)?;

    let response = json!({
        "schema_version": 1,
        "tool": STATUS_TOOL,
        "mode": "read_only",
        "runtime_source": "neurochain_mcp_status_view",
        "status": status,
        "decision": decision,
        "exit_code": exit_code,
        "reason_code": reason_code,
        "action_plan_hash": optional_status_string_value(latest_result, "action_plan_hash")?,
        "policy_commitment": optional_status_string_value(latest_result, "policy_commitment")?,
        "policy_version": optional_status_u64_value(latest_result, "policy_version")?,
        "stellar_verification": stellar_verification,
        "attestation_submitted": false,
        "verification_transaction_submitted": false,
        "transaction_hash": null,
        "nullifier_consumed": false,
        "underlying_action_submit_allowed": false,
        "local_binding": status_local_binding(latest_tool, latest_result, stellar_verification),
        "verification_mode": status_string_value(latest_result, "verification_mode")
            .unwrap_or_else(|| Value::String("read_only".to_string())),
        "status_source": "latest_result",
        "last_tool": latest_tool,
        "cryptographically_verified": latest_result
            .get("cryptographically_verified")
            .cloned()
            .unwrap_or(Value::Bool(stellar_verification == "verified_on_stellar")),
        "stellar_verification_required": latest_result
            .get("stellar_verification_required")
            .cloned()
            .unwrap_or(Value::Bool(stellar_verification == "required_on_stellar")),
        "requires_approval": latest_result
            .get("requires_approval")
            .cloned()
            .unwrap_or(Value::Bool(decision == "requires_approval")),
        "audit_nullifier": optional_status_string_value(latest_result, "audit_nullifier")?,
        "logs": [
            "latest MCP read-only result normalized into a guardrail status view",
            "status is observational and did not trigger verification, attestation, nullifier consume, or submit",
            "proof, payment, verification, or status is not underlying ActionPlan submit permission"
        ]
    });
    validate_no_submit_value("get_guardrail_status runtime", &response)?;
    Ok(response)
}

#[derive(Debug, Clone)]
struct ZkStellarVerifyConfig {
    contract_id: String,
    network: String,
    source: String,
    stellar_cli: String,
    instruction_leeway: u32,
}

impl ZkStellarVerifyConfig {
    fn from_env(arguments: &Map<String, Value>) -> Result<Self, String> {
        let network = optional_trimmed_string_for(VERIFY_TOOL, arguments, "network")?
            .map(str::to_string)
            .or_else(|| {
                env::var("NC_STELLAR_NETWORK")
                    .or_else(|_| env::var("NC_SOROBAN_NETWORK"))
                    .ok()
            })
            .unwrap_or_else(|| "testnet".to_string());
        if network != "testnet" {
            return Err("verify_zk_on_stellar v0 accepts only network=testnet".to_string());
        }

        let contract_argument = optional_trimmed_string_for(VERIFY_TOOL, arguments, "contract_id")?;
        let configured_contract = env::var("NC_ZK_GUARDRAIL_CONTRACT")
            .ok()
            .filter(|value| !value.trim().is_empty());
        if let (Some(argument), Some(configured)) =
            (contract_argument, configured_contract.as_ref())
        {
            if !argument.eq_ignore_ascii_case(configured.trim()) {
                return Err(
                    "verify_zk_on_stellar contract_id does not match configured verifier"
                        .to_string(),
                );
            }
        }
        let contract_id = contract_argument
            .map(str::to_string)
            .or_else(|| configured_contract.map(|value| value.trim().to_string()))
            .ok_or_else(|| {
                "verify_zk_on_stellar requires contract_id or NC_ZK_GUARDRAIL_CONTRACT".to_string()
            })?;

        let source = env::var("NC_SOROBAN_SOURCE")
            .or_else(|_| env::var("NC_STELLAR_SOURCE"))
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                "verify_zk_on_stellar requires server-configured NC_SOROBAN_SOURCE/NC_STELLAR_SOURCE"
                    .to_string()
            })?;
        let stellar_cli = env::var("NC_STELLAR_CLI").unwrap_or_else(|_| "stellar".to_string());
        let instruction_leeway = env::var("NC_ZK_INSTRUCTION_LEEWAY")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(10_000_000);

        Ok(Self {
            contract_id,
            network,
            source,
            stellar_cli,
            instruction_leeway,
        })
    }
}

#[derive(Debug, Clone)]
struct ZkStellarAccepted {
    action_plan_hash: String,
    policy_commitment: String,
    policy_version: u32,
    decision_status: u32,
    exit_code: u32,
    reason_code: u32,
    requires_approval: bool,
    audit_nullifier: String,
    next_step: String,
}

fn verify_zk_on_stellar_with_runner<F>(
    arguments: &Map<String, Value>,
    config: ZkStellarVerifyConfig,
    runner: F,
) -> Result<Value, String>
where
    F: FnOnce(&ZkStellarVerifyConfig, &ZkProofArtifact) -> Result<String, String>,
{
    validate_verify_arguments(arguments)?;

    let verification_mode =
        optional_trimmed_string_for(VERIFY_TOOL, arguments, "verification_mode")?
            .unwrap_or("read_only");
    if verification_mode != "read_only" {
        return Err("verify_zk_on_stellar v0 accepts only verification_mode=read_only".to_string());
    }

    let serialized_size = serde_json::to_vec(arguments)
        .map_err(|err| format!("verify_zk_on_stellar arguments are invalid: {err}"))?
        .len();
    if serialized_size > MAX_ZK_REQUEST_JSON_BYTES {
        return Err(format!(
            "verify_zk_on_stellar arguments exceed {MAX_ZK_REQUEST_JSON_BYTES} serialized bytes"
        ));
    }

    let action_plan: ZkTypedActionPlan = serde_json::from_value(
        arguments
            .get("action_plan")
            .cloned()
            .ok_or_else(|| "verify_zk_on_stellar requires action_plan".to_string())?,
    )
    .map_err(|err| format!("verify_zk_on_stellar action_plan is invalid: {err}"))?;
    let proof: ZkProofArtifact = serde_json::from_value(
        arguments
            .get("proof")
            .cloned()
            .ok_or_else(|| "verify_zk_on_stellar requires proof".to_string())?,
    )
    .map_err(|err| format!("verify_zk_on_stellar proof is invalid: {err}"))?;
    let journal_digest = proof.journal_digest_hex.to_ascii_lowercase();

    let inspected = inspect_zk_attestation(ZkAttestationViewRequest {
        action_plan,
        proof: proof.clone(),
    })
    .map_err(|err| {
        format!(
            "verify_zk_on_stellar rejected public artifact before Stellar verification: {}",
            err.code()
        )
    })?;
    let attestation = inspected
        .zk_attestation
        .ok_or_else(|| "verify_zk_on_stellar inspection returned no attestation".to_string())?;

    let output = runner(&config, &proof)?;
    let accepted = parse_zk_stellar_accepted(&output)?;
    validate_zk_stellar_accepted(&attestation, &accepted)?;
    let decision = attestation.attested_decision;

    let response = json!({
        "schema_version": 1,
        "tool": VERIFY_TOOL,
        "mode": "read_only",
        "runtime_source": "neurochain_soroban_read_only_verifier",
        "status": if decision.status == "blocked" { "blocked" } else { "ok" },
        "decision": decision.status,
        "exit_code": decision.exit_code,
        "reason_code": decision.reason,
        "action_plan_hash": accepted.action_plan_hash,
        "policy_commitment": accepted.policy_commitment,
        "policy_version": accepted.policy_version,
        "stellar_verification": "verified_on_stellar",
        "verification_mode": "read_only",
        "cryptographically_verified": true,
        "stellar_verification_required": false,
        "attestation_submitted": false,
        "verification_transaction_submitted": false,
        "transaction_hash": null,
        "nullifier_consumed": false,
        "underlying_action_submit_allowed": false,
        "proof_artifact_ref": format!("inline:{journal_digest}"),
        "proof_kind": attestation.proof_kind,
        "proof_binding": attestation.verification_state,
        "contract_id": config.contract_id,
        "network": config.network,
        "evaluator_image_id": attestation.evaluator_image_id,
        "verifier_selector": attestation.verifier_selector,
        "journal_digest": journal_digest,
        "audit_nullifier": accepted.audit_nullifier,
        "requires_approval": accepted.requires_approval,
        "next_recommended_tool": "get_guardrail_status",
        "logs": [
            "public ZK journal and canonical typed ActionPlan bindings validated locally",
            "Soroban verifier accepted the Groth16 proof in read-only mode",
            "read only: --send no; no signing, broadcast, attestation, nullifier consume, or ActionPlan submit"
        ]
    });
    validate_no_submit_value("verify_zk_on_stellar runtime", &response)?;
    Ok(response)
}

struct GuardrailRuntimeConfig {
    allowlist: Allowlist,
    allowlist_enforced: bool,
    policies: Vec<ContractPolicy>,
    policy_load_errors: Vec<String>,
    policy_enforced: bool,
    requires_approval: bool,
}

impl GuardrailRuntimeConfig {
    fn from_env(requires_approval: bool) -> Self {
        let policy_load = load_contract_policies();
        Self {
            allowlist: Allowlist::from_env(),
            allowlist_enforced: env_bool("NC_ALLOWLIST_ENFORCE"),
            policies: policy_load.policies,
            policy_load_errors: policy_load.errors,
            policy_enforced: env_bool("NC_CONTRACT_POLICY_ENFORCE"),
            requires_approval,
        }
    }
}

#[derive(Default)]
struct ContractPolicyLoad {
    policies: Vec<ContractPolicy>,
    errors: Vec<String>,
}

fn load_contract_policies() -> ContractPolicyLoad {
    let mut load = ContractPolicyLoad::default();

    if let Ok(path) = env::var("NC_CONTRACT_POLICY") {
        if !path.trim().is_empty() {
            load_contract_policy_file(&path, &mut load);
        }
    }

    let explicit_policy_dir = env::var("NC_CONTRACT_POLICY_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let policy_dir = explicit_policy_dir
        .clone()
        .unwrap_or_else(|| "contracts".to_string());
    match fs::read_dir(&policy_dir) {
        Ok(entries) => {
            let mut policy_paths: Vec<_> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .map(|path| path.join("policy.json"))
                .filter(|path| path.exists())
                .collect();
            policy_paths.sort();
            for policy_path in policy_paths {
                load_contract_policy_file(&policy_path.to_string_lossy(), &mut load);
            }
        }
        Err(err) if explicit_policy_dir.is_some() => load.errors.push(format!(
            "policy_load_failed: policy dir not found or unreadable: {policy_dir}: {err}"
        )),
        Err(_) => {}
    }

    load
}

fn load_contract_policy_file(path: &str, load: &mut ContractPolicyLoad) {
    match fs::read_to_string(path) {
        Ok(data) => match serde_json::from_str::<ContractPolicy>(&data) {
            Ok(policy) => load.policies.push(policy),
            Err(err) => load.errors.push(format!(
                "policy_load_failed: policy parse failed for {path}: {err}"
            )),
        },
        Err(err) => load.errors.push(format!(
            "policy_load_failed: policy file not found or unreadable: {path}: {err}"
        )),
    }
}

fn evaluate_guardrails_with_config(
    arguments: &Map<String, Value>,
    config: GuardrailRuntimeConfig,
) -> Result<Value, String> {
    validate_evaluate_arguments(arguments)?;

    let policy_ref = optional_trimmed_string_for(EVALUATE_TOOL, arguments, "policy_ref")?
        .unwrap_or("configured");
    if policy_ref != "configured" {
        return Err("evaluate_guardrails v0 accepts only policy_ref=configured".to_string());
    }
    let evaluation_mode = optional_trimmed_string_for(EVALUATE_TOOL, arguments, "evaluation_mode")?
        .unwrap_or("deterministic");
    if evaluation_mode != "deterministic" {
        return Err(
            "evaluate_guardrails v0 accepts only evaluation_mode=deterministic".to_string(),
        );
    }

    let action_plan_value = arguments
        .get("action_plan")
        .ok_or_else(|| "evaluate_guardrails requires action_plan".to_string())?;
    let action_plan_size = serde_json::to_vec(action_plan_value)
        .map_err(|err| format!("evaluate_guardrails action_plan is invalid: {err}"))?
        .len();
    if action_plan_size > MAX_ACTION_PLAN_JSON_BYTES {
        return Err(format!(
            "evaluate_guardrails action_plan exceeds {MAX_ACTION_PLAN_JSON_BYTES} serialized bytes"
        ));
    }
    let plan: ActionPlan = serde_json::from_value(action_plan_value.clone())
        .map_err(|err| format!("evaluate_guardrails action_plan is invalid: {err}"))?;
    if plan.actions.len() > MAX_ACTIONS_PER_PLAN {
        return Err(format!(
            "evaluate_guardrails action_plan exceeds {MAX_ACTIONS_PER_PLAN} actions"
        ));
    }
    let expected_hash = required_trimmed_string_for(EVALUATE_TOOL, arguments, "action_plan_hash")?;
    validate_hash(expected_hash, "action_plan_hash")?;
    let action_plan_hash = canonical_action_plan_hash(&plan)?;
    if !expected_hash.eq_ignore_ascii_case(&action_plan_hash) {
        return Err(
            "evaluate_guardrails action_plan_hash does not match the canonical ActionPlan"
                .to_string(),
        );
    }

    let allowlist_violations = if config.allowlist_enforced {
        validate_enforced_plan(&plan, &config.allowlist)
    } else {
        validate_plan(&plan, &config.allowlist)
    };
    let (policy_warnings, mut policy_errors) =
        soroban_deep::validate_contract_policies(&plan, &config.policies);
    if config.policy_enforced && plan_needs_contract_policy(&plan) {
        policy_errors.extend(config.policy_load_errors.iter().cloned());
        if config.policies.is_empty() {
            policy_errors.push(
                "policy_unconfigured: contract_policy_enforce enabled but no contract policies loaded"
                    .to_string(),
            );
        }
    }
    let intent_blocked = plan.actions.is_empty() || has_intent_blocking_issue(&plan);

    let exit_code = if config.allowlist_enforced && !allowlist_violations.is_empty() {
        3
    } else if config.policy_enforced && !policy_errors.is_empty() {
        4
    } else if intent_blocked {
        5
    } else {
        0
    };
    let blocked = exit_code != 0;
    let requires_approval = config.requires_approval && !blocked;
    let (decision, reason_code) = match exit_code {
        3 => ("blocked", "allowlist"),
        4 => ("blocked", "contract_policy"),
        5 => ("blocked", "intent_safety"),
        _ if requires_approval => ("requires_approval", "approval_threshold"),
        _ => ("approved", "passed"),
    };

    let allowlist_status = guardrail_status(
        config.allowlist_enforced,
        !allowlist_violations.is_empty(),
        false,
        false,
    );
    let contract_policy_status = guardrail_status(
        config.policy_enforced,
        !policy_errors.is_empty(),
        !policy_warnings.is_empty(),
        requires_approval,
    );
    let intent_status = if intent_blocked { "blocked" } else { "passed" };
    let next_recommended_tool = if decision == "approved" {
        "prove_guardrail_decision"
    } else {
        "get_guardrail_status"
    };

    let response = json!({
        "schema_version": 1,
        "tool": EVALUATE_TOOL,
        "mode": "read_only",
        "runtime_source": "neurochain_guardrails",
        "status": if blocked { "blocked" } else { "ok" },
        "decision": decision,
        "exit_code": exit_code,
        "reason_code": reason_code,
        "action_plan_hash": action_plan_hash,
        "policy_ref": policy_ref,
        "policy_commitment": null,
        "policy_version": null,
        "stellar_verification": "not_requested",
        "attestation_submitted": false,
        "verification_transaction_submitted": false,
        "transaction_hash": null,
        "nullifier_consumed": false,
        "underlying_action_submit_allowed": false,
        "guardrails": {
            "allowlist": allowlist_status,
            "contract_policy": contract_policy_status,
            "intent_safety": intent_status
        },
        "observations": {
            "allowlist_enforced": config.allowlist_enforced,
            "allowlist_violation_count": allowlist_violations.len(),
            "contract_policy_enforced": config.policy_enforced,
            "contract_policy_count": config.policies.len(),
            "contract_policy_warning_count": policy_warnings.len(),
            "contract_policy_error_count": policy_errors.len()
        },
        "next_recommended_tool": next_recommended_tool,
        "logs": [
            "deterministic NeuroChain guardrails evaluated the canonical ActionPlan",
            format!(
                "guardrail summary: allowlist_violations={} policy_warnings={} policy_errors={} intent_blocked={intent_blocked}",
                allowlist_violations.len(),
                policy_warnings.len(),
                policy_errors.len()
            ),
            "read only: no simulation, signing, broadcast, attestation, nullifier consume, or submit"
        ]
    });
    validate_no_submit_value("evaluate_guardrails runtime", &response)?;
    Ok(response)
}

fn guardrail_status(
    enforced: bool,
    has_blocking_findings: bool,
    has_warnings: bool,
    requires_approval: bool,
) -> &'static str {
    if requires_approval {
        "requires_approval"
    } else if enforced && has_blocking_findings {
        "blocked"
    } else if has_blocking_findings || has_warnings {
        "warning_only"
    } else {
        "passed"
    }
}

fn plan_needs_contract_policy(plan: &ActionPlan) -> bool {
    plan.actions.iter().any(|action| {
        matches!(
            action,
            Action::SorobanContractInvoke { .. } | Action::SorobanContractDeploy { .. }
        )
    })
}

fn env_bool(name: &str) -> bool {
    matches!(
        env::var(name)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
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

fn validate_evaluate_arguments(arguments: &Map<String, Value>) -> Result<(), String> {
    let allowed = BTreeSet::from([
        "action_plan",
        "action_plan_hash",
        "policy_ref",
        "evaluation_mode",
        "requires_approval",
    ]);
    if let Some(field) = arguments
        .keys()
        .find(|field| !allowed.contains(field.as_str()))
    {
        return Err(format!(
            "evaluate_guardrails does not accept argument {field}"
        ));
    }
    Ok(())
}

fn validate_prove_arguments(arguments: &Map<String, Value>) -> Result<(), String> {
    let allowed = BTreeSet::from(["action_plan", "proof", "proof_mode"]);
    if let Some(field) = arguments
        .keys()
        .find(|field| !allowed.contains(field.as_str()))
    {
        return Err(format!(
            "prove_guardrail_decision does not accept argument {field}"
        ));
    }
    Ok(())
}

fn validate_verify_arguments(arguments: &Map<String, Value>) -> Result<(), String> {
    let allowed = BTreeSet::from([
        "action_plan",
        "proof",
        "contract_id",
        "network",
        "verification_mode",
    ]);
    if let Some(field) = arguments
        .keys()
        .find(|field| !allowed.contains(field.as_str()))
    {
        return Err(format!(
            "verify_zk_on_stellar does not accept argument {field}"
        ));
    }
    Ok(())
}

fn validate_status_arguments(arguments: &Map<String, Value>) -> Result<(), String> {
    let allowed = BTreeSet::from(["latest_result", "session_id", "proof_artifact_ref"]);
    if let Some(field) = arguments
        .keys()
        .find(|field| !allowed.contains(field.as_str()))
    {
        return Err(format!(
            "get_guardrail_status does not accept argument {field}"
        ));
    }
    for field in ["session_id", "proof_artifact_ref"] {
        if let Some(value) = arguments.get(field) {
            let value = value
                .as_str()
                .ok_or_else(|| format!("get_guardrail_status argument {field} must be a string"))?;
            if value.trim().is_empty() {
                return Err(format!(
                    "get_guardrail_status argument {field} must not be empty"
                ));
            }
        }
    }
    Ok(())
}

fn required_status_string<'a>(
    result: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, String> {
    result
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("get_guardrail_status latest_result missing string field {field}"))
}

fn status_string_or<'a>(
    result: &'a Map<String, Value>,
    field: &str,
    default: &'a str,
) -> Result<&'a str, String> {
    match result.get(field) {
        Some(value) => value.as_str().ok_or_else(|| {
            format!("get_guardrail_status latest_result field {field} must be a string")
        }),
        None => Ok(default),
    }
}

fn status_string_value(result: &Map<String, Value>, field: &str) -> Option<Value> {
    result
        .get(field)
        .and_then(Value::as_str)
        .map(|value| Value::String(value.to_string()))
}

fn optional_status_string_value(result: &Map<String, Value>, field: &str) -> Result<Value, String> {
    match result.get(field) {
        Some(Value::Null) | None => Ok(Value::Null),
        Some(Value::String(value)) => Ok(Value::String(value.clone())),
        Some(_) => Err(format!(
            "get_guardrail_status latest_result field {field} must be a string or null"
        )),
    }
}

fn optional_status_u64_value(result: &Map<String, Value>, field: &str) -> Result<Value, String> {
    match result.get(field) {
        Some(Value::Null) | None => Ok(Value::Null),
        Some(Value::Number(number)) if number.as_u64().is_some() => {
            Ok(Value::Number(number.clone()))
        }
        Some(_) => Err(format!(
            "get_guardrail_status latest_result field {field} must be an integer or null"
        )),
    }
}

fn status_local_binding(
    latest_tool: &str,
    result: &Map<String, Value>,
    stellar_verification: &str,
) -> Value {
    result
        .get("local_binding")
        .or_else(|| result.get("proof_binding"))
        .cloned()
        .unwrap_or_else(|| {
            if matches!(latest_tool, PROVE_TOOL | VERIFY_TOOL)
                && matches!(
                    stellar_verification,
                    "required_on_stellar" | "verified_on_stellar"
                )
            {
                Value::String("binding_validated".to_string())
            } else {
                Value::Null
            }
        })
}

fn optional_status_exit_code(result: &Map<String, Value>) -> Result<Value, String> {
    let value = optional_status_u64_value(result, "exit_code")?;
    match value.as_u64() {
        Some(0 | 3 | 4 | 5) | None => Ok(value),
        Some(other) => Err(format!(
            "get_guardrail_status latest_result exit_code {other} is outside MCP v0"
        )),
    }
}

fn validate_status_decision(value: &str) -> Result<(), String> {
    if matches!(
        value,
        "not_evaluated" | "approved" | "requires_approval" | "blocked"
    ) {
        Ok(())
    } else {
        Err(format!(
            "get_guardrail_status latest_result decision {value} is outside MCP v0"
        ))
    }
}

fn validate_status_value_name(value: &str) -> Result<(), String> {
    if matches!(value, "ok" | "blocked" | "state_unavailable") {
        Ok(())
    } else {
        Err(format!(
            "get_guardrail_status latest_result status {value} is outside MCP v0"
        ))
    }
}

fn validate_stellar_verification(value: &str) -> Result<(), String> {
    if matches!(
        value,
        "not_requested" | "required_on_stellar" | "verified_on_stellar" | "failed_on_stellar"
    ) {
        Ok(())
    } else {
        Err(format!(
            "get_guardrail_status latest_result stellar_verification {value} is outside MCP v0"
        ))
    }
}

fn run_zk_stellar_read_only_cli(
    config: &ZkStellarVerifyConfig,
    proof: &ZkProofArtifact,
) -> Result<String, String> {
    let instruction_leeway = config.instruction_leeway.to_string();
    let output = Command::new(&config.stellar_cli)
        .args([
            "contract",
            "invoke",
            "--id",
            &config.contract_id,
            "--source",
            &config.source,
            "--network",
            &config.network,
            "--send",
            "no",
            "--instruction-leeway",
            &instruction_leeway,
            "--",
            "verify",
            "--seal",
            &proof.seal_hex,
            "--journal_bytes",
            &proof.journal_hex,
        ])
        .output()
        .map_err(|err| format!("failed to run Stellar CLI for read-only ZK verification: {err}"))?;

    if !output.status.success() {
        return Err(format!(
            "Stellar read-only ZK verification failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        Ok(String::from_utf8_lossy(&output.stderr).trim().to_string())
    } else {
        Ok(stdout)
    }
}

fn parse_zk_stellar_accepted(output: &str) -> Result<ZkStellarAccepted, String> {
    let value = serde_json::from_str::<Value>(output.trim()).or_else(|_| {
        output
            .lines()
            .rev()
            .find_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
            .ok_or_else(|| "Stellar ZK response did not contain a JSON contract result".to_string())
    })?;

    Ok(ZkStellarAccepted {
        action_plan_hash: json_string_field(&value, "action_plan_hash")?,
        policy_commitment: json_string_field(&value, "policy_commitment")?,
        policy_version: json_u32_field(&value, "policy_version")?,
        decision_status: json_u32_field(&value, "decision_status")?,
        exit_code: json_u32_field(&value, "exit_code")?,
        reason_code: json_u32_field(&value, "reason_code")?,
        requires_approval: value
            .get("requires_approval")
            .and_then(Value::as_bool)
            .ok_or_else(|| {
                "Stellar ZK response is missing boolean field `requires_approval`".to_string()
            })?,
        audit_nullifier: json_string_field(&value, "audit_nullifier")?,
        next_step: json_string_field(&value, "next_step")?,
    })
}

fn json_string_field(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("Stellar ZK response is missing string field `{field}`"))
}

fn json_u32_field(value: &Value, field: &str) -> Result<u32, String> {
    let number = value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("Stellar ZK response is missing integer field `{field}`"))?;
    u32::try_from(number).map_err(|_| format!("Stellar ZK field `{field}` exceeds u32"))
}

fn validate_zk_stellar_accepted(
    local: &crate::zk_attestation::ZkAttestationView,
    accepted: &ZkStellarAccepted,
) -> Result<(), String> {
    let expected_next_step = match local.attested_decision.status.as_str() {
        "approved" => "eligible_for_separate_approval_flow",
        "requires_approval" => "requires_approval",
        "blocked" => "blocked",
        status => return Err(format!("unknown local ZK decision status `{status}`")),
    };
    let bindings_match = accepted
        .action_plan_hash
        .eq_ignore_ascii_case(&local.action_plan_hash)
        && accepted
            .policy_commitment
            .eq_ignore_ascii_case(&local.policy_commitment)
        && accepted.policy_version == local.policy_version
        && accepted
            .audit_nullifier
            .eq_ignore_ascii_case(&local.audit_nullifier);
    let decision_matches = accepted.decision_status
        == expected_zk_decision_status(&local.attested_decision.status)?
        && accepted.exit_code == u32::from(local.attested_decision.exit_code)
        && accepted.reason_code == expected_zk_reason_code(&local.attested_decision.reason)?
        && accepted.requires_approval == local.attested_decision.requires_approval
        && normalized_zk_next_step(&accepted.next_step)
            == normalized_zk_next_step(expected_next_step);
    if !bindings_match || !decision_matches {
        return Err(
            "Stellar ZK result does not match the locally bound ActionPlan and journal (exit 4)"
                .to_string(),
        );
    }
    Ok(())
}

fn expected_zk_decision_status(status: &str) -> Result<u32, String> {
    match status {
        "approved" => Ok(0),
        "blocked" => Ok(1),
        "requires_approval" => Ok(2),
        _ => Err(format!("unknown local ZK decision status `{status}`")),
    }
}

fn expected_zk_reason_code(reason: &str) -> Result<u32, String> {
    match reason {
        "passed" => Ok(0),
        "allowlist" => Ok(1),
        "contract_policy" => Ok(2),
        "intent_safety" => Ok(3),
        "approval_threshold" => Ok(4),
        "invalid_attestation" => Ok(5),
        "replay" => Ok(6),
        _ => Err(format!("unknown local ZK reason code `{reason}`")),
    }
}

fn normalized_zk_next_step(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn required_trimmed_string<'a>(
    arguments: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, String> {
    required_trimmed_string_for(PLAN_TOOL, arguments, field)
}

fn required_trimmed_string_for<'a>(
    tool: &str,
    arguments: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, String> {
    let value = arguments
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{tool} requires non-empty {field}"))?;
    Ok(value)
}

fn optional_trimmed_string<'a>(
    arguments: &'a Map<String, Value>,
    field: &str,
) -> Result<Option<&'a str>, String> {
    optional_trimmed_string_for(PLAN_TOOL, arguments, field)
}

fn optional_trimmed_string_for<'a>(
    tool: &str,
    arguments: &'a Map<String, Value>,
    field: &str,
) -> Result<Option<&'a str>, String> {
    let Some(value) = arguments.get(field) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .ok_or_else(|| format!("{tool} argument {field} must be a string"))?
        .trim();
    if value.is_empty() {
        return Err(format!("{tool} argument {field} must not be empty"));
    }
    Ok(Some(value))
}

fn optional_bool(arguments: &Map<String, Value>, field: &str) -> Result<Option<bool>, String> {
    arguments
        .get(field)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| format!("evaluate_guardrails argument {field} must be a boolean"))
        })
        .transpose()
}

fn validate_hash(value: &str, field: &str) -> Result<(), String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "{field} must be a 32-byte hexadecimal SHA-256 value"
        ));
    }
    Ok(())
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

    fn evaluate_arguments(plan: &ActionPlan) -> Map<String, Value> {
        let hash = canonical_action_plan_hash(plan).expect("canonical plan hash");
        json!({
            "action_plan": plan,
            "action_plan_hash": hash,
            "policy_ref": "configured",
            "evaluation_mode": "deterministic"
        })
        .as_object()
        .expect("evaluation arguments")
        .clone()
    }

    fn guardrail_config(
        allowlist: Allowlist,
        allowlist_enforced: bool,
        policies: Vec<ContractPolicy>,
        policy_enforced: bool,
        requires_approval: bool,
    ) -> GuardrailRuntimeConfig {
        GuardrailRuntimeConfig {
            allowlist,
            allowlist_enforced,
            policies,
            policy_load_errors: Vec::new(),
            policy_enforced,
            requires_approval,
        }
    }

    fn balance_plan() -> ActionPlan {
        ActionPlan {
            schema_version: 1,
            actions: vec![Action::StellarAccountBalance {
                account: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF".to_string(),
                asset: None,
            }],
            warnings: Vec::new(),
            source: Some("mcp-test".to_string()),
        }
    }

    fn proof_arguments(proof_json: &str) -> Map<String, Value> {
        json!({
            "action_plan": serde_json::from_str::<Value>(include_str!(
                "../hackathons/stellar-real-world-zk/fixtures/typed_action_plan.json"
            ))
            .expect("ZK typed ActionPlan fixture"),
            "proof": serde_json::from_str::<Value>(proof_json).expect("ZK proof fixture"),
            "proof_mode": "inspect_public_artifact"
        })
        .as_object()
        .expect("proof arguments")
        .clone()
    }

    fn verify_arguments(proof_json: &str) -> Map<String, Value> {
        json!({
            "action_plan": serde_json::from_str::<Value>(include_str!(
                "../hackathons/stellar-real-world-zk/fixtures/typed_action_plan.json"
            ))
            .expect("ZK typed ActionPlan fixture"),
            "proof": serde_json::from_str::<Value>(proof_json).expect("ZK proof fixture"),
            "contract_id": "CTESTZKGUARDRAIL",
            "network": "testnet",
            "verification_mode": "read_only"
        })
        .as_object()
        .expect("verify arguments")
        .clone()
    }

    fn verify_config() -> ZkStellarVerifyConfig {
        ZkStellarVerifyConfig {
            contract_id: "CTESTZKGUARDRAIL".to_string(),
            network: "testnet".to_string(),
            source: "demo-source".to_string(),
            stellar_cli: "stellar".to_string(),
            instruction_leeway: 10_000_000,
        }
    }

    fn accepted_contract_output(action_plan_hash: &str) -> String {
        json!({
            "action_plan_hash": action_plan_hash,
            "policy_commitment": "f208fb657dcf4a6b4f339e6402da536dd1f86a3e353282426d622c1bb5e21150",
            "policy_version": 7,
            "decision_status": 0,
            "exit_code": 0,
            "reason_code": 0,
            "requires_approval": false,
            "audit_nullifier": "c62e6a97e27f67c0370a45b52ff84f27796b9d7f55df02ad35aff2e90b7328da",
            "next_step": "EligibleForSeparateApprovalFlow"
        })
        .to_string()
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

    #[test]
    fn real_guardrail_adapter_approves_valid_plan_without_enforced_findings() {
        let plan = balance_plan();
        let arguments = evaluate_arguments(&plan);
        let response = evaluate_guardrails_with_config(
            &arguments,
            guardrail_config(Allowlist::default(), false, Vec::new(), false, false),
        )
        .expect("guardrail response");

        assert_eq!(response["runtime_source"], "neurochain_guardrails");
        assert_eq!(response["decision"], "approved");
        assert_eq!(response["exit_code"], 0);
        assert_eq!(response["reason_code"], "passed");
        assert_eq!(response["guardrails"]["allowlist"], "passed");
        assert_eq!(response["guardrails"]["contract_policy"], "passed");
        assert_eq!(response["guardrails"]["intent_safety"], "passed");
        assert_eq!(response["underlying_action_submit_allowed"], false);
        assert_eq!(response["attestation_submitted"], false);
        assert!(response["transaction_hash"].is_null());
    }

    #[test]
    fn real_guardrail_adapter_preserves_exit_precedence() {
        let plan = ActionPlan {
            schema_version: 1,
            actions: vec![Action::SorobanContractInvoke {
                contract_id: CONTRACT_ID.to_string(),
                function: "purchase_credits".to_string(),
                args: json!({"amount": 100}),
            }],
            warnings: vec!["intent_warning: low confidence".to_string()],
            source: Some("mcp-test".to_string()),
        };
        let arguments = evaluate_arguments(&plan);
        let response = evaluate_guardrails_with_config(
            &arguments,
            guardrail_config(Allowlist::default(), true, Vec::new(), true, false),
        )
        .expect("guardrail response");

        assert_eq!(response["decision"], "blocked");
        assert_eq!(response["exit_code"], 3);
        assert_eq!(response["reason_code"], "allowlist");
        assert_eq!(response["guardrails"]["allowlist"], "blocked");
        assert_eq!(response["guardrails"]["contract_policy"], "blocked");
        assert_eq!(response["guardrails"]["intent_safety"], "blocked");
    }

    #[test]
    fn real_guardrail_adapter_uses_exit_four_for_unconfigured_enforced_policy() {
        let plan = ActionPlan {
            schema_version: 1,
            actions: vec![Action::SorobanContractInvoke {
                contract_id: CONTRACT_ID.to_string(),
                function: "hello".to_string(),
                args: json!({"to": "world"}),
            }],
            warnings: Vec::new(),
            source: Some("mcp-test".to_string()),
        };
        let arguments = evaluate_arguments(&plan);
        let response = evaluate_guardrails_with_config(
            &arguments,
            guardrail_config(Allowlist::default(), false, Vec::new(), true, false),
        )
        .expect("guardrail response");

        assert_eq!(response["decision"], "blocked");
        assert_eq!(response["exit_code"], 4);
        assert_eq!(response["reason_code"], "contract_policy");
        assert_eq!(response["underlying_action_submit_allowed"], false);
    }

    #[test]
    fn real_guardrail_adapter_uses_configured_contract_policy_validator() {
        let plan = ActionPlan {
            schema_version: 1,
            actions: vec![Action::SorobanContractInvoke {
                contract_id: CONTRACT_ID.to_string(),
                function: "hello".to_string(),
                args: json!({"to": "world"}),
            }],
            warnings: Vec::new(),
            source: Some("mcp-test".to_string()),
        };
        let arguments = evaluate_arguments(&plan);
        let policy = |allowed_function: &str| {
            serde_json::from_value::<ContractPolicy>(json!({
                "contract_id": CONTRACT_ID,
                "allowed_functions": [allowed_function]
            }))
            .expect("contract policy")
        };

        let approved = evaluate_guardrails_with_config(
            &arguments,
            guardrail_config(
                Allowlist::default(),
                false,
                vec![policy("hello")],
                true,
                false,
            ),
        )
        .expect("approved policy response");
        assert_eq!(approved["decision"], "approved");
        assert_eq!(approved["exit_code"], 0);

        let warning_only_policy = serde_json::from_value::<ContractPolicy>(json!({
            "contract_id": CONTRACT_ID,
            "allowed_functions": ["hello"],
            "max_fee_stroops": 1000
        }))
        .expect("warning-only contract policy");
        let warning_only = evaluate_guardrails_with_config(
            &arguments,
            guardrail_config(
                Allowlist::default(),
                false,
                vec![warning_only_policy],
                true,
                false,
            ),
        )
        .expect("warning-only policy response");
        assert_eq!(warning_only["decision"], "approved");
        assert_eq!(warning_only["exit_code"], 0);
        assert_eq!(
            warning_only["guardrails"]["contract_policy"],
            "warning_only"
        );

        let blocked = evaluate_guardrails_with_config(
            &arguments,
            guardrail_config(
                Allowlist::default(),
                false,
                vec![policy("transfer")],
                true,
                false,
            ),
        )
        .expect("blocked policy response");
        assert_eq!(blocked["decision"], "blocked");
        assert_eq!(blocked["exit_code"], 4);
        assert_eq!(blocked["reason_code"], "contract_policy");
    }

    #[test]
    fn real_guardrail_adapter_uses_exit_five_for_unknown_or_empty_plan() {
        for plan in [
            ActionPlan {
                schema_version: 1,
                actions: vec![Action::Unknown {
                    reason: "intent_warning: low confidence".to_string(),
                }],
                warnings: Vec::new(),
                source: Some("mcp-test".to_string()),
            },
            ActionPlan::default(),
        ] {
            let arguments = evaluate_arguments(&plan);
            let response = evaluate_guardrails_with_config(
                &arguments,
                guardrail_config(Allowlist::default(), false, Vec::new(), false, false),
            )
            .expect("guardrail response");

            assert_eq!(response["decision"], "blocked");
            assert_eq!(response["exit_code"], 5);
            assert_eq!(response["reason_code"], "intent_safety");
        }
    }

    #[test]
    fn real_guardrail_adapter_keeps_requires_approval_terminal() {
        let plan = balance_plan();
        let arguments = evaluate_arguments(&plan);
        let response = evaluate_guardrails_with_config(
            &arguments,
            guardrail_config(Allowlist::default(), false, Vec::new(), false, true),
        )
        .expect("guardrail response");

        assert_eq!(response["decision"], "requires_approval");
        assert_eq!(response["exit_code"], 0);
        assert_eq!(response["reason_code"], "approval_threshold");
        assert_eq!(
            response["guardrails"]["contract_policy"],
            "requires_approval"
        );
        assert_eq!(response["next_recommended_tool"], "get_guardrail_status");
        assert_eq!(response["underlying_action_submit_allowed"], false);
    }

    #[test]
    fn real_guardrail_adapter_rejects_hash_mismatch_and_policy_overrides() {
        let plan = balance_plan();
        let mut arguments = evaluate_arguments(&plan);
        arguments.insert(
            "action_plan_hash".to_string(),
            Value::String("0".repeat(64)),
        );
        let error = evaluate_guardrails_with_config(
            &arguments,
            guardrail_config(Allowlist::default(), false, Vec::new(), false, false),
        )
        .expect_err("hash mismatch must fail closed");
        assert!(error.contains("does not match"));

        let mut arguments = evaluate_arguments(&plan);
        arguments.insert("allowlist_enforce".to_string(), Value::Bool(false));
        let error = evaluate_guardrails_with_config(
            &arguments,
            guardrail_config(Allowlist::default(), true, Vec::new(), false, false),
        )
        .expect_err("client policy override must fail closed");
        assert!(error.contains("does not accept argument allowlist_enforce"));
    }

    #[test]
    fn real_guardrail_adapter_bounds_action_plan_input() {
        let too_many_actions = ActionPlan {
            schema_version: 1,
            actions: (0..=MAX_ACTIONS_PER_PLAN)
                .map(|_| Action::Unknown {
                    reason: "bounded test".to_string(),
                })
                .collect(),
            warnings: Vec::new(),
            source: None,
        };
        let arguments = evaluate_arguments(&too_many_actions);
        let error = evaluate_guardrails_with_config(
            &arguments,
            guardrail_config(Allowlist::default(), false, Vec::new(), false, false),
        )
        .expect_err("action count must be bounded");
        assert!(error.contains("exceeds 64 actions"));

        let oversized_plan = ActionPlan {
            schema_version: 1,
            actions: vec![Action::Unknown {
                reason: "x".repeat(MAX_ACTION_PLAN_JSON_BYTES),
            }],
            warnings: Vec::new(),
            source: None,
        };
        let arguments = evaluate_arguments(&oversized_plan);
        let error = evaluate_guardrails_with_config(
            &arguments,
            guardrail_config(Allowlist::default(), false, Vec::new(), false, false),
        )
        .expect_err("serialized plan size must be bounded");
        assert!(error.contains("serialized bytes"));
    }

    #[test]
    fn real_proof_adapter_inspects_public_artifacts_without_submit() {
        for (proof, expected_decision, expected_exit, expected_reason) in [
            (
                include_str!("../hackathons/stellar-real-world-zk/fixtures/groth16_approved.json"),
                "approved",
                0,
                "passed",
            ),
            (
                include_str!(
                    "../hackathons/stellar-real-world-zk/fixtures/groth16_requires_approval.json"
                ),
                "requires_approval",
                0,
                "approval_threshold",
            ),
            (
                include_str!(
                    "../hackathons/stellar-real-world-zk/fixtures/groth16_blocked_exit_3.json"
                ),
                "blocked",
                3,
                "allowlist",
            ),
        ] {
            let response = prove_guardrail_decision_value(&proof_arguments(proof))
                .expect("public artifact inspection");

            assert_eq!(response["runtime_source"], "neurochain_zk_attestation_view");
            assert_eq!(response["decision"], expected_decision);
            assert_eq!(response["exit_code"], expected_exit);
            assert_eq!(response["reason_code"], expected_reason);
            assert_eq!(response["proof_binding"], "binding_validated");
            assert_eq!(response["cryptographically_verified"], false);
            assert_eq!(response["stellar_verification_required"], true);
            assert_eq!(response["stellar_verification"], "required_on_stellar");
            assert_eq!(response["underlying_action_submit_allowed"], false);
            assert_eq!(response["attestation_submitted"], false);
            assert_eq!(response["verification_transaction_submitted"], false);
            assert_eq!(response["nullifier_consumed"], false);
            assert!(response["transaction_hash"].is_null());
            assert!(response.get("seal_hex").is_none());
            assert!(response.get("journal_hex").is_none());
        }
    }

    #[test]
    fn real_proof_adapter_rejects_tampered_binding() {
        let mut arguments = proof_arguments(include_str!(
            "../hackathons/stellar-real-world-zk/fixtures/groth16_approved.json"
        ));
        arguments["action_plan"]["args"][0]["value"] = Value::String("500000001".to_string());

        let error = prove_guardrail_decision_value(&arguments)
            .expect_err("tampered ActionPlan binding must fail closed");
        assert!(error.contains("action_plan_hash_mismatch"));
    }

    #[test]
    fn real_proof_adapter_rejects_paths_and_bounds_inline_input() {
        let mut arguments = proof_arguments(include_str!(
            "../hackathons/stellar-real-world-zk/fixtures/groth16_approved.json"
        ));
        arguments.insert(
            "proof_path".to_string(),
            Value::String("private/proof.json".to_string()),
        );
        let error = prove_guardrail_decision_value(&arguments)
            .expect_err("client path argument must fail closed");
        assert!(error.contains("does not accept argument proof_path"));

        let mut arguments = proof_arguments(include_str!(
            "../hackathons/stellar-real-world-zk/fixtures/groth16_approved.json"
        ));
        arguments["proof"]["seal_hex"] = Value::String("aa".repeat(MAX_ZK_REQUEST_JSON_BYTES));
        let error = prove_guardrail_decision_value(&arguments)
            .expect_err("oversized public artifact must fail closed");
        assert!(error.contains("serialized bytes"));
    }

    #[test]
    fn real_stellar_verify_adapter_uses_read_only_runner_and_preserves_no_submit() {
        let arguments = verify_arguments(include_str!(
            "../hackathons/stellar-real-world-zk/fixtures/groth16_approved.json"
        ));
        let response =
            verify_zk_on_stellar_with_runner(&arguments, verify_config(), |config, proof| {
                assert_eq!(config.contract_id, "CTESTZKGUARDRAIL");
                assert_eq!(config.network, "testnet");
                assert_eq!(config.source, "demo-source");
                assert_eq!(config.instruction_leeway, 10_000_000);
                assert!(proof.seal_hex.len() > 4);
                Ok(accepted_contract_output(
                    "a008efa4f3ecbdf88b9bcc3ed4c7672994136f16074e8fddd6bb8192ea7970cd",
                ))
            })
            .expect("read-only Stellar verification response");

        assert_eq!(
            response["runtime_source"],
            "neurochain_soroban_read_only_verifier"
        );
        assert_eq!(response["stellar_verification"], "verified_on_stellar");
        assert_eq!(response["verification_mode"], "read_only");
        assert_eq!(response["cryptographically_verified"], true);
        assert_eq!(response["stellar_verification_required"], false);
        assert_eq!(response["underlying_action_submit_allowed"], false);
        assert_eq!(response["attestation_submitted"], false);
        assert_eq!(response["verification_transaction_submitted"], false);
        assert_eq!(response["nullifier_consumed"], false);
        assert!(response["transaction_hash"].is_null());
    }

    #[test]
    fn real_stellar_verify_adapter_rejects_mismatched_contract_result() {
        let arguments = verify_arguments(include_str!(
            "../hackathons/stellar-real-world-zk/fixtures/groth16_approved.json"
        ));
        let error =
            verify_zk_on_stellar_with_runner(&arguments, verify_config(), |_config, _proof| {
                Ok(accepted_contract_output(
                    "0000000000000000000000000000000000000000000000000000000000000000",
                ))
            })
            .expect_err("Stellar result mismatch must fail closed");

        assert!(error
            .contains("Stellar ZK result does not match the locally bound ActionPlan and journal"));
    }

    #[test]
    fn real_stellar_verify_adapter_rejects_paths_and_non_read_only_mode() {
        let mut arguments = verify_arguments(include_str!(
            "../hackathons/stellar-real-world-zk/fixtures/groth16_approved.json"
        ));
        arguments.insert(
            "proof_artifact_ref".to_string(),
            Value::String("inline:abc".to_string()),
        );
        let error =
            verify_zk_on_stellar_with_runner(&arguments, verify_config(), |_config, _proof| {
                unreachable!("path argument must fail before runner")
            })
            .expect_err("artifact refs must fail closed in runtime calls");
        assert!(error.contains("does not accept argument proof_artifact_ref"));

        let mut arguments = verify_arguments(include_str!(
            "../hackathons/stellar-real-world-zk/fixtures/groth16_approved.json"
        ));
        arguments.insert(
            "verification_mode".to_string(),
            Value::String("submit".to_string()),
        );
        let error =
            verify_zk_on_stellar_with_runner(&arguments, verify_config(), |_config, _proof| {
                unreachable!("non-read-only mode must fail before runner")
            })
            .expect_err("non-read-only mode must fail closed");
        assert!(error.contains("only verification_mode=read_only"));
    }

    #[test]
    fn real_status_adapter_reports_latest_result_without_submit() {
        let latest_result = verify_zk_on_stellar_with_runner(
            &verify_arguments(include_str!(
                "../hackathons/stellar-real-world-zk/fixtures/groth16_approved.json"
            )),
            verify_config(),
            |_config, _proof| {
                Ok(accepted_contract_output(
                    "a008efa4f3ecbdf88b9bcc3ed4c7672994136f16074e8fddd6bb8192ea7970cd",
                ))
            },
        )
        .expect("read-only verified result");
        let arguments = json!({
            "latest_result": latest_result,
            "session_id": "session-1"
        });
        let response =
            get_guardrail_status_value(arguments.as_object().expect("status arguments object"))
                .expect("status response");

        assert_eq!(response["runtime_source"], "neurochain_mcp_status_view");
        assert_eq!(response["status_source"], "latest_result");
        assert_eq!(response["last_tool"], "verify_zk_on_stellar");
        assert_eq!(response["decision"], "approved");
        assert_eq!(response["stellar_verification"], "verified_on_stellar");
        assert_eq!(response["local_binding"], "binding_validated");
        assert_eq!(response["cryptographically_verified"], true);
        assert_eq!(response["underlying_action_submit_allowed"], false);
        assert_eq!(response["attestation_submitted"], false);
        assert_eq!(response["verification_transaction_submitted"], false);
        assert_eq!(response["nullifier_consumed"], false);
        assert!(response["transaction_hash"].is_null());
    }

    #[test]
    fn real_status_adapter_reports_state_unavailable_without_latest_result() {
        let arguments = json!({"session_id": "session-1"});
        let response =
            get_guardrail_status_value(arguments.as_object().expect("status arguments object"))
                .expect("state unavailable status response");

        assert_eq!(response["status"], "state_unavailable");
        assert_eq!(response["decision"], "not_evaluated");
        assert_eq!(response["stellar_verification"], "not_requested");
        assert_eq!(response["status_source"], "no_latest_result");
        assert_eq!(response["underlying_action_submit_allowed"], false);
        assert_eq!(response["attestation_submitted"], false);
        assert_eq!(response["verification_transaction_submitted"], false);
        assert_eq!(response["nullifier_consumed"], false);
        assert!(response["transaction_hash"].is_null());
    }

    #[test]
    fn real_status_adapter_rejects_submit_authority_in_latest_result() {
        let mut latest_result = verify_zk_on_stellar_with_runner(
            &verify_arguments(include_str!(
                "../hackathons/stellar-real-world-zk/fixtures/groth16_approved.json"
            )),
            verify_config(),
            |_config, _proof| {
                Ok(accepted_contract_output(
                    "a008efa4f3ecbdf88b9bcc3ed4c7672994136f16074e8fddd6bb8192ea7970cd",
                ))
            },
        )
        .expect("read-only verified result");
        latest_result["underlying_action_submit_allowed"] = Value::Bool(true);
        let arguments = json!({"latest_result": latest_result});
        let error =
            get_guardrail_status_value(arguments.as_object().expect("status arguments object"))
                .expect_err("status must reject submit authority");

        assert!(error.contains("underlying_action_submit_allowed"));
    }
}
