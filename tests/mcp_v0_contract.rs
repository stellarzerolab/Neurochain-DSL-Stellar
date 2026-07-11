use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

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

#[test]
fn mcp_v0_fixture_runner_lists_only_safe_default_tools() {
    let output = run_fixture_runner(&["--list"]);
    assert!(
        output.status.success(),
        "runner list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("runner list json");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["mode"], "read_only");

    let fixtures = value["fixtures"].as_array().expect("fixtures array");
    assert_eq!(fixtures.len(), fixture_paths().len());

    for fixture in fixtures {
        let tool = fixture["tool"].as_str().expect("tool string");
        assert!(DEFAULT_TOOLS.contains(&tool), "unexpected tool {tool}");
        assert!(
            !EXCLUDED_TOOLS.contains(&tool),
            "excluded tool {tool} leaked into runner list"
        );
    }
}

#[test]
fn mcp_v0_fixture_runner_returns_no_submit_envelope() {
    let output = run_fixture_runner(&["--fixture", "verify_zk_on_stellar_read_only"]);
    assert!(
        output.status.success(),
        "runner fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("runner fixture json");

    assert_eq!(value["tool"], "verify_zk_on_stellar");
    assert_eq!(value["stellar_verification"], "verified_on_stellar");
    assert_eq!(value["underlying_action_submit_allowed"], false);
    assert_eq!(value["attestation_submitted"], false);
    assert_eq!(value["verification_transaction_submitted"], false);
    assert_eq!(value["nullifier_consumed"], false);
    assert!(value["transaction_hash"].is_null());
    assert_decision_exit_consistency("runner verify fixture", &value);
}

#[test]
fn mcp_v0_fixture_runner_supports_tool_and_scenario_lookup() {
    let output = run_fixture_runner(&[
        "--tool",
        "evaluate_guardrails",
        "--scenario",
        "requires_approval",
    ]);
    assert!(
        output.status.success(),
        "runner tool/scenario failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("runner fixture json");

    assert_eq!(value["tool"], "evaluate_guardrails");
    assert_eq!(value["decision"], "requires_approval");
    assert_eq!(value["underlying_action_submit_allowed"], false);
}

#[test]
fn mcp_v0_fixture_runner_accepts_mcp_style_call_json() {
    let output = run_fixture_runner(&[
        "--call-json",
        r#"{"name":"evaluate_guardrails","arguments":{"scenario":"requires_approval"}}"#,
    ]);
    assert!(
        output.status.success(),
        "runner call-json failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("runner fixture json");

    assert_eq!(value["tool"], "evaluate_guardrails");
    assert_eq!(value["decision"], "requires_approval");
    assert_eq!(value["underlying_action_submit_allowed"], false);
    assert_eq!(value["attestation_submitted"], false);
    assert_eq!(value["verification_transaction_submitted"], false);
}

#[test]
fn mcp_v0_fixture_runner_accepts_fixture_call_json() {
    let output = run_fixture_runner(&[
        "--call-json",
        r#"{"fixture":"get_guardrail_status_verified"}"#,
    ]);
    assert!(
        output.status.success(),
        "runner fixture call-json failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("runner fixture json");

    assert_eq!(value["tool"], "get_guardrail_status");
    assert_eq!(value["stellar_verification"], "verified_on_stellar");
    assert_eq!(value["underlying_action_submit_allowed"], false);
}

#[test]
fn mcp_v0_fixture_runner_rejects_submit_like_fixture_names() {
    let output = run_fixture_runner(&["--fixture", "submit_testnet_attestation"]);
    assert!(
        !output.status.success(),
        "runner accepted a submit-like fixture name"
    );
}

#[test]
fn mcp_v0_fixture_runner_rejects_submit_like_call_json_tools() {
    let output = run_fixture_runner(&[
        "--call-json",
        r#"{"name":"submit_underlying_action","arguments":{}}"#,
    ]);
    assert!(
        !output.status.success(),
        "runner accepted a submit-like call-json tool"
    );
}

#[test]
fn mcp_v0_fixture_runner_rejects_secret_like_call_json_fields() {
    let output = run_fixture_runner(&[
        "--call-json",
        r#"{"name":"evaluate_guardrails","arguments":{"scenario":"approved","seed_phrase":"never-store-this"}}"#,
    ]);
    assert!(
        !output.status.success(),
        "runner accepted a secret-like call-json field"
    );
}

#[test]
fn mcp_v0_stdio_lists_only_safe_tools() {
    let output = run_mcp_stdio(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    assert!(
        output.status.success(),
        "stdio tools/list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdio list json");
    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 1);

    let tools = value["result"]["tools"].as_array().expect("tools array");
    let tool_names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();

    for expected in DEFAULT_TOOLS {
        assert!(
            tool_names.contains(expected),
            "stdio tools/list missing {expected}"
        );
    }

    for excluded in EXCLUDED_TOOLS {
        assert!(
            !tool_names.contains(excluded),
            "stdio tools/list leaked excluded tool {excluded}"
        );
    }
}

#[test]
fn mcp_v0_stdio_initializes_with_read_only_no_submit_capabilities() {
    let output = run_mcp_stdio(
        r#"{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"fixture-harness","version":"0.1.0"}}}"#,
    );
    assert!(
        output.status.success(),
        "stdio initialize failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdio initialize json");
    let result = &value["result"];

    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], "init-1");
    assert_eq!(result["protocolVersion"], "2025-06-18");
    assert_eq!(result["capabilities"]["tools"]["listChanged"], false);
    assert_eq!(
        result["capabilities"]["experimental"]["neurochainNoSubmit"]["mode"],
        "read_only"
    );
    assert_eq!(
        result["capabilities"]["experimental"]["neurochainNoSubmit"]["noSubmit"],
        true
    );
    assert_eq!(
        result["capabilities"]["experimental"]["neurochainNoSubmit"]
            ["underlyingActionSubmitAllowed"],
        false
    );
    assert_eq!(result["serverInfo"]["name"], "neurochain-mcp-v0-stdio");
    assert!(result["instructions"]
        .as_str()
        .expect("initialize instructions")
        .contains("never grant signing"));

    let excluded = result["capabilities"]["experimental"]["neurochainNoSubmit"]["excludedTools"]
        .as_array()
        .expect("excluded tools array");
    for tool in EXCLUDED_TOOLS {
        assert!(
            excluded.iter().any(|value| value == tool),
            "initialize capability missing excluded tool {tool}"
        );
    }
}

#[test]
fn mcp_v0_stdio_negotiates_to_supported_protocol_version() {
    let output = run_mcp_stdio(
        r#"{"jsonrpc":"2.0","id":4,"method":"initialize","params":{"protocolVersion":"2099-01-01","capabilities":{},"clientInfo":{"name":"future-client","version":"1.0.0"}}}"#,
    );
    assert!(output.status.success(), "stdio initialize should serialize");
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdio initialize json");

    assert_eq!(value["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(
        value["result"]["capabilities"]["experimental"]["neurochainNoSubmit"]
            ["underlyingActionSubmitAllowed"],
        false
    );
}

#[test]
fn mcp_v0_stdio_rejects_incomplete_initialize_params() {
    let output = run_mcp_stdio(
        r#"{"jsonrpc":"2.0","id":5,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{}}}"#,
    );
    assert!(
        output.status.success(),
        "stdio initialize error should serialize"
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdio initialize error");

    assert_eq!(value["error"]["code"], -32602);
    assert!(value["error"]["message"]
        .as_str()
        .expect("initialize error message")
        .contains("clientInfo"));
}

#[test]
fn mcp_v0_stdio_calls_fixture_without_submit() {
    let output = run_mcp_stdio(
        r#"{"jsonrpc":"2.0","id":"call-1","method":"tools/call","params":{"name":"evaluate_guardrails","arguments":{"scenario":"requires_approval"}}}"#,
    );
    assert!(
        output.status.success(),
        "stdio tools/call failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdio call json");
    let result = &value["result"];

    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], "call-1");
    assert_eq!(result["tool"], "evaluate_guardrails");
    assert_eq!(result["decision"], "requires_approval");
    assert_eq!(result["underlying_action_submit_allowed"], false);
    assert_eq!(result["attestation_submitted"], false);
    assert_eq!(result["verification_transaction_submitted"], false);
    assert_eq!(result["nullifier_consumed"], false);
    assert!(result["transaction_hash"].is_null());
}

#[test]
fn mcp_v0_stdio_rejects_submit_like_tools() {
    let output = run_mcp_stdio(
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"submit_underlying_action","arguments":{}}}"#,
    );
    assert!(
        output.status.success(),
        "stdio error response should still serialize successfully"
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdio error json");

    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 2);
    assert_eq!(value["error"]["code"], -32000);
    assert!(
        value["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("excluded from default MCP v0"),
        "unexpected error: {value}"
    );
}

#[test]
fn mcp_v0_stdio_rejects_secret_like_arguments() {
    let output = run_mcp_stdio(
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"evaluate_guardrails","arguments":{"scenario":"approved","api_key":null}}}"#,
    );
    assert!(
        output.status.success(),
        "stdio error response should still serialize successfully"
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdio error json");

    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 3);
    assert_eq!(value["error"]["code"], -32000);
    assert!(
        value["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("secret-like field"),
        "unexpected error: {value}"
    );
}

fn run_fixture_runner(args: &[&str]) -> Output {
    Command::new(assert_cmd::cargo::cargo_bin!(
        "neurochain-mcp-v0-fixture-runner"
    ))
    .args(args)
    .output()
    .expect("run fixture runner")
}

fn run_mcp_stdio(request: &str) -> Output {
    let mut child = Command::new(assert_cmd::cargo::cargo_bin!("neurochain-mcp-v0-stdio"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn stdio shim");
    child
        .stdin
        .as_mut()
        .expect("stdio shim stdin")
        .write_all(request.as_bytes())
        .expect("write stdio request");
    child.wait_with_output().expect("stdio shim output")
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
