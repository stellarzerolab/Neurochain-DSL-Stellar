use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    x402_bazaar::{BazaarCatalog, BazaarCatalogKey, BazaarResourceType},
    x402_bazaar_mcp::{execute_bazaar_mcp_search, BazaarMcpSearchResult},
    x402_bazaar_paid_call::{
        execute_bazaar_mcp_paid_call, BazaarMcpPaidCallArguments, BazaarMcpPaidCallResult,
        BazaarPaidCallAccessGate,
    },
    x402_service_boundary::{
        X402BoundaryDecision, X402BoundaryNetwork, X402ServiceEvaluationRequest,
        X402ServiceEvaluationResponse,
    },
};

pub const X402_LOCAL_REFERENCE_PATH_SCHEMA_VERSION: u32 = 1;
const MAX_REFERENCE_REQUEST_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct X402LocalReferencePathRequest {
    pub schema_version: u32,
    pub discovery_arguments: Value,
    pub evaluation_request: X402ServiceEvaluationRequest,
    pub paid_call_arguments: Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum X402LocalAccessState {
    SettledAccessReady,
    PaymentRequired,
    SettlementPending,
    SettlementRejected,
    SettlementOutcomeUnknown,
    Unavailable,
}

impl X402LocalAccessState {
    fn code(self) -> &'static str {
        match self {
            Self::SettledAccessReady => "settled_access_ready",
            Self::PaymentRequired => "payment_required",
            Self::SettlementPending => "settlement_pending",
            Self::SettlementRejected => "settlement_rejected",
            Self::SettlementOutcomeUnknown => "settlement_outcome_unknown",
            Self::Unavailable => "access_state_unavailable",
        }
    }
}

/// Trusted, read-only view of x402 access state. This stage cannot consume an
/// access grant; the separate paid-call gate remains the only consumer.
pub trait X402LocalAccessStatePort {
    fn inspect_access(&self, resource_key: &BazaarCatalogKey) -> X402LocalAccessState;
}

/// Trusted NeuroChain planning and deterministic-policy boundary. The
/// untrusted reference request cannot provide its own ActionPlan or decision.
pub trait X402LocalEvaluationPort {
    fn plan_and_evaluate(
        &mut self,
        request: &X402ServiceEvaluationRequest,
    ) -> Result<X402ServiceEvaluationResponse, String>;
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum X402LocalReferenceOutcome {
    CapabilityReady,
    PolicyBlocked,
    ApprovalRequired,
    CapabilityDenied,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct X402LocalCapabilityGateResult {
    pub code: String,
    pub service_call_allowed: bool,
    pub access_consumed: bool,
    pub service_dispatch_allowed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paid_call_result: Option<BazaarMcpPaidCallResult>,
}

impl X402LocalCapabilityGateResult {
    fn policy_denied(code: &str) -> Self {
        Self {
            code: code.to_string(),
            service_call_allowed: false,
            access_consumed: false,
            service_dispatch_allowed: false,
            paid_call_result: None,
        }
    }

    fn from_paid_call(result: BazaarMcpPaidCallResult) -> Self {
        let allowed = result.ok;
        let code = result.code.clone();
        Self {
            code,
            service_call_allowed: allowed,
            access_consumed: allowed,
            service_dispatch_allowed: false,
            paid_call_result: Some(result),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct X402LocalReferenceAuthority {
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
    action_plan_submit_allowed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct X402LocalReferencePathResult {
    pub schema_version: u32,
    pub outcome: X402LocalReferenceOutcome,
    pub discovery: BazaarMcpSearchResult,
    pub access_state: X402LocalAccessState,
    pub evaluation: X402ServiceEvaluationResponse,
    pub capability_gate: X402LocalCapabilityGateResult,
    pub authority: X402LocalReferenceAuthority,
}

pub(crate) struct PreparedX402LocalReferencePath {
    pub discovery: BazaarMcpSearchResult,
    pub access_state: X402LocalAccessState,
    pub evaluation: X402ServiceEvaluationResponse,
    paid_call_arguments: Value,
}

pub fn run_x402_local_reference_path(
    catalog: &BazaarCatalog,
    access_state_port: &dyn X402LocalAccessStatePort,
    evaluation_port: &mut dyn X402LocalEvaluationPort,
    capability_gate: Option<&mut dyn BazaarPaidCallAccessGate>,
    request: X402LocalReferencePathRequest,
) -> Result<X402LocalReferencePathResult, String> {
    let prepared =
        prepare_x402_local_reference_path(catalog, access_state_port, evaluation_port, request)?;
    complete_x402_local_reference_path(catalog, capability_gate, prepared)
}

pub(crate) fn prepare_x402_local_reference_path(
    catalog: &BazaarCatalog,
    access_state_port: &dyn X402LocalAccessStatePort,
    evaluation_port: &mut dyn X402LocalEvaluationPort,
    request: X402LocalReferencePathRequest,
) -> Result<PreparedX402LocalReferencePath, String> {
    validate_reference_request(&request)?;

    let discovery = execute_bazaar_mcp_search(Some(catalog), request.discovery_arguments.clone());
    if !discovery.ok {
        return Err(format!(
            "Bazaar discovery failed closed: {}",
            discovery.code
        ));
    }

    let paid_call: BazaarMcpPaidCallArguments =
        serde_json::from_value(request.paid_call_arguments.clone()).map_err(|_| {
            "reference path paid-call arguments do not match the strict contract".to_string()
        })?;
    let resource_key = BazaarCatalogKey(paid_call.resource_key.clone());
    let resource = catalog
        .get(&resource_key)
        .ok_or_else(|| "reference path resourceKey is absent from the local catalog".to_string())?;

    let preflight =
        execute_bazaar_mcp_paid_call(Some(catalog), None, request.paid_call_arguments.clone());
    if preflight.code != "access_gate_unavailable" {
        return Err(format!(
            "paid-call preflight failed closed: {}",
            preflight.code
        ));
    }

    bind_discovery_and_evaluation(&request, &paid_call, resource, &discovery)?;

    let access_state = access_state_port.inspect_access(&resource_key);
    if access_state != X402LocalAccessState::SettledAccessReady {
        return Err(format!(
            "x402 access state failed closed: {}",
            access_state.code()
        ));
    }

    let evaluation = evaluation_port
        .plan_and_evaluate(&request.evaluation_request)
        .map_err(|error| format!("NeuroChain evaluation failed closed: {error}"))?;
    evaluation
        .validate()
        .map_err(|error| format!("NeuroChain evaluation response failed closed: {error}"))?;
    if evaluation.request_id != request.evaluation_request.request_id {
        return Err("evaluation response request_id does not match the request".to_string());
    }

    Ok(PreparedX402LocalReferencePath {
        discovery,
        access_state,
        evaluation,
        paid_call_arguments: request.paid_call_arguments,
    })
}

pub(crate) fn complete_x402_local_reference_path(
    catalog: &BazaarCatalog,
    capability_gate: Option<&mut dyn BazaarPaidCallAccessGate>,
    prepared: PreparedX402LocalReferencePath,
) -> Result<X402LocalReferencePathResult, String> {
    let PreparedX402LocalReferencePath {
        discovery,
        access_state,
        evaluation,
        paid_call_arguments,
    } = prepared;

    let (outcome, capability_gate) = match evaluation.decision {
        X402BoundaryDecision::Approved => {
            let result =
                execute_bazaar_mcp_paid_call(Some(catalog), capability_gate, paid_call_arguments);
            let outcome = if result.ok {
                X402LocalReferenceOutcome::CapabilityReady
            } else {
                X402LocalReferenceOutcome::CapabilityDenied
            };
            (
                outcome,
                X402LocalCapabilityGateResult::from_paid_call(result),
            )
        }
        X402BoundaryDecision::RequiresApproval => (
            X402LocalReferenceOutcome::ApprovalRequired,
            X402LocalCapabilityGateResult::policy_denied("approval_required"),
        ),
        X402BoundaryDecision::Blocked => (
            X402LocalReferenceOutcome::PolicyBlocked,
            X402LocalCapabilityGateResult::policy_denied("policy_blocked"),
        ),
    };

    Ok(X402LocalReferencePathResult {
        schema_version: X402_LOCAL_REFERENCE_PATH_SCHEMA_VERSION,
        outcome,
        discovery,
        access_state,
        evaluation,
        capability_gate,
        authority: X402LocalReferenceAuthority::default(),
    })
}

fn validate_reference_request(request: &X402LocalReferencePathRequest) -> Result<(), String> {
    if request.schema_version != X402_LOCAL_REFERENCE_PATH_SCHEMA_VERSION {
        return Err(format!(
            "reference path schema_version must be {X402_LOCAL_REFERENCE_PATH_SCHEMA_VERSION}"
        ));
    }
    let encoded_size = serde_json::to_vec(request)
        .map_err(|error| format!("reference path request is not serializable: {error}"))?
        .len();
    if encoded_size > MAX_REFERENCE_REQUEST_BYTES {
        return Err(format!(
            "reference path request exceeds {MAX_REFERENCE_REQUEST_BYTES} serialized bytes"
        ));
    }
    request
        .evaluation_request
        .validate()
        .map_err(|error| format!("evaluation request failed closed: {error}"))
}

fn bind_discovery_and_evaluation(
    request: &X402LocalReferencePathRequest,
    paid_call: &BazaarMcpPaidCallArguments,
    resource: &crate::x402_bazaar::BazaarCatalogResource,
    discovery: &BazaarMcpSearchResult,
) -> Result<(), String> {
    if request.evaluation_request.request_id != paid_call.request_id {
        return Err("evaluation and paid-call request identifiers do not match".to_string());
    }
    if request.evaluation_request.resource_id != paid_call.resource_key {
        return Err("evaluation resource_id is not the exact paid-call resourceKey".to_string());
    }
    if resource.resource_type != BazaarResourceType::Mcp {
        return Err("reference path resource is not an MCP service".to_string());
    }

    let expected_network = match request.evaluation_request.network {
        X402BoundaryNetwork::Testnet => "stellar:testnet",
        X402BoundaryNetwork::Pubnet => "stellar:pubnet",
    };
    if resource.payment.network != expected_network {
        return Err("evaluation network does not match the catalog payment network".to_string());
    }

    let service_arguments = paid_call
        .arguments
        .as_object()
        .ok_or_else(|| "paid-call service arguments must be an object".to_string())?;
    if service_arguments.len() != 2
        || service_arguments.get("intent_text").and_then(Value::as_str)
            != Some(request.evaluation_request.intent_text.as_str())
        || service_arguments.get("network").and_then(Value::as_str) != Some(expected_network)
    {
        return Err(
            "paid-call service arguments are not exactly bound to intent_text and network"
                .to_string(),
        );
    }

    let discovered = discovery
        .data
        .as_ref()
        .map(|data| {
            data.resources.iter().any(|item| {
                item.resource == resource.resource_url
                    && item.resource_type == BazaarResourceType::Mcp
                    && item.accepts == [resource.payment.clone()]
            })
        })
        .unwrap_or(false);
    if !discovered {
        return Err("Bazaar discovery did not return the exact paid resource".to_string());
    }
    Ok(())
}
