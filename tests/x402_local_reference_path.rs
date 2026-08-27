use std::{cell::RefCell, collections::VecDeque, fs, path::Path};

use neurochain::{
    x402_bazaar::{BazaarCatalog, BazaarCatalogCandidate, BazaarCatalogKey},
    x402_bazaar_paid_call::{
        BazaarPaidCallAccessDecision, BazaarPaidCallAccessGate, BazaarPaidCallBinding,
    },
    x402_local_reference_path::{
        run_x402_local_reference_path, X402LocalAccessState, X402LocalAccessStatePort,
        X402LocalEvaluationPort, X402LocalReferenceOutcome, X402LocalReferencePathRequest,
        X402_LOCAL_REFERENCE_PATH_SCHEMA_VERSION,
    },
    x402_service_boundary::{X402ServiceEvaluationRequest, X402ServiceEvaluationResponse},
};
use serde_json::Value;

const FIXTURE_DIR: &str = "examples/x402_local_reference_path";
const CATALOG_FIXTURE: &str = "examples/x402_bazaar_catalog/mcp_tool.json";

fn read_value(path: impl AsRef<Path>) -> Value {
    let path = path.as_ref();
    let raw =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn read_fixture(name: &str) -> Value {
    read_value(Path::new(FIXTURE_DIR).join(name))
}

fn request(name: &str) -> X402LocalReferencePathRequest {
    serde_json::from_value(read_fixture(name))
        .unwrap_or_else(|error| panic!("parse reference request {name}: {error}"))
}

fn response(name: &str) -> X402ServiceEvaluationResponse {
    serde_json::from_value(read_fixture(name))
        .unwrap_or_else(|error| panic!("parse evaluation response {name}: {error}"))
}

fn catalog() -> BazaarCatalog {
    let candidate: BazaarCatalogCandidate =
        serde_json::from_value(read_value(CATALOG_FIXTURE)).expect("parse catalog candidate");
    let mut catalog = BazaarCatalog::default();
    catalog
        .insert(candidate, 1_723_000_001)
        .expect("insert MCP resource");
    catalog
}

#[derive(Debug)]
struct RecordingAccessState {
    state: X402LocalAccessState,
    keys: RefCell<Vec<BazaarCatalogKey>>,
}

impl RecordingAccessState {
    fn new(state: X402LocalAccessState) -> Self {
        Self {
            state,
            keys: RefCell::new(Vec::new()),
        }
    }
}

impl X402LocalAccessStatePort for RecordingAccessState {
    fn inspect_access(&self, resource_key: &BazaarCatalogKey) -> X402LocalAccessState {
        self.keys.borrow_mut().push(resource_key.clone());
        self.state
    }
}

#[derive(Debug)]
struct RecordingEvaluation {
    responses: VecDeque<Result<X402ServiceEvaluationResponse, String>>,
    requests: Vec<X402ServiceEvaluationRequest>,
}

impl RecordingEvaluation {
    fn returning(response: X402ServiceEvaluationResponse) -> Self {
        Self {
            responses: [Ok(response)].into(),
            requests: Vec::new(),
        }
    }
}

impl X402LocalEvaluationPort for RecordingEvaluation {
    fn plan_and_evaluate(
        &mut self,
        request: &X402ServiceEvaluationRequest,
    ) -> Result<X402ServiceEvaluationResponse, String> {
        self.requests.push(request.clone());
        self.responses
            .pop_front()
            .unwrap_or_else(|| Err("fixture evaluation exhausted".to_string()))
    }
}

#[derive(Debug)]
struct RecordingCapabilityGate {
    decisions: VecDeque<BazaarPaidCallAccessDecision>,
    bindings: Vec<BazaarPaidCallBinding>,
}

impl RecordingCapabilityGate {
    fn with(decisions: impl IntoIterator<Item = BazaarPaidCallAccessDecision>) -> Self {
        Self {
            decisions: decisions.into_iter().collect(),
            bindings: Vec::new(),
        }
    }
}

impl BazaarPaidCallAccessGate for RecordingCapabilityGate {
    fn consume_settled_access(
        &mut self,
        binding: &BazaarPaidCallBinding,
    ) -> BazaarPaidCallAccessDecision {
        self.bindings.push(binding.clone());
        self.decisions
            .pop_front()
            .unwrap_or(BazaarPaidCallAccessDecision::ReplayBlocked)
    }
}

fn assert_all_false(value: &Value, expected_fields: usize) {
    let object = value.as_object().expect("authority object");
    assert_eq!(object.len(), expected_fields);
    assert!(object.values().all(|value| value == &Value::Bool(false)));
}

#[test]
fn approved_path_reaches_only_the_exact_capability_without_dispatch_or_submit() {
    let catalog = catalog();
    let access = RecordingAccessState::new(X402LocalAccessState::SettledAccessReady);
    let mut evaluation =
        RecordingEvaluation::returning(response("approved_evaluation_response.json"));
    let mut capability = RecordingCapabilityGate::with([BazaarPaidCallAccessDecision::Authorized]);

    let result = run_x402_local_reference_path(
        &catalog,
        &access,
        &mut evaluation,
        Some(&mut capability),
        request("approved_request.json"),
    )
    .expect("approved reference path");

    assert_eq!(
        result.schema_version,
        X402_LOCAL_REFERENCE_PATH_SCHEMA_VERSION
    );
    assert_eq!(result.outcome, X402LocalReferenceOutcome::CapabilityReady);
    assert!(result.discovery.ok);
    assert_eq!(
        result.access_state,
        X402LocalAccessState::SettledAccessReady
    );
    assert_eq!(
        result.evaluation.decision,
        neurochain::x402_service_boundary::X402BoundaryDecision::Approved
    );
    assert!(!result.evaluation.underlying_action_submit_allowed);
    assert!(result.capability_gate.service_call_allowed);
    assert!(result.capability_gate.access_consumed);
    assert!(!result.capability_gate.service_dispatch_allowed);
    assert_eq!(result.capability_gate.code, "service_call_authorized");

    let authority = serde_json::to_value(result.authority).expect("serialize authority");
    assert_all_false(&authority, 11);
    let paid_call = result
        .capability_gate
        .paid_call_result
        .expect("paid-call capability result");
    let paid_authority = serde_json::to_value(paid_call.authority).expect("paid authority");
    assert_eq!(paid_authority["serviceCallAllowed"], true);
    for forbidden in [
        "paymentAllowed",
        "proofAllowed",
        "approvalAllowed",
        "settlementAllowed",
        "signingAllowed",
        "underlyingExecutionAllowed",
        "walletAccessAllowed",
        "shellAccessAllowed",
        "rpcSubmitAllowed",
        "actionPlanSubmitAllowed",
    ] {
        assert_eq!(
            paid_authority[forbidden], false,
            "authority leak: {forbidden}"
        );
    }

    assert_eq!(evaluation.requests.len(), 1);
    assert_eq!(access.keys.borrow().len(), 1);
    assert_eq!(capability.bindings.len(), 1);
    assert_eq!(
        capability.bindings[0].resource_key.0,
        "mcp:https://api.example.com/mcp#plan_stellar_action"
    );
    assert_eq!(capability.bindings[0].request_id, "reference-approved-001");
}

#[test]
fn blocked_policy_never_reaches_or_consumes_the_capability_gate() {
    let catalog = catalog();
    let access = RecordingAccessState::new(X402LocalAccessState::SettledAccessReady);
    let mut evaluation =
        RecordingEvaluation::returning(response("blocked_evaluation_response.json"));
    let mut capability = RecordingCapabilityGate::with([BazaarPaidCallAccessDecision::Authorized]);

    let result = run_x402_local_reference_path(
        &catalog,
        &access,
        &mut evaluation,
        Some(&mut capability),
        request("blocked_request.json"),
    )
    .expect("blocked reference path is a valid terminal outcome");

    assert_eq!(result.outcome, X402LocalReferenceOutcome::PolicyBlocked);
    assert_eq!(result.evaluation.exit_code, Some(4));
    assert_eq!(result.evaluation.reason_code, "contract_policy");
    assert!(!result.capability_gate.service_call_allowed);
    assert!(!result.capability_gate.access_consumed);
    assert!(!result.capability_gate.service_dispatch_allowed);
    assert_eq!(result.capability_gate.code, "policy_blocked");
    assert!(result.capability_gate.paid_call_result.is_none());
    assert!(capability.bindings.is_empty());
    assert_eq!(evaluation.requests.len(), 1);
    assert_eq!(access.keys.borrow().len(), 1);
    assert_all_false(
        &serde_json::to_value(result.authority).expect("serialize authority"),
        11,
    );
}

#[test]
fn binding_or_evaluation_tampering_fails_closed_before_capability_consumption() {
    let catalog = catalog();
    let access = RecordingAccessState::new(X402LocalAccessState::SettledAccessReady);
    let mut changed_intent = request("approved_request.json");
    changed_intent.paid_call_arguments["arguments"]["intent_text"] =
        Value::String("different intent".to_string());
    let mut evaluation =
        RecordingEvaluation::returning(response("approved_evaluation_response.json"));
    let mut capability = RecordingCapabilityGate::with([BazaarPaidCallAccessDecision::Authorized]);

    let error = run_x402_local_reference_path(
        &catalog,
        &access,
        &mut evaluation,
        Some(&mut capability),
        changed_intent,
    )
    .expect_err("changed paid-call intent must fail closed");
    assert!(error.contains("exactly bound"));
    assert!(evaluation.requests.is_empty());
    assert!(capability.bindings.is_empty());
    assert!(access.keys.borrow().is_empty());

    let access = RecordingAccessState::new(X402LocalAccessState::SettledAccessReady);
    let mut escalated = read_fixture("approved_evaluation_response.json");
    escalated["authority_grants"]["stellar_submission"] = Value::Bool(true);
    let escalated: X402ServiceEvaluationResponse =
        serde_json::from_value(escalated).expect("deserialize escalated response");
    let mut evaluation = RecordingEvaluation::returning(escalated);
    let mut capability = RecordingCapabilityGate::with([BazaarPaidCallAccessDecision::Authorized]);
    let error = run_x402_local_reference_path(
        &catalog,
        &access,
        &mut evaluation,
        Some(&mut capability),
        request("approved_request.json"),
    )
    .expect_err("authority escalation must fail closed");
    assert!(error.contains("must not grant"));
    assert!(capability.bindings.is_empty());
}

#[test]
fn access_preview_and_atomic_exact_consume_are_both_required() {
    let catalog = catalog();
    let access = RecordingAccessState::new(X402LocalAccessState::PaymentRequired);
    let mut evaluation =
        RecordingEvaluation::returning(response("approved_evaluation_response.json"));
    let mut capability = RecordingCapabilityGate::with([BazaarPaidCallAccessDecision::Authorized]);
    let error = run_x402_local_reference_path(
        &catalog,
        &access,
        &mut evaluation,
        Some(&mut capability),
        request("approved_request.json"),
    )
    .expect_err("unsettled access must stop before evaluation");
    assert!(error.contains("payment_required"));
    assert!(evaluation.requests.is_empty());
    assert!(capability.bindings.is_empty());

    let access = RecordingAccessState::new(X402LocalAccessState::SettledAccessReady);
    let mut evaluation =
        RecordingEvaluation::returning(response("approved_evaluation_response.json"));
    let mut capability =
        RecordingCapabilityGate::with([BazaarPaidCallAccessDecision::ReplayBlocked]);
    let result = run_x402_local_reference_path(
        &catalog,
        &access,
        &mut evaluation,
        Some(&mut capability),
        request("approved_request.json"),
    )
    .expect("exact consume rejection is a typed terminal outcome");
    assert_eq!(result.outcome, X402LocalReferenceOutcome::CapabilityDenied);
    assert_eq!(result.capability_gate.code, "access_replay_blocked");
    assert!(!result.capability_gate.service_call_allowed);
    assert!(!result.capability_gate.access_consumed);
    assert!(!result.capability_gate.service_dispatch_allowed);
    assert_eq!(capability.bindings.len(), 1);
    let paid_authority = serde_json::to_value(
        result
            .capability_gate
            .paid_call_result
            .expect("rejected paid-call result")
            .authority,
    )
    .expect("serialize rejected authority");
    assert_all_false(&paid_authority, 11);
}

#[test]
fn versioned_fixtures_and_docs_lock_the_same_non_bypass_path() {
    let manifest = read_fixture("manifest.json");
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["scenarios"].as_array().map(Vec::len), Some(2));
    assert_all_false(&manifest["authority"], 11);

    for scenario in manifest["scenarios"].as_array().expect("scenario array") {
        let request_name = scenario["request_fixture"]
            .as_str()
            .expect("request fixture");
        let response_name = scenario["evaluation_response_fixture"]
            .as_str()
            .expect("response fixture");
        let request = request(request_name);
        assert_eq!(
            request.schema_version,
            X402_LOCAL_REFERENCE_PATH_SCHEMA_VERSION
        );
        request
            .evaluation_request
            .validate()
            .expect("valid evaluation request");
        response(response_name)
            .validate()
            .expect("valid evaluation response");
    }

    let mut unknown_field = read_fixture("approved_request.json");
    unknown_field["capability_granted"] = Value::Bool(true);
    assert!(serde_json::from_value::<X402LocalReferencePathRequest>(unknown_field).is_err());

    for path in [
        "docs/x402_local_reference_path.md",
        "examples/x402_local_reference_path/README.md",
    ] {
        let docs = fs::read_to_string(path).unwrap_or_else(|error| panic!("read {path}: {error}"));
        for required in [
            "Bazaar discovery",
            "typed ActionPlan",
            "deterministic policy",
            "capability gate",
            "no dispatch",
            "wallet",
            "ActionPlan-submit",
        ] {
            assert!(
                docs.contains(required),
                "{path} missing boundary term {required}"
            );
        }
    }
}
