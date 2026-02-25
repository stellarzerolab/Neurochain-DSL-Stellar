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
set stellar intent from AI: "Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.20")
        .output()
        .expect("run neurochain-stellar script mode");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Script execution setup:"));
    assert!(combined.contains("- source:"));
    assert!(combined.contains("- network: testnet"));
    assert!(combined.contains("- flow_mode: off"));
    assert!(combined.contains("\"kind\": \"stellar_payment\""));
    assert!(combined.contains("\"asset_code\": \"XLM\""));
    assert!(combined.contains("intent_model: path=models/intent_stellar/model.onnx"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_supports_if_gate_with_multiple_models() {
    let intent_model = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    let sst2_model = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("distilbert-sst2")
        .join("model.onnx");
    if !intent_model.exists() {
        eprintln!("skipping test; missing model: {}", intent_model.display());
        return;
    }
    if !sst2_model.exists() {
        eprintln!("skipping test; missing model: {}", sst2_model.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_if_gate_multimodel.nc");
    let script = r#"
AI: "models/distilbert-sst2/model.onnx"
set mood from AI: "This is wonderful!"
if mood == "Positive":
    AI: "models/intent_stellar/model.onnx"
    set stellar intent from AI: "Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"
else:
    neuro "No payment"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.20")
        .output()
        .expect("run neurochain-stellar script mode");

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

#[test]
fn nc_script_model_agnostic_gate_golden_path_produces_payment_plan() {
    let intent_model = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    let sst2_model = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("distilbert-sst2")
        .join("model.onnx");
    if !intent_model.exists() {
        eprintln!("skipping test; missing model: {}", intent_model.display());
        return;
    }
    if !sst2_model.exists() {
        eprintln!("skipping test; missing model: {}", sst2_model.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_model_agnostic_gate_golden.nc");
    let script = r#"
network: testnet
wallet: nc-testnet
AI: "models/distilbert-sst2/model.onnx"
set gate from AI: "This is wonderful!"
set allow_label = "Positive"
if gate == allow_label:
    AI: "models/intent_stellar/model.onnx"
    set stellar intent from AI: "Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"
else:
    neuro "Gate blocked payment"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.20")
        .output()
        .expect("run neurochain-stellar script mode");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("\"kind\": \"stellar_payment\""));
    assert!(combined.contains("\"asset_code\": \"XLM\""));
    assert!(combined.contains("Script execution setup:"));
    assert!(combined.contains("- network: testnet"));
    assert!(combined.contains("- source:"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_allowlist_settings_can_enforce_without_env() {
    let tmp = std::env::temp_dir().join("nc_script_allowlist_enforce.nc");
    let script = r#"
asset_allowlist: USDC:GISSUER
allowlist_enforce
stellar.payment to="GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P" amount="5" asset_code="XLM"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .output()
        .expect("run neurochain-stellar script mode");

    assert_eq!(output.status.code(), Some(3));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Allowlist violations (enforced)"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_policy_settings_can_enforce_without_env() {
    let policy_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("contracts")
        .join("CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ")
        .join("policy.json");
    if !policy_path.exists() {
        eprintln!("skipping test; missing policy: {}", policy_path.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_policy_settings_enforce.nc");
    let script = format!(
        "contract_policy: {}\ncontract_policy_enforce\nAI: \"models/intent_stellar/model.onnx\"\nset stellar intent from AI: \"Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello\"\n",
        policy_path.to_string_lossy()
    );
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.00")
        .output()
        .expect("run neurochain-stellar script mode with policy settings");

    assert_eq!(output.status.code(), Some(4));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Contract policy violations (enforced):"));
    assert!(combined.contains("- contract_policy:"));
    assert!(combined.contains("- contract_policy_enforce: on"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_intent_safety_blocks_flow_with_exit_5() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_intent_safety_block.nc");
    let script = r#"
AI: "models/intent_stellar/model.onnx"
set stellar intent from AI: "Tell me a joke about stars"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.99")
        .arg("--flow")
        .arg("--yes")
        .output()
        .expect("run neurochain-stellar script mode");

    assert_eq!(output.status.code(), Some(5));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Intent safety guard blocked flow"));
    assert!(combined.contains("\"kind\": \"unknown\""));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_policy_enforced_blocks_with_exit_4() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let policy_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("contracts")
        .join("CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ")
        .join("policy.json");
    if !policy_path.exists() {
        eprintln!("skipping test; missing policy: {}", policy_path.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_policy_enforced_block.nc");
    let script = r#"
AI: "models/intent_stellar/model.onnx"
set stellar intent from AI: "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.00")
        .arg("--flow")
        .arg("--yes")
        .env(
            "NC_CONTRACT_POLICY",
            policy_path.to_string_lossy().to_string(),
        )
        .env("NC_CONTRACT_POLICY_ENFORCE", "1")
        .output()
        .expect("run neurochain-stellar script mode");

    assert_eq!(output.status.code(), Some(4));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Contract policy violations (enforced):"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_policy_typed_slot_error_blocks_flow_with_exit_5() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let policy_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("contracts")
        .join("CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ")
        .join("policy.json");
    if !policy_path.exists() {
        eprintln!("skipping test; missing policy: {}", policy_path.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_policy_typed_slot_error_block.nc");
    let script = format!(
        "contract_policy: {}\nAI: \"models/intent_stellar/model.onnx\"\nset stellar intent from AI: \"Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={{\"to\":\"Hello World\"}}\"\n",
        policy_path.to_string_lossy()
    );
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.00")
        .arg("--flow")
        .arg("--yes")
        .output()
        .expect("run neurochain-stellar script mode");

    assert_eq!(output.status.code(), Some(5));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("slot_type_error"));
    assert!(combined.contains("Intent safety guard blocked flow"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_typed_slot_error_blocks_flow_with_exit_5() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_typed_slot_error_block.nc");
    let script = r#"
AI: "models/intent_stellar/model.onnx"
set stellar intent from AI: "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={"to":"World"} arg_types={"to":"address"}"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.00")
        .arg("--flow")
        .arg("--yes")
        .output()
        .expect("run neurochain-stellar script mode");

    assert_eq!(output.status.code(), Some(5));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("slot_type_error"));
    assert!(combined.contains("Intent safety guard blocked flow"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_rejects_macro_from_ai_in_stellar_mode() {
    let tmp = std::env::temp_dir().join("nc_script_macro_from_ai_rejected.nc");
    let script = r#"
AI: "models/intent_macro/model.onnx"
macro from AI: "Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .output()
        .expect("run neurochain-stellar script mode");

    assert_eq!(output.status.code(), Some(1));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("macro from AI is not supported in neurochain-stellar"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_set_var_from_ai_fails_fast_without_fallback() {
    let tmp = std::env::temp_dir().join("nc_script_set_var_failfast.nc");
    let script = r#"
AI: "models/does_not_exist/model.onnx"
set mood from AI: "This is wonderful!"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .output()
        .expect("run neurochain-stellar script mode");

    assert_eq!(output.status.code(), Some(1));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("set_from_ai_failed: variable `mood`"));
    assert!(!combined.contains("raw prompt fallback"));
    assert!(!combined.contains("set_from_ai_fallback"));

    let _ = fs::remove_file(tmp);
}
