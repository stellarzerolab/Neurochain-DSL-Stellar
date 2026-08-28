use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::Value;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn inventory() -> Value {
    serde_json::from_str(include_str!(
        "../examples/product_surface_inventory/v1.json"
    ))
    .expect("product surface inventory must be valid JSON")
}

fn array<'a>(value: &'a Value, field: &str) -> &'a Vec<Value> {
    value[field]
        .as_array()
        .unwrap_or_else(|| panic!("inventory field `{field}` must be an array"))
}

fn required_string<'a>(value: &'a Value, field: &str) -> &'a str {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("inventory field `{field}` must be a string"))
}

fn all_inventory_objects(inventory: &Value) -> Vec<&Value> {
    [
        "binaries",
        "cliModes",
        "cliFlags",
        "replCommandGroups",
        "mcpTools",
        "apiRoutes",
        "contracts",
        "quickstarts",
    ]
    .into_iter()
    .flat_map(|field| array(inventory, field).iter())
    .collect()
}

#[test]
fn inventory_has_unique_ids_valid_classes_and_existing_evidence() {
    let inventory = inventory();
    assert_eq!(inventory["schemaVersion"], 1);

    let allowed_classes: BTreeSet<&str> = array(&inventory, "classifications")
        .iter()
        .map(|value| value.as_str().expect("classification must be a string"))
        .collect();
    assert_eq!(
        allowed_classes,
        BTreeSet::from(["advanced", "core", "deprecated_candidate", "internal"])
    );

    let mut ids = BTreeSet::new();
    for surface in all_inventory_objects(&inventory) {
        let id = required_string(surface, "id");
        assert!(ids.insert(id), "duplicate product surface id `{id}`");
        let class = required_string(surface, "classification");
        assert!(
            allowed_classes.contains(class),
            "surface `{id}` has unknown classification `{class}`"
        );
        assert!(
            !required_string(surface, "role").trim().is_empty(),
            "surface `{id}` must explain its role"
        );
    }

    for field in ["binaries", "contracts", "quickstarts"] {
        for item in array(&inventory, field) {
            let path = required_string(item, "path");
            assert!(
                repo_root().join(path).is_file(),
                "inventoried evidence path `{path}` must exist"
            );
        }
    }

    let authority = inventory["defaultAuthority"]
        .as_object()
        .expect("defaultAuthority must be an object");
    assert_eq!(authority.len(), 12);
    for (field, value) in authority {
        assert_eq!(
            value.as_bool(),
            Some(false),
            "default authority `{field}` must remain false"
        );
    }
}

#[test]
fn canonical_vocabulary_defines_shared_stages_decisions_and_surface_roles() {
    let inventory = inventory();
    let vocabulary = inventory["canonicalVocabulary"]
        .as_object()
        .expect("canonicalVocabulary must be an object");

    let stages = vocabulary["stages"]
        .as_array()
        .expect("canonical stages must be an array");
    let stage_ids: BTreeSet<&str> = stages
        .iter()
        .map(|stage| required_string(stage, "id"))
        .collect();
    assert_eq!(
        stage_ids,
        BTreeSet::from(["capability_gate", "evaluate", "plan", "prove", "verify"])
    );
    let capability_issuing_stages: BTreeSet<&str> = stages
        .iter()
        .filter(|stage| {
            stage["mayIssueExactServiceCallCapability"]
                .as_bool()
                .expect("stage capability flag must be a boolean")
        })
        .map(|stage| required_string(stage, "id"))
        .collect();
    assert_eq!(
        capability_issuing_stages,
        BTreeSet::from(["capability_gate"])
    );
    for stage in stages {
        assert_eq!(
            stage["executionOrSubmitAuthorityGranted"].as_bool(),
            Some(false),
            "canonical stage `{}` must not grant execution or submit authority",
            required_string(stage, "id")
        );
        assert!(!required_string(stage, "meaning").trim().is_empty());
    }

    let decisions = vocabulary["decisions"]
        .as_array()
        .expect("canonical decisions must be an array");
    let decision_ids: BTreeSet<&str> = decisions
        .iter()
        .map(|decision| required_string(decision, "id"))
        .collect();
    assert_eq!(
        decision_ids,
        BTreeSet::from(["approved", "blocked", "not_evaluated", "requires_approval"])
    );
    for decision in decisions {
        assert_eq!(
            decision["executionAuthorityGranted"].as_bool(),
            Some(false),
            "canonical decision `{}` must not grant execution authority",
            required_string(decision, "id")
        );
        assert!(decision["terminalNoSubmit"].is_boolean());
        assert!(!required_string(decision, "meaning").trim().is_empty());
    }

    let surface_roles = vocabulary["surfaceRoles"]
        .as_array()
        .expect("surfaceRoles must be an array");
    let surface_ids: BTreeSet<&str> = surface_roles
        .iter()
        .map(|surface| required_string(surface, "id"))
        .collect();
    assert_eq!(
        surface_ids,
        BTreeSet::from(["api", "cli", "mcp", "nc", "repl", "x402_bazaar", "zk"])
    );
    for surface in surface_roles {
        assert!(!required_string(surface, "role").trim().is_empty());
        assert!(!required_string(surface, "defaultBoundary")
            .trim()
            .is_empty());
    }

    let help_source = fs::read_to_string(repo_root().join("src/bin/neurochain-stellar.rs"))
        .expect("Stellar CLI source must be readable");
    for marker in [
        "Canonical stages: Plan -> Evaluate -> optional Prove -> Verify -> separate capability decision.",
        "REPL role: human learning and diagnostics; use --no-flow for the plan-only path.",
        "Approved is a policy decision, not execution or submit permission.",
        "Advanced operator setup (value required)",
    ] {
        assert!(
            help_source.contains(marker),
            "canonical vocabulary marker `{marker}` must remain in REPL help"
        );
    }
    assert!(!help_source.contains("Core setup (value required)"));
}

#[test]
fn root_readme_and_short_help_lock_one_core_start_without_hiding_advanced_surfaces() {
    let readme =
        fs::read_to_string(repo_root().join("README.md")).expect("README must be readable");
    let start = readme
        .find("## Start Here: Offline Product Path")
        .expect("README must expose the offline product start first");
    let advanced_zk = readme
        .find("## Advanced Evidence: ZK")
        .expect("README must retain advanced ZK evidence");
    assert!(
        start < advanced_zk,
        "core start must precede advanced evidence"
    );
    assert_eq!(
        readme
            .matches("cargo run --offline --quiet --example product_local_quickstart")
            .count(),
        1,
        "README must have exactly one canonical first-run command"
    );
    for marker in [
        "Plan -> Evaluate -> optional Prove -> Verify -> separate capability gate",
        "cryptographicallyVerified=false",
        "stellarVerificationRequired=true",
        "neurochain-stellar --no-flow",
        "neurochain-mcp-v0-stdio",
        "POST /api/stellar/intent-plan",
        "`.nc`",
    ] {
        assert!(
            readme.contains(marker),
            "README missing core marker `{marker}`"
        );
    }

    let help_source = fs::read_to_string(repo_root().join("src/bin/neurochain-stellar.rs"))
        .expect("Stellar CLI source must be readable");
    let quick_start = help_source
        .find("fn print_repl_help_quick")
        .expect("short help function must exist");
    let all_start = help_source
        .find("fn print_repl_help_all")
        .expect("full help function must exist");
    let quick_help = &help_source[quick_start..all_start];
    for marker in [
        "Stellar REPL core quick start",
        "Restart with --no-flow before planning",
        "plain text intent",
        "zk.demo approved|requires_approval|blocked",
        "help all",
    ] {
        assert!(quick_help.contains(marker), "short help missing `{marker}`");
    }
    for advanced in [
        "wallet_generate: demo-alias",
        "wallet_bootstrap: demo-alias",
        "x402.request to=",
        "zk.stellar.attest approved",
        "soroban.contract.deploy alias=",
    ] {
        assert!(
            !quick_help.contains(advanced),
            "short help must leave advanced command `{advanced}` to help all"
        );
        assert!(
            help_source[all_start..].contains(advanced.split(' ').next().unwrap()),
            "help all must retain advanced command family `{advanced}`"
        );
    }
}

#[test]
fn inventory_covers_every_compiled_binary() {
    let inventory = inventory();
    let expected: BTreeMap<&str, &str> = array(&inventory, "binaries")
        .iter()
        .map(|binary| {
            (
                required_string(binary, "name"),
                required_string(binary, "path"),
            )
        })
        .collect();

    let mut actual = BTreeMap::from([("neurochain".to_string(), "src/main.rs".to_string())]);
    let bin_dir = repo_root().join("src/bin");
    for entry in fs::read_dir(&bin_dir).expect("src/bin must be readable") {
        let path = entry.expect("src/bin entry must be readable").path();
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .expect("binary filename must be UTF-8")
            .to_string();
        actual.insert(name.clone(), format!("src/bin/{name}.rs"));
    }

    let expected_owned: BTreeMap<String, String> = expected
        .into_iter()
        .map(|(name, path)| (name.to_string(), path.to_string()))
        .collect();
    assert_eq!(actual, expected_owned, "binary surface inventory drifted");
}

fn source_routes(source: &Path, prefix: &str) -> BTreeSet<String> {
    let text = fs::read_to_string(source)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()));
    let route = Regex::new(r#"\.route\(\s*\"([^\"]+)\""#).expect("route regex must compile");
    route
        .captures_iter(&text)
        .map(|capture| format!("{prefix}{}", &capture[1]))
        .collect()
}

#[test]
fn inventory_matches_public_and_demo_server_routes_exactly() {
    let inventory = inventory();
    let mut expected_by_source: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for route in array(&inventory, "apiRoutes") {
        expected_by_source
            .entry(required_string(route, "source").to_string())
            .or_default()
            .insert(format!(
                "{}{}",
                required_string(route, "prefix"),
                required_string(route, "path")
            ));
    }

    for (source, expected) in expected_by_source {
        let actual = source_routes(&repo_root().join(&source), "/api");
        assert_eq!(
            actual, expected,
            "API route inventory drifted for `{source}`"
        );
    }
}

#[test]
fn inventory_matches_default_mcp_tools_and_repl_help_markers() {
    let inventory = inventory();
    let expected_tools: BTreeSet<&str> = array(&inventory, "mcpTools")
        .iter()
        .map(|tool| required_string(tool, "name"))
        .collect();
    let actual_tools: BTreeSet<&str> = neurochain::mcp_v0_fixture::DEFAULT_TOOLS
        .iter()
        .copied()
        .collect();
    assert_eq!(
        actual_tools, expected_tools,
        "default MCP tool inventory drifted"
    );

    let repl_source = fs::read_to_string(repo_root().join("src/bin/neurochain-stellar.rs"))
        .expect("Stellar CLI source must be readable");
    let repl_help_text = repl_source.replace("\\\"", "\"");
    for group in array(&inventory, "replCommandGroups") {
        for marker in array(group, "markers") {
            let marker = marker.as_str().expect("REPL marker must be a string");
            assert!(
                repl_help_text.contains(marker),
                "REPL marker `{marker}` from `{}` is missing from help/runtime source",
                required_string(group, "id")
            );
        }
    }
}

#[test]
fn inventory_matches_stellar_cli_flags_exactly() {
    let inventory = inventory();
    let expected: BTreeSet<&str> = array(&inventory, "cliFlags")
        .iter()
        .map(|flag| required_string(flag, "flag"))
        .collect();

    let source = fs::read_to_string(repo_root().join("src/bin/neurochain-stellar.rs"))
        .expect("Stellar CLI source must be readable");
    let start = source
        .find("fn parse_cli_args")
        .expect("parse_cli_args must exist");
    let end = source[start..]
        .find("fn parse_bool_value")
        .map(|offset| start + offset)
        .expect("parse_bool_value must follow parse_cli_args");
    let flag = Regex::new(r#"\"(--[a-z-]+|-[a-z])\""#).expect("CLI flag regex must compile");
    let actual: BTreeSet<&str> = flag
        .captures_iter(&source[start..end])
        .map(|capture| capture.get(1).expect("flag capture must exist").as_str())
        .collect();

    assert_eq!(actual, expected, "Stellar CLI flag inventory drifted");
}
