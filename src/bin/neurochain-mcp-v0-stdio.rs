use serde_json::{json, Value};
use std::io::{self, Read};
use std::process::ExitCode;

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
    "Usage: send one JSON-RPC request on stdin, for example tools/list or tools/call"
}
