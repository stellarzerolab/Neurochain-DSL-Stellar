use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn nc_script_supports_ai_network_wallet_and_intent_lines() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_intent_mode.nc");
    let script = r#"
AI: "models/intent_stellar/model.onnx"
network: testnet
wallet: nc-testnet
set intent from AI: "Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-soroban");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.20")
        .output()
        .expect("run neurochain-soroban script mode");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("\"kind\": \"stellar_payment\""));
    assert!(combined.contains("\"asset_code\": \"XLM\""));
    assert!(combined.contains("intent_model: path=models/intent_stellar/model.onnx"));

    let _ = fs::remove_file(tmp);
}
