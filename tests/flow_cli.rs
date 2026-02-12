use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::thread;

fn spawn_test_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = [0u8; 2048];
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

            let (status, body) = if path.starts_with("/fee_stats") {
                ("200 OK", r#"{"last_ledger_base_fee":"100"}"#.to_string())
            } else if path.starts_with("/accounts/") {
                (
                    "200 OK",
                    r#"{"balances":[{"asset_type":"native","balance":"10000.0000000"}]}"#
                        .to_string(),
                )
            } else if path.starts_with("/transactions/") {
                ("400 Bad Request", r#"{"status":400}"#.to_string())
            } else if path.starts_with("/friendbot") {
                ("400 Bad Request", "bad request".to_string())
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

#[test]
fn flow_preview_reports_friendbot_and_tx_status_errors() {
    let base_url = spawn_test_server();
    let friendbot_url = format!("{}/friendbot", base_url);

    let tmp = std::env::temp_dir().join("nc_flow_test.nc");
    let input = r#"
stellar.account.balance account="GTEST" asset="XLM"
stellar.account.fund_testnet account="GTEST"
stellar.tx.status hash="ABC123"
"#;
    fs::write(&tmp, input).expect("write temp nc");

    let bin = env!("CARGO_BIN_EXE_neurochain-soroban");
    let output = Command::new(bin)
        .arg(tmp.to_str().unwrap())
        .arg("--flow")
        .arg("--yes")
        .env("NC_STELLAR_HORIZON_URL", &base_url)
        .env("NC_FRIENDBOT_URL", &friendbot_url)
        .output()
        .expect("run neurochain-soroban");

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(combined.contains("Estimated fee: 100 stroops x 0 ops = 0 stroops"));
    assert!(combined.contains("balance GTEST: XLM = 10000.0000000"));
    assert!(combined.contains("friendbot failed"));
    assert!(combined.contains("simulate_error: tx status failed"));
}

#[test]
fn intent_mode_low_confidence_blocks_flow_and_returns_exit_5() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let bin = env!("CARGO_BIN_EXE_neurochain-soroban");
    let output = Command::new(bin)
        .arg("--intent-text")
        .arg("Tell me a joke about stars")
        .arg("--intent-model")
        .arg(model_path.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.99")
        .arg("--flow")
        .arg("--yes")
        .output()
        .expect("run neurochain-soroban in intent mode");

    assert_eq!(output.status.code(), Some(5));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Intent safety guard blocked flow"));
    assert!(combined.contains("\"kind\": \"unknown\""));
    assert!(!combined.contains("=== Preview ==="));
}
