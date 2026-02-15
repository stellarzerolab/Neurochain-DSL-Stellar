use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::path::PathBuf;

fn assert_contains_in_order(haystack: &str, needles: &[&str]) {
    let mut pos = 0usize;
    for needle in needles {
        let Some(offset) = haystack[pos..].find(needle) else {
            panic!("expected to find `{needle}` after byte position {pos}\n{haystack}");
        };
        pos += offset + needle.len();
    }
}

fn help_row(command: &str, description: &str) -> String {
    format!("- {:<42} {}", command, description)
}

#[test]
fn stellar_repl_help_and_exit_work() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.write_stdin("help\n\nexit\n\n")
        .assert()
        .success()
        .stdout(contains("NeuroChain Stellar REPL"))
        .stdout(contains("Soroban REPL quick start"))
        .stdout(contains(
            "- help dsl            (show normal NeuroChain DSL help)",
        ))
        .stdout(contains(
            "Toggle commands are listed in `help all` under Toggles (on/off).",
        ))
        .stdout(contains("Exiting"));
}

#[test]
fn stellar_repl_starts_with_flow_flag_only() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.arg("--flow")
        .write_stdin("exit\n\n")
        .assert()
        .success()
        .stdout(contains("NeuroChain Stellar REPL"))
        .stdout(contains("Exiting"));
}

#[test]
fn stellar_repl_accepts_ai_model_line() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.write_stdin("AI: \"models/intent_stellar/model.onnx\"\n\nexit\n\n")
        .assert()
        .success()
        .stdout(contains(
            "Intent model path set to: models/intent_stellar/model.onnx",
        ))
        .stdout(contains("Exiting"));
}

#[test]
fn stellar_repl_accepts_network_and_wallet_commands() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.write_stdin("set network = \"testnet\"\n\nset wallet = \"nc-testnet\"\n\nexit\n\n")
        .assert()
        .success()
        .stdout(contains("Network set to: testnet"))
        .stdout(contains("Wallet/source set to: nc-testnet"))
        .stdout(contains("Exiting"));
}

#[test]
fn stellar_repl_accepts_runtime_setting_commands() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.write_stdin(
        "intent_threshold: 0.60\n\nhorizon: https://horizon-testnet.stellar.org\n\nfriendbot: off\n\nstellar_cli: stellar\n\nsimulate_flag: \"--send no\"\n\ntxrep\n\ntxrep off\n\nasset_allowlist: XLM\n\nsoroban_allowlist: CTEST:transfer\n\nallowlist_enforce\n\ndebug\n\ndebug off\n\nallowlist_enforce off\n\nallowlist_enforce\n\nexit\n\n",
    )
    .assert()
    .success()
    .stdout(contains("Intent threshold set to: 0.60"))
    .stdout(contains("Horizon URL set to: https://horizon-testnet.stellar.org"))
    .stdout(contains("Friendbot set to: (disabled)"))
    .stdout(contains("Stellar CLI binary set to: stellar"))
    .stdout(contains("Soroban simulate flag set to: --send no"))
    .stdout(contains("Txrep preview: enabled"))
    .stdout(contains("Txrep preview: disabled"))
    .stdout(contains("Asset allowlist set to: XLM"))
    .stdout(contains("Soroban allowlist set to: CTEST:transfer"))
    .stdout(contains("Allowlist enforce: enabled"))
    .stdout(contains("Intent debug trace: enabled"))
    .stdout(contains("Intent debug trace: disabled"))
    .stdout(contains("Allowlist enforce: disabled"))
    .stdout(contains("Exiting"));
}

#[test]
fn stellar_repl_supports_help_all_show_config_and_setup_testnet() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.write_stdin(
        "help all\n\nshow config\n\nsetup testnet\n\ntxrep off\n\nshow config\n\nexit\n\n",
    )
    .assert()
    .success()
    .stdout(contains("Soroban REPL commands (all)"))
    .stdout(contains("Current REPL config:"))
    .stdout(contains(
        "Applied testnet baseline (network+horizon+friendbot).",
    ))
    .stdout(contains("Txrep preview: disabled"))
    .stdout(contains("- txrep_preview: off"))
    .stdout(contains("Exiting"));
}

#[test]
fn stellar_repl_help_all_is_sectioned_and_single_line_formatted() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    let output = cmd
        .write_stdin("help all\n\nexit\n\n")
        .output()
        .expect("run help all in repl");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_contains_in_order(
        &stdout,
        &[
            "Soroban REPL commands (all):",
            "Core setup (value required):",
            "Toggles (on/off):",
            "Prompt/Action commands:",
            "Utility commands:",
        ],
    );

    let ai_row = help_row("AI: \"path\"", "set intent model path");
    let threshold_row = help_row("intent_threshold: <f32>", "set intent confidence threshold");
    let network_row = help_row(
        "network: testnet|mainnet|public",
        "set active network for flow",
    );
    let txrep_row = help_row("txrep", "enable txrep preview in flow");
    let enforce_row = help_row("allowlist_enforce", "enable allowlist enforce");
    let debug_row = help_row("debug", "enable intent pipeline trace");
    let intent_row = help_row(
        "set stellar intent from AI: \"Transfer 5 XLM to G...\"",
        "classify prompt -> ActionPlan",
    );
    let set_var_row = help_row(
        "set <var> from AI: \"...\"",
        "predict with active model -> store variable",
    );
    let setup_row = help_row("show setup", "print active setup");
    let help_dsl_row = help_row("help dsl", "show normal NeuroChain DSL language help");

    assert!(stdout.contains(&ai_row));
    assert!(stdout.contains(&threshold_row));
    assert!(stdout.contains(&network_row));
    assert!(stdout.contains(&txrep_row));
    assert!(stdout.contains(&enforce_row));
    assert!(stdout.contains(&debug_row));
    assert!(stdout.contains(&set_var_row));
    assert!(stdout.contains(&intent_row));
    assert!(stdout.contains(&help_dsl_row));
    assert!(stdout.contains(&setup_row));

    let core_start = stdout
        .find("Core setup (value required):")
        .expect("core setup header");
    let toggle_start = stdout.find("Toggles (on/off):").expect("toggle header");
    let prompt_start = stdout
        .find("Prompt/Action commands:")
        .expect("prompt/action header");
    let utility_start = stdout.find("Utility commands:").expect("utility header");

    let core_section = &stdout[core_start..toggle_start];
    let toggle_section = &stdout[toggle_start..prompt_start];
    let prompt_section = &stdout[prompt_start..utility_start];

    assert!(core_section.contains("intent_threshold: <f32>"));
    assert!(!core_section.contains("txrep"));

    assert!(toggle_section.contains("txrep"));
    assert!(toggle_section.contains("allowlist_enforce"));
    assert!(toggle_section.contains("debug"));
    assert!(!toggle_section.contains("intent_threshold: <f32>"));

    assert!(prompt_section.contains("set stellar intent from AI: \"Transfer 5 XLM to G...\""));
}

#[test]
fn stellar_repl_set_var_from_ai_does_not_trigger_intent_flow() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    let output = cmd
        .write_stdin(
            "AI: \"models/intent_stellar/model.onnx\"\n\nset mood from AI: \"Send 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P\"\n\nexit\n\n",
        )
        .output()
        .expect("run repl set var from ai");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("Variable mood set from AI:"));
    assert!(!stdout.contains("\"schema_version\""));
    assert!(!stderr.contains("=== Preview ==="));
}

#[test]
fn stellar_repl_macro_from_ai_is_rejected_with_guidance() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    let output = cmd
        .write_stdin("macro from AI: \"Transfer 5 XLM to G...\"\n\nexit\n\n")
        .output()
        .expect("run repl macro from ai");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(
        "macro from AI is not supported in neurochain-stellar; use set stellar intent from AI"
    ));
    assert!(!stdout.contains("\"schema_version\""));
    assert!(!stderr.contains("=== Preview ==="));
}

#[test]
fn stellar_repl_defaults_to_flow_mode_without_flag() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.env_remove("NC_ALLOWLIST_ENFORCE")
        .env_remove("NC_CONTRACT_POLICY_ENFORCE")
        .env_remove("NC_ASSET_ALLOWLIST")
        .env_remove("NC_SOROBAN_ALLOWLIST")
        .write_stdin("stellar.payment to=\"GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P\" amount=\"1\" asset_code=\"XLM\"\n\nn\n\nexit\n\n")
        .assert()
        .success()
        .stdout(contains("Flow mode: enabled"))
        .stderr(contains("=== Preview ==="))
        .stderr(contains("Confirm submit? [y/N]"))
        .stderr(contains("Submit aborted by user."));
}

#[test]
fn stellar_repl_no_flow_flag_disables_preview() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.arg("--no-flow")
        .write_stdin("stellar.payment to=\"GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P\" amount=\"1\" asset_code=\"XLM\"\n\nexit\n\n")
        .assert()
        .success()
        .stdout(contains("Flow mode: disabled"))
        .stdout(contains("=== Preview ===").not());
}

#[test]
fn stellar_repl_starts_without_wallet_even_when_env_source_is_set() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.env("NC_SOROBAN_SOURCE", "nc-testnet")
        .write_stdin("exit\n\n")
        .assert()
        .success()
        .stdout(contains("Current wallet/source: (not set)"));
}

#[test]
fn stellar_repl_help_dsl_shows_language_help() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.write_stdin("help dsl\n\nexit\n\n")
        .assert()
        .success()
        .stdout(contains("NeuroChain language"))
        .stdout(contains("Basic syntax:"))
        .stdout(contains("AI: \"path/to/model.onnx\""))
        .stdout(contains("Exiting"));
}
