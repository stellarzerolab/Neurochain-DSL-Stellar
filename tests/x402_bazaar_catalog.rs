use std::{fs, path::Path};

use neurochain::x402_bazaar::{
    is_valid_route_template, sanitize_service_metadata, BazaarCatalog, BazaarCatalogCandidate,
    BazaarCatalogError, BazaarCatalogKey, BazaarResourceInput, BazaarServiceMetadataInput,
};
use serde_json::Value;

const FIXTURE_DIR: &str = "examples/x402_bazaar_catalog";

fn read_candidate(name: &str) -> BazaarCatalogCandidate {
    let path = Path::new(FIXTURE_DIR).join(name);
    let raw =
        fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
}

#[test]
fn dynamic_http_resource_uses_template_key_and_sanitized_metadata() {
    let mut catalog = BazaarCatalog::default();
    let resource = catalog
        .insert(read_candidate("http_dynamic.json"), 1_723_000_000)
        .expect("insert HTTP resource");

    assert_eq!(
        resource.key,
        BazaarCatalogKey("http:https://api.example.com/weather/:country/:city".to_string())
    );
    assert_eq!(
        resource.route_template.as_deref(),
        Some("/weather/:country/:city")
    );
    assert_eq!(
        resource.service_metadata.service_name.as_deref(),
        Some("Example Weather")
    );
    assert_eq!(resource.service_metadata.tags, ["weather", "Forecast"]);
    assert_eq!(
        resource.service_metadata.icon_url.as_deref(),
        Some("https://cdn.example.com/weather.png")
    );
}

#[test]
fn hostile_optional_fields_soft_drop_and_use_concrete_path() {
    let mut catalog = BazaarCatalog::default();
    let resource = catalog
        .insert(read_candidate("hostile_soft_drop.json"), 1_723_000_001)
        .expect("soft-drop optional fields");

    assert_eq!(
        resource.key,
        BazaarCatalogKey("http:https://api.example.com/weather/fi/helsinki".to_string())
    );
    assert_eq!(resource.route_template, None);
    assert_eq!(resource.service_metadata.service_name, None);
    assert_eq!(
        resource.service_metadata.tags,
        ["weather", "forecast", "climate", "alerts", "hourly"]
    );
    assert_eq!(resource.service_metadata.icon_url, None);
}

#[test]
fn duplicate_dynamic_route_fails_without_overwrite() {
    let mut catalog = BazaarCatalog::default();
    let first = read_candidate("http_dynamic.json");
    let key = catalog
        .insert(first, 1_723_000_000)
        .expect("insert first")
        .key
        .clone();

    let mut duplicate = read_candidate("http_dynamic.json");
    duplicate.resource.url = "https://api.example.com/weather/us/boston".to_string();
    duplicate.resource.description = "attacker overwrite".to_string();
    let error = catalog
        .insert(duplicate, 1_723_000_999)
        .expect_err("duplicate must fail closed");

    assert_eq!(error.code(), "duplicate_resource");
    assert_eq!(catalog.len(), 1);
    let preserved = catalog.get(&key).expect("original resource remains");
    assert_eq!(preserved.last_updated, 1_723_000_000);
    assert_eq!(
        preserved.resource_url,
        "https://api.example.com/weather/fi/helsinki"
    );
}

#[test]
fn mcp_identity_includes_tool_name_and_rejects_route_template() {
    let mut catalog = BazaarCatalog::default();
    catalog
        .insert(read_candidate("mcp_tool.json"), 1_723_000_000)
        .expect("insert first MCP tool");

    let mut second = read_candidate("mcp_tool.json");
    second.input = BazaarResourceInput::Mcp {
        tool_name: "evaluate_guardrails".to_string(),
    };
    catalog
        .insert(second, 1_723_000_001)
        .expect("same MCP URL with a different tool is distinct");
    assert_eq!(catalog.len(), 2);

    let duplicate_error = catalog
        .insert(read_candidate("mcp_tool.json"), 1_723_000_002)
        .expect_err("same MCP tuple must be duplicate");
    assert_eq!(duplicate_error.code(), "duplicate_resource");

    let mut invalid = read_candidate("mcp_tool.json");
    invalid.route_template = Some("/mcp/:tool".to_string());
    assert_eq!(
        catalog.insert(invalid, 1_723_000_003),
        Err(BazaarCatalogError::UnexpectedMcpRouteTemplate)
    );
}

#[test]
fn route_template_matrix_decodes_before_security_checks() {
    for valid in [
        "/users/:userId",
        "/weather/:country/:city",
        "/v1/files/:name.json",
        "/encoded/%7Euser",
    ] {
        assert!(is_valid_route_template(valid), "expected valid: {valid}");
    }
    for invalid in [
        "",
        "users/:userId",
        "/users?name=:name",
        "/users/../admin",
        "/users/%2e%2e/admin",
        "/%68%74%74%70%3a%2f%2fevil",
        "/broken/%zz",
    ] {
        assert!(
            !is_valid_route_template(invalid),
            "expected invalid: {invalid}"
        );
    }
}

#[test]
fn metadata_matrix_soft_drops_unsafe_values() {
    let sanitized = sanitize_service_metadata(BazaarServiceMetadataInput {
        service_name: Some("x".repeat(33)),
        tags: vec![
            "One".to_string(),
            "one".to_string(),
            "kaksi".to_string(),
            "kolme".to_string(),
            "nelja".to_string(),
            "viisi".to_string(),
            "kuusi".to_string(),
        ],
        icon_url: Some("https://127.0.0.1/icon.png".to_string()),
    });
    assert_eq!(sanitized.service_name, None);
    assert_eq!(sanitized.tags, ["One", "kaksi", "kolme", "nelja", "viisi"]);
    assert_eq!(sanitized.icon_url, None);

    for icon_url in [
        "data:image/png;base64,AAAA",
        "https://user@example.com/icon.png",
        "https://localhost/icon.png",
        "https://2130706433/icon.png",
        "https://0x7f000001/icon.png",
        "https://[::1]/icon.png",
    ] {
        let sanitized = sanitize_service_metadata(BazaarServiceMetadataInput {
            icon_url: Some(icon_url.to_string()),
            ..BazaarServiceMetadataInput::default()
        });
        assert_eq!(sanitized.icon_url, None, "unsafe icon URL: {icon_url}");
    }
}

#[test]
fn hard_envelope_errors_fail_closed_with_machine_codes() {
    let mut invalid = read_candidate("http_dynamic.json");
    invalid.schema_version = 2;
    assert_eq!(
        BazaarCatalog::default()
            .insert(invalid, 1)
            .expect_err("schema version"),
        BazaarCatalogError::UnsupportedSchemaVersion
    );

    let mut invalid = read_candidate("http_dynamic.json");
    invalid.resource.url = "file:///etc/passwd".to_string();
    assert_eq!(
        BazaarCatalog::default()
            .insert(invalid, 1)
            .expect_err("resource URL"),
        BazaarCatalogError::InvalidResourceUrl
    );

    let mut invalid = read_candidate("http_dynamic.json");
    invalid.payment.network = "eip155:8453".to_string();
    let error = BazaarCatalog::default()
        .insert(invalid, 1)
        .expect_err("non-Stellar network");
    assert_eq!(error.code(), "invalid_payment_summary");

    let mut invalid = read_candidate("http_dynamic.json");
    invalid.payment.pay_to = "not-a-stellar-recipient".to_string();
    let error = BazaarCatalog::default()
        .insert(invalid, 1)
        .expect_err("non-Stellar recipient");
    assert_eq!(error.code(), "invalid_payment_summary");

    let mut value: Value =
        serde_json::to_value(read_candidate("http_dynamic.json")).expect("serialize fixture");
    value["payment"]["credential"] = Value::String("forbidden".to_string());
    assert!(
        serde_json::from_value::<BazaarCatalogCandidate>(value).is_err(),
        "unknown credential field must fail deserialization"
    );

    let mut value: Value =
        serde_json::to_value(read_candidate("http_dynamic.json")).expect("serialize fixture");
    value["resource"]["policyOverride"] = Value::Bool(true);
    assert!(
        serde_json::from_value::<BazaarCatalogCandidate>(value).is_err(),
        "unknown resource field must fail deserialization"
    );
}

#[test]
fn docs_and_fixtures_lock_offline_authority_boundary() {
    let docs = fs::read_to_string("docs/x402_bazaar_catalog.md").expect("read catalog docs");
    for required in [
        "no HTTP or MCP discovery runtime",
        "does not receive a `PaymentPayload`",
        "does not enable a pubnet operation",
        "without resolving external `$ref` or `$id` values",
    ] {
        assert!(docs.contains(required), "docs missing boundary: {required}");
    }

    let readme =
        fs::read_to_string("examples/x402_bazaar_catalog/README.md").expect("read fixture README");
    for forbidden in [
        "payment verification is active",
        "settlement is active",
        "submit is active",
    ] {
        assert!(!readme.contains(forbidden));
    }
}
