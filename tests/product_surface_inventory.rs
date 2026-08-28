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
