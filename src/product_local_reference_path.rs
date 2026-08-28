use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    actions::Action,
    x402_bazaar::BazaarCatalog,
    x402_bazaar_paid_call::BazaarPaidCallAccessGate,
    x402_local_reference_path::{
        complete_x402_local_reference_path, prepare_x402_local_reference_path,
        X402LocalAccessStatePort, X402LocalEvaluationPort, X402LocalReferencePathRequest,
        X402LocalReferencePathResult,
    },
    x402_service_boundary::{X402BoundaryDecision, X402ServiceEvaluationResponse},
    zk_attestation::{
        inspect_zk_attestation, ZkAttestationViewRequest, ZkTypedActionPlan, ZkTypedArg,
    },
};

pub const PRODUCT_LOCAL_REFERENCE_PATH_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProductLocalAuthority {
    payment_allowed: bool,
    proof_allowed: bool,
    approval_allowed: bool,
    settlement_allowed: bool,
    signing_allowed: bool,
    underlying_execution_allowed: bool,
    service_dispatch_allowed: bool,
    wallet_access_allowed: bool,
    shell_access_allowed: bool,
    rpc_submit_allowed: bool,
    transaction_submit_allowed: bool,
    action_plan_submit_allowed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductLocalZkEvidence {
    pub source: String,
    pub artifact_present: bool,
    pub proof_kind: String,
    pub action_plan_projection_validated: bool,
    pub local_binding: String,
    pub cryptographically_verified: bool,
    pub stellar_verification_required: bool,
    pub decision: X402BoundaryDecision,
    pub evaluation_exit_code: Option<i32>,
    pub proof_exit_code: u8,
    pub evaluation_reason_code: String,
    pub proof_reason_code: String,
    pub proof_action_plan_hash: String,
    pub policy_commitment: String,
    pub policy_version: u32,
    pub private_policy_revealed: bool,
    pub authority: ProductLocalAuthority,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductLocalReferencePathResult {
    pub schema_version: u32,
    pub reference_path: X402LocalReferencePathResult,
    pub zk_evidence: ProductLocalZkEvidence,
    pub authority: ProductLocalAuthority,
}

/// Runs the existing x402/Bazaar reference coordinator, inserts bounded public
/// ZK evidence inspection after deterministic policy, and only then evaluates
/// the existing exact capability gate. It does not generate a proof, perform
/// cryptographic Stellar verification, dispatch a service, sign, or submit.
pub fn run_product_local_reference_path(
    catalog: &BazaarCatalog,
    access_state_port: &dyn X402LocalAccessStatePort,
    evaluation_port: &mut dyn X402LocalEvaluationPort,
    capability_gate: Option<&mut dyn BazaarPaidCallAccessGate>,
    request: X402LocalReferencePathRequest,
    zk_request: ZkAttestationViewRequest,
) -> Result<ProductLocalReferencePathResult, String> {
    let prepared =
        prepare_x402_local_reference_path(catalog, access_state_port, evaluation_port, request)?;
    let zk_evidence = inspect_product_zk_evidence(&prepared.evaluation, zk_request)?;
    let reference_path = complete_x402_local_reference_path(catalog, capability_gate, prepared)?;

    Ok(ProductLocalReferencePathResult {
        schema_version: PRODUCT_LOCAL_REFERENCE_PATH_SCHEMA_VERSION,
        reference_path,
        zk_evidence,
        authority: ProductLocalAuthority::default(),
    })
}

fn inspect_product_zk_evidence(
    evaluation: &X402ServiceEvaluationResponse,
    request: ZkAttestationViewRequest,
) -> Result<ProductLocalZkEvidence, String> {
    validate_action_plan_projection(evaluation, &request.action_plan)?;

    let inspected = inspect_zk_attestation(request)
        .map_err(|error| format!("product ZK evidence failed closed: {}", error.code()))?;
    if !inspected.ok || inspected.execution.submit_allowed {
        return Err("product ZK evidence returned an invalid authority boundary".to_string());
    }
    let attestation = inspected
        .zk_attestation
        .ok_or_else(|| "product ZK evidence returned no attestation".to_string())?;
    if attestation.verification_state != "binding_validated"
        || attestation.cryptographically_verified
        || !attestation.stellar_verification_required
        || attestation.private_policy_revealed
    {
        return Err(
            "product ZK evidence must be local binding-only with private policy hidden".to_string(),
        );
    }

    let proof_decision = match attestation.attested_decision.status.as_str() {
        "approved" => X402BoundaryDecision::Approved,
        "requires_approval" => X402BoundaryDecision::RequiresApproval,
        "blocked" => X402BoundaryDecision::Blocked,
        _ => return Err("product ZK evidence returned an unknown decision".to_string()),
    };
    if proof_decision != evaluation.decision {
        return Err("product ZK decision does not match deterministic policy".to_string());
    }

    validate_decision_parity(
        evaluation,
        attestation.attested_decision.exit_code,
        &attestation.attested_decision.reason,
        attestation.attested_decision.requires_approval,
    )?;

    Ok(ProductLocalZkEvidence {
        source: "neurochain_zk_attestation_view".to_string(),
        artifact_present: true,
        proof_kind: attestation.proof_kind,
        action_plan_projection_validated: true,
        local_binding: attestation.verification_state,
        cryptographically_verified: false,
        stellar_verification_required: true,
        decision: proof_decision,
        evaluation_exit_code: evaluation.exit_code,
        proof_exit_code: attestation.attested_decision.exit_code,
        evaluation_reason_code: evaluation.reason_code.clone(),
        proof_reason_code: attestation.attested_decision.reason,
        proof_action_plan_hash: attestation.action_plan_hash,
        policy_commitment: attestation.policy_commitment,
        policy_version: attestation.policy_version,
        private_policy_revealed: false,
        authority: ProductLocalAuthority::default(),
    })
}

fn validate_decision_parity(
    evaluation: &X402ServiceEvaluationResponse,
    proof_exit_code: u8,
    proof_reason: &str,
    proof_requires_approval: bool,
) -> Result<(), String> {
    let matches = match evaluation.decision {
        X402BoundaryDecision::Approved => {
            evaluation.exit_code.is_none()
                && evaluation.reason_code == "approved"
                && proof_exit_code == 0
                && proof_reason == "passed"
                && !proof_requires_approval
        }
        X402BoundaryDecision::RequiresApproval => {
            evaluation.exit_code.is_none()
                && evaluation.reason_code == "approval_required"
                && proof_exit_code == 0
                && proof_reason == "approval_threshold"
                && proof_requires_approval
        }
        X402BoundaryDecision::Blocked => {
            evaluation.exit_code == Some(i32::from(proof_exit_code))
                && evaluation.reason_code == proof_reason
                && matches!(proof_exit_code, 3..=5)
                && !proof_requires_approval
        }
    };
    if matches {
        Ok(())
    } else {
        Err("product ZK exit/reason state does not match deterministic policy".to_string())
    }
}

fn validate_action_plan_projection(
    evaluation: &X402ServiceEvaluationResponse,
    zk_plan: &ZkTypedActionPlan,
) -> Result<(), String> {
    if evaluation.action_plan.schema_version != 1
        || !evaluation.action_plan.warnings.is_empty()
        || evaluation.action_plan.source.is_some()
        || evaluation.action_plan.actions.len() != 1
    {
        return Err(
            "product ZK projection requires one canonical source-free ActionPlan action"
                .to_string(),
        );
    }
    if zk_plan.schema_version != 1
        || zk_plan.intent_label != "ContractInvoke"
        || zk_plan.action_kind != "soroban_contract_invoke"
        || zk_plan.intent_confidence_bps > 10_000
    {
        return Err("product ZK typed ActionPlan metadata is invalid".to_string());
    }

    let Action::SorobanContractInvoke {
        contract_id,
        function,
        args,
    } = &evaluation.action_plan.actions[0]
    else {
        return Err("product ZK projection supports only soroban_contract_invoke".to_string());
    };
    if contract_id != &zk_plan.contract_id || function != &zk_plan.function {
        return Err("product ZK contract/function projection mismatch".to_string());
    }

    let runtime_args = args
        .as_object()
        .ok_or_else(|| "product ActionPlan args must be an object".to_string())?;
    let mut proof_args = BTreeMap::new();
    for arg in &zk_plan.args {
        validate_zk_arg(arg)?;
        if proof_args
            .insert(arg.name.as_str(), arg.value.as_str())
            .is_some()
        {
            return Err("product ZK typed ActionPlan contains duplicate arg names".to_string());
        }
    }
    if runtime_args.len() != proof_args.len()
        || runtime_args.iter().any(|(name, value)| {
            value.as_str().and_then(|actual| {
                proof_args
                    .get(name.as_str())
                    .map(|expected| actual == *expected)
            }) != Some(true)
        })
    {
        return Err("product ZK typed args do not exactly project the ActionPlan".to_string());
    }
    Ok(())
}

fn validate_zk_arg(arg: &ZkTypedArg) -> Result<(), String> {
    if arg.name.trim().is_empty() || arg.value.trim().is_empty() {
        return Err("product ZK typed args must be non-empty".to_string());
    }
    match arg.value_type.as_str() {
        "address" | "symbol" => Ok(()),
        "bytes" => hex::decode(&arg.value)
            .map(|_| ())
            .map_err(|_| "product ZK bytes arg is not valid hex".to_string()),
        "u64" => arg
            .value
            .parse::<u64>()
            .map(|_| ())
            .map_err(|_| "product ZK u64 arg is invalid".to_string()),
        _ => Err("product ZK arg type is unsupported".to_string()),
    }
}
