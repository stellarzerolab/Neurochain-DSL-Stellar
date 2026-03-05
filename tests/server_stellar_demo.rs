use std::{
    fs,
    io::ErrorKind,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use serde::Deserialize;
use serde_json::json;
use tempfile::TempDir;

const TEST_ACCOUNT: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const CONTRACT_ID: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const FUNDBOT_HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const DEPLOY_HASH: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const INVOKE_HASH: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const API_KEY: &str = "stellar-demo-test-key";

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct DemoResp {
    ok: bool,
    error: Option<String>,
    state: DemoState,
    logs: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
struct DemoState {
    alias: Option<String>,
    account: Option<String>,
    #[serde(default)]
    balances: Vec<String>,
    friendbot_status: Option<String>,
    contract_id: Option<String>,
    contract_alias: Option<String>,
    last_tx_hash: Option<String>,
    last_tx_status: Option<String>,
    last_result: Option<String>,
    explorer_url: Option<String>,
}

struct Server {
    child: Child,
    _temp: TempDir,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn find_free_port() -> u16 {
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

fn http_post_json(
    addr: SocketAddr,
    path: &str,
    json_body: &str,
    api_key: Option<&str>,
) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("set_read_timeout");

    let mut extra_headers = String::new();
    if let Some(key) = api_key {
        extra_headers.push_str(&format!("x-api-key: {key}\r\n"));
    }

    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nConnection: close\r\n{extra}Content-Length: {len}\r\n\r\n{body}",
        host = addr,
        extra = extra_headers,
        len = json_body.len(),
        body = json_body,
    );
    stream.write_all(req.as_bytes()).expect("write request");

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
    };

    let (header_pos, header_len) = header_end;
    let split_at = header_pos + header_len;
    let (head_bytes, body_bytes) = buf.split_at(split_at);
    let head_str = String::from_utf8_lossy(head_bytes);

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

    let body_str = String::from_utf8_lossy(&body).to_string();
    (code, body_str)
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn spawn_mock_stellar_services() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().unwrap();
    let tx_counter = Arc::new(AtomicUsize::new(0));
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = [0u8; 4096];
            let n = match stream.read(&mut buf) {
                Ok(n) => n,
                Err(_) => continue,
            };
            if n == 0 {
                continue;
            }
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = req
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");

            let (status, body) = if path.starts_with("/accounts/") && path.contains("/transactions")
            {
                let idx = tx_counter.fetch_add(1, Ordering::SeqCst);
                let hash = match idx {
                    0 => DEPLOY_HASH,
                    _ => INVOKE_HASH,
                };
                (
                    "200 OK",
                    format!(r#"{{"_embedded":{{"records":[{{"hash":"{hash}"}}]}}}}"#),
                )
            } else if path.starts_with("/accounts/") {
                (
                    "200 OK",
                    r#"{"balances":[{"asset_type":"native","balance":"10000.0000000"}]}"#
                        .to_string(),
                )
            } else if path.starts_with("/friendbot") {
                (
                    "200 OK",
                    format!(r#"{{"hash":"{FUNDBOT_HASH}","successful":true}}"#),
                )
            } else if path.starts_with(&format!("/transactions/{FUNDBOT_HASH}")) {
                (
                    "200 OK",
                    format!(r#"{{"successful":true,"ledger":101,"hash":"{FUNDBOT_HASH}"}}"#),
                )
            } else if path.starts_with(&format!("/transactions/{DEPLOY_HASH}")) {
                (
                    "200 OK",
                    format!(r#"{{"successful":true,"ledger":102,"hash":"{DEPLOY_HASH}"}}"#),
                )
            } else if path.starts_with(&format!("/transactions/{INVOKE_HASH}")) {
                (
                    "200 OK",
                    format!(r#"{{"successful":true,"ledger":103,"hash":"{INVOKE_HASH}"}}"#),
                )
            } else {
                ("404 Not Found", "not found".to_string())
            };

            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{}", addr)
}

fn write_fake_stellar_cli(dir: &Path) -> PathBuf {
    let path = dir.join("stellar");
    let script = format!(
        r#"#!/bin/sh
set -eu
config_dir=""
if [ "$#" -ge 2 ] && [ "$1" = "--config-dir" ]; then
  config_dir="$2"
  shift 2
fi
cmd="$1"
shift
case "$cmd" in
  keys)
    sub="$1"
    shift
    case "$sub" in
      generate)
        alias="$1"
        mkdir -p "$config_dir"
        printf '%s\n' "$alias" > "$config_dir/last-alias"
        exit 0
        ;;
      public-key)
        printf '%s\n' "{account}"
        exit 0
        ;;
    esac
    ;;
  contract)
    sub="$1"
    shift
    case "$sub" in
      build)
        outdir=""
        while [ "$#" -gt 0 ]; do
          if [ "$1" = "--out-dir" ]; then
            outdir="$2"
            shift 2
            continue
          fi
          shift
        done
        mkdir -p "$outdir"
        : > "$outdir/stellar_demo_contract.wasm"
        printf '%s\n' "built"
        exit 0
        ;;
      deploy)
        deploy_alias=""
        deploy_wasm=""
        while [ "$#" -gt 0 ]; do
          case "$1" in
            --alias)
              deploy_alias="$2"
              shift 2
              continue
              ;;
            --wasm)
              deploy_wasm="$2"
              shift 2
              continue
              ;;
          esac
          shift
        done
        mkdir -p "$config_dir"
        printf '%s\n' "$deploy_alias" > "$config_dir/last-contract-alias"
        printf '%s\n' "$deploy_wasm" > "$config_dir/last-contract-wasm"
        printf '%s\n' "{contract}"
        exit 0
        ;;
      invoke)
        printf '%s\n' "1"
        exit 0
        ;;
    esac
    ;;
esac
printf 'unexpected args: %s\n' "$cmd $*" >&2
exit 1
"#,
        account = TEST_ACCOUNT,
        contract = CONTRACT_ID,
    );
    fs::write(&path, script).expect("write fake stellar cli");
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).expect("chmod fake stellar cli");
    }
    path
}

fn spawn_server() -> (Server, SocketAddr) {
    #[cfg(not(unix))]
    panic!("server_stellar_demo tests currently require unix shell support");

    let port = find_free_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let temp = tempfile::tempdir().expect("tempdir");
    let fake_cli = write_fake_stellar_cli(temp.path());
    let base_url = spawn_mock_stellar_services();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("demo_contracts")
        .join("stellar_demo_contract")
        .join("Cargo.toml");

    let child = Command::new(assert_cmd::cargo::cargo_bin!(
        "neurochain-stellar-demo-server"
    ))
    .current_dir(env!("CARGO_MANIFEST_DIR"))
    .env("HOST", "127.0.0.1")
    .env("PORT", port.to_string())
    .env("NC_STELLAR_CLI", fake_cli.to_string_lossy().to_string())
    .env(
        "NC_STELLAR_DEMO_CONFIG_DIR",
        temp.path().join("config").to_string_lossy().to_string(),
    )
    .env(
        "NC_STELLAR_DEMO_BUILD_DIR",
        temp.path().join("build").to_string_lossy().to_string(),
    )
    .env(
        "NC_STELLAR_DEMO_MANIFEST_PATH",
        manifest.to_string_lossy().to_string(),
    )
    .env("NC_STELLAR_HORIZON_URL", &base_url)
    .env("NC_FRIENDBOT_URL", format!("{}/friendbot", base_url))
    .env("NC_API_KEY", API_KEY)
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .expect("spawn neurochain-stellar-demo-server");

    let server = Server { child, _temp: temp };
    wait_for_listen(addr, Duration::from_secs(3));
    (server, addr)
}

#[test]
fn stellar_demo_workspace_flow_smoke() {
    let (server, addr) = spawn_server();

    let (status, body) = http_post_json(
        addr,
        "/api/stellar/demo/workspace/create",
        &json!({}).to_string(),
        None,
    );
    assert_eq!(status, 401, "expected unauthorized without API key: {body}");

    let (status, body) = http_post_json(
        addr,
        "/api/stellar/demo/workspace/create",
        &json!({}).to_string(),
        Some(API_KEY),
    );
    assert_eq!(status, 200);
    let create: DemoResp = serde_json::from_str(&body).expect("parse create resp");
    assert!(create.ok, "workspace create failed: {body}");
    let alias = create.state.alias.clone().expect("alias");
    assert_eq!(create.state.account.as_deref(), Some(TEST_ACCOUNT));
    assert!(create.logs.iter().any(|line| line.contains("alias=")));

    let (status, body) = http_post_json(
        addr,
        "/api/stellar/demo/workspace/fund",
        &json!({ "alias": alias }).to_string(),
        Some(API_KEY),
    );
    assert_eq!(status, 200);
    let fund: DemoResp = serde_json::from_str(&body).expect("parse fund resp");
    assert!(fund.ok, "workspace fund failed: {body}");
    assert_eq!(
        fund.state.friendbot_status.as_deref(),
        Some("friendbot funded account")
    );
    assert_eq!(fund.state.last_tx_hash.as_deref(), Some(FUNDBOT_HASH));
    assert!(fund.state.balances.iter().any(|b| b.contains("XLM")));

    let alias = fund.state.alias.clone().expect("alias after fund");
    let (status, body) = http_post_json(
        addr,
        "/api/stellar/demo/contract/deploy",
        &json!({ "alias": alias.clone() }).to_string(),
        Some(API_KEY),
    );
    assert_eq!(status, 200);
    let deploy: DemoResp = serde_json::from_str(&body).expect("parse deploy resp");
    assert!(deploy.ok, "contract deploy failed: {body}");
    assert_eq!(deploy.state.contract_id.as_deref(), Some(CONTRACT_ID));
    assert_eq!(deploy.state.last_tx_hash.as_deref(), Some(DEPLOY_HASH));
    assert!(deploy
        .state
        .explorer_url
        .as_deref()
        .unwrap_or_default()
        .contains(DEPLOY_HASH));
    let used_default_contract_alias = fs::read_to_string(
        server
            ._temp
            .path()
            .join("config")
            .join("last-contract-alias"),
    )
    .expect("read last-contract-alias after default deploy");
    assert_eq!(used_default_contract_alias.trim(), format!("{alias}-demo"));

    let override_wasm_path = server._temp.path().join("custom_demo_contract.wasm");
    fs::write(&override_wasm_path, b"demo wasm").expect("write override wasm");
    let requested_contract_alias = "intent-hello-demo";
    let (status, body) = http_post_json(
        addr,
        "/api/stellar/demo/contract/deploy",
        &json!({
          "alias": alias.clone(),
          "contract_alias": requested_contract_alias,
          "wasm": override_wasm_path.to_string_lossy().to_string()
        })
        .to_string(),
        Some(API_KEY),
    );
    assert_eq!(status, 200);
    let deploy_override: DemoResp =
        serde_json::from_str(&body).expect("parse override deploy resp");
    assert!(deploy_override.ok, "override deploy failed: {body}");
    assert_eq!(
        deploy_override.state.contract_id.as_deref(),
        Some(CONTRACT_ID)
    );
    assert_eq!(
        deploy_override.state.contract_alias.as_deref(),
        Some(requested_contract_alias)
    );
    let used_override_contract_alias = fs::read_to_string(
        server
            ._temp
            .path()
            .join("config")
            .join("last-contract-alias"),
    )
    .expect("read last-contract-alias after override deploy");
    assert_eq!(
        used_override_contract_alias.trim(),
        requested_contract_alias
    );
    let used_override_wasm = fs::read_to_string(
        server
            ._temp
            .path()
            .join("config")
            .join("last-contract-wasm"),
    )
    .expect("read last-contract-wasm after override deploy");
    assert_eq!(
        used_override_wasm.trim(),
        override_wasm_path.to_string_lossy()
    );

    let alias = deploy_override
        .state
        .alias
        .clone()
        .expect("alias after override deploy");
    let contract_id = deploy_override
        .state
        .contract_id
        .clone()
        .expect("contract_id");
    let (status, body) = http_post_json(
        addr,
        "/api/stellar/demo/contract/invoke",
        &json!({ "alias": alias, "contract_id": contract_id }).to_string(),
        Some(API_KEY),
    );
    assert_eq!(status, 200);
    let invoke: DemoResp = serde_json::from_str(&body).expect("parse invoke resp");
    assert!(invoke.ok, "contract invoke failed: {body}");
    assert_eq!(invoke.state.last_tx_hash.as_deref(), Some(INVOKE_HASH));
    assert_eq!(invoke.state.last_result.as_deref(), Some("1"));
    assert!(invoke
        .state
        .last_tx_status
        .as_deref()
        .unwrap_or_default()
        .contains("successful=true"));

    let (status, body) = http_post_json(
        addr,
        "/api/stellar/demo/tx/status",
        &json!({ "hash": INVOKE_HASH }).to_string(),
        Some(API_KEY),
    );
    assert_eq!(status, 200);
    let tx_status: DemoResp = serde_json::from_str(&body).expect("parse tx status resp");
    assert!(tx_status.ok, "tx status failed: {body}");
    assert_eq!(tx_status.state.last_tx_hash.as_deref(), Some(INVOKE_HASH));
    assert!(tx_status
        .state
        .last_tx_status
        .as_deref()
        .unwrap_or_default()
        .contains("ledger=103"));
}
