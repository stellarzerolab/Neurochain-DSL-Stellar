use neurochain::mcp_v0_fixture::{DEFAULT_TOOLS, EXCLUDED_TOOLS};
use serde_json::{json, Value};
use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

const SESSION: &str = include_str!("../../examples/mcp_v0_stdio_client/session.jsonl");

fn main() -> ExitCode {
    match run() {
        Ok(summary) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&summary).expect("summary must serialize")
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("MCP v0 client smoke failed: {err}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<Value, String> {
    let server = parse_server_path()?;
    let mut child = Command::new(&server)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("could not start {}: {err}", server.display()))?;

    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "stdio server stdin was unavailable".to_string())?;
        stdin
            .write_all(SESSION.as_bytes())
            .map_err(|err| format!("could not write MCP session: {err}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|err| format!("could not wait for stdio server: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "stdio server exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|err| format!("stdio server returned non-UTF-8 output: {err}"))?;
    let responses = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .map_err(|err| format!("invalid JSON-RPC response: {err}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    validate_responses(&responses)
}

fn parse_server_path() -> Result<PathBuf, String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [] => {
            let current = env::current_exe()
                .map_err(|err| format!("could not resolve current executable: {err}"))?;
            let parent = current
                .parent()
                .ok_or_else(|| "current executable has no parent directory".to_string())?;
            Ok(parent.join(format!(
                "neurochain-mcp-v0-stdio{}",
                env::consts::EXE_SUFFIX
            )))
        }
        [flag, path] if flag == "--server" => Ok(PathBuf::from(path)),
        [flag] if flag == "--help" || flag == "-h" => Err(usage().to_string()),
        _ => Err(format!("invalid arguments\n{}", usage())),
    }
}

fn validate_responses(responses: &[Value]) -> Result<Value, String> {
    if responses.len() != 3 {
        return Err(format!(
            "expected 3 responses; initialized notification must stay silent, got {}",
            responses.len()
        ));
    }

    let initialize = result_for_id(&responses[0], "init-1")?;
    expect_eq(
        initialize,
        &[
            "capabilities",
            "experimental",
            "neurochainNoSubmit",
            "noSubmit",
        ],
        &json!(true),
    )?;
    expect_eq(
        initialize,
        &[
            "capabilities",
            "experimental",
            "neurochainNoSubmit",
            "underlyingActionSubmitAllowed",
        ],
        &json!(false),
    )?;

    let tools_result = result_for_id(&responses[1], "list-1")?;
    let tools = tools_result
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| "tools/list response is missing tools".to_string())?;
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    if names != DEFAULT_TOOLS {
        return Err(format!("unexpected tool list: {names:?}"));
    }
    for excluded in EXCLUDED_TOOLS {
        if names.contains(excluded) {
            return Err(format!("tools/list exposed excluded tool {excluded}"));
        }
    }

    let call_result = result_for_id(&responses[2], "call-1")?;
    expect_eq(call_result, &["isError"], &json!(false))?;
    let content = call_result
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| "tools/call response is missing MCP content".to_string())?;
    let text = content
        .first()
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .ok_or_else(|| "tools/call response is missing text content".to_string())?;
    let call = call_result
        .get("structuredContent")
        .ok_or_else(|| "tools/call response is missing structuredContent".to_string())?;
    let text_value: Value = serde_json::from_str(text)
        .map_err(|err| format!("tools/call text content is not JSON: {err}"))?;
    if &text_value != call {
        return Err("text content and structuredContent differ".to_string());
    }
    expect_eq(call, &["decision"], &json!("requires_approval"))?;
    for field in [
        "underlying_action_submit_allowed",
        "attestation_submitted",
        "verification_transaction_submitted",
        "nullifier_consumed",
    ] {
        expect_eq(call, &[field], &json!(false))?;
    }
    expect_eq(call, &["transaction_hash"], &Value::Null)?;

    Ok(json!({
        "status": "passed",
        "transport": "stdio",
        "protocol_version": initialize["protocolVersion"],
        "tools": names,
        "sample_decision": call["decision"],
        "underlying_action_submit_allowed": false,
        "attestation_submitted": false,
        "verification_transaction_submitted": false,
        "nullifier_consumed": false
    }))
}

fn result_for_id<'a>(response: &'a Value, expected_id: &str) -> Result<&'a Value, String> {
    if response.get("id") != Some(&json!(expected_id)) {
        return Err(format!(
            "expected response id {expected_id}, got {response}"
        ));
    }
    response
        .get("result")
        .ok_or_else(|| format!("response {expected_id} has no result: {response}"))
}

fn expect_eq(root: &Value, path: &[&str], expected: &Value) -> Result<(), String> {
    let mut value = root;
    for key in path {
        value = value
            .get(*key)
            .ok_or_else(|| format!("missing response field {}", path.join(".")))?;
    }
    if value == expected {
        Ok(())
    } else {
        Err(format!(
            "response field {} expected {expected}, got {value}",
            path.join(".")
        ))
    }
}

fn usage() -> &'static str {
    "Usage: neurochain-mcp-v0-client-smoke [--server <absolute-path-to-neurochain-mcp-v0-stdio>]"
}
