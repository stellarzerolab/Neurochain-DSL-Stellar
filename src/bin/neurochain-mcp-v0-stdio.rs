use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleState {
    Uninitialized,
    Initialized,
    Ready,
}

fn main() -> ExitCode {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    let mut state = LifecycleState::Uninitialized;
    let mut saw_request = false;

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                eprintln!("failed to read stdin: {err}");
                return ExitCode::from(2);
            }
        };
        let raw = line.trim();
        if raw.is_empty() {
            continue;
        }
        saw_request = true;

        let response = match handle_json_rpc(raw, &mut state) {
            Ok(value) => value,
            Err(err) => Some(json_rpc_error(Value::Null, -32700, err)),
        };

        let Some(response) = response else {
            continue;
        };
        if let Err(err) = serde_json::to_writer(&mut stdout, &response) {
            eprintln!("failed to serialize response: {err}");
            return ExitCode::from(2);
        }
        if let Err(err) = writeln!(stdout).and_then(|_| stdout.flush()) {
            eprintln!("failed to write response: {err}");
            return ExitCode::from(2);
        }
    }

    if !saw_request {
        eprintln!("{}", usage());
        return ExitCode::from(2);
    }

    ExitCode::SUCCESS
}

fn handle_json_rpc(raw: &str, state: &mut LifecycleState) -> Result<Option<Value>, String> {
    let request: Value =
        serde_json::from_str(raw).map_err(|err| format!("invalid JSON-RPC request: {err}"))?;
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| "JSON-RPC request must include method".to_string())?;

    match method {
        "initialize" => {
            if *state != LifecycleState::Uninitialized {
                return Ok(Some(json_rpc_error(
                    id,
                    -32600,
                    "MCP session is already initialized",
                )));
            }
            match initialize_result(&request) {
                Ok(result) => {
                    *state = LifecycleState::Initialized;
                    Ok(Some(json_rpc_result(id, result)))
                }
                Err(err) => Ok(Some(json_rpc_error(id, -32602, err))),
            }
        }
        "notifications/initialized" => {
            if request.get("id").is_some() {
                return Ok(Some(json_rpc_error(
                    id,
                    -32600,
                    "notifications/initialized must not include id",
                )));
            }
            if *state == LifecycleState::Initialized {
                *state = LifecycleState::Ready;
            }
            Ok(None)
        }
        "tools/list" => {
            if *state != LifecycleState::Ready {
                return Ok(Some(session_not_ready(id)));
            }
            Ok(Some(json_rpc_result(
                id,
                neurochain::mcp_v0_fixture::tool_list_value(),
            )))
        }
        "tools/call" => {
            if *state != LifecycleState::Ready {
                return Ok(Some(session_not_ready(id)));
            }
            let params = request
                .get("params")
                .ok_or_else(|| "tools/call must include params".to_string())?;
            match neurochain::mcp_v0_fixture::fixture_value_by_call_value(params) {
                Ok(result) => Ok(Some(json_rpc_result(id, tool_call_result(result)))),
                Err(err) => Ok(Some(json_rpc_error(id, -32000, err))),
            }
        }
        other => Ok(Some(json_rpc_error(
            id,
            -32601,
            format!("unsupported read-only MCP v0 method: {other}"),
        ))),
    }
}

fn tool_call_result(structured_content: Value) -> Value {
    let text = serde_json::to_string(&structured_content)
        .expect("fixture value must serialize as MCP text content");
    json!({
        "content": [{
            "type": "text",
            "text": text
        }],
        "structuredContent": structured_content,
        "isError": false
    })
}

fn session_not_ready(id: Value) -> Value {
    json_rpc_error(
        id,
        -32002,
        "MCP session is not ready; send initialize and notifications/initialized first",
    )
}

fn initialize_result(request: &Value) -> Result<Value, String> {
    let params = request
        .get("params")
        .and_then(Value::as_object)
        .ok_or_else(|| "initialize must include object params".to_string())?;
    let requested_protocol = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .ok_or_else(|| "initialize params must include protocolVersion".to_string())?;
    params
        .get("capabilities")
        .and_then(Value::as_object)
        .ok_or_else(|| "initialize params must include object capabilities".to_string())?;
    let client_info = params
        .get("clientInfo")
        .and_then(Value::as_object)
        .ok_or_else(|| "initialize params must include object clientInfo".to_string())?;
    client_info
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "initialize clientInfo must include name".to_string())?;
    client_info
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| "initialize clientInfo must include version".to_string())?;

    let protocol_version = if requested_protocol == MCP_PROTOCOL_VERSION {
        requested_protocol
    } else {
        MCP_PROTOCOL_VERSION
    };

    Ok(json!({
        "protocolVersion": protocol_version,
        "capabilities": {
            "tools": {
                "listChanged": false
            },
            "experimental": {
                "neurochainNoSubmit": {
                    "mode": "read_only",
                    "noSubmit": true,
                    "underlyingActionSubmitAllowed": false,
                    "excludedTools": neurochain::mcp_v0_fixture::EXCLUDED_TOOLS
                }
            }
        },
        "serverInfo": {
            "name": "neurochain-mcp-v0-stdio",
            "title": "NeuroChain MCP V0 Offline Fixture Shim",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": "Offline read-only fixture shim. Plan, evaluate, prove, and verify responses never grant signing, broadcast, nullifier-consume, attestation, or underlying ActionPlan submit authority."
    }))
}

fn json_rpc_result(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

fn json_rpc_error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message.into(),
        },
    })
}

fn usage() -> &'static str {
    "Usage: send newline-delimited JSON-RPC messages on stdin: initialize, notifications/initialized, then tools/list or tools/call"
}
