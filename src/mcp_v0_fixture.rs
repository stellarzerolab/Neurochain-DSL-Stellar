use serde_json::{json, Value};

pub const DEFAULT_TOOLS: &[&str] = &[
    "plan_stellar_action",
    "evaluate_guardrails",
    "prove_guardrail_decision",
    "verify_zk_on_stellar",
    "get_guardrail_status",
];

pub const EXCLUDED_TOOLS: &[&str] = &[
    "submit_testnet_attestation",
    "consume_nullifier",
    "submit_underlying_action",
    "sign_transaction",
    "configure_server",
];

struct Fixture {
    name: &'static str,
    tool: &'static str,
    scenario: &'static str,
    json: &'static str,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        name: "plan_stellar_action",
        tool: "plan_stellar_action",
        scenario: "preview",
        json: include_str!("../examples/mcp_v0_no_submit_contract/plan_stellar_action.json"),
    },
    Fixture {
        name: "evaluate_guardrails_approved",
        tool: "evaluate_guardrails",
        scenario: "approved",
        json: include_str!(
            "../examples/mcp_v0_no_submit_contract/evaluate_guardrails_approved.json"
        ),
    },
    Fixture {
        name: "evaluate_guardrails_requires_approval",
        tool: "evaluate_guardrails",
        scenario: "requires_approval",
        json: include_str!(
            "../examples/mcp_v0_no_submit_contract/evaluate_guardrails_requires_approval.json"
        ),
    },
    Fixture {
        name: "evaluate_guardrails_blocked_exit_4",
        tool: "evaluate_guardrails",
        scenario: "blocked_exit_4",
        json: include_str!(
            "../examples/mcp_v0_no_submit_contract/evaluate_guardrails_blocked_exit_4.json"
        ),
    },
    Fixture {
        name: "prove_guardrail_decision",
        tool: "prove_guardrail_decision",
        scenario: "approved",
        json: include_str!("../examples/mcp_v0_no_submit_contract/prove_guardrail_decision.json"),
    },
    Fixture {
        name: "verify_zk_on_stellar_read_only",
        tool: "verify_zk_on_stellar",
        scenario: "read_only",
        json: include_str!(
            "../examples/mcp_v0_no_submit_contract/verify_zk_on_stellar_read_only.json"
        ),
    },
    Fixture {
        name: "get_guardrail_status_verified",
        tool: "get_guardrail_status",
        scenario: "verified",
        json: include_str!(
            "../examples/mcp_v0_no_submit_contract/get_guardrail_status_verified.json"
        ),
    },
];

pub fn run_fixture_args(args: Vec<String>) -> Result<String, String> {
    match args.as_slice() {
        [] => Ok(usage()),
        [arg] if arg == "--help" || arg == "-h" => Ok(usage()),
        [arg] if arg == "--list" => list_fixtures(),
        [flag, name] if flag == "--fixture" => fixture_by_name(name),
        [flag, raw] if flag == "--call-json" => fixture_by_call_json(raw),
        [tool_flag, tool, scenario_flag, scenario]
            if tool_flag == "--tool" && scenario_flag == "--scenario" =>
        {
            fixture_by_tool_and_scenario(tool, scenario)
        }
        _ => Err("invalid arguments".to_string()),
    }
}

pub fn list_fixtures() -> Result<String, String> {
    serde_json::to_string_pretty(&list_fixtures_value()).map_err(|err| err.to_string())
}

pub fn list_fixtures_value() -> Value {
    let fixtures: Vec<Value> = FIXTURES
        .iter()
        .map(|fixture| {
            json!({
                "name": fixture.name,
                "tool": fixture.tool,
                "scenario": fixture.scenario,
            })
        })
        .collect();
    json!({
        "schema_version": 1,
        "mode": "read_only",
        "fixtures": fixtures,
        "excluded_from_default_mcp_v0": EXCLUDED_TOOLS,
    })
}

pub fn tool_list_value() -> Value {
    let tools: Vec<Value> = DEFAULT_TOOLS
        .iter()
        .map(|tool| {
            json!({
                "name": tool,
                "description": tool_description(tool),
                "annotations": {
                    "readOnlyHint": true,
                    "destructiveHint": false,
                    "idempotentHint": true,
                    "openWorldHint": false
                },
                "inputSchema": tool_input_schema(tool)
            })
        })
        .collect();
    json!({
        "tools": tools,
        "excluded_from_default_mcp_v0": EXCLUDED_TOOLS,
        "mode": "read_only",
    })
}

fn tool_input_schema(tool: &str) -> Value {
    if tool == "plan_stellar_action" {
        return json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "intent_text": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 4096,
                    "description": "Natural-language Stellar intent classified by the local NeuroChain runtime"
                },
                "network": {
                    "type": "string",
                    "enum": ["testnet"],
                    "default": "testnet"
                },
                "source_hint": {
                    "type": "string",
                    "description": "Optional public wallet alias only; never a secret"
                },
                "plan_mode": {
                    "type": "string",
                    "enum": ["preview_only"],
                    "default": "preview_only"
                },
                "scenario": {
                    "type": "string",
                    "description": "Explicit offline fixture scenario selector for conformance tests"
                },
                "fixture": {
                    "type": "string",
                    "description": "Explicit offline fixture name for conformance tests"
                }
            },
            "anyOf": [
                {"required": ["intent_text"]},
                {"required": ["scenario"]},
                {"required": ["fixture"]}
            ]
        });
    }

    if tool == "evaluate_guardrails" {
        return json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "action_plan": {
                    "type": "object",
                    "description": "Canonical typed NeuroChain ActionPlan returned by plan_stellar_action; maximum 64 actions and 65536 serialized bytes"
                },
                "action_plan_hash": {
                    "type": "string",
                    "pattern": "^[0-9a-fA-F]{64}$",
                    "description": "SHA-256 binding returned with the canonical ActionPlan"
                },
                "policy_ref": {
                    "type": "string",
                    "enum": ["configured"],
                    "default": "configured",
                    "description": "Use only server-configured allowlists and contract policies"
                },
                "evaluation_mode": {
                    "type": "string",
                    "enum": ["deterministic"],
                    "default": "deterministic"
                },
                "requires_approval": {
                    "type": "boolean",
                    "default": false,
                    "description": "Optional stricter terminal approval boundary; never submit permission"
                },
                "scenario": {
                    "type": "string",
                    "description": "Explicit offline fixture scenario selector for conformance tests"
                },
                "fixture": {
                    "type": "string",
                    "description": "Explicit offline fixture name for conformance tests"
                }
            },
            "anyOf": [
                {"required": ["action_plan", "action_plan_hash"]},
                {"required": ["scenario"]},
                {"required": ["fixture"]}
            ]
        });
    }

    if tool == "prove_guardrail_decision" {
        return json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "action_plan": {
                    "type": "object",
                    "additionalProperties": false,
                    "description": "Exact public ZK typed ActionPlan bound into the supplied proof journal",
                    "required": [
                        "schema_version", "intent_label", "action_kind", "contract_id",
                        "function", "args", "intent_confidence_bps"
                    ],
                    "properties": {
                        "schema_version": {"const": 1},
                        "intent_label": {"type": "string"},
                        "action_kind": {"type": "string"},
                        "contract_id": {"type": "string"},
                        "function": {"type": "string"},
                        "args": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["name", "type", "value"],
                                "properties": {
                                    "name": {"type": "string"},
                                    "type": {"type": "string", "enum": ["address", "bytes", "symbol", "u64"]},
                                    "value": {"type": "string"}
                                }
                            }
                        },
                        "intent_confidence_bps": {"type": "integer", "minimum": 0, "maximum": 10000}
                    }
                },
                "proof": {
                    "type": "object",
                    "additionalProperties": false,
                    "description": "Inline public Groth16 artifact; client file paths are not accepted",
                    "required": [
                        "schema_version", "seal_hex", "image_id_hex", "journal_hex",
                        "journal_digest_hex"
                    ],
                    "properties": {
                        "schema_version": {"const": 1},
                        "seal_hex": {"type": "string", "pattern": "^[0-9a-fA-F]+$"},
                        "image_id_hex": {"type": "string", "pattern": "^[0-9a-fA-F]{64}$"},
                        "journal_hex": {"type": "string", "pattern": "^[0-9a-fA-F]+$"},
                        "journal_digest_hex": {"type": "string", "pattern": "^[0-9a-fA-F]{64}$"}
                    }
                },
                "proof_mode": {
                    "type": "string",
                    "enum": ["inspect_public_artifact"],
                    "default": "inspect_public_artifact"
                },
                "scenario": {
                    "type": "string",
                    "description": "Explicit offline fixture scenario selector for conformance tests"
                },
                "fixture": {
                    "type": "string",
                    "description": "Explicit offline fixture name for conformance tests"
                }
            },
            "anyOf": [
                {"required": ["action_plan", "proof"]},
                {"required": ["scenario"]},
                {"required": ["fixture"]}
            ]
        });
    }

    if tool == "verify_zk_on_stellar" {
        return json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "action_plan": {
                    "type": "object",
                    "additionalProperties": false,
                    "description": "Exact public ZK typed ActionPlan bound into the supplied proof journal",
                    "required": [
                        "schema_version", "intent_label", "action_kind", "contract_id",
                        "function", "args", "intent_confidence_bps"
                    ],
                    "properties": {
                        "schema_version": {"const": 1},
                        "intent_label": {"type": "string"},
                        "action_kind": {"type": "string"},
                        "contract_id": {"type": "string"},
                        "function": {"type": "string"},
                        "args": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["name", "type", "value"],
                                "properties": {
                                    "name": {"type": "string"},
                                    "type": {"type": "string", "enum": ["address", "bytes", "symbol", "u64"]},
                                    "value": {"type": "string"}
                                }
                            }
                        },
                        "intent_confidence_bps": {"type": "integer", "minimum": 0, "maximum": 10000}
                    }
                },
                "proof": {
                    "type": "object",
                    "additionalProperties": false,
                    "description": "Inline public Groth16 artifact; client file paths are not accepted",
                    "required": [
                        "schema_version", "seal_hex", "image_id_hex", "journal_hex",
                        "journal_digest_hex"
                    ],
                    "properties": {
                        "schema_version": {"const": 1},
                        "seal_hex": {"type": "string", "pattern": "^[0-9a-fA-F]+$"},
                        "image_id_hex": {"type": "string", "pattern": "^[0-9a-fA-F]{64}$"},
                        "journal_hex": {"type": "string", "pattern": "^[0-9a-fA-F]+$"},
                        "journal_digest_hex": {"type": "string", "pattern": "^[0-9a-fA-F]{64}$"}
                    }
                },
                "contract_id": {
                    "type": "string",
                    "description": "Soroban verifier contract ID; must match NC_ZK_GUARDRAIL_CONTRACT when that variable is set"
                },
                "network": {
                    "type": "string",
                    "enum": ["testnet"],
                    "default": "testnet"
                },
                "verification_mode": {
                    "type": "string",
                    "enum": ["read_only"],
                    "default": "read_only"
                },
                "scenario": {
                    "type": "string",
                    "description": "Explicit offline fixture scenario selector for conformance tests"
                },
                "fixture": {
                    "type": "string",
                    "description": "Explicit offline fixture name for conformance tests"
                }
            },
            "anyOf": [
                {"required": ["action_plan", "proof", "contract_id"]},
                {"required": ["action_plan", "proof"]},
                {"required": ["scenario"]},
                {"required": ["fixture"]}
            ]
        });
    }

    if tool == "get_guardrail_status" {
        return json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "latest_result": {
                    "type": "object",
                    "description": "The latest structuredContent returned by plan_stellar_action, evaluate_guardrails, prove_guardrail_decision, or verify_zk_on_stellar"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional host session identifier for display only; the v0 adapter is stateless"
                },
                "proof_artifact_ref": {
                    "type": "string",
                    "description": "Optional public artifact reference for display only; the tool does not read client paths"
                },
                "scenario": {
                    "type": "string",
                    "description": "Explicit offline fixture scenario selector for conformance tests"
                },
                "fixture": {
                    "type": "string",
                    "description": "Explicit offline fixture name for conformance tests"
                }
            },
            "anyOf": [
                {"required": ["latest_result"]},
                {"required": ["session_id"]},
                {"required": ["proof_artifact_ref"]},
                {"required": ["scenario"]},
                {"required": ["fixture"]},
                {"maxProperties": 0}
            ]
        });
    }

    json!({
        "type": "object",
        "additionalProperties": true,
        "properties": {
            "scenario": {
                "type": "string",
                "description": "Optional offline fixture scenario selector"
            },
            "fixture": {
                "type": "string",
                "description": "Optional exact offline fixture name"
            }
        }
    })
}

pub fn fixture_by_call_json(raw: &str) -> Result<String, String> {
    let value: Value =
        serde_json::from_str(raw).map_err(|err| format!("invalid call JSON: {err}"))?;
    let fixture = fixture_value_by_call_value(&value)?;
    serde_json::to_string_pretty(&fixture).map_err(|err| err.to_string())
}

pub fn fixture_value_by_call_value(value: &Value) -> Result<Value, String> {
    validate_no_secret_like_fields("call", value)?;

    if let Some(fixture) =
        string_at(value, &["fixture"]).or_else(|| string_at(value, &["arguments", "fixture"]))
    {
        return fixture_value_by_name(fixture);
    }

    let tool = string_at(value, &["tool"])
        .or_else(|| string_at(value, &["name"]))
        .ok_or_else(|| "call JSON must include tool/name or fixture".to_string())?;
    if EXCLUDED_TOOLS.contains(&tool) {
        return Err(format!("tool {tool} is excluded from default MCP v0"));
    }

    let scenario = string_at(value, &["scenario"])
        .or_else(|| string_at(value, &["arguments", "scenario"]))
        .or_else(|| default_scenario(tool))
        .ok_or_else(|| format!("tool {tool} needs an explicit scenario"))?;

    fixture_value_by_tool_and_scenario(tool, scenario)
}

pub fn fixture_by_name(name: &str) -> Result<String, String> {
    let value = fixture_value_by_name(name)?;
    serde_json::to_string_pretty(&value).map_err(|err| err.to_string())
}

pub fn fixture_by_tool_and_scenario(tool: &str, scenario: &str) -> Result<String, String> {
    let value = fixture_value_by_tool_and_scenario(tool, scenario)?;
    serde_json::to_string_pretty(&value).map_err(|err| err.to_string())
}

fn fixture_value_by_name(name: &str) -> Result<Value, String> {
    let fixture = FIXTURES
        .iter()
        .find(|fixture| fixture.name == name)
        .ok_or_else(|| format!("unknown fixture: {name}"))?;
    fixture_value(fixture)
}

fn fixture_value_by_tool_and_scenario(tool: &str, scenario: &str) -> Result<Value, String> {
    let fixture = FIXTURES
        .iter()
        .find(|fixture| fixture.tool == tool && fixture.scenario == scenario)
        .ok_or_else(|| format!("unknown tool/scenario: {tool}/{scenario}"))?;
    fixture_value(fixture)
}

fn fixture_value(fixture: &Fixture) -> Result<Value, String> {
    let value: Value = serde_json::from_str(fixture.json)
        .map_err(|err| format!("fixture {} is invalid JSON: {err}", fixture.name))?;
    validate_no_submit_fixture(fixture, &value)?;
    Ok(value)
}

fn string_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str()
}

fn default_scenario(tool: &str) -> Option<&'static str> {
    match tool {
        "plan_stellar_action" => Some("preview"),
        "evaluate_guardrails" => Some("approved"),
        "prove_guardrail_decision" => Some("approved"),
        "verify_zk_on_stellar" => Some("read_only"),
        "get_guardrail_status" => Some("verified"),
        _ => None,
    }
}

fn validate_no_submit_fixture(fixture: &Fixture, value: &Value) -> Result<(), String> {
    validate_no_submit_value(fixture.name, value)
}

pub fn validate_no_submit_value(context: &str, value: &Value) -> Result<(), String> {
    let tool = value
        .get("tool")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{context} missing tool"))?;
    if !DEFAULT_TOOLS.contains(&tool) {
        return Err(format!("{context} uses non-v0 tool {tool}"));
    }
    if EXCLUDED_TOOLS.contains(&tool) {
        return Err(format!(
            "{context} uses excluded submit/stateful tool {tool}"
        ));
    }

    expect_context_eq(context, value, "mode", &json!("read_only"))?;
    expect_context_eq(
        context,
        value,
        "underlying_action_submit_allowed",
        &json!(false),
    )?;
    expect_context_eq(context, value, "attestation_submitted", &json!(false))?;
    expect_context_eq(
        context,
        value,
        "verification_transaction_submitted",
        &json!(false),
    )?;
    expect_context_eq(context, value, "nullifier_consumed", &json!(false))?;

    if !value
        .get("transaction_hash")
        .is_some_and(serde_json::Value::is_null)
    {
        return Err(format!(
            "{context} must keep transaction_hash null in default MCP v0"
        ));
    }

    Ok(())
}

fn expect_context_eq(
    context: &str,
    value: &Value,
    field: &str,
    expected: &Value,
) -> Result<(), String> {
    match value.get(field) {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => Err(format!(
            "{context} field {field} expected {expected}, got {actual}"
        )),
        None => Err(format!("{context} missing {field}")),
    }
}

pub fn validate_no_secret_like_fields(context: &str, value: &Value) -> Result<(), String> {
    fn walk(context: &str, value: &Value, path: &str) -> Result<(), String> {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    let lowered = key.to_ascii_lowercase();
                    if matches!(
                        lowered.as_str(),
                        "seed_phrase"
                            | "secret_key"
                            | "private_key"
                            | "wallet_secret"
                            | "api_key"
                            | "bearer_token"
                    ) {
                        return Err(format!("{context} contains secret-like field {path}.{key}"));
                    }
                    walk(context, child, &format!("{path}.{key}"))?;
                }
            }
            Value::Array(items) => {
                for (idx, child) in items.iter().enumerate() {
                    walk(context, child, &format!("{path}[{idx}]"))?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    walk(context, value, "$")
}

fn tool_description(tool: &str) -> &'static str {
    match tool {
        "plan_stellar_action" => {
            "Classify a Stellar intent locally and preview the real typed ActionPlan without submit capability."
        }
        "evaluate_guardrails" => {
            "Evaluate the canonical ActionPlan with configured NeuroChain guardrails without submitting."
        }
        "prove_guardrail_decision" => {
            "Inspect a public ZK artifact against its exact typed ActionPlan without cryptographic verification or submit capability."
        }
        "verify_zk_on_stellar" => "Return read-only Stellar verification status.",
        "get_guardrail_status" => "Return the final no-submit guardrail status view.",
        _ => "Unknown MCP v0 fixture tool.",
    }
}

pub fn usage() -> String {
    format!(
        "Usage:\n  neurochain-mcp-v0-fixture-runner --list\n  neurochain-mcp-v0-fixture-runner --fixture <name>\n  neurochain-mcp-v0-fixture-runner --tool <tool> --scenario <scenario>\n  neurochain-mcp-v0-fixture-runner --call-json <json>\n\nCall JSON shape:\n  {{\"name\":\"evaluate_guardrails\",\"arguments\":{{\"scenario\":\"requires_approval\"}}}}\n\nAvailable fixtures:\n{}",
        FIXTURES
            .iter()
            .map(|fixture| format!(
                "  {} (tool={}, scenario={})",
                fixture.name, fixture.tool, fixture.scenario
            ))
            .collect::<Vec<_>>()
            .join("\n")
    )
}
