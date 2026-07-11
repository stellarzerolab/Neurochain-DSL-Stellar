use serde_json::{json, Value};
use std::io::{self, Read};
use std::process::ExitCode;

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

fn main() -> ExitCode {
    let mut raw = String::new();
    if let Err(err) = io::stdin().read_to_string(&mut raw) {
        eprintln!("failed to read stdin: {err}");
        return ExitCode::from(2);
    }

    if raw.trim().is_empty() {
        eprintln!("{}", usage());
        return ExitCode::from(2);
    }

    let response = match handle_json_rpc(raw.trim()) {
        Ok(value) => value,
        Err(err) => json_rpc_error(Value::Null, -32700, err),
    };

    match serde_json::to_string_pretty(&response) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("failed to serialize response: {err}");
            ExitCode::from(2)
        }
    }
}

fn handle_json_rpc(raw: &str) -> Result<Value, String> {
    let request: Value =
        serde_json::from_str(raw).map_err(|err| format!("invalid JSON-RPC request: {err}"))?;
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| "JSON-RPC request must include method".to_string())?;

    match method {
        "initialize" => match initialize_result(&request) {
            Ok(result) => Ok(json_rpc_result(id, result)),
            Err(err) => Ok(json_rpc_error(id, -32602, err)),
        },
        "tools/list" => Ok(json_rpc_result(
            id,
            neurochain::mcp_v0_fixture::tool_list_value(),
        )),
        "tools/call" => {
            let params = request
                .get("params")
                .ok_or_else(|| "tools/call must include params".to_string())?;
            match neurochain::mcp_v0_fixture::fixture_value_by_call_value(params) {
                Ok(result) => Ok(json_rpc_result(id, result)),
                Err(err) => Ok(json_rpc_error(id, -32000, err)),
            }
        }
        other => Ok(json_rpc_error(
            id,
            -32601,
            format!("unsupported read-only MCP v0 method: {other}"),
        )),
    }
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
    "Usage: send one JSON-RPC request on stdin, for example initialize, tools/list, or tools/call"
}
