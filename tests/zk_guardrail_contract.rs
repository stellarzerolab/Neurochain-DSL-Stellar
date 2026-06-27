use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{Map, Value};

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("hackathons")
        .join("stellar-real-world-zk")
        .join("fixtures")
        .join(name);
    let raw = fs::read_to_string(path).expect("read ZK fixture");
    serde_json::from_str(&raw).expect("parse ZK fixture")
}

fn object<'a>(value: &'a Value, context: &str) -> &'a Map<String, Value> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("{context} must be an object"))
}

fn assert_exact_keys(object: &Map<String, Value>, expected: &[&str], context: &str) {
    let actual: BTreeSet<&str> = object.keys().map(String::as_str).collect();
    let expected: BTreeSet<&str> = expected.iter().copied().collect();
    assert_eq!(actual, expected, "unexpected keys in {context}");
}

fn assert_hex32(value: &Value, context: &str) {
    let value = value
        .as_str()
        .unwrap_or_else(|| panic!("{context} must be a string"));
    assert_eq!(value.len(), 64, "{context} must encode 32 bytes");
    assert!(
        value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "{context} must be hex"
    );
}

fn assert_sorted_unique_strings(value: &Value, context: &str) {
    let values = value
        .as_array()
        .unwrap_or_else(|| panic!("{context} must be an array"));
    assert!(!values.is_empty(), "{context} must not be empty");
    let values: Vec<&str> = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .unwrap_or_else(|| panic!("{context} values must be strings"))
        })
        .collect();
    assert!(
        values.windows(2).all(|pair| pair[0] < pair[1]),
        "{context} must be sorted and unique"
    );
}

#[test]
fn typed_action_plan_fixture_uses_existing_contract_invoke_label() {
    let value = fixture("typed_action_plan.json");
    let plan = object(&value, "typed ActionPlan");
    assert_exact_keys(
        plan,
        &[
            "schema_version",
            "intent_label",
            "action_kind",
            "contract_id",
            "function",
            "args",
            "intent_confidence_bps",
        ],
        "typed ActionPlan",
    );
    assert_eq!(plan["schema_version"], 1);
    assert_eq!(plan["intent_label"], "ContractInvoke");
    assert_eq!(plan["action_kind"], "soroban_contract_invoke");
    assert!(plan["intent_confidence_bps"].as_u64().unwrap() <= 10_000);

    let args = plan["args"].as_array().expect("args array");
    let names: Vec<&str> = args
        .iter()
        .map(|arg| arg["name"].as_str().expect("arg name"))
        .collect();
    assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    for arg in args {
        let arg = object(arg, "typed arg");
        assert_exact_keys(arg, &["name", "type", "value"], "typed arg");
        assert!(matches!(
            arg["type"].as_str(),
            Some("address" | "bytes" | "symbol" | "u64")
        ));
    }
}

#[test]
fn private_policy_fixtures_are_canonical_and_keep_rules_private() {
    for name in [
        "private_policy_approved.json",
        "private_policy_requires_approval.json",
        "private_policy_blocked_exit_3.json",
        "private_policy_blocked_exit_4.json",
    ] {
        let value = fixture(name);
        let policy = object(&value, name);
        assert_exact_keys(
            policy,
            &[
                "schema_version",
                "policy_version",
                "commitment_salt",
                "allowed_contracts",
                "allowed_contract_functions",
                "allowed_assets",
                "allowed_recipients",
                "max_amount_minor",
                "approval_threshold_minor",
                "min_intent_confidence_bps",
            ],
            name,
        );
        assert_hex32(&policy["commitment_salt"], "commitment_salt");
        for field in [
            "allowed_contracts",
            "allowed_contract_functions",
            "allowed_assets",
            "allowed_recipients",
        ] {
            assert_sorted_unique_strings(&policy[field], field);
        }
        let max = policy["max_amount_minor"]
            .as_str()
            .expect("max amount string")
            .parse::<u64>()
            .expect("max amount u64");
        let approval = policy["approval_threshold_minor"]
            .as_str()
            .expect("approval amount string")
            .parse::<u64>()
            .expect("approval amount u64");
        assert!(approval <= max);
        assert!(policy["min_intent_confidence_bps"].as_u64().unwrap() <= 10_000);
    }
}

#[test]
fn exit_5_action_plan_fixture_is_missing_required_recipient() {
    let value = fixture("typed_action_plan_blocked_exit_5.json");
    let plan = object(&value, "exit 5 typed ActionPlan");
    assert_eq!(plan["intent_label"], "ContractInvoke");
    assert_eq!(plan["action_kind"], "soroban_contract_invoke");
    let args = plan["args"].as_array().expect("args array");
    let names: Vec<&str> = args
        .iter()
        .map(|arg| arg["name"].as_str().expect("arg name"))
        .collect();
    assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(!names.contains(&"recipient"));
}

#[test]
fn public_journal_fixture_matrix_locks_exit_semantics() {
    let cases = [
        ("journal_approved.json", "approved", 0, "passed", false),
        (
            "journal_requires_approval.json",
            "requires_approval",
            0,
            "approval_threshold",
            true,
        ),
        (
            "journal_blocked_exit_3.json",
            "blocked",
            3,
            "allowlist",
            false,
        ),
        (
            "journal_blocked_exit_4.json",
            "blocked",
            4,
            "contract_policy",
            false,
        ),
        (
            "journal_blocked_exit_5.json",
            "blocked",
            5,
            "intent_safety",
            false,
        ),
    ];

    for (name, decision, exit, reason, requires_approval) in cases {
        let value = fixture(name);
        let journal = object(&value, name);
        assert_exact_keys(
            journal,
            &[
                "contract_version",
                "evaluator_image_id",
                "action_plan_hash",
                "policy_commitment",
                "policy_version",
                "decision_status",
                "exit_code",
                "reason_code",
                "requires_approval",
                "audit_nullifier",
            ],
            name,
        );
        for field in [
            "evaluator_image_id",
            "action_plan_hash",
            "policy_commitment",
            "audit_nullifier",
        ] {
            assert_hex32(&journal[field], field);
        }
        assert_eq!(journal["decision_status"], decision, "{name}");
        assert_eq!(journal["exit_code"], exit, "{name}");
        assert_eq!(journal["reason_code"], reason, "{name}");
        assert_eq!(journal["requires_approval"], requires_approval, "{name}");
        assert!(
            !journal.keys().any(|key| key.starts_with("allowed_")),
            "public journal must not reveal private policy lists"
        );
        assert!(
            !journal.contains_key("commitment_salt"),
            "public journal must not reveal the policy salt"
        );
    }
}
