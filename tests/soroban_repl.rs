use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn soroban_repl_help_and_exit_work() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-soroban").expect("bin build");
    cmd.write_stdin("help\n\nexit\n\n")
        .assert()
        .success()
        .stdout(contains("NeuroChain Soroban REPL"))
        .stdout(contains("Soroban REPL commands"))
        .stdout(contains("Exiting"));
}

#[test]
fn soroban_repl_accepts_ai_model_line() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-soroban").expect("bin build");
    cmd.write_stdin("AI: \"models/intent_stellar/model.onnx\"\n\nexit\n\n")
        .assert()
        .success()
        .stdout(contains(
            "Intent model path set to: models/intent_stellar/model.onnx",
        ))
        .stdout(contains("Exiting"));
}
