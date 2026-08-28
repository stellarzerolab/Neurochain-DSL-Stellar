use std::fs;

use neurochain::{
    product_local_reference_path::run_product_local_reference_path,
    x402_bazaar::{BazaarCatalog, BazaarCatalogCandidate, BazaarCatalogKey},
    x402_bazaar_paid_call::{
        BazaarPaidCallAccessDecision, BazaarPaidCallAccessGate, BazaarPaidCallBinding,
    },
    x402_local_reference_path::{
        X402LocalAccessState, X402LocalAccessStatePort, X402LocalEvaluationPort,
        X402LocalReferencePathRequest,
    },
    x402_service_boundary::{X402ServiceEvaluationRequest, X402ServiceEvaluationResponse},
    zk_attestation::{ZkAttestationViewRequest, ZkProofArtifact, ZkTypedActionPlan},
};
use serde::de::DeserializeOwned;
use serde_json::Value;

#[path = "../examples/product_local_quickstart.rs"]
mod quickstart;

const CATALOG_JSON: &str = include_str!("../examples/x402_bazaar_catalog/mcp_tool.json");
const REQUEST_JSON: &str =
    include_str!("../examples/product_local_quickstart/approved_request.json");
const RESPONSE_JSON: &str =
    include_str!("../examples/product_local_quickstart/approved_evaluation_response.json");
const ACTION_PLAN_JSON: &str =
    include_str!("../hackathons/stellar-real-world-zk/fixtures/typed_action_plan.json");
const APPROVED_PROOF_JSON: &str =
    include_str!("../hackathons/stellar-real-world-zk/fixtures/groth16_approved.json");
const REQUIRES_APPROVAL_PROOF_JSON: &str =
    include_str!("../hackathons/stellar-real-world-zk/fixtures/groth16_requires_approval.json");

fn parse<T: DeserializeOwned>(raw: &str) -> T {
    serde_json::from_str(raw).expect("parse test fixture")
}

struct SettledAccess;

impl X402LocalAccessStatePort for SettledAccess {
    fn inspect_access(&self, _resource_key: &BazaarCatalogKey) -> X402LocalAccessState {
        X402LocalAccessState::SettledAccessReady
    }
}

struct FixtureEvaluation(Option<X402ServiceEvaluationResponse>);

impl X402LocalEvaluationPort for FixtureEvaluation {
    fn plan_and_evaluate(
        &mut self,
        _request: &X402ServiceEvaluationRequest,
    ) -> Result<X402ServiceEvaluationResponse, String> {
        self.0
            .take()
            .ok_or_else(|| "evaluation fixture consumed twice".to_string())
    }
}

#[derive(Default)]
struct CountingCapabilityGate {
    calls: usize,
}

impl BazaarPaidCallAccessGate for CountingCapabilityGate {
    fn consume_settled_access(
        &mut self,
        _binding: &BazaarPaidCallBinding,
    ) -> BazaarPaidCallAccessDecision {
        self.calls += 1;
        BazaarPaidCallAccessDecision::Authorized
    }
}

fn catalog() -> BazaarCatalog {
    let mut catalog = BazaarCatalog::default();
    catalog
        .insert(parse::<BazaarCatalogCandidate>(CATALOG_JSON), 1_723_000_001)
        .expect("insert catalog fixture");
    catalog
}

fn run_with_evidence(
    action_plan: ZkTypedActionPlan,
    proof: ZkProofArtifact,
) -> (Result<(), String>, usize) {
    let mut evaluation = FixtureEvaluation(Some(parse(RESPONSE_JSON)));
    let mut capability = CountingCapabilityGate::default();
    let result = run_product_local_reference_path(
        &catalog(),
        &SettledAccess,
        &mut evaluation,
        Some(&mut capability),
        parse::<X402LocalReferencePathRequest>(REQUEST_JSON),
        ZkAttestationViewRequest { action_plan, proof },
    )
    .map(|_| ());
    (result, capability.calls)
}

#[test]
fn quickstart_report_matches_the_versioned_whole_product_fixture() {
    let actual = quickstart::quickstart_report().expect("run product local quickstart");
    let expected: Value = serde_json::from_str(include_str!(
        "../examples/product_local_quickstart/quickstart_output.json"
    ))
    .expect("parse expected product quickstart output");
    assert_eq!(actual, expected);

    assert_eq!(
        actual["path"],
        serde_json::json!([
            "bazaar_discovery",
            "x402_access_state",
            "typed_action_plan",
            "deterministic_policy",
            "optional_zk_proof_artifact",
            "local_zk_binding_verify",
            "separate_exact_capability_gate"
        ])
    );
    let boundary = actual["authorityBoundary"]
        .as_object()
        .expect("authority boundary");
    assert_eq!(boundary.len(), 12);
    assert!(boundary.values().all(|value| value == false));

    for scenario in actual["scenarios"].as_array().expect("scenario array") {
        for field in ["authority", "zkEvidence"] {
            let authority = if field == "zkEvidence" {
                scenario[field]["authority"]
                    .as_object()
                    .expect("ZK authority")
            } else {
                scenario[field].as_object().expect("scenario authority")
            };
            assert_eq!(authority.len(), 12);
            assert!(authority.values().all(|value| value == false));
        }
        assert_eq!(scenario["capability"]["serviceDispatchAllowed"], false);
        assert_eq!(scenario["zkEvidence"]["artifactPresent"], true);
        assert_eq!(
            scenario["zkEvidence"]["actionPlanProjectionValidated"],
            true
        );
        assert_eq!(scenario["zkEvidence"]["localBinding"], "binding_validated");
        assert_eq!(scenario["zkEvidence"]["cryptographicallyVerified"], false);
        assert_eq!(scenario["zkEvidence"]["stellarVerificationRequired"], true);
        assert_eq!(scenario["zkEvidence"]["privatePolicyRevealed"], false);
    }
    assert_eq!(actual["scenarios"][0]["capability"]["gateCalls"], 1);
    assert_eq!(actual["scenarios"][1]["capability"]["gateCalls"], 0);
    assert_eq!(actual["scenarios"][2]["capability"]["gateCalls"], 0);
}

#[test]
fn mismatched_proof_decision_fails_before_capability_consumption() {
    let (result, gate_calls) =
        run_with_evidence(parse(ACTION_PLAN_JSON), parse(REQUIRES_APPROVAL_PROOF_JSON));
    assert_eq!(
        result.expect_err("mismatched proof must fail closed"),
        "product ZK decision does not match deterministic policy"
    );
    assert_eq!(gate_calls, 0);
}

#[test]
fn mismatched_typed_action_fails_before_capability_consumption() {
    let mut action_plan: ZkTypedActionPlan = parse(ACTION_PLAN_JSON);
    action_plan.function = "different_function".to_string();
    let (result, gate_calls) = run_with_evidence(action_plan, parse(APPROVED_PROOF_JSON));
    assert_eq!(
        result.expect_err("mismatched typed action must fail closed"),
        "product ZK contract/function projection mismatch"
    );
    assert_eq!(gate_calls, 0);
}

#[test]
fn developer_docs_lock_the_offline_command_and_verification_boundary() {
    for path in [
        "docs/product_local_quickstart.md",
        "examples/product_local_quickstart/README.md",
    ] {
        let docs = fs::read_to_string(path).unwrap_or_else(|error| panic!("read {path}: {error}"));
        for required in [
            "cargo run --offline --quiet --example product_local_quickstart",
            "local",
            "cryptographic",
            "credential",
            "network",
        ] {
            assert!(docs.contains(required), "{path} missing {required}");
        }
    }

    let docs = fs::read_to_string("docs/product_local_quickstart.md").expect("read product docs");
    for required in [
        "Bazaar discovery",
        "typed ActionPlan",
        "requires_approval",
        "ZK proof artifact",
        "capability gate",
        "transaction submit",
        "ActionPlan-submit",
        "does **not** cryptographically",
    ] {
        assert!(docs.contains(required), "product docs missing {required}");
    }
}
