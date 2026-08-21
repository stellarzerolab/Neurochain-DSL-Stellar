use std::{fs, path::Path};

use neurochain::x402_bazaar::{
    is_valid_route_template, sanitize_service_metadata, BazaarCatalog, BazaarCatalogCandidate,
    BazaarCatalogError, BazaarCatalogKey, BazaarListQuery, BazaarResourceInput, BazaarResourceType,
    BazaarServiceMetadataInput, BAZAAR_LIST_DEFAULT_LIMIT,
};
use serde_json::Value;

const FIXTURE_DIR: &str = "examples/x402_bazaar_catalog";

fn read_candidate(name: &str) -> BazaarCatalogCandidate {
    let path = Path::new(FIXTURE_DIR).join(name);
    let raw =
        fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
}

fn two_resource_catalog() -> BazaarCatalog {
    let mut catalog = BazaarCatalog::default();
    catalog
        .insert(read_candidate("mcp_tool.json"), 1_723_000_001)
        .expect("insert MCP resource");
    catalog
        .insert(read_candidate("http_dynamic.json"), 1_723_000_000)
        .expect("insert HTTP resource");
    catalog
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

    for mutate in ["amount", "asset", "timeout"] {
        let mut invalid = read_candidate("http_dynamic.json");
        match mutate {
            "amount" => invalid.payment.amount = "0".to_string(),
            "asset" => invalid.payment.asset = invalid.payment.pay_to.clone(),
            "timeout" => invalid.payment.max_timeout_seconds = 0,
            _ => unreachable!(),
        }
        let error = BazaarCatalog::default()
            .insert(invalid, 1)
            .expect_err("invalid wire payment field");
        assert_eq!(error.code(), "invalid_payment_summary", "field: {mutate}");
    }

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
fn list_defaults_are_deterministic_and_match_wire_fixture() {
    let response = two_resource_catalog()
        .list(BazaarListQuery::default())
        .expect("list catalog");

    assert_eq!(response.x402_version, 2);
    assert_eq!(response.pagination.limit, BAZAAR_LIST_DEFAULT_LIMIT);
    assert_eq!(response.pagination.offset, 0);
    assert_eq!(response.pagination.total, 2);
    assert_eq!(response.items.len(), 2);
    assert_eq!(response.items[0].resource_type, BazaarResourceType::Http);
    assert_eq!(response.items[1].resource_type, BazaarResourceType::Mcp);

    let expected: Value = serde_json::from_str(
        &fs::read_to_string(Path::new(FIXTURE_DIR).join("list_response.json"))
            .expect("read list response fixture"),
    )
    .expect("parse list response fixture");
    assert_eq!(
        serde_json::to_value(response).expect("serialize list response"),
        expected
    );
}

#[test]
fn list_filters_type_payment_and_extension_without_authority() {
    let mut catalog = BazaarCatalog::default();
    catalog
        .insert(read_candidate("http_dynamic.json"), 1_723_000_000)
        .expect("insert HTTP resource");

    let mut mcp = read_candidate("mcp_tool.json");
    mcp.payment.scheme = "upto".to_string();
    mcp.payment.network = "stellar:pubnet".to_string();
    mcp.payment.pay_to = "GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72Q".to_string();
    catalog
        .insert(mcp, 1_723_000_001)
        .expect("insert distinct MCP payment summary");

    for query in [
        BazaarListQuery {
            resource_type: Some(BazaarResourceType::Mcp),
            ..BazaarListQuery::default()
        },
        BazaarListQuery {
            pay_to: Some("GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72Q".to_string()),
            ..BazaarListQuery::default()
        },
        BazaarListQuery {
            scheme: Some("upto".to_string()),
            ..BazaarListQuery::default()
        },
        BazaarListQuery {
            network: Some("stellar:pubnet".to_string()),
            ..BazaarListQuery::default()
        },
    ] {
        let response = catalog.list(query).expect("filter catalog");
        assert_eq!(response.pagination.total, 1);
        assert_eq!(response.items[0].resource_type, BazaarResourceType::Mcp);
    }

    let bazaar = catalog
        .list(BazaarListQuery {
            extensions: Some("bazaar".to_string()),
            ..BazaarListQuery::default()
        })
        .expect("filter by Bazaar extension");
    assert_eq!(bazaar.pagination.total, 2);

    let empty = catalog
        .list(BazaarListQuery {
            extensions: Some("future-extension".to_string()),
            ..BazaarListQuery::default()
        })
        .expect("unknown well-formed extension is an empty match");
    assert_eq!(empty.pagination.total, 0);
    assert!(empty.items.is_empty());
}

#[test]
fn list_pagination_keeps_filtered_total_and_catalog_key_order() {
    let response = two_resource_catalog()
        .list(BazaarListQuery {
            limit: Some(1),
            offset: Some(1),
            ..BazaarListQuery::default()
        })
        .expect("list second page");

    assert_eq!(response.pagination.limit, 1);
    assert_eq!(response.pagination.offset, 1);
    assert_eq!(response.pagination.total, 2);
    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].resource_type, BazaarResourceType::Mcp);

    let beyond_end = two_resource_catalog()
        .list(BazaarListQuery {
            limit: Some(10),
            offset: Some(10),
            ..BazaarListQuery::default()
        })
        .expect("offset beyond catalog is valid");
    assert_eq!(beyond_end.pagination.total, 2);
    assert!(beyond_end.items.is_empty());
}

#[test]
fn invalid_list_envelope_fails_closed_with_stable_codes() {
    let catalog = two_resource_catalog();
    for (query, code) in [
        (
            BazaarListQuery {
                limit: Some(0),
                ..BazaarListQuery::default()
            },
            "invalid_list_limit",
        ),
        (
            BazaarListQuery {
                limit: Some(101),
                ..BazaarListQuery::default()
            },
            "invalid_list_limit",
        ),
        (
            BazaarListQuery {
                offset: Some(1_000_001),
                ..BazaarListQuery::default()
            },
            "invalid_list_offset",
        ),
        (
            BazaarListQuery {
                pay_to: Some("not-a-strkey".to_string()),
                ..BazaarListQuery::default()
            },
            "invalid_list_filter",
        ),
        (
            BazaarListQuery {
                scheme: Some("any".to_string()),
                ..BazaarListQuery::default()
            },
            "invalid_list_filter",
        ),
        (
            BazaarListQuery {
                network: Some("stellar:futurenet".to_string()),
                ..BazaarListQuery::default()
            },
            "invalid_list_filter",
        ),
        (
            BazaarListQuery {
                extensions: Some("bad extension".to_string()),
                ..BazaarListQuery::default()
            },
            "invalid_list_filter",
        ),
    ] {
        assert_eq!(
            catalog.list(query).expect_err("query must fail").code(),
            code
        );
    }

    assert!(
        serde_json::from_value::<BazaarListQuery>(serde_json::json!({"limit": 1, "submit": true}))
            .is_err(),
        "unknown authority-shaped query field must fail deserialization"
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
        "BTreeMap key order",
        "`limit=20`",
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
