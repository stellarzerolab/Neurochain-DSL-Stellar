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
                "inputSchema": {
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
                }
            })
        })
        .collect();
    json!({
        "tools": tools,
        "excluded_from_default_mcp_v0": EXCLUDED_TOOLS,
        "mode": "read_only",
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
    let tool = value
        .get("tool")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("fixture {} missing tool", fixture.name))?;
    if !DEFAULT_TOOLS.contains(&tool) {
        return Err(format!("fixture {} uses non-v0 tool {tool}", fixture.name));
    }
    if EXCLUDED_TOOLS.contains(&tool) {
        return Err(format!(
            "fixture {} uses excluded submit/stateful tool {tool}",
            fixture.name
        ));
    }

    expect_eq(fixture, value, "mode", &json!("read_only"))?;
    expect_eq(
        fixture,
        value,
        "underlying_action_submit_allowed",
        &json!(false),
    )?;
    expect_eq(fixture, value, "attestation_submitted", &json!(false))?;
    expect_eq(
        fixture,
        value,
        "verification_transaction_submitted",
        &json!(false),
    )?;
    expect_eq(fixture, value, "nullifier_consumed", &json!(false))?;

    if !value
        .get("transaction_hash")
        .is_some_and(serde_json::Value::is_null)
    {
        return Err(format!(
            "fixture {} must keep transaction_hash null in default MCP v0",
            fixture.name
        ));
    }

    Ok(())
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

fn expect_eq(
    fixture: &Fixture,
    value: &Value,
    field: &str,
    expected: &Value,
) -> Result<(), String> {
    match value.get(field) {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => Err(format!(
            "fixture {} field {field} expected {expected}, got {actual}",
            fixture.name
        )),
        None => Err(format!("fixture {} missing {field}", fixture.name)),
    }
}

fn tool_description(tool: &str) -> &'static str {
    match tool {
        "plan_stellar_action" => "Preview a typed Stellar ActionPlan without submit capability.",
        "evaluate_guardrails" => "Evaluate guardrail decision fields without submitting.",
        "prove_guardrail_decision" => "Return a fixture proof artifact reference.",
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
