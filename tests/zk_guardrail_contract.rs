use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use neurochain::zk_attestation::ZkProofArtifact;
use neurochain_zk_guardrail_contract::{DecisionStatus, ExitCode, PublicJournal};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

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

#[test]
fn historical_testnet_record_matches_current_public_proof_bindings() {
    // Windows PowerShell wrote this historical record with a UTF-8 BOM.
    // Decode that encoding marker without rewriting the evidence file.
    let raw = include_str!("../hackathons/stellar-real-world-zk/deployments/testnet.json");
    let value: Value = serde_json::from_str(raw.strip_prefix('\u{feff}').unwrap_or(raw))
        .expect("parse historical testnet record");
    let record = object(&value, "historical testnet record");
    assert_exact_keys(
        record,
        &[
            "schema_version",
            "generated_at_utc",
            "network",
            "repository_commit",
            "verifier_upstream_commit",
            "selector",
            "evaluator_image_id",
            "contracts",
            "wasm_sha256",
            "authorized_policies",
            "safety",
        ],
        "historical testnet record",
    );
    assert_eq!(record["schema_version"], 1);
    assert_eq!(record["network"], "testnet");
    assert_eq!(record["generated_at_utc"], "2026-07-02T13:51:25.0071028Z");
    assert_eq!(
        record["repository_commit"],
        "12ef1d45dce4dfc34bb9a911cda90c18bd61420f"
    );
    let verifier_revision = "e8ff6ea202db195352c0141ecc533ff649393fe4";
    assert_eq!(record["verifier_upstream_commit"], verifier_revision);
    assert!(
        include_str!("../hackathons/stellar-real-world-zk/soroban/Cargo.toml")
            .contains(&format!("rev = \"{verifier_revision}\""))
    );
    assert_hex32(&record["evaluator_image_id"], "evaluator image ID");
    let selector = record["selector"].as_str().expect("selector string");
    assert_eq!(selector, "73c457ba");

    let contracts = object(&record["contracts"], "contract IDs");
    let wasm_hashes = object(&record["wasm_sha256"], "WASM hashes");
    let contract_names = [
        "groth16_verifier",
        "risc0_router",
        "neurochain_zk_guardrail",
    ];
    assert_exact_keys(contracts, &contract_names, "contract IDs");
    assert_exact_keys(wasm_hashes, &contract_names, "WASM hashes");
    for name in contract_names {
        let address = contracts[name].as_str().expect("contract address");
        assert_eq!(address.len(), 56);
        assert!(address.starts_with('C'));
        assert!(address
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || (b'2'..=b'7').contains(&byte)));
        assert_hex32(&wasm_hashes[name], name);
    }

    let safety = object(&record["safety"], "historical safety boundary");
    assert_exact_keys(
        safety,
        &[
            "read_only_verify_is_repeatable",
            "consume_requires_owner_auth",
            "underlying_action_submit_allowed",
        ],
        "historical safety boundary",
    );
    assert_eq!(safety["read_only_verify_is_repeatable"], true);
    assert_eq!(safety["consume_requires_owner_auth"], true);
    assert_eq!(safety["underlying_action_submit_allowed"], false);

    let policies = record["authorized_policies"]
        .as_array()
        .expect("policy array");
    assert_eq!(policies.len(), 3);
    let mut seen = BTreeSet::new();
    for policy in policies {
        let policy = object(policy, "historical policy binding");
        assert_exact_keys(
            policy,
            &[
                "scenario",
                "commitment",
                "version",
                "action_plan_hash",
                "audit_nullifier",
                "read_only_verified",
            ],
            "historical policy binding",
        );
        let scenario = policy["scenario"].as_str().expect("scenario name");
        assert!(seen.insert(scenario), "duplicate historical scenario");
        let (proof_name, decision, exit) = match scenario {
            "approved" => (
                "groth16_approved.json",
                DecisionStatus::Approved,
                ExitCode::Passed,
            ),
            "requires_approval" => (
                "groth16_requires_approval.json",
                DecisionStatus::RequiresApproval,
                ExitCode::Passed,
            ),
            "blocked" => (
                "groth16_blocked_exit_3.json",
                DecisionStatus::Blocked,
                ExitCode::Allowlist,
            ),
            _ => panic!("unexpected historical scenario: {scenario}"),
        };
        let proof: ZkProofArtifact =
            serde_json::from_value(fixture(proof_name)).expect("strict public proof fixture");
        assert_eq!(proof.schema_version, 1);
        assert!(proof.seal_hex.starts_with(selector));
        let journal_bytes = hex::decode(&proof.journal_hex).expect("journal hex");
        assert_eq!(
            hex::encode(Sha256::digest(&journal_bytes)),
            proof.journal_digest_hex
        );
        let journal = PublicJournal::decode(&journal_bytes).expect("canonical public journal");
        assert_eq!(record["evaluator_image_id"], proof.image_id_hex);
        assert_eq!(hex::encode(journal.evaluator_image_id), proof.image_id_hex);
        assert_eq!(policy["commitment"], hex::encode(journal.policy_commitment));
        assert_eq!(policy["version"], journal.policy_version);
        assert_eq!(
            policy["action_plan_hash"],
            hex::encode(journal.action_plan_hash)
        );
        assert_eq!(
            policy["audit_nullifier"],
            hex::encode(journal.audit_nullifier)
        );
        assert_eq!(policy["read_only_verified"], true);
        assert_eq!(journal.decision_status, decision);
        assert_eq!(journal.exit_code, exit);
        assert_eq!(journal.requires_approval, scenario == "requires_approval");
    }
    assert_eq!(
        seen,
        BTreeSet::from(["approved", "requires_approval", "blocked"])
    );
}

#[test]
fn readiness_docs_separate_offline_crypto_historical_and_live_evidence() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let readme = fs::read_to_string(root.join("README.md")).expect("read root README");
    assert!(readme.contains("docs/product_readiness.md"));
    assert!(readme.contains("historical testnet evidence recorded on 2026-07-02"));
    assert!(readme.contains("does not establish current network availability"));
    let docs = fs::read_to_string(root.join("docs/product_readiness.md"))
        .expect("read product evidence map");
    for required in [
        "Soroban SDK test environment",
        "2026-07-02T13:51:25.0071028Z",
        "read_only_verified=true",
        "cryptographicallyVerified=false",
        "stellarVerificationRequired=true",
        "exact_canonical_client_e2e",
        "approval_blocked",
        "outcome_unknown",
        "Manual acceptance with Ville",
        "not run or approval-blocked",
        "starting an HTTP/MCP listener needs separate approval",
        "An independent developer's usefulness assessment comes after Ville's own",
    ] {
        assert!(docs.contains(required), "readiness docs missing {required}");
    }
}
