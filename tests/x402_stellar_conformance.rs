use std::{fs, path::Path};

use neurochain::x402_stellar_conformance::{
    validate_x402_stellar_conformance_plan, X402ConformanceStatus, X402StellarConformancePlan,
};
use serde_json::{json, Map, Value};

const FIXTURE_DIR: &str = "examples/x402_stellar_conformance";

fn read_value(name: &str) -> Value {
    let path = Path::new(FIXTURE_DIR).join(name);
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn plan_value() -> Value {
    read_value("plan.json")
}

fn parse_plan(value: Value) -> X402StellarConformancePlan {
    serde_json::from_value(value).expect("deserialize conformance plan")
}

fn merge_patch(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                merge_patch(target.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

fn apply_operation(plan: &mut Value, operation: &str) {
    let cases = plan["cases"].as_array_mut().expect("cases array");
    match operation {
        "remove_last_case" => {
            cases.pop().expect("case to remove");
        }
        "duplicate_first_case" => cases.push(cases[0].clone()),
        "mark_upto_ready" => {
            let case = cases
                .iter_mut()
                .find(|case| case["id"] == "upto_stellar_upstream_spec")
                .expect("upto case");
            case["status"] = Value::String("ready".to_string());
        }
        "mark_live_ready" => {
            let case = cases
                .iter_mut()
                .find(|case| case["id"] == "exact_canonical_client_e2e")
                .expect("live case");
            case["status"] = Value::String("ready".to_string());
        }
        other => panic!("unknown fixture operation {other}"),
    }
}

#[test]
fn pinned_offline_plan_is_complete_but_grants_no_authority() {
    let plan = parse_plan(plan_value());
    let report = validate_x402_stellar_conformance_plan(&plan);
    assert!(report.ok);
    assert_eq!(report.code, "conformance_plan_ready");
    assert!(report.reason.contains("does not claim"));

    let summary = report.data.expect("ready summary");
    assert_eq!(summary.total_cases, 24);
    assert_eq!(summary.offline_ready_cases, 14);
    assert_eq!(summary.approval_blocked_cases.len(), 8);
    assert_eq!(summary.upstream_blocked_cases.len(), 2);

    let authority = serde_json::to_value(report.authority).expect("serialize authority");
    let authority = authority.as_object().expect("authority object");
    assert_eq!(authority.len(), 7);
    assert!(authority.values().all(|value| value == &Value::Bool(false)));
}

#[test]
fn plan_keeps_package_owner_and_runtime_approval_separate() {
    let value = plan_value();
    let dependency = &value["dependencyBoundary"];
    assert_eq!(dependency["packageName"], "@x402/stellar");
    assert_eq!(dependency["license"], "Apache-2.0");
    assert_eq!(dependency["verifySettleOwner"], "upstream_package");
    assert_eq!(dependency["packageSelectionStatus"], "approval_required");
    assert_eq!(dependency["packageInstalled"], false);
    assert_eq!(dependency["runtimeApproved"], false);

    let mut injected = value;
    injected["dependencyBoundary"]["credential"] = Value::String("forbidden".to_string());
    assert!(serde_json::from_value::<X402StellarConformancePlan>(injected).is_err());
}

#[test]
fn adversarial_and_drift_fixtures_fail_closed_with_stable_codes() {
    let scenarios = read_value("adversarial_patches.json");
    for scenario in scenarios.as_array().expect("scenario array") {
        let mut value = plan_value();
        if let Some(patch) = scenario.get("patch") {
            merge_patch(&mut value, patch);
        }
        if let Some(operation) = scenario.get("operation").and_then(Value::as_str) {
            apply_operation(&mut value, operation);
        }
        let plan = parse_plan(value);
        let report = validate_x402_stellar_conformance_plan(&plan);
        assert!(!report.ok, "scenario {}", scenario["name"]);
        assert_eq!(
            report.code, scenario["expectedCode"],
            "{}",
            scenario["name"]
        );
        assert!(!report.reason.is_empty());
        assert!(report.data.is_none());
    }
}

#[test]
fn missing_sources_network_drift_and_unknown_cases_are_rejected() {
    let mut missing_source = plan_value();
    missing_source["sourceSnapshot"]["sources"]
        .as_array_mut()
        .expect("sources")
        .pop();
    assert_eq!(
        validate_x402_stellar_conformance_plan(&parse_plan(missing_source)).code,
        "spec_drift_detected"
    );

    let mut reversed_networks = plan_value();
    reversed_networks["sourceSnapshot"]["networks"] = json!(["stellar:pubnet", "stellar:testnet"]);
    assert_eq!(
        validate_x402_stellar_conformance_plan(&parse_plan(reversed_networks)).code,
        "spec_drift_detected"
    );

    let mut unknown_case = plan_value();
    unknown_case["cases"][0]["id"] = Value::String("self_asserted_conformance".to_string());
    assert_eq!(
        validate_x402_stellar_conformance_plan(&parse_plan(unknown_case)).code,
        "unexpected_conformance_case"
    );
}

#[test]
fn reasons_and_evidence_are_bounded_and_exact() {
    let mut empty_reason = plan_value();
    empty_reason["cases"][0]["reason"] = Value::String(String::new());
    assert_eq!(
        validate_x402_stellar_conformance_plan(&parse_plan(empty_reason)).code,
        "invalid_conformance_case_reason"
    );

    let mut duplicate_evidence = plan_value();
    duplicate_evidence["cases"][0]["evidence"] = json!(["wire_fixture", "wire_fixture"]);
    assert_eq!(
        validate_x402_stellar_conformance_plan(&parse_plan(duplicate_evidence)).code,
        "conformance_case_mismatch"
    );

    let mut missing_evidence = plan_value();
    missing_evidence["cases"][0]["evidence"] = Value::Array(Vec::new());
    assert_eq!(
        validate_x402_stellar_conformance_plan(&parse_plan(missing_evidence)).code,
        "conformance_case_mismatch"
    );
}

#[test]
fn upto_is_explicitly_upstream_blocked_and_not_claimed_supported() {
    let plan = parse_plan(plan_value());
    assert!(!plan.source_snapshot.upto_stellar_spec_present);
    let upto = plan
        .cases
        .iter()
        .filter(|case| case.id.starts_with("upto_"))
        .collect::<Vec<_>>();
    assert_eq!(upto.len(), 2);
    assert!(upto
        .iter()
        .all(|case| case.status == X402ConformanceStatus::UpstreamBlocked));
    assert!(upto.iter().all(|case| !case.reason.trim().is_empty()));
}

#[test]
fn matrix_covers_rfp_security_wire_and_operations_requirements() {
    let value = plan_value();
    let encoded = serde_json::to_string(&value).expect("encode plan");
    for required in [
        "stellar:testnet",
        "stellar:pubnet",
        "supported_are_fees_sponsored",
        "wire_v2_payload_transaction",
        "exact_keypair_auth",
        "exact_custom_check_auth",
        "exact_sep41_seven_decimals",
        "exact_tampered_signature_reject",
        "exact_asset_mismatch_reject",
        "exact_amount_mismatch_reject",
        "exact_recipient_mismatch_reject",
        "exact_expired_auth_reject",
        "exact_replay_reject",
        "exact_auth_structure_reject",
        "exact_facilitator_non_custodial",
        "exact_simulation_balance_change_reject",
        "exact_missing_trustline_reject",
        "rejections_non_null_reason",
        "observability_and_audit",
        "third_party_security_review",
    ] {
        assert!(encoded.contains(required), "missing matrix case {required}");
    }
}

#[test]
fn fixture_and_docs_lock_the_offline_no_runtime_boundary() {
    let paths = [
        "docs/x402_stellar_conformance.md",
        "examples/x402_stellar_conformance/README.md",
    ];
    for path in paths {
        let content =
            fs::read_to_string(path).unwrap_or_else(|error| panic!("read {path}: {error}"));
        for required in [
            "offline",
            "@x402/stellar",
            "does not claim",
            "upto",
            "wallet",
            "settlement",
            "ActionPlan",
            "no authority",
        ] {
            assert!(content.contains(required), "{path} missing {required}");
        }
    }
}

#[test]
fn strict_envelopes_reject_unknown_top_level_fields() {
    let mut value = plan_value();
    let object: &mut Map<String, Value> = value.as_object_mut().expect("plan object");
    object.insert("runNetworkNow".to_string(), Value::Bool(true));
    assert!(serde_json::from_value::<X402StellarConformancePlan>(value).is_err());
}

#[test]
fn checked_in_schema_locks_structure_and_no_runtime_claims() {
    let schema = read_value("schema.json");
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["schemaVersion"]["const"], 1);
    assert_eq!(
        schema["$defs"]["dependencyBoundary"]["properties"]["packageName"]["const"],
        "@x402/stellar"
    );
    assert_eq!(
        schema["$defs"]["dependencyBoundary"]["properties"]["packageInstalled"]["const"],
        false
    );
    assert_eq!(
        schema["$defs"]["dependencyBoundary"]["properties"]["runtimeApproved"]["const"],
        false
    );
    assert_eq!(schema["properties"]["cases"]["minItems"], 24);
    assert_eq!(schema["properties"]["cases"]["maxItems"], 24);
    assert_eq!(schema["$defs"]["case"]["additionalProperties"], false);
    assert_eq!(
        schema["$defs"]["case"]["properties"]["evidence"]["uniqueItems"],
        true
    );
}
