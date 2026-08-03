use neurochain::actions::ActionPlan;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

const FIXTURE_DIR: &str = "examples/mcp_v0_no_submit_contract";
const STDIO_CLIENT_DIR: &str = "examples/mcp_v0_stdio_client";
const GUARDRAILS_SKILL_DIR: &str = "skills/neurochain-stellar-guardrails";
const PLAN_HASH_DOMAIN: &[u8] = b"neurochain:mcp-v0:action-plan-json:v1\0";

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
    let value = run_ready_mcp_stdio(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
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

    for tool in tools {
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(tool["annotations"]["destructiveHint"], false);
        assert_eq!(tool["annotations"]["idempotentHint"], true);
        assert_eq!(tool["annotations"]["openWorldHint"], false);
    }

    for excluded in EXCLUDED_TOOLS {
        assert!(
            !tool_names.contains(excluded),
            "stdio tools/list leaked excluded tool {excluded}"
        );
    }
}

#[test]
fn mcp_v0_stdio_advertises_real_plan_runtime_input() {
    let value = run_ready_mcp_stdio(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    let tools = value["result"]["tools"].as_array().expect("tools array");
    let plan_tool = tools
        .iter()
        .find(|tool| tool["name"] == "plan_stellar_action")
        .expect("plan tool");
    let schema = &plan_tool["inputSchema"];

    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["properties"]["network"]["enum"],
        serde_json::json!(["testnet"])
    );
    assert_eq!(
        schema["properties"]["plan_mode"]["enum"],
        serde_json::json!(["preview_only"])
    );
    assert!(schema["properties"].get("intent_text").is_some());
    assert!(schema["properties"].get("source_hint").is_some());
    assert!(schema["properties"].get("secret_key").is_none());
}

#[test]
fn mcp_v0_stdio_advertises_real_guardrail_runtime_input() {
    let value = run_ready_mcp_stdio(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    let tools = value["result"]["tools"].as_array().expect("tools array");
    let evaluate_tool = tools
        .iter()
        .find(|tool| tool["name"] == "evaluate_guardrails")
        .expect("evaluate tool");
    let schema = &evaluate_tool["inputSchema"];

    assert_eq!(schema["additionalProperties"], false);
    assert!(schema["properties"].get("action_plan").is_some());
    assert_eq!(
        schema["properties"]["policy_ref"]["enum"],
        serde_json::json!(["configured"])
    );
    assert_eq!(
        schema["properties"]["evaluation_mode"]["enum"],
        serde_json::json!(["deterministic"])
    );
    assert_eq!(
        schema["properties"]["action_plan_hash"]["pattern"],
        "^[0-9a-fA-F]{64}$"
    );
    assert!(schema["properties"].get("allowlist_enforce").is_none());
    assert!(schema["properties"].get("contract_policy").is_none());
}

#[test]
fn mcp_v0_stdio_advertises_bounded_inline_proof_inspection() {
    let value = run_ready_mcp_stdio(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    let tools = value["result"]["tools"].as_array().expect("tools array");
    let prove_tool = tools
        .iter()
        .find(|tool| tool["name"] == "prove_guardrail_decision")
        .expect("prove tool");
    let schema = &prove_tool["inputSchema"];

    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["properties"]["proof_mode"]["enum"],
        serde_json::json!(["inspect_public_artifact"])
    );
    assert_eq!(schema["properties"]["proof"]["additionalProperties"], false);
    assert_eq!(
        schema["properties"]["proof"]["properties"]["journal_digest_hex"]["pattern"],
        "^[0-9a-fA-F]{64}$"
    );
    assert!(schema["properties"].get("proof_path").is_none());
    assert!(schema["properties"].get("private_policy").is_none());
    assert!(schema["properties"]["proof"]["properties"]
        .get("policy")
        .is_none());
}

#[test]
fn mcp_v0_stdio_advertises_read_only_stellar_verify_input() {
    let value = run_ready_mcp_stdio(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    let tools = value["result"]["tools"].as_array().expect("tools array");
    let verify_tool = tools
        .iter()
        .find(|tool| tool["name"] == "verify_zk_on_stellar")
        .expect("verify tool");
    let schema = &verify_tool["inputSchema"];

    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["properties"]["network"]["enum"],
        serde_json::json!(["testnet"])
    );
    assert_eq!(
        schema["properties"]["verification_mode"]["enum"],
        serde_json::json!(["read_only"])
    );
    assert_eq!(schema["properties"]["proof"]["additionalProperties"], false);
    assert!(schema["properties"].get("proof_artifact_ref").is_none());
    assert!(schema["properties"].get("source").is_none());
    assert!(schema["properties"].get("source_hint").is_none());
    assert!(schema["properties"].get("secret_key").is_none());
    assert!(schema["properties"].get("private_policy").is_none());
}

#[test]
fn mcp_v0_stdio_advertises_observational_status_input() {
    let value = run_ready_mcp_stdio(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    let tools = value["result"]["tools"].as_array().expect("tools array");
    let status_tool = tools
        .iter()
        .find(|tool| tool["name"] == "get_guardrail_status")
        .expect("status tool");
    let schema = &status_tool["inputSchema"];

    assert_eq!(schema["additionalProperties"], false);
    assert!(schema["properties"].get("latest_result").is_some());
    assert!(schema["properties"].get("session_id").is_some());
    assert!(schema["properties"].get("proof_artifact_ref").is_some());
    assert!(schema["properties"].get("source").is_none());
    assert!(schema["properties"].get("source_hint").is_none());
    assert!(schema["properties"].get("secret_key").is_none());
    assert!(schema["properties"].get("private_policy").is_none());
}

#[test]
fn mcp_v0_stdio_rejects_tools_before_session_is_ready() {
    let output = run_mcp_stdio(r#"{"jsonrpc":"2.0","id":6,"method":"tools/list"}"#);
    assert!(
        output.status.success(),
        "stdio readiness error should serialize"
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdio readiness error");

    assert_eq!(value["id"], 6);
    assert_eq!(value["error"]["code"], -32002);
    assert!(value["error"]["message"]
        .as_str()
        .expect("readiness error message")
        .contains("notifications/initialized"));
}

#[test]
fn mcp_v0_stdio_runs_persistent_initialized_session() {
    let output = run_mcp_stdio_session(&[
        MCP_INIT_REQUEST,
        MCP_INITIALIZED_NOTIFICATION,
        r#"{"jsonrpc":"2.0","id":"list-1","method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":"call-2","method":"tools/call","params":{"name":"evaluate_guardrails","arguments":{"scenario":"approved"}}}"#,
    ]);
    assert!(
        output.status.success(),
        "stdio session failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = parse_mcp_stdio_responses(&output);

    assert_eq!(responses.len(), 3, "notification must not emit response");
    assert_eq!(responses[0]["id"], "init-1");
    assert_eq!(responses[1]["id"], "list-1");
    assert_eq!(responses[2]["id"], "call-2");
    assert_eq!(
        responses[2]["result"]["structuredContent"]["decision"],
        "approved"
    );
    assert_eq!(
        responses[2]["result"]["structuredContent"]["underlying_action_submit_allowed"],
        false
    );
    assert_eq!(responses[2]["result"]["isError"], false);
}

#[test]
fn mcp_v0_stdio_client_examples_preserve_safe_host_configuration() {
    let config_path = Path::new(STDIO_CLIENT_DIR).join("mcp_servers.json.example");
    assert_safe_host_config(
        &config_path,
        "neurochain-mcp-v0-stdio",
        "models/intent_stellar/model.onnx",
    );

    let windows_config_path = Path::new(STDIO_CLIENT_DIR).join("mcp_servers.windows.json.example");
    assert_safe_host_config(
        &windows_config_path,
        "neurochain-mcp-v0-stdio.exe",
        "models\\intent_stellar\\model.onnx",
    );

    let session_path = Path::new(STDIO_CLIENT_DIR).join("session.jsonl");
    let session = fs::read_to_string(session_path).expect("read MCP session example");
    let messages = session
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).expect("session JSON-RPC line"))
        .collect::<Vec<_>>();

    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0]["method"], "initialize");
    assert_eq!(messages[1]["method"], "notifications/initialized");
    assert!(messages[1].get("id").is_none());
    assert_eq!(messages[2]["method"], "tools/list");
    assert_eq!(messages[3]["method"], "tools/call");
    assert_eq!(messages[3]["params"]["name"], "evaluate_guardrails");
    assert_eq!(
        messages[3]["params"]["arguments"]["scenario"],
        "requires_approval"
    );
}

#[test]
fn mcp_v0_release_gate_validates_generated_host_config_launch() {
    let script_path = Path::new("scripts").join("verify_mcp_v0_release.ps1");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", script_path.display()));

    for required in [
        "HostConfigOut",
        "validated_by_launch = $true",
        "Get-Content -LiteralPath $ResolvedHostConfigOut -Raw",
        "$ParsedHostConfig.mcpServers.\"neurochain-stellar-guardrails\"",
        "$ClientPath --server $ServerConfig.command",
        "submit_testnet_attestation|consume_nullifier|submit_underlying_action|sign_transaction",
        "NC_STELLAR_SOURCE|NC_SOROBAN_SOURCE|SECRET|SEED|PRIVATE|API_KEY|TOKEN",
    ] {
        assert!(
            script.contains(required),
            "release gate must validate generated host config launch: {required}"
        );
    }
}

#[test]
fn mcp_v0_stdio_client_conformance_session_covers_fail_closed_cases() {
    let session_path = Path::new(STDIO_CLIENT_DIR).join("conformance_session.jsonl");
    let session = fs::read_to_string(session_path).expect("read MCP conformance session");
    let messages = session
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).expect("conformance JSON-RPC line"))
        .collect::<Vec<_>>();

    assert_eq!(messages.len(), 9);
    assert_eq!(messages[0]["method"], "initialize");
    assert_eq!(messages[1]["method"], "notifications/initialized");
    assert_eq!(messages[2]["method"], "notifications/progress");
    assert!(messages[1].get("id").is_none());
    assert!(messages[2].get("id").is_none());
    assert_eq!(messages[5]["params"]["name"], "submit_underlying_action");
    assert!(messages[6]["params"]["arguments"].get("api_key").is_some());
    assert_eq!(messages[7]["method"], "resources/list");
    assert!(messages[8].get("params").is_none());
}

#[test]
fn neurochain_guardrails_skill_documents_runtime_mcp_sequence() {
    let skill_path = Path::new(GUARDRAILS_SKILL_DIR).join("SKILL.md");
    let skill = fs::read_to_string(&skill_path).unwrap_or_else(|err| {
        panic!("read {}: {err}", skill_path.display());
    });

    let expected_order = [
        "plan_stellar_action",
        "evaluate_guardrails",
        "prove_guardrail_decision",
        "verify_zk_on_stellar",
        "get_guardrail_status",
    ];
    let table_start = skill
        .find("## MCP V0 Tools")
        .expect("skill should document MCP v0 tools section");
    let tool_table = &skill[table_start..];
    let mut last_index = 0;
    for tool in expected_order {
        let index = tool_table
            .find(tool)
            .unwrap_or_else(|| panic!("skill must document {tool}"));
        assert!(
            index >= last_index,
            "skill should document MCP tools in Plan -> Evaluate -> Prove -> Verify -> Status order"
        );
        last_index = index;
    }

    for required in [
        "latest_result",
        "structuredContent",
        "read_only",
        "underlying_action_submit_allowed: false",
        "Do not invent missing ZK evidence",
    ] {
        assert!(
            skill.contains(required),
            "skill must document host/runtime boundary: {required}"
        );
    }

    for excluded in EXCLUDED_TOOLS {
        assert!(
            skill.contains(excluded),
            "skill must name excluded submit/stateful tool {excluded}"
        );
    }

    let openai_path = Path::new(GUARDRAILS_SKILL_DIR)
        .join("agents")
        .join("openai.yaml");
    let openai = fs::read_to_string(&openai_path).unwrap_or_else(|err| {
        panic!("read {}: {err}", openai_path.display());
    });
    assert!(openai.contains("NeuroChain Stellar Guardrails"));
    assert!(openai.contains("without granting submit permission"));
}

#[test]
fn neurochain_guardrails_skill_packaging_stays_separate_from_runtime() {
    let packaging_path = Path::new(GUARDRAILS_SKILL_DIR).join("PACKAGING.md");
    let packaging = fs::read_to_string(&packaging_path).unwrap_or_else(|err| {
        panic!("read {}: {err}", packaging_path.display());
    });

    for required in [
        "Packaging is Phase 2 work",
        "Do not use this checklist to claim",
        "not:",
        "a NeuroChain runtime dependency",
        "MCP v0 release gate passes with `validated_by_launch=true`",
        "The skill lists only the five default read-only MCP v0 tools",
        "Every example preserves `underlying_action_submit_allowed=false`",
        "Raven is mentioned only as development-time context",
        "No wallet secret, seed phrase, API key, private key",
    ] {
        assert!(
            packaging.contains(required),
            "skill packaging checklist must preserve boundary: {required}"
        );
    }

    for tool in DEFAULT_TOOLS {
        assert!(
            packaging.contains(tool),
            "skill packaging checklist must name default tool {tool}"
        );
    }
    for excluded in EXCLUDED_TOOLS {
        assert!(
            packaging.contains(excluded),
            "skill packaging checklist must exclude submit/stateful tool {excluded}"
        );
    }
}

#[test]
fn neurochain_guardrails_skill_examples_cover_terminal_states() {
    let examples_dir = Path::new(GUARDRAILS_SKILL_DIR).join("examples");
    let examples = [
        ("approved.md", "decision: `approved`"),
        ("requires_approval.md", "decision: `requires_approval`"),
        ("blocked.md", "decision: `blocked`"),
        ("state_unavailable.md", "status: `state_unavailable`"),
    ];

    for (file_name, marker) in examples {
        let path = examples_dir.join(file_name);
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        assert!(
            content.contains(marker),
            "{} must document its state marker {marker}",
            path.display()
        );
        for required in [
            "underlying action submit allowed: `false`",
            "nullifier consumed: `false`",
        ] {
            assert!(
                content.contains(required),
                "{} must preserve no-submit state: {required}",
                path.display()
            );
        }
    }

    let readme_path = examples_dir.join("README.md");
    let readme = fs::read_to_string(&readme_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", readme_path.display()));
    for required in [
        "underlying_action_submit_allowed",
        "attestation_submitted",
        "verification_transaction_submitted",
        "nullifier_consumed",
        "Plan -> Evaluate -> Prove -> Verify -> Status -> no automatic submit",
    ] {
        assert!(
            readme.contains(required),
            "skill examples README must preserve shared boundary: {required}"
        );
    }

    let blocked =
        fs::read_to_string(examples_dir.join("blocked.md")).expect("read blocked example");
    for exit_code in [
        "`3`: allowlist block",
        "`4`: contract policy",
        "`5`: missing input",
    ] {
        assert!(
            blocked.contains(exit_code),
            "blocked example must preserve exit semantics: {exit_code}"
        );
    }

    let state_unavailable =
        fs::read_to_string(examples_dir.join("state_unavailable.md")).expect("read unavailable");
    for forbidden in [
        "wallet",
        "secrets",
        "signing",
        "submit",
        "attestation",
        "nullifier-consume",
    ] {
        assert!(
            state_unavailable.contains(forbidden),
            "state_unavailable example must reject unsafe recovery path: {forbidden}"
        );
    }
}

#[test]
fn neurochain_guardrails_skill_install_note_matches_host_boundary() {
    let install_path = Path::new(GUARDRAILS_SKILL_DIR).join("INSTALL.md");
    let install = fs::read_to_string(&install_path).unwrap_or_else(|err| {
        panic!("read {}: {err}", install_path.display());
    });

    for required in [
        "verify_mcp_v0_release.ps1",
        "-HostConfigOut",
        "validated_by_launch = true",
        "secrets_included = false",
        "submit_tools_included = false",
        "examples/mcp_v0_stdio_client/mcp_servers.json.example",
        "examples/mcp_v0_stdio_client/mcp_servers.windows.json.example",
        "NC_INTENT_STELLAR_MODEL",
        "Plan -> Evaluate -> Prove -> Verify -> Status -> no automatic submit",
        "`latest_result`",
        "Those remain separate product surfaces",
    ] {
        assert!(
            install.contains(required),
            "skill install note must document host/no-submit boundary: {required}"
        );
    }

    for tool in DEFAULT_TOOLS {
        assert!(
            install.contains(tool),
            "skill install note must name default tool {tool}"
        );
    }
    for excluded in EXCLUDED_TOOLS {
        assert!(
            install.contains(excluded),
            "skill install note must exclude submit/stateful tool {excluded}"
        );
    }
    for forbidden_guidance in [
        "NC_STELLAR_SOURCE=",
        "NC_SOROBAN_SOURCE=",
        "--flow",
        "--yes",
        "submit permission",
    ] {
        assert!(
            !install.contains(forbidden_guidance),
            "skill install note must not suggest unsafe default guidance: {forbidden_guidance}"
        );
    }

    let packaging = fs::read_to_string(Path::new(GUARDRAILS_SKILL_DIR).join("PACKAGING.md"))
        .expect("read packaging");
    assert!(packaging.contains("`INSTALL.md`"));
}

#[test]
fn neurochain_guardrails_skill_release_candidate_manifest_is_bounded() {
    let manifest_path = Path::new(GUARDRAILS_SKILL_DIR).join("RELEASE_CANDIDATE.md");
    let manifest = fs::read_to_string(&manifest_path).unwrap_or_else(|err| {
        panic!("read {}: {err}", manifest_path.display());
    });

    for required in [
        "internal_release_candidate = true",
        "published = false",
        "runtime_dependency = false",
        "submit_surface = false",
        "validated_by_launch=true",
        "mode = read_only_no_submit",
        "secrets_included = false",
        "submit_tools_included = false",
        "verify_guardrails_skill_release_candidate.ps1",
        "verify_guardrails_skill_package.ps1",
        "release_candidate = true",
        "Plan -> Evaluate -> Prove -> Verify -> Status -> no automatic submit",
        "ZK is beyond a lite demo",
        "x402 is beyond a lite UI idea",
        "x402 is not production until real facilitator settlement is implemented",
        "never imply underlying ActionPlan submit permission",
    ] {
        assert!(
            manifest.contains(required),
            "release candidate manifest must preserve bounded claim: {required}"
        );
    }

    for package_file in [
        "SKILL.md",
        "PACKAGING.md",
        "INSTALL.md",
        "RELEASE_CANDIDATE.md",
        "agents/openai.yaml",
        "examples/approved.md",
        "examples/requires_approval.md",
        "examples/blocked.md",
        "examples/state_unavailable.md",
    ] {
        assert!(
            manifest.contains(package_file),
            "release candidate manifest must list package file {package_file}"
        );
    }

    for tool in DEFAULT_TOOLS {
        assert!(
            manifest.contains(tool),
            "release candidate manifest must name default tool {tool}"
        );
    }
    for excluded in EXCLUDED_TOOLS {
        assert!(
            manifest.contains(excluded),
            "release candidate manifest must exclude submit/stateful tool {excluded}"
        );
    }

    let packaging = fs::read_to_string(Path::new(GUARDRAILS_SKILL_DIR).join("PACKAGING.md"))
        .expect("read packaging");
    assert!(packaging.contains("`RELEASE_CANDIDATE.md`"));
}

#[test]
fn neurochain_guardrails_skill_package_check_is_bounded() {
    let script_path = Path::new("scripts").join("verify_guardrails_skill_package.ps1");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", script_path.display()));

    for required in [
        "requiredFiles",
        "defaultTools",
        "excludedTools",
        "requiredPhrases",
        "forbiddenPatterns",
        "runtime_dependency",
        "submit_surface",
        "secrets_included",
        "ConvertTo-Json",
    ] {
        assert!(
            script.contains(required),
            "skill package check must preserve bounded publish gate: {required}"
        );
    }

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            script_path.to_str().expect("script path"),
        ])
        .output()
        .expect("run skill package check");
    assert!(
        output.status.success(),
        "skill package check failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("skill check JSON");
    assert_eq!(value["status"], "passed");
    assert_eq!(value["runtime_dependency"], false);
    assert_eq!(value["submit_surface"], false);
    assert_eq!(value["secrets_included"], false);
}

#[test]
fn neurochain_guardrails_skill_release_candidate_gate_orchestrates_release_and_package_checks() {
    let script_path = Path::new("scripts").join("verify_guardrails_skill_release_candidate.ps1");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", script_path.display()));

    for required in [
        "verify_mcp_v0_release.ps1",
        "verify_guardrails_skill_package.ps1",
        "validated_by_launch",
        "read_only_no_submit",
        "release_candidate",
        "published",
        "runtime_dependency",
        "submit_surface",
        "secrets_included",
        "submit_tools_included",
        "ConvertTo-Json",
    ] {
        assert!(
            script.contains(required),
            "combined skill release candidate gate must preserve evidence field: {required}"
        );
    }

    for forbidden in [
        "submit_underlying_action",
        "sign_transaction",
        "consume_nullifier",
        "submit_testnet_attestation",
    ] {
        assert!(
            !script.contains(forbidden),
            "combined release candidate gate must not call submit/stateful surface: {forbidden}"
        );
    }
}

#[test]
fn zk_package_docs_explain_fresh_clone_manifest_path_build() {
    let root_readme = fs::read_to_string("README.md").expect("read root README");
    let zk_readme =
        fs::read_to_string("hackathons/stellar-real-world-zk/README.md").expect("read ZK README");

    for (name, content) in [
        ("root README", root_readme.as_str()),
        ("ZK README", zk_readme.as_str()),
    ] {
        assert!(
            content.contains("repository root is the main NeuroChain CLI crate"),
            "{name} must explain why a fresh clone should not start ZK evidence with root cargo build"
        );
        assert!(
            content.contains(
                "cargo test --release --manifest-path hackathons/stellar-real-world-zk/soroban/Cargo.toml"
            ),
            "{name} must provide the direct Soroban manifest-path command"
        );
    }
}

#[test]
fn x402_phase3_contract_preserves_paid_ingress_boundary() {
    let phase3_path = Path::new("docs").join("x402_facilitator_phase3.md");
    let phase3 = fs::read_to_string(&phase3_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", phase3_path.display()));

    for required in [
        "x402 is already beyond a lite UI idea",
        "This is not production x402 yet",
        "Real facilitator verify/settle transport behind `src/x402_facilitator.rs`",
        "`payment finalized` is not guardrail approval",
        "`payment finalized` is not a ZK proof",
        "`payment finalized` is not `underlying_action_submit_allowed`",
        "payment is not submit permission",
        "`NC_X402_STELLAR_VERIFIER=mock` fails closed in production runtimes",
        "`NC_X402_STELLAR_VERIFIER=facilitator` fails closed after verify until real",
        "underlying_action_submit_allowed=false",
        "requires_approval",
        "blocked",
        "invalid_payment",
    ] {
        assert!(
            phase3.contains(required),
            "x402 Phase 3 contract must preserve boundary: {required}"
        );
    }

    for forbidden in [
        "payment finalized is guardrail approval",
        "payment finalized is a ZK proof",
        "payment finalized is `underlying_action_submit_allowed`",
        "payment is submit permission",
    ] {
        assert!(
            !phase3.contains(forbidden),
            "x402 Phase 3 contract must not imply unsafe shortcut: {forbidden}"
        );
    }

    let product_finish =
        fs::read_to_string("docs/mcp_v0_product_finish.md").expect("read MCP product finish doc");
    assert!(
        product_finish.contains("docs/x402_facilitator_phase3.md"),
        "MCP product finish doc should link the detailed x402 Phase 3 contract"
    );
}

#[test]
fn x402_facilitator_adapter_contract_separates_payment_from_action_submit() {
    let root = Path::new("examples").join("x402_facilitator_adapter");
    let schema: Value = serde_json::from_str(
        &fs::read_to_string(root.join("schema.json")).expect("read facilitator adapter schema"),
    )
    .expect("parse facilitator adapter schema");

    assert_eq!(
        schema["properties"]["operation"]["enum"],
        serde_json::json!(["verify", "settle"])
    );
    assert_eq!(
        schema["properties"]["underlying_action_submit_allowed"]["const"],
        false
    );
    assert!(schema["required"]
        .as_array()
        .expect("required array")
        .contains(&Value::String("idempotency_key".to_string())));

    for fixture in [
        "verify_valid.json",
        "verify_rejected.json",
        "verify_unavailable.json",
        "settle_success.json",
    ] {
        let value: Value = serde_json::from_str(
            &fs::read_to_string(root.join(fixture))
                .unwrap_or_else(|err| panic!("read {fixture}: {err}")),
        )
        .unwrap_or_else(|err| panic!("parse {fixture}: {err}"));

        assert_eq!(value["underlying_action_submit_allowed"], false);
        assert!(!value["idempotency_key"].as_str().unwrap_or("").is_empty());
        assert!(value["payment_payload"].is_object());
        assert!(value["payment_requirements"].is_object());
    }

    let verify: Value =
        serde_json::from_str(&fs::read_to_string(root.join("verify_valid.json")).unwrap()).unwrap();
    assert_eq!(verify["operation"], "verify");
    assert_eq!(verify["outcome"], "verified");
    assert_eq!(verify["verification"]["is_valid"], true);
    assert!(verify.get("settlement").is_none());

    let settle: Value =
        serde_json::from_str(&fs::read_to_string(root.join("settle_success.json")).unwrap())
            .unwrap();
    assert_eq!(settle["operation"], "settle");
    assert_eq!(settle["outcome"], "settled");
    assert_eq!(settle["verification"]["is_valid"], true);
    assert_eq!(settle["settlement"]["success"], true);

    for (fixture, outcome, reason) in [
        ("verify_rejected.json", "rejected", "facilitator_rejected"),
        (
            "verify_unavailable.json",
            "unavailable",
            "facilitator_unavailable",
        ),
    ] {
        let value: Value =
            serde_json::from_str(&fs::read_to_string(root.join(fixture)).unwrap()).unwrap();
        assert_eq!(value["operation"], "verify");
        assert_eq!(value["outcome"], outcome);
        assert_eq!(value["verification"]["is_valid"], false);
        assert_eq!(value["verification"]["invalid_reason"], reason);
        assert!(value.get("settlement").is_none());
        assert_eq!(value["underlying_action_submit_allowed"], false);
    }

    let phase3 = fs::read_to_string("docs/x402_facilitator_phase3.md")
        .expect("read x402 facilitator phase 3 doc");
    assert!(phase3.contains("examples/x402_facilitator_adapter/schema.json"));
    for required in [
        "X402FacilitatorVerifyOnlyAdapter",
        "It intentionally has no settle method",
        "FacilitatorX402PaymentVerifier",
        "payment_verified_settlement_required",
    ] {
        assert!(
            phase3.contains(required),
            "verify-only adapter boundary must remain explicit: {required}"
        );
    }
}

#[test]
fn x402_facilitator_state_transitions_fail_closed_on_replay_and_unknown_state() {
    let path = Path::new("examples")
        .join("x402_facilitator_adapter")
        .join("state_transitions.json");
    let matrix: Value = serde_json::from_str(
        &fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display())),
    )
    .expect("parse facilitator state transitions");
    let transitions = matrix["transitions"].as_array().expect("transition array");

    assert_eq!(matrix["idempotency_scope"], "resource_request");
    assert_eq!(matrix["unknown_state_policy"]["outcome"], "unavailable");
    assert_eq!(matrix["unknown_state_policy"]["settlement_allowed"], false);
    assert_eq!(
        matrix["unknown_state_policy"]["underlying_action_submit_allowed"],
        false
    );

    for transition in transitions {
        assert_eq!(
            transition["underlying_action_submit_allowed"], false,
            "no payment transition may grant ActionPlan submit authority"
        );
    }

    let settle_entry = transitions
        .iter()
        .find(|transition| {
            transition["from"] == "verified" && transition["event"] == "settle_success"
        })
        .expect("verified settle transition");
    assert_eq!(settle_entry["to"], "settled");

    let replay_entry = transitions
        .iter()
        .find(|transition| {
            transition["from"] == "settled" && transition["event"] == "repeat_same_idempotency_key"
        })
        .expect("settled replay transition");
    assert_eq!(replay_entry["to"], "replay_blocked");
    assert_eq!(replay_entry["settlement_allowed"], false);

    for terminal in ["rejected", "unavailable", "expired"] {
        let entry = transitions
            .iter()
            .find(|transition| {
                transition["from"] == terminal
                    && transition["event"] == "repeat_same_idempotency_key"
            })
            .unwrap_or_else(|| panic!("missing terminal replay transition for {terminal}"));
        assert_eq!(entry["to"], terminal);
        assert_eq!(entry["settlement_allowed"], false);
    }
}

#[test]
fn root_readme_summarizes_mcp_skill_zk_and_x402_release_boundaries() {
    let readme = fs::read_to_string("README.md").expect("read root README");

    for required in [
        "## MCP And Skill Release Status",
        "verify_guardrails_skill_release_candidate.ps1",
        "release_candidate=true",
        "published=false",
        "runtime_dependency=false",
        "submit_surface=false",
        "mode=read_only_no_submit",
        "validated_by_launch=true",
        "ZK is beyond a lite demo",
        "x402 is beyond a lite UI idea",
        "x402 is not production until real facilitator settlement is implemented",
        "Payment is not guardrail approval",
        "docs/x402_facilitator_phase3.md",
        "docs/mcp_skill_completion_audit.md",
    ] {
        assert!(
            readme.contains(required),
            "root README must summarize current release boundary: {required}"
        );
    }

    for forbidden in [
        "The current implementation is x402-lite, not a full x402/MPP stack",
        "Payment is submit permission",
        "proof is submit permission",
    ] {
        assert!(
            !readme.contains(forbidden),
            "root README must not keep stale or unsafe wording: {forbidden}"
        );
    }
}

#[test]
fn mcp_skill_completion_audit_covers_requested_last_mile_scope() {
    let audit_path = Path::new("docs").join("mcp_skill_completion_audit.md");
    let audit = fs::read_to_string(&audit_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", audit_path.display()));

    for required in [
        "finish the remaining MCP/Skills packaging work",
        "keep publishable skill work in",
        "clearly state whether x402 and ZK are beyond lite",
        "MCP v0 is a real product package",
        "Default MCP remains no-submit",
        "Skill publication/packaging is separate from runtime",
        "Skill has release-candidate evidence",
        "Skill is not falsely claimed as published",
        "ZK status is clear",
        "x402 status is clear",
        "Payment/proof cannot become submit permission",
        "status = passed",
        "published = false",
        "release_candidate = true",
        "runtime_dependency = false",
        "submit_surface = false",
        "mode = read_only_no_submit",
        "validated_by_launch = true",
        "conformance_cases = 7",
        "publishing the skill to a specific external registry",
        "real x402 facilitator settlement transport",
        "external MCP host or MCP Inspector validation",
    ] {
        assert!(
            audit.contains(required),
            "completion audit must cover last-mile requirement or boundary: {required}"
        );
    }

    for forbidden in [
        "Complete enough maybe",
        "payment is submit permission",
        "proof is submit permission",
        "published = true",
    ] {
        assert!(
            !audit.contains(forbidden),
            "completion audit must not overclaim or weaken boundary: {forbidden}"
        );
    }
}

#[test]
fn external_mcp_host_readiness_reports_the_exact_unvalidated_port() {
    let script_path = Path::new("scripts").join("check_mcp_external_host_readiness.ps1");
    let docs_path = Path::new("docs").join("mcp_external_host_validation.md");
    let script = fs::read_to_string(&script_path).expect("read external host readiness script");
    let docs = fs::read_to_string(&docs_path).expect("read external host validation docs");

    for required in [
        "external_host_available",
        "external_mcp_host_or_inspector_executable",
        "installation_attempted",
        "runtime_dependency_added",
        "submit_surface_added",
        "verify_mcp_v0_release.ps1",
    ] {
        assert!(
            script.contains(required),
            "host readiness script must preserve field or boundary: {required}"
        );
        assert!(
            docs.contains(required),
            "host validation docs must preserve field or boundary: {required}"
        );
    }

    for forbidden in [
        "npm install",
        "npx -y",
        "submit_testnet_attestation",
        "consume_nullifier",
        "submit_underlying_action",
        "sign_transaction",
    ] {
        assert!(
            !script.contains(forbidden),
            "host readiness script must not install or expose stateful surface: {forbidden}"
        );
    }

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            script_path.to_str().expect("script path"),
            "-InspectorCommand",
            "__neurochain_missing_mcp_inspector__",
        ])
        .output()
        .expect("run external host readiness script");
    assert!(
        output.status.success(),
        "host readiness script should report unavailable host without failing: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let result: Value = serde_json::from_slice(&output.stdout).expect("parse host readiness JSON");
    assert_eq!(result["status"], "host_unavailable");
    assert_eq!(result["external_host_available"], false);
    assert_eq!(
        result["missing_port"],
        "external_mcp_host_or_inspector_executable"
    );
    assert_eq!(result["installation_attempted"], false);
    assert_eq!(result["runtime_dependency_added"], false);
    assert_eq!(result["submit_surface_added"], false);
}

#[test]
fn stellar_skills_community_card_is_publish_ready_but_unpublished() {
    let card_path = Path::new("distribution").join("stellar-skills-community-card.json");
    let review_path = Path::new("docs").join("stellar_skills_publish_review.md");
    let packaging_path = Path::new(GUARDRAILS_SKILL_DIR).join("PACKAGING.md");

    let card = read_json(&card_path);
    let review = fs::read_to_string(&review_path).expect("read Stellar Skills review");
    let packaging = fs::read_to_string(&packaging_path).expect("read skill packaging");

    assert_eq!(card["title"], "NeuroChain Stellar Guardrails");
    assert_eq!(card["pathLabel"], "stellarzerolab/Neurochain-DSL-Stellar");
    assert_eq!(
        card["copyValue"],
        "https://github.com/stellarzerolab/Neurochain-DSL-Stellar/blob/main/skills/neurochain-stellar-guardrails/SKILL.md"
    );

    let description = card["description"]
        .as_str()
        .expect("community card description");
    assert!(
        description.starts_with("Route "),
        "community card description must be verb-led"
    );
    for required in [
        "Stellar ActionPlans",
        "deterministic guardrails",
        "private-policy ZK evidence",
        "read-only Soroban verification",
        "without granting transaction submit permission",
    ] {
        assert!(
            description.contains(required),
            "community card description must preserve bounded claim: {required}"
        );
    }

    for required in [
        "distribution_channel = skills.stellar.org community skills",
        "publish_candidate = true",
        "published = false",
        "external_pull_request_created = false",
        "runtime_dependency = false",
        "submit_surface = false",
        "ECOSYSTEM_CARDS",
        "separate explicit publication decision",
    ] {
        assert!(
            review.contains(required),
            "publish review must preserve channel boundary: {required}"
        );
    }

    for required in [
        "docs/stellar_skills_publish_review.md",
        "distribution/stellar-skills-community-card.json",
        "published=false",
        "explicit external-publication approval",
    ] {
        assert!(
            packaging.contains(required),
            "skill packaging must link channel review boundary: {required}"
        );
    }
}

#[test]
fn mcp_v0_client_smoke_validates_real_stdio_process() {
    let output = Command::new(assert_cmd::cargo::cargo_bin!(
        "neurochain-mcp-v0-client-smoke"
    ))
    .arg("--server")
    .arg(assert_cmd::cargo::cargo_bin!("neurochain-mcp-v0-stdio"))
    .output()
    .expect("run MCP v0 client smoke");

    assert!(
        output.status.success(),
        "client smoke failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Value = serde_json::from_slice(&output.stdout).expect("client smoke summary JSON");

    assert_eq!(summary["status"], "passed");
    assert_eq!(summary["transport"], "stdio");
    assert_eq!(summary["conformance_cases"], 7);
    assert_eq!(summary["protocol_version"], "2025-06-18");
    assert_eq!(summary["sample_decision"], "requires_approval");
    assert_eq!(summary["underlying_action_submit_allowed"], false);
    assert_eq!(summary["attestation_submitted"], false);
    assert_eq!(summary["verification_transaction_submitted"], false);
    assert_eq!(summary["nullifier_consumed"], false);

    let tools = summary["tools"].as_array().expect("summary tools");
    for tool in DEFAULT_TOOLS {
        assert!(tools.iter().any(|value| value == tool));
    }
    for excluded in EXCLUDED_TOOLS {
        assert!(!tools.iter().any(|value| value == excluded));
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
    let value = run_ready_mcp_stdio(
        r#"{"jsonrpc":"2.0","id":"call-1","method":"tools/call","params":{"name":"evaluate_guardrails","arguments":{"scenario":"requires_approval"}}}"#,
    );
    let result = &value["result"]["structuredContent"];

    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], "call-1");
    assert_eq!(value["result"]["isError"], false);
    let content = value["result"]["content"]
        .as_array()
        .expect("MCP content array");
    assert_eq!(content.len(), 1);
    assert_eq!(content[0]["type"], "text");
    let text_value: Value = serde_json::from_str(
        content[0]["text"]
            .as_str()
            .expect("MCP text content string"),
    )
    .expect("MCP text content JSON");
    assert_eq!(&text_value, result);
    assert_eq!(result["tool"], "evaluate_guardrails");
    assert_eq!(result["decision"], "requires_approval");
    assert_eq!(result["underlying_action_submit_allowed"], false);
    assert_eq!(result["attestation_submitted"], false);
    assert_eq!(result["verification_transaction_submitted"], false);
    assert_eq!(result["nullifier_consumed"], false);
    assert!(result["transaction_hash"].is_null());
}

#[test]
fn mcp_v0_stdio_plan_requires_intent_or_explicit_fixture() {
    let value = run_ready_mcp_stdio(
        r#"{"jsonrpc":"2.0","id":"plan-empty","method":"tools/call","params":{"name":"plan_stellar_action","arguments":{}}}"#,
    );

    assert_eq!(value["id"], "plan-empty");
    assert_eq!(value["error"]["code"], -32602);
    assert!(value["error"]["message"]
        .as_str()
        .expect("error message")
        .contains("requires non-empty intent_text"));
}

#[test]
fn mcp_v0_stdio_keeps_explicit_plan_fixture_for_conformance() {
    let value = run_ready_mcp_stdio(
        r#"{"jsonrpc":"2.0","id":"plan-fixture","method":"tools/call","params":{"name":"plan_stellar_action","arguments":{"scenario":"preview"}}}"#,
    );
    let result = &value["result"]["structuredContent"];

    assert_eq!(value["id"], "plan-fixture");
    assert_eq!(result["tool"], "plan_stellar_action");
    assert_eq!(result["mode"], "read_only");
    assert_eq!(result["underlying_action_submit_allowed"], false);
    assert_eq!(result["attestation_submitted"], false);
    assert_eq!(result["verification_transaction_submitted"], false);
    assert_eq!(result["nullifier_consumed"], false);
    assert!(result["transaction_hash"].is_null());
}

#[test]
fn mcp_v0_stdio_evaluates_unknown_plan_with_real_guardrails() {
    let action_plan = serde_json::json!({
        "schema_version": 1,
        "actions": [{
            "kind": "unknown",
            "reason": "intent_warning: low confidence"
        }]
    });
    let action_plan_hash = canonical_action_plan_hash(&action_plan);
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "evaluate-runtime",
        "method": "tools/call",
        "params": {
            "name": "evaluate_guardrails",
            "arguments": {
                "action_plan": action_plan,
                "action_plan_hash": action_plan_hash,
                "policy_ref": "configured",
                "evaluation_mode": "deterministic"
            }
        }
    });
    let value = run_ready_mcp_stdio(&request.to_string());
    let result = &value["result"]["structuredContent"];

    assert_eq!(value["id"], "evaluate-runtime");
    assert_eq!(result["runtime_source"], "neurochain_guardrails");
    assert_eq!(result["decision"], "blocked");
    assert_eq!(result["exit_code"], 5);
    assert_eq!(result["reason_code"], "intent_safety");
    assert_eq!(result["guardrails"]["intent_safety"], "blocked");
    assert_eq!(result["underlying_action_submit_allowed"], false);
    assert_eq!(result["attestation_submitted"], false);
    assert_eq!(result["verification_transaction_submitted"], false);
    assert_eq!(result["nullifier_consumed"], false);
    assert!(result["transaction_hash"].is_null());
}

#[test]
fn mcp_v0_stdio_rejects_action_plan_hash_mismatch() {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "evaluate-hash-mismatch",
        "method": "tools/call",
        "params": {
            "name": "evaluate_guardrails",
            "arguments": {
                "action_plan": {
                    "schema_version": 1,
                    "actions": [{
                        "kind": "unknown",
                        "reason": "intent_warning: low confidence"
                    }]
                },
                "action_plan_hash": "0000000000000000000000000000000000000000000000000000000000000000"
            }
        }
    });
    let value = run_ready_mcp_stdio(&request.to_string());

    assert_eq!(value["id"], "evaluate-hash-mismatch");
    assert_eq!(value["error"]["code"], -32602);
    assert!(value["error"]["message"]
        .as_str()
        .expect("hash error message")
        .contains("does not match"));
}

#[test]
fn mcp_v0_stdio_inspects_real_zk_artifact_without_submit() {
    let action_plan = read_json(Path::new(
        "hackathons/stellar-real-world-zk/fixtures/typed_action_plan.json",
    ));
    let proof = read_json(Path::new(
        "hackathons/stellar-real-world-zk/fixtures/groth16_approved.json",
    ));
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "prove-runtime",
        "method": "tools/call",
        "params": {
            "name": "prove_guardrail_decision",
            "arguments": {
                "action_plan": action_plan,
                "proof": proof,
                "proof_mode": "inspect_public_artifact"
            }
        }
    });
    let value = run_ready_mcp_stdio(&request.to_string());
    let result = &value["result"]["structuredContent"];

    assert_eq!(value["id"], "prove-runtime");
    assert_eq!(result["runtime_source"], "neurochain_zk_attestation_view");
    assert_eq!(result["decision"], "approved");
    assert_eq!(result["exit_code"], 0);
    assert_eq!(result["reason_code"], "passed");
    assert_eq!(result["local_binding"], "binding_validated");
    assert_eq!(result["proof_binding"], "binding_validated");
    assert_eq!(result["cryptographically_verified"], false);
    assert_eq!(result["stellar_verification_required"], true);
    assert_eq!(result["stellar_verification"], "required_on_stellar");
    assert_eq!(
        result["evaluator_image_id"],
        "d12dc4e578c8000108c739bc4a071a451791ac4011c2688033ee13f5d60b3473"
    );
    assert_eq!(
        result["journal_digest"],
        "55d9f3e4223db1687f060a00f0caa64c081f441347e0f00c2d285e539b5ee13c"
    );
    assert_eq!(
        result["proof_artifact_ref"],
        "inline:55d9f3e4223db1687f060a00f0caa64c081f441347e0f00c2d285e539b5ee13c"
    );
    assert_eq!(result["underlying_action_submit_allowed"], false);
    assert_eq!(result["attestation_submitted"], false);
    assert_eq!(result["verification_transaction_submitted"], false);
    assert_eq!(result["nullifier_consumed"], false);
    assert!(result["transaction_hash"].is_null());
    assert!(result.get("seal_hex").is_none());
    assert!(result.get("journal_hex").is_none());
}

#[test]
fn mcp_v0_stdio_verifies_zk_on_stellar_read_only_without_submit() {
    let (_tmp_dir, fake_cli, log_path) = create_fake_zk_stellar_cli();
    let action_plan = read_json(Path::new(
        "hackathons/stellar-real-world-zk/fixtures/typed_action_plan.json",
    ));
    let proof = read_json(Path::new(
        "hackathons/stellar-real-world-zk/fixtures/groth16_approved.json",
    ));
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "verify-runtime",
        "method": "tools/call",
        "params": {
            "name": "verify_zk_on_stellar",
            "arguments": {
                "action_plan": action_plan,
                "proof": proof,
                "contract_id": "CTESTZKGUARDRAIL",
                "network": "testnet",
                "verification_mode": "read_only"
            }
        }
    });
    let output = run_mcp_stdio_session_with_env(
        &[
            MCP_INIT_REQUEST,
            MCP_INITIALIZED_NOTIFICATION,
            &request.to_string(),
        ],
        &[
            ("NC_STELLAR_CLI", fake_cli.to_string_lossy().to_string()),
            ("NC_SOROBAN_SOURCE", "demo-source".to_string()),
            ("NC_ZK_GUARDRAIL_CONTRACT", "CTESTZKGUARDRAIL".to_string()),
        ],
    );
    assert!(
        output.status.success(),
        "ready stdio session failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = parse_mcp_stdio_responses(&output);
    let result = &responses[1]["result"]["structuredContent"];

    assert_eq!(responses[1]["id"], "verify-runtime");
    assert_eq!(
        result["runtime_source"],
        "neurochain_soroban_read_only_verifier"
    );
    assert_eq!(result["stellar_verification"], "verified_on_stellar");
    assert_eq!(result["verification_mode"], "read_only");
    assert_eq!(result["local_binding"], "binding_validated");
    assert_eq!(result["cryptographically_verified"], true);
    assert_eq!(result["underlying_action_submit_allowed"], false);
    assert_eq!(result["attestation_submitted"], false);
    assert_eq!(result["verification_transaction_submitted"], false);
    assert_eq!(result["nullifier_consumed"], false);
    assert!(result["transaction_hash"].is_null());
    assert!(result.get("seal_hex").is_none());
    assert!(result.get("journal_hex").is_none());

    let args = fs::read_to_string(log_path).expect("read fake Stellar CLI args");
    assert!(args.contains("--send no"));
    assert!(args.contains("-- verify --seal"));
    assert!(!args.contains("--send yes"));
    assert!(!args.contains("verify_and_consume"));
}

#[test]
fn mcp_v0_stdio_returns_status_from_latest_structured_result_without_submit() {
    let latest_result = read_json(Path::new(
        "examples/mcp_v0_no_submit_contract/verify_zk_on_stellar_read_only.json",
    ));
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "status-runtime",
        "method": "tools/call",
        "params": {
            "name": "get_guardrail_status",
            "arguments": {
                "latest_result": latest_result,
                "session_id": "demo-session"
            }
        }
    });
    let value = run_ready_mcp_stdio(&request.to_string());
    let result = &value["result"]["structuredContent"];

    assert_eq!(value["id"], "status-runtime");
    assert_eq!(result["runtime_source"], "neurochain_mcp_status_view");
    assert_eq!(result["status_source"], "latest_result");
    assert_eq!(result["last_tool"], "verify_zk_on_stellar");
    assert_eq!(result["decision"], "approved");
    assert_eq!(result["stellar_verification"], "verified_on_stellar");
    assert_eq!(result["local_binding"], "binding_validated");
    assert_eq!(result["cryptographically_verified"], true);
    assert_eq!(result["underlying_action_submit_allowed"], false);
    assert_eq!(result["attestation_submitted"], false);
    assert_eq!(result["verification_transaction_submitted"], false);
    assert_eq!(result["nullifier_consumed"], false);
    assert!(result["transaction_hash"].is_null());
}

#[test]
fn mcp_v0_stdio_rejects_tampered_zk_action_plan_binding() {
    let mut action_plan = read_json(Path::new(
        "hackathons/stellar-real-world-zk/fixtures/typed_action_plan.json",
    ));
    action_plan["args"][0]["value"] = Value::String("500000001".to_string());
    let proof = read_json(Path::new(
        "hackathons/stellar-real-world-zk/fixtures/groth16_approved.json",
    ));
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "prove-tampered",
        "method": "tools/call",
        "params": {
            "name": "prove_guardrail_decision",
            "arguments": {
                "action_plan": action_plan,
                "proof": proof
            }
        }
    });
    let value = run_ready_mcp_stdio(&request.to_string());

    assert_eq!(value["id"], "prove-tampered");
    assert_eq!(value["error"]["code"], -32602);
    assert!(value["error"]["message"]
        .as_str()
        .expect("proof binding error")
        .contains("action_plan_hash_mismatch"));
}

#[test]
fn mcp_v0_stdio_rejects_submit_like_tools() {
    let value = run_ready_mcp_stdio(
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"submit_underlying_action","arguments":{}}}"#,
    );

    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 2);
    assert_eq!(value["error"]["code"], -32602);
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
    let value = run_ready_mcp_stdio(
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"evaluate_guardrails","arguments":{"scenario":"approved","api_key":null}}}"#,
    );

    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 3);
    assert_eq!(value["error"]["code"], -32602);
    assert!(
        value["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("secret-like field"),
        "unexpected error: {value}"
    );
}

#[test]
fn mcp_v0_stdio_uses_standard_json_rpc_error_codes() {
    let parse_error = run_mcp_stdio("{");
    let parse_value: Value =
        serde_json::from_slice(&parse_error.stdout).expect("parse error response");
    assert_eq!(parse_value["id"], Value::Null);
    assert_eq!(parse_value["error"]["code"], -32700);

    let invalid_request = run_mcp_stdio(r#"{"jsonrpc":"1.0","id":7,"method":"initialize"}"#);
    let invalid_value: Value =
        serde_json::from_slice(&invalid_request.stdout).expect("invalid request response");
    assert_eq!(invalid_value["id"], 7);
    assert_eq!(invalid_value["error"]["code"], -32600);

    let unknown_method =
        run_ready_mcp_stdio(r#"{"jsonrpc":"2.0","id":8,"method":"resources/list","params":{}}"#);
    assert_eq!(unknown_method["id"], 8);
    assert_eq!(unknown_method["error"]["code"], -32601);

    let invalid_params = run_ready_mcp_stdio(r#"{"jsonrpc":"2.0","id":9,"method":"tools/call"}"#);
    assert_eq!(invalid_params["id"], 9);
    assert_eq!(invalid_params["error"]["code"], -32602);
}

#[test]
fn mcp_v0_stdio_keeps_notifications_silent() {
    let output = run_mcp_stdio_session(&[
        MCP_INIT_REQUEST,
        MCP_INITIALIZED_NOTIFICATION,
        r#"{"jsonrpc":"2.0","method":"notifications/progress","params":{"progressToken":"offline-test","progress":1}}"#,
        r#"{"jsonrpc":"2.0","method":"tools/call","params":{"name":"evaluate_guardrails","arguments":{"scenario":"approved"}}}"#,
        r#"{"jsonrpc":"2.0","id":"list-after-notifications","method":"tools/list","params":{}}"#,
    ]);
    assert!(
        output.status.success(),
        "notification session failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = parse_mcp_stdio_responses(&output);
    assert_eq!(responses.len(), 2, "notifications must not emit responses");
    assert_eq!(responses[0]["id"], "init-1");
    assert_eq!(responses[1]["id"], "list-after-notifications");
}

fn run_fixture_runner(args: &[&str]) -> Output {
    Command::new(assert_cmd::cargo::cargo_bin!(
        "neurochain-mcp-v0-fixture-runner"
    ))
    .args(args)
    .output()
    .expect("run fixture runner")
}

fn create_fake_zk_stellar_cli() -> (TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().expect("create temp dir for fake ZK Stellar CLI");
    let log_path = dir.path().join("stellar-args.log");
    #[cfg(windows)]
    let cli_path = dir.path().join("stellar-zk.cmd");
    #[cfg(not(windows))]
    let cli_path = dir.path().join("stellar-zk");

    let accepted = r#"{"action_plan_hash":"a008efa4f3ecbdf88b9bcc3ed4c7672994136f16074e8fddd6bb8192ea7970cd","policy_commitment":"f208fb657dcf4a6b4f339e6402da536dd1f86a3e353282426d622c1bb5e21150","policy_version":7,"decision_status":0,"exit_code":0,"reason_code":0,"requires_approval":false,"audit_nullifier":"c62e6a97e27f67c0370a45b52ff84f27796b9d7f55df02ad35aff2e90b7328da","next_step":"EligibleForSeparateApprovalFlow"}"#;
    #[cfg(windows)]
    let script = format!(
        "@echo off\r\necho %*>>\"{}\"\r\necho {}\r\nexit /b 0\r\n",
        log_path.to_string_lossy(),
        accepted
    );
    #[cfg(not(windows))]
    let script = format!(
        "#!/usr/bin/env sh\nprintf '%s\\n' \"$*\" >> '{}'\necho '{}'\n",
        log_path.to_string_lossy(),
        accepted
    );

    fs::write(&cli_path, script).expect("write fake ZK Stellar CLI");
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&cli_path)
            .expect("metadata for fake ZK Stellar CLI")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&cli_path, perms).expect("chmod fake ZK Stellar CLI");
    }

    (dir, cli_path, log_path)
}

const MCP_INIT_REQUEST: &str = r#"{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"fixture-harness","version":"0.1.0"}}}"#;
const MCP_INITIALIZED_NOTIFICATION: &str =
    r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;

fn run_ready_mcp_stdio(request: &str) -> Value {
    let output = run_mcp_stdio_session(&[MCP_INIT_REQUEST, MCP_INITIALIZED_NOTIFICATION, request]);
    assert!(
        output.status.success(),
        "ready stdio session failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut responses = parse_mcp_stdio_responses(&output);
    assert_eq!(
        responses.len(),
        2,
        "expected initialize and request responses"
    );
    responses.pop().expect("request response")
}

fn run_mcp_stdio(request: &str) -> Output {
    run_mcp_stdio_session(&[request])
}

fn run_mcp_stdio_session(requests: &[&str]) -> Output {
    run_mcp_stdio_session_with_env(requests, &[])
}

fn run_mcp_stdio_session_with_env(requests: &[&str], envs: &[(&str, String)]) -> Output {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("neurochain-mcp-v0-stdio"));
    for (name, value) in envs {
        command.env(name, value);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn stdio shim");
    child
        .stdin
        .as_mut()
        .expect("stdio shim stdin")
        .write_all(format!("{}\n", requests.join("\n")).as_bytes())
        .expect("write stdio requests");
    child.wait_with_output().expect("stdio shim output")
}

fn parse_mcp_stdio_responses(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("stdio JSON response line"))
        .collect()
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

fn assert_safe_host_config(
    path: impl AsRef<Path>,
    expected_command_suffix: &str,
    expected_model_suffix: &str,
) {
    let path = path.as_ref();
    let config = read_json(path);
    let server = &config["mcpServers"]["neurochain-stellar-guardrails"];
    let name = path.display().to_string();

    let command = server["command"]
        .as_str()
        .unwrap_or_else(|| panic!("{name} command must be a string"));
    assert!(
        command.ends_with(expected_command_suffix),
        "{name} command should point to {expected_command_suffix}"
    );
    assert_eq!(server["args"], serde_json::json!([]), "{name} args");

    let env = server["env"].as_object().expect("host env object");
    assert_eq!(
        env.len(),
        1,
        "{name} should expose only non-secret local model configuration"
    );
    let model_path = env["NC_INTENT_STELLAR_MODEL"]
        .as_str()
        .unwrap_or_else(|| panic!("{name} model path must be a string"));
    assert!(
        model_path.ends_with(expected_model_suffix),
        "{name} model path should point to {expected_model_suffix}"
    );

    for forbidden in [
        "NC_STELLAR_SOURCE",
        "NC_API_KEY",
        "NC_WALLET_SECRET",
        "NC_PRIVATE_KEY",
        "NC_SEED_PHRASE",
    ] {
        assert!(
            env.get(forbidden).is_none(),
            "{name} must not set {forbidden}"
        );
    }
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

fn canonical_action_plan_hash(value: &Value) -> String {
    let plan: ActionPlan =
        serde_json::from_value(value.clone()).expect("canonical ActionPlan fixture");
    let encoded = serde_json::to_vec(&plan).expect("serialize canonical ActionPlan");
    let mut hasher = Sha256::new();
    hasher.update(PLAN_HASH_DOMAIN);
    hasher.update(encoded);
    hex::encode(hasher.finalize())
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
