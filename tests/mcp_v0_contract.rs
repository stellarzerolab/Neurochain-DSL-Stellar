use serde_json::Value;
use std::fs;
use std::path::Path;

const FIXTURE_DIR: &str = "examples/mcp_v0_no_submit_contract";

const REQUIRED_FIELDS: &[&str] = &[
    "schema_version",
    "tool",
    "mode",
    "status",
    "decision",
    "exit_code",
    "reason_code",
    "action_plan_hash",
    "policy_commitment",
    "policy_version",
    "stellar_verification",
    "attestation_submitted",
    "verification_transaction_submitted",
    "transaction_hash",
    "nullifier_consumed",
    "underlying_action_submit_allowed",
    "logs",
];

const DEFAULT_TOOLS: &[&str] = &[
    "plan_stellar_action",
    "evaluate_guardrails",
    "prove_guardrail_decision",
    "verify_zk_on_stellar",
    "get_guardrail_status",
];

const EXCLUDED_TOOLS: &[&str] = &[
    "submit_testnet_attestation",
    "consume_nullifier",
    "submit_underlying_action",
    "sign_transaction",
    "configure_server",
];

#[test]
fn mcp_v0_fixtures_preserve_no_submit_contract() {
    let fixture_paths = fixture_paths();
    assert!(
        fixture_paths.len() >= 7,
        "expected one fixture for each MCP v0 phase"
    );

    for path in fixture_paths {
        let value = read_json(&path);
        let name = path.display().to_string();

        for field in REQUIRED_FIELDS {
            assert!(value.get(field).is_some(), "{name} missing {field}");
        }

        assert_eq!(value["schema_version"], 1, "{name} schema version");
        assert_eq!(value["mode"], "read_only", "{name} must be read-only");

        let tool = value["tool"].as_str().expect("tool string");
        assert!(
            DEFAULT_TOOLS.contains(&tool),
            "{name} uses non-v0 tool {tool}"
        );
        assert!(
            !EXCLUDED_TOOLS.contains(&tool),
            "{name} must not use excluded submit/stateful tool {tool}"
        );

        assert_eq!(
            value["underlying_action_submit_allowed"], false,
            "{name} must never allow underlying submit"
        );
        assert_eq!(
            value["attestation_submitted"], false,
            "{name} must not submit attestation in default MCP v0"
        );
        assert_eq!(
            value["verification_transaction_submitted"], false,
            "{name} must not submit verification transactions"
        );
        assert_eq!(
            value["nullifier_consumed"], false,
            "{name} must not consume nullifiers"
        );
        assert!(
            value["transaction_hash"].is_null(),
            "{name} must not expose a transaction hash in the default path"
        );
        assert!(value["logs"].is_array(), "{name} logs must be an array");

        assert_decision_exit_consistency(&name, &value);
        assert_no_secret_like_field_names(&name, &value);
    }
}

#[test]
fn mcp_v0_schema_excludes_submit_like_tools() {
    let schema = read_json(Path::new(FIXTURE_DIR).join("schema.json"));
    let tool_enum = schema["properties"]["tool"]["enum"]
        .as_array()
        .expect("tool enum");
    let tool_names: Vec<&str> = tool_enum.iter().filter_map(Value::as_str).collect();

    for expected in DEFAULT_TOOLS {
        assert!(
            tool_names.contains(expected),
            "schema missing default MCP v0 tool {expected}"
        );
    }

    for excluded in EXCLUDED_TOOLS {
        assert!(
            !tool_names.contains(excluded),
            "schema must exclude submit/stateful tool {excluded}"
        );
    }

    for field in [
        "underlying_action_submit_allowed",
        "attestation_submitted",
        "verification_transaction_submitted",
        "nullifier_consumed",
    ] {
        assert_eq!(
            schema["properties"][field]["const"], false,
            "schema should pin {field} to false"
        );
    }

    assert_eq!(
        schema["properties"]["transaction_hash"]["type"], "null",
        "default MCP v0 schema should not include transaction hashes"
    );
}

fn fixture_paths() -> Vec<std::path::PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(FIXTURE_DIR)
        .expect("fixture dir exists")
        .map(|entry| entry.expect("read fixture entry").path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "json")
                && path.file_name().is_some_and(|name| name != "schema.json")
        })
        .collect();
    paths.sort();
    paths
}

fn read_json(path: impl AsRef<Path>) -> Value {
    let path = path.as_ref();
    let raw = fs::read_to_string(path).unwrap_or_else(|err| {
        panic!("read {}: {err}", path.display());
    });
    serde_json::from_str(&raw).unwrap_or_else(|err| {
        panic!("parse {}: {err}", path.display());
    })
}

fn assert_decision_exit_consistency(name: &str, value: &Value) {
    let decision = value["decision"].as_str().expect("decision string");
    let exit_code = value["exit_code"].as_i64();
    let reason = value["reason_code"].as_str().expect("reason string");

    match decision {
        "not_evaluated" => assert!(exit_code.is_none(), "{name} preview should not set exit"),
        "approved" => assert_eq!(exit_code, Some(0), "{name} approved should exit 0"),
        "requires_approval" => {
            assert_eq!(exit_code, Some(0), "{name} requires_approval should exit 0");
            assert_eq!(
                value["underlying_action_submit_allowed"], false,
                "{name} requires_approval remains no-submit"
            );
        }
        "blocked" => {
            assert!(
                matches!(exit_code, Some(3..=5)),
                "{name} blocked decision should use exit 3, 4, or 5"
            );
            match exit_code {
                Some(3) => assert_eq!(reason, "allowlist", "{name} exit 3 reason"),
                Some(4) => assert!(
                    matches!(
                        reason,
                        "contract_policy"
                            | "invalid_attestation"
                            | "unauthorized_policy"
                            | "replay"
                    ),
                    "{name} exit 4 reason"
                ),
                Some(5) => assert!(
                    matches!(
                        reason,
                        "intent_safety" | "slot_missing" | "slot_type_error" | "low_confidence"
                    ),
                    "{name} exit 5 reason"
                ),
                _ => unreachable!("blocked exit already checked"),
            }
        }
        other => panic!("{name} invalid decision {other}"),
    }
}

fn assert_no_secret_like_field_names(name: &str, value: &Value) {
    fn walk(name: &str, value: &Value, path: &str) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    let lowered = key.to_ascii_lowercase();
                    assert!(
                        !matches!(
                            lowered.as_str(),
                            "seed_phrase"
                                | "secret_key"
                                | "private_key"
                                | "wallet_secret"
                                | "api_key"
                                | "bearer_token"
                        ),
                        "{name} exposes secret-like field {path}.{key}"
                    );
                    walk(name, child, &format!("{path}.{key}"));
                }
            }
            Value::Array(items) => {
                for (idx, child) in items.iter().enumerate() {
                    walk(name, child, &format!("{path}[{idx}]"));
                }
            }
            _ => {}
        }
    }

    walk(name, value, "$");
}
