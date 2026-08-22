use std::{fs, path::Path};

use neurochain::x402_service_boundary::{
    X402ServiceEvaluationRequest, X402ServiceEvaluationResponse,
};
use serde_json::Value;

const FIXTURE_DIR: &str = "examples/x402_service_boundary";

fn read_json(name: &str) -> Value {
    let path = Path::new(FIXTURE_DIR).join(name);
    let raw =
        fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
}

#[test]
fn request_is_bounded_and_excludes_payment_or_execution_authority() {
    let value = read_json("evaluation_request.json");
    let request: X402ServiceEvaluationRequest =
        serde_json::from_value(value.clone()).expect("deserialize request fixture");
    request.validate().expect("validate request fixture");

    for forbidden in [
        "payment_payload",
        "payment_signature",
        "authorization_entry",
        "transaction_envelope",
        "secret",
        "wallet",
        "model_path",
        "policy_override",
        "action_plan",
        "submit",
    ] {
        assert!(
            value.get(forbidden).is_none(),
            "request includes {forbidden}"
        );
    }

    let mut tampered = value;
    tampered["payment_payload"] = serde_json::json!({ "opaque": "forbidden" });
    assert!(
        serde_json::from_value::<X402ServiceEvaluationRequest>(tampered).is_err(),
        "unknown payment payload must fail closed"
    );
}

#[test]
fn response_matrix_binds_action_plan_and_grants_no_authority() {
    for name in [
        "evaluation_approved.json",
        "evaluation_requires_approval.json",
        "evaluation_blocked_exit_4.json",
    ] {
        let response: X402ServiceEvaluationResponse =
            serde_json::from_value(read_json(name)).expect("deserialize response fixture");
        response
            .validate()
            .unwrap_or_else(|err| panic!("validate {name}: {err}"));
        assert!(!response.underlying_action_submit_allowed);
        assert!(!response.authority_grants.payment_verification);
        assert!(!response.authority_grants.payment_settlement);
        assert!(!response.authority_grants.guardrail_override);
        assert!(!response.authority_grants.wallet_signing);
        assert!(!response.authority_grants.stellar_submission);
    }
}

#[test]
fn parity_manifest_is_shared_with_the_typescript_service() {
    let manifest = read_json("parity_manifest.json");
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["request_fixture"], "evaluation_request.json");
    assert_eq!(manifest["authority_grants"]["payment_verification"], false);
    assert_eq!(manifest["authority_grants"]["payment_settlement"], false);
    assert_eq!(manifest["authority_grants"]["guardrail_override"], false);
    assert_eq!(manifest["authority_grants"]["wallet_signing"], false);
    assert_eq!(manifest["authority_grants"]["stellar_submission"], false);
    assert_eq!(manifest["underlying_action_submit_allowed"], false);

    let request: X402ServiceEvaluationRequest =
        serde_json::from_value(read_json(manifest["request_fixture"].as_str().unwrap()))
            .expect("deserialize manifest request fixture");
    request
        .validate()
        .expect("validate manifest request fixture");

    let responses = manifest["response_fixtures"]
        .as_array()
        .expect("response_fixtures array");
    assert_eq!(responses.len(), 3);
    for entry in responses {
        let name = entry["file"].as_str().expect("response fixture file");
        let response: X402ServiceEvaluationResponse =
            serde_json::from_value(read_json(name)).expect("deserialize response fixture");
        response
            .validate()
            .unwrap_or_else(|err| panic!("validate {name}: {err}"));
        assert_eq!(
            serde_json::to_value(response.decision).expect("serialize decision"),
            entry["decision"]
        );
        assert_eq!(
            response.exit_code.map(Value::from).unwrap_or(Value::Null),
            entry["exit_code"]
        );
        assert_eq!(response.reason_code, entry["reason_code"]);
    }
}

#[test]
fn response_rejects_hash_decision_and_authority_escalation() {
    let value = read_json("evaluation_approved.json");

    let mut changed_hash = value.clone();
    changed_hash["action_plan_hash"] = Value::String("0".repeat(64));
    let changed_hash: X402ServiceEvaluationResponse =
        serde_json::from_value(changed_hash).expect("deserialize changed hash");
    assert!(changed_hash.validate().is_err());

    let mut changed_decision = value.clone();
    changed_decision["decision"] = Value::String("blocked".to_string());
    let changed_decision: X402ServiceEvaluationResponse =
        serde_json::from_value(changed_decision).expect("deserialize changed decision");
    assert!(changed_decision.validate().is_err());

    let mut escalated = value.clone();
    escalated["authority_grants"]["stellar_submission"] = Value::Bool(true);
    let escalated: X402ServiceEvaluationResponse =
        serde_json::from_value(escalated).expect("deserialize escalated authority");
    assert!(escalated.validate().is_err());

    let mut submit_allowed = value;
    submit_allowed["underlying_action_submit_allowed"] = Value::Bool(true);
    let submit_allowed: X402ServiceEvaluationResponse =
        serde_json::from_value(submit_allowed).expect("deserialize submit authority");
    assert!(submit_allowed.validate().is_err());
}

#[test]
fn schema_and_docs_lock_module_ownership() {
    let schema = read_json("schema.json");
    let request = &schema["$defs"]["evaluationRequest"];
    assert_eq!(request["additionalProperties"], false);
    assert!(request["properties"].get("payment_payload").is_none());
    assert!(request["properties"].get("payment_signature").is_none());
    assert!(request["properties"].get("action_plan").is_none());

    let grants = &schema["$defs"]["authorityGrants"]["properties"];
    for field in [
        "payment_verification",
        "payment_settlement",
        "guardrail_override",
        "wallet_signing",
        "stellar_submission",
    ] {
        assert_eq!(grants[field]["const"], false, "{field} must stay false");
    }
    assert_eq!(
        schema["$defs"]["evaluationResponse"]["properties"]["underlying_action_submit_allowed"]
            ["const"],
        false
    );

    let docs =
        fs::read_to_string("docs/x402_service_boundary.md").expect("read service boundary docs");
    for required in [
        "@x402/stellar",
        "does not reimplement",
        "Must not receive",
        "Evidence is not authority",
        "No endpoint or transport is wired",
        "Pubnet operation remains a separate user confirmation boundary",
    ] {
        assert!(docs.contains(required), "docs missing boundary: {required}");
    }
}
