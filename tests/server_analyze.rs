use std::{
    fs,
    io::ErrorKind,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
struct AnalyzeResp {
    ok: bool,
    output: String,
    logs: Vec<String>,
}

struct Server {
    child: Child,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn find_free_port() -> u16 {
    // Bind to port 0 to let the OS pick a free port, then release it.
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral port");
    let port = listener.local_addr().expect("local_addr").port();
    drop(listener);
    port
}

fn wait_for_listen(addr: SocketAddr, timeout: Duration) {
    let start = Instant::now();
    loop {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_ok() {
            return;
        }
        if start.elapsed() > timeout {
            panic!("server did not start listening on {addr} within {timeout:?}");
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn http_post_json(addr: SocketAddr, path: &str, json_body: &str) -> (u16, String) {
    http_post_json_with_headers(addr, path, json_body, &[])
}

fn http_post_json_with_headers(
    addr: SocketAddr,
    path: &str,
    json_body: &str,
    headers: &[(&str, &str)],
) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("set_read_timeout");

    let extra_headers = headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}\r\n"))
        .collect::<String>();

    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n{extra}Connection: close\r\nContent-Length: {len}\r\n\r\n{body}",
        host = addr,
        len = json_body.len(),
        body = json_body,
        extra = extra_headers
    );

    stream.write_all(req.as_bytes()).expect("write request");

    // Read headers first, then read exactly Content-Length bytes (do not rely on EOF).
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    let start = Instant::now();
    let header_end = loop {
        let n = match stream.read(&mut chunk) {
            Ok(n) => n,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if start.elapsed() > Duration::from_secs(30) {
                    panic!("timeout waiting for response headers from {addr}");
                }
                continue;
            }
            Err(e) => panic!("read response: {e}"),
        };
        if n == 0 {
            panic!("unexpected EOF while reading headers");
        }
        buf.extend_from_slice(&chunk[..n]);

        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            break (pos, 4usize);
        }
        if let Some(pos) = find_subsequence(&buf, b"\n\n") {
            break (pos, 2usize);
        }
        if buf.len() > 64 * 1024 {
            panic!("headers too large");
        }
    };

    let (header_pos, header_len) = header_end;
    let split_at = header_pos + header_len;
    let (head_bytes, body_bytes) = buf.split_at(split_at);
    let head_str = String::from_utf8_lossy(head_bytes);

    // Split headers/body
    let status_line = head_str.lines().next().unwrap_or_default();
    let mut parts = status_line.split_whitespace();
    let _http = parts.next().unwrap_or_default();
    let code = parts
        .next()
        .unwrap_or_default()
        .parse::<u16>()
        .expect("status code");

    let content_len = head_str
        .lines()
        .find_map(|line| {
            let lower = line.to_ascii_lowercase();
            lower
                .strip_prefix("content-length:")
                .and_then(|v| v.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);

    let mut body: Vec<u8> = body_bytes.to_vec();
    if content_len == 0 {
        // Fallback: read until EOF (Connection: close).
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => body.extend_from_slice(&chunk[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    if start.elapsed() > Duration::from_secs(30) {
                        panic!("timeout waiting for response body from {addr}");
                    }
                    continue;
                }
                Err(e) => panic!("read body: {e}"),
            };
        }
    } else {
        while body.len() < content_len {
            let n = match stream.read(&mut chunk) {
                Ok(n) => n,
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    if start.elapsed() > Duration::from_secs(30) {
                        panic!("timeout waiting for response body from {addr}");
                    }
                    continue;
                }
                Err(e) => panic!("read body: {e}"),
            };
            if n == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..n]);
        }
        body.truncate(content_len);
    }

    let body_str = String::from_utf8_lossy(&body).to_string();
    (code, body_str)
}

fn models_dir() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("models");
    path
}

fn macro_model_path() -> PathBuf {
    let base = std::env::var("NC_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| models_dir());

    base.join("intent_macro").join("model.onnx")
}

fn intent_stellar_model_path() -> PathBuf {
    let base = std::env::var("NC_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| models_dir());

    base.join("intent_stellar").join("model.onnx")
}

fn spawn_server(port: u16, extra_env: &[(&str, &str)]) -> Server {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("neurochain-server"));
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("NC_MODELS_DIR", models_dir())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    for (key, value) in extra_env {
        command.env(key, value);
    }

    let child = command.spawn().expect("spawn neurochain-server");
    Server { child }
}

fn payment_challenge_id(resp_body: &str) -> String {
    let resp: serde_json::Value = serde_json::from_str(resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["error"], "payment_required");
    assert_eq!(resp["payment"]["state"], "payment_required");
    assert_eq!(resp["decision"]["status"], "not_evaluated");
    assert_eq!(resp["decision"]["requires_approval"], false);
    assert_eq!(resp["guardrails"]["state"], "not_run");
    assert!(
        resp["audit_id"]
            .as_str()
            .unwrap_or_default()
            .starts_with("x402-stellar-x402s"),
        "expected x402 audit_id"
    );
    resp["challenge_id"]
        .as_str()
        .expect("challenge_id")
        .to_string()
}

fn read_jsonl(path: &PathBuf) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("read audit jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).expect("audit json row"))
        .collect()
}

#[test]
fn api_analyze_smoke_and_errors() {
    let port = find_free_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    let child = Command::new(assert_cmd::cargo::cargo_bin!("neurochain-server"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("NC_MODELS_DIR", models_dir())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn neurochain-server");

    // Ensure cleanup even if the test fails.
    let _server = Server { child };

    wait_for_listen(addr, Duration::from_secs(3));

    // 1) Empty input -> ok=false
    let body = json!({"model":"macro","content":""}).to_string();
    let (status, resp_body) = http_post_json(addr, "/api/analyze", &body);
    assert_eq!(status, 200);
    let resp: AnalyzeResp = serde_json::from_str(&resp_body).expect("json parse");
    assert!(!resp.ok, "empty input should return ok=false");

    // 2) Unknown model id should warn but still run simple scripts
    let body = json!({"model":"unknown","content":"neuro \"hi\""}).to_string();
    let (status, resp_body) = http_post_json(addr, "/api/analyze", &body);
    assert_eq!(status, 200);
    let resp: AnalyzeResp = serde_json::from_str(&resp_body).expect("json parse");
    assert!(resp.ok, "unknown model should not break non-AI scripts");
    assert!(
        resp.logs
            .iter()
            .any(|l| l.contains("warn: unknown model id")),
        "expected warn log for unknown model id"
    );

    // 3) Known model id should auto-inject AI model path when missing
    let macro_model = macro_model_path();
    if !macro_model.exists() {
        eprintln!(
            "api_analyze_smoke_and_errors skipped: model not found at {}",
            macro_model.display()
        );
        return;
    }

    let body = json!({"model":"macro","content":"neuro \"hi\""}).to_string();
    let (status, resp_body) = http_post_json(addr, "/api/analyze", &body);
    assert_eq!(status, 200);
    let resp: AnalyzeResp = serde_json::from_str(&resp_body).expect("json parse");
    assert!(resp.ok);
    assert!(
        resp.logs
            .iter()
            .any(|l| l.contains("auto: injected AI model path")),
        "expected auto-injection log"
    );
    assert!(
        resp.logs
            .iter()
            .any(|l| l.contains("intent_macro/model.onnx")),
        "expected injected macro model path in logs"
    );

    // Keep the assertion loose to avoid coupling to exact formatting.
    assert!(
        !resp.output.trim().is_empty(),
        "server output should not be empty"
    );
}

#[test]
fn api_analyze_requires_api_key_when_configured() {
    let port = find_free_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let api_key = "test-key-123";

    let child = Command::new(assert_cmd::cargo::cargo_bin!("neurochain-server"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("NC_MODELS_DIR", models_dir())
        .env("NC_API_KEY", api_key)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn neurochain-server");

    let _server = Server { child };

    wait_for_listen(addr, Duration::from_secs(3));

    let body = json!({"model":"unknown","content":"neuro \"hi\""}).to_string();

    // 1) Missing key -> 401
    let (status, resp_body) = http_post_json(addr, "/api/analyze", &body);
    assert_eq!(status, 401);
    let resp: AnalyzeResp = serde_json::from_str(&resp_body).expect("json parse");
    assert!(!resp.ok);

    // 2) With key -> 200
    let (status, resp_body) =
        http_post_json_with_headers(addr, "/api/analyze", &body, &[("X-API-Key", api_key)]);
    assert_eq!(status, 200);
    let resp: AnalyzeResp = serde_json::from_str(&resp_body).expect("json parse");
    assert!(resp.ok);
    assert!(resp.output.contains("hi"));
}

#[test]
fn api_stellar_intent_plan_smoke_and_blocks() {
    let port = find_free_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    let child = Command::new(assert_cmd::cargo::cargo_bin!("neurochain-server"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("NC_MODELS_DIR", models_dir())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn neurochain-server");

    let _server = Server { child };

    wait_for_listen(addr, Duration::from_secs(3));

    let model = intent_stellar_model_path();
    if !model.exists() {
        eprintln!(
            "api_stellar_intent_plan_smoke_and_blocks skipped: model not found at {}",
            model.display()
        );
        return;
    }

    let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    let body = json!({
        "model": "intent_stellar",
        "prompt": format!("Check balance for {account} asset XLM"),
        "threshold": 0.0
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["blocked"], false);
    assert_eq!(
        resp["plan"]["actions"][0]["kind"],
        "stellar_account_balance"
    );

    let body = json!({
        "model": "intent_stellar",
        "prompt": "Invoke deploy contract alias hello-demo wasm ./contracts/hello.wasm",
        "threshold": 0.0
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["blocked"], false);
    assert_eq!(
        resp["plan"]["actions"][0]["kind"],
        "soroban_contract_deploy"
    );
    assert_eq!(resp["plan"]["actions"][0]["alias"], "hello-demo");
    assert_eq!(resp["plan"]["actions"][0]["wasm"], "./contracts/hello.wasm");

    let body = json!({
        "model": "intent_stellar",
        "prompt": "Please say hello to World",
        "threshold": 0.0
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["blocked"], false);
    assert_eq!(
        resp["plan"]["actions"][0]["kind"],
        "soroban_contract_invoke"
    );
    assert_eq!(resp["plan"]["actions"][0]["function"], "hello");
    assert_eq!(resp["plan"]["actions"][0]["args"]["to"], "World");
    let logs = resp["logs"].as_array().cloned().unwrap_or_default();
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l.contains("soroban_deep_template: expanded=true template=hello")),
        "expected soroban_deep_template expansion log"
    );

    let body = json!({
        "model": "intent_stellar",
        "prompt": "Invoke deploy contract alias hello-demo",
        "threshold": 0.0
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["blocked"], true);
    assert_eq!(resp["exit_code"], 5);
    assert_eq!(resp["plan"]["actions"][0]["kind"], "unknown");
    let warnings = resp["plan"]["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        warnings
            .iter()
            .filter_map(|v| v.as_str())
            .any(|w| w.contains("slot_missing") && w.contains("ContractDeploy missing wasm")),
        "expected ContractDeploy slot_missing warning"
    );

    let body = json!({
        "model": "intent_stellar",
        "prompt": "Tell me a joke about stars",
        "threshold": 0.99
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["blocked"], true);
    assert_eq!(resp["exit_code"], 5);
    assert_eq!(resp["plan"]["actions"][0]["kind"], "unknown");

    let body = json!({
        "model": "intent_stellar",
        "prompt": format!("Send 5 XLM to {account}"),
        "threshold": 0.20,
        "allowlist_assets": "USDC:GISSUER",
        "allowlist_enforce": true
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["blocked"], true);
    assert_eq!(resp["exit_code"], 3);
    let logs = resp["logs"].as_array().cloned().unwrap_or_default();
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l.contains("allowlist: violations=")),
        "expected allowlist summary in logs"
    );
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l == "block: allowlist_enforced"),
        "expected allowlist block marker in logs"
    );

    let body = json!({
        "model": "intent_stellar",
        "prompt": "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello",
        "threshold": 0.00,
        "contract_policy_enforce": true
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["blocked"], true);
    assert_eq!(resp["exit_code"], 4);

    let body = json!({
        "model": "intent_stellar",
        "prompt": "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={\"to\":\" World \"}",
        "threshold": 0.00
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["blocked"], false);
    assert_eq!(
        resp["plan"]["actions"][0]["kind"],
        "soroban_contract_invoke"
    );
    assert_eq!(resp["plan"]["actions"][0]["args"]["to"], "World");
    let logs = resp["logs"].as_array().cloned().unwrap_or_default();
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l.contains("typed_template_v2:") && l.contains("normalized_args=")),
        "expected typed_template_v2 normalized_args summary in logs"
    );

    let body = json!({
        "model": "intent_stellar",
        "prompt": "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={\"to\":\"World\"} arg_types={\"to\":\"address\"}",
        "threshold": 0.00
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["blocked"], true);
    assert_eq!(resp["exit_code"], 5);
    let logs = resp["logs"].as_array().cloned().unwrap_or_default();
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l.contains("typed_template_v2: policy_slot_type_converted=")),
        "expected typed_template_v2 summary in logs"
    );
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l == "block: intent_safety"),
        "expected intent safety block marker in logs"
    );
    let warnings = resp["plan"]["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        warnings
            .iter()
            .filter_map(|v| v.as_str())
            .any(|w| w.contains("slot_type_error")),
        "expected slot_type_error warning"
    );

    let body = json!({
        "model": "intent_stellar",
        "prompt": "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={\"to\":\"Hello World\"}",
        "threshold": 0.00
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["blocked"], true);
    assert_eq!(resp["exit_code"], 5);
    let warnings = resp["plan"]["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        warnings
            .iter()
            .filter_map(|v| v.as_str())
            .any(|w| w.contains("slot_type_error") && w.contains("policy")),
        "expected policy-derived slot_type_error warning"
    );
}

#[test]
fn api_stellar_intent_plan_stage3_typed_v2_parity() {
    let port = find_free_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    let model = intent_stellar_model_path();
    if !model.exists() {
        eprintln!(
            "api_stellar_intent_plan_stage3_typed_v2_parity skipped: model not found at {}",
            model.display()
        );
        return;
    }

    let contract = "CELFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let policy_path = std::env::temp_dir().join("nc_server_api_stage3_typed_v2_policy.json");
    let policy_dir = std::env::temp_dir().join("nc_server_api_stage3_empty_policies");
    let _ = fs::create_dir_all(&policy_dir);
    let policy = format!(
        r#"{{
  "contract_id": "{contract}",
  "allowed_functions": ["hello"],
  "args_schema": {{
    "hello": {{
      "required": {{
        "to": "address",
        "blob": "bytes",
        "ticker": "symbol",
        "amount": "u64"
      }},
      "optional": {{}}
    }}
  }}
}}"#
    );
    fs::write(&policy_path, policy).expect("write temp stage3 policy");

    let child = Command::new(assert_cmd::cargo::cargo_bin!("neurochain-server"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("NC_MODELS_DIR", models_dir())
        .env(
            "NC_CONTRACT_POLICY",
            policy_path.to_string_lossy().to_string(),
        )
        .env(
            "NC_CONTRACT_POLICY_DIR",
            policy_dir.to_string_lossy().to_string(),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn neurochain-server");
    let _server = Server { child };

    wait_for_listen(addr, Duration::from_secs(3));

    let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    let body = json!({
        "model": "intent_stellar",
        "prompt": format!(
            "Invoke contract {contract} function hello args={{\"to\":\" {} \",\"blob\":\"0XDE AD_be-EF\",\"ticker\":\" USDC \",\"amount\":\"1_000,000\"}}",
            account.to_ascii_lowercase()
        ),
        "threshold": 0.00
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["blocked"], false);
    assert_eq!(
        resp["plan"]["actions"][0]["kind"],
        "soroban_contract_invoke"
    );
    assert_eq!(resp["plan"]["actions"][0]["args"]["to"], account);
    assert_eq!(resp["plan"]["actions"][0]["args"]["blob"], "0xdeadbeef");
    assert_eq!(resp["plan"]["actions"][0]["args"]["ticker"], "USDC");
    assert_eq!(resp["plan"]["actions"][0]["args"]["amount"], 1000000);
    let logs = resp["logs"].as_array().cloned().unwrap_or_default();
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l.contains("typed_template_v2:") && l.contains("normalized_args=")),
        "expected typed_template_v2 normalized_args summary in logs"
    );

    let body = json!({
        "model": "intent_stellar",
        "prompt": format!(
            "Invoke contract {contract} function hello args={{\"to\":\"World\",\"blob\":\"0xABC\",\"ticker\":\" BAD VALUE \",\"amount\":\"18446744073709551616\"}}"
        ),
        "threshold": 0.00
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["blocked"], true);
    assert_eq!(resp["exit_code"], 5);
    assert_eq!(resp["plan"]["actions"][0]["kind"], "unknown");
    let warnings = resp["plan"]["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let warning_text = warnings
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(warning_text.contains("slot_type_error"));
    assert!(warning_text.contains("ContractInvoke to"));
    assert!(warning_text.contains("ContractInvoke blob"));
    assert!(warning_text.contains("ContractInvoke ticker"));
    assert!(warning_text.contains("ContractInvoke amount"));
    let logs = resp["logs"].as_array().cloned().unwrap_or_default();
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l.contains("typed_template_v2: policy_slot_type_converted=1")),
        "expected typed_template_v2 conversion summary in logs"
    );
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l == "block: intent_safety"),
        "expected intent safety block marker in logs"
    );

    let _ = fs::remove_file(&policy_path);
    let _ = fs::remove_dir(&policy_dir);
}

#[test]
fn api_x402_stellar_intent_plan_requires_payment_finalizes_and_blocks_replay() {
    let model = intent_stellar_model_path();
    if !model.exists() {
        eprintln!(
            "api_x402_stellar_intent_plan_requires_payment_finalizes_and_blocks_replay skipped: model not found at {}",
            model.display()
        );
        return;
    }

    let port = find_free_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let audit_path = std::env::temp_dir().join(format!("nc_x402_stellar_audit_flow_{port}.jsonl"));
    let _ = fs::remove_file(&audit_path);
    let audit_path_s = audit_path.to_string_lossy().to_string();
    let _server = spawn_server(
        port,
        &[("NC_X402_STELLAR_AUDIT_PATH", audit_path_s.as_str())],
    );
    wait_for_listen(addr, Duration::from_secs(3));

    let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    let body = json!({
        "model": "intent_stellar",
        "prompt": format!("Check balance for {account} asset XLM"),
        "threshold": 0.0
    })
    .to_string();

    let (status, resp_body) = http_post_json(addr, "/api/x402/stellar/intent-plan", &body);
    assert_eq!(status, 402);
    let challenge_id = payment_challenge_id(&resp_body);
    let signature = format!("paid:{challenge_id}");

    let (status, resp_body) = http_post_json_with_headers(
        addr,
        "/api/x402/stellar/intent-plan",
        &body,
        &[("PAYMENT-SIGNATURE", signature.as_str())],
    );
    assert_eq!(status, 200);
    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["blocked"], false);
    assert_eq!(
        resp["audit_id"].as_str().unwrap(),
        format!("x402-stellar-{challenge_id}")
    );
    assert_eq!(resp["payment"]["state"], "finalized");
    assert_eq!(
        resp["payment"]["challenge_id"].as_str().unwrap(),
        challenge_id
    );
    assert_eq!(resp["decision"]["status"], "approved");
    assert_eq!(resp["decision"]["approved"], true);
    assert_eq!(resp["decision"]["blocked"], false);
    assert_eq!(resp["decision"]["requires_approval"], false);
    assert_eq!(resp["guardrails"]["state"], "passed");
    assert_eq!(
        resp["plan"]["actions"][0]["kind"],
        "stellar_account_balance"
    );
    let logs = resp["logs"].as_array().cloned().unwrap_or_default();
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|log| log.contains("x402: finalized challenge=")),
        "expected finalized x402 log"
    );

    let (status, resp_body) = http_post_json_with_headers(
        addr,
        "/api/x402/stellar/intent-plan",
        &body,
        &[("PAYMENT-SIGNATURE", signature.as_str())],
    );
    assert_eq!(status, 409);
    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["error"], "payment_replay_blocked");
    assert_eq!(resp["audit_id"], format!("x402-stellar-{challenge_id}"));
    assert_eq!(resp["payment"]["state"], "replay_blocked");
    assert_eq!(resp["decision"]["status"], "blocked");
    assert_eq!(resp["decision"]["blocked"], true);
    assert_eq!(resp["guardrails"]["state"], "not_run");

    let audit_raw = fs::read_to_string(&audit_path).expect("read audit jsonl");
    assert!(
        !audit_raw.contains("PAYMENT-SIGNATURE") && !audit_raw.contains("paid:"),
        "audit rows must not leak raw payment signature material"
    );
    let rows = read_jsonl(&audit_path);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["event"], "payment_required");
    assert_eq!(rows[0]["payment"]["state"], "payment_required");
    assert_eq!(rows[0]["decision"]["status"], "not_evaluated");
    assert_eq!(rows[0]["guardrails"]["state"], "not_run");
    assert_eq!(rows[1]["event"], "approved");
    assert_eq!(rows[1]["payment"]["state"], "finalized");
    assert_eq!(rows[1]["decision"]["status"], "approved");
    assert_eq!(rows[1]["guardrails"]["state"], "passed");
    assert_eq!(rows[2]["event"], "payment_replay_blocked");
    assert_eq!(rows[2]["payment"]["state"], "replay_blocked");
    assert_eq!(rows[2]["decision"]["status"], "blocked");
    assert_eq!(rows[2]["guardrails"]["state"], "not_run");

    let _ = fs::remove_file(&audit_path);
}

#[test]
fn api_x402_stellar_intent_plan_payment_does_not_bypass_allowlist_guardrail() {
    let model = intent_stellar_model_path();
    if !model.exists() {
        eprintln!(
            "api_x402_stellar_intent_plan_payment_does_not_bypass_allowlist_guardrail skipped: model not found at {}",
            model.display()
        );
        return;
    }

    let port = find_free_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let audit_path =
        std::env::temp_dir().join(format!("nc_x402_stellar_audit_guardrail_{port}.jsonl"));
    let _ = fs::remove_file(&audit_path);
    let audit_path_s = audit_path.to_string_lossy().to_string();
    let _server = spawn_server(
        port,
        &[("NC_X402_STELLAR_AUDIT_PATH", audit_path_s.as_str())],
    );
    wait_for_listen(addr, Duration::from_secs(3));

    let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    let body = json!({
        "model": "intent_stellar",
        "prompt": format!("Send 5 XLM to {account}"),
        "threshold": 0.20,
        "allowlist_assets": "USDC:GISSUER",
        "allowlist_enforce": true
    })
    .to_string();

    let (status, resp_body) = http_post_json(addr, "/api/x402/stellar/intent-plan", &body);
    assert_eq!(status, 402);
    let signature = format!("paid:{}", payment_challenge_id(&resp_body));

    let (status, resp_body) = http_post_json_with_headers(
        addr,
        "/api/x402/stellar/intent-plan",
        &body,
        &[("PAYMENT-SIGNATURE", signature.as_str())],
    );
    assert_eq!(status, 200);
    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["blocked"], true);
    assert_eq!(resp["exit_code"], 3);
    assert_eq!(resp["payment"]["state"], "finalized");
    assert_eq!(resp["decision"]["status"], "blocked");
    assert_eq!(resp["decision"]["approved"], false);
    assert_eq!(resp["decision"]["blocked"], true);
    assert_eq!(resp["decision"]["requires_approval"], false);
    assert_eq!(resp["decision"]["reason"], "allowlist");
    assert_eq!(resp["guardrails"]["state"], "blocked");
    assert_eq!(resp["guardrails"]["exit_code"], 3);
    assert_eq!(resp["guardrails"]["reason"], "allowlist");
    let logs = resp["logs"].as_array().cloned().unwrap_or_default();
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|log| log.contains("x402: finalized challenge=")),
        "expected finalized x402 log"
    );
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|log| log == "block: allowlist_enforced"),
        "expected allowlist block after payment"
    );

    let audit_raw = fs::read_to_string(&audit_path).expect("read audit jsonl");
    assert!(
        !audit_raw.contains("PAYMENT-SIGNATURE") && !audit_raw.contains("paid:"),
        "audit rows must not leak raw payment signature material"
    );
    let rows = read_jsonl(&audit_path);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["event"], "payment_required");
    assert_eq!(rows[1]["event"], "blocked");
    assert_eq!(rows[1]["payment"]["state"], "finalized");
    assert_eq!(rows[1]["decision"]["status"], "blocked");
    assert_eq!(rows[1]["decision"]["reason"], "allowlist");
    assert_eq!(rows[1]["guardrails"]["state"], "blocked");
    assert_eq!(rows[1]["guardrails"]["exit_code"], 3);

    let _ = fs::remove_file(&audit_path);
}

#[test]
fn api_x402_stellar_intent_plan_expired_challenge_blocks_finalize() {
    let port = find_free_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let audit_path =
        std::env::temp_dir().join(format!("nc_x402_stellar_audit_expired_{port}.jsonl"));
    let _ = fs::remove_file(&audit_path);
    let audit_path_s = audit_path.to_string_lossy().to_string();
    let _server = spawn_server(
        port,
        &[
            ("NC_X402_STELLAR_TTL_SECS", "0"),
            ("NC_X402_STELLAR_AUDIT_PATH", audit_path_s.as_str()),
        ],
    );
    wait_for_listen(addr, Duration::from_secs(3));

    let body = json!({
        "model": "intent_stellar",
        "prompt": "Check balance for GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX asset XLM",
        "threshold": 0.0
    })
    .to_string();

    let (status, resp_body) = http_post_json(addr, "/api/x402/stellar/intent-plan", &body);
    assert_eq!(status, 402);
    let signature = format!("paid:{}", payment_challenge_id(&resp_body));

    let (status, resp_body) = http_post_json_with_headers(
        addr,
        "/api/x402/stellar/intent-plan",
        &body,
        &[("PAYMENT-SIGNATURE", signature.as_str())],
    );
    assert_eq!(status, 402);
    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["error"], "payment_expired");
    assert_eq!(resp["payment"]["state"], "expired");
    assert_eq!(resp["decision"]["status"], "blocked");
    assert_eq!(resp["decision"]["reason"], "payment_expired");
    assert_eq!(resp["guardrails"]["state"], "not_run");
    let logs = resp["logs"].as_array().cloned().unwrap_or_default();
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|log| log.contains("x402: expired challenge=")),
        "expected expired x402 log"
    );

    let audit_raw = fs::read_to_string(&audit_path).expect("read audit jsonl");
    assert!(
        !audit_raw.contains("PAYMENT-SIGNATURE") && !audit_raw.contains("paid:"),
        "audit rows must not leak raw payment signature material"
    );
    let rows = read_jsonl(&audit_path);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["event"], "payment_required");
    assert_eq!(rows[1]["event"], "payment_expired");
    assert_eq!(rows[1]["payment"]["state"], "expired");
    assert_eq!(rows[1]["decision"]["status"], "blocked");
    assert_eq!(rows[1]["guardrails"]["state"], "not_run");

    let _ = fs::remove_file(&audit_path);
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
