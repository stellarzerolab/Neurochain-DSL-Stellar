use std::fs;

use serde_json::Value;

#[path = "../examples/x402_local_reference_path.rs"]
mod quickstart;

#[test]
fn quickstart_report_matches_the_versioned_offline_fixture() {
    let actual = quickstart::quickstart_report().expect("run offline quickstart");
    let expected: Value = serde_json::from_str(include_str!(
        "../examples/x402_local_reference_path/quickstart_output.json"
    ))
    .expect("parse expected quickstart output");
    assert_eq!(actual, expected);

    for scenario in actual["scenarios"].as_array().expect("scenario array") {
        let authority = scenario["authority"].as_object().expect("authority object");
        assert_eq!(authority.len(), 11);
        assert!(authority.values().all(|value| value == false));
        assert_eq!(scenario["capability"]["serviceDispatchAllowed"], false);
    }
    assert_eq!(actual["scenarios"][0]["capability"]["gateCalls"], 1);
    assert_eq!(actual["scenarios"][1]["capability"]["gateCalls"], 0);
    assert_eq!(actual["scenarios"][1]["decision"], "requires_approval");
    assert_eq!(actual["scenarios"][2]["capability"]["gateCalls"], 0);
}

#[test]
fn developer_docs_lock_the_single_offline_command_and_role_boundaries() {
    for path in [
        "README.md",
        "docs/x402_local_reference_quickstart.md",
        "examples/x402_local_reference_path/README.md",
    ] {
        let docs = fs::read_to_string(path).unwrap_or_else(|error| panic!("read {path}: {error}"));
        for required in [
            "cargo run --offline --quiet --example x402_local_reference_path",
            "Bazaar discovery",
            "typed ActionPlan",
            "requires_approval",
            "capability gate",
            "no dispatch",
            "wallet",
            "ActionPlan-submit",
        ] {
            assert!(docs.contains(required), "{path} missing {required}");
        }
    }
}
