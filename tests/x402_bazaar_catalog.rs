use std::{fs, path::Path};

use neurochain::x402_bazaar::{
    is_valid_route_template, sanitize_service_metadata, BazaarCatalog, BazaarCatalogCandidate,
    BazaarCatalogError, BazaarCatalogKey, BazaarListQuery, BazaarResourceInput, BazaarResourceType,
    BazaarSearchQuery, BazaarSearchResponse, BazaarServiceMetadataInput, BAZAAR_LIST_DEFAULT_LIMIT,
    BAZAAR_SEARCH_DEFAULT_LIMIT,
};
use serde::Deserialize;
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

fn three_resource_catalog() -> BazaarCatalog {
    let mut catalog = two_resource_catalog();
    catalog
        .insert(read_candidate("market_data.json"), 1_723_000_002)
        .expect("insert market resource");
    catalog
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SearchEvaluation {
    schema_version: u32,
    candidate_files: Vec<String>,
    minimum_mean_reciprocal_rank_milli: usize,
    cases: Vec<SearchEvaluationCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SearchEvaluationCase {
    query: String,
    expected_resource: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SearchPagesFixture {
    schema_version: u32,
    candidates: Vec<SearchCandidateFixture>,
    pages: Vec<SearchPageFixture>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SearchCandidateFixture {
    file: String,
    observed_at: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SearchPageFixture {
    request: BazaarSearchQuery,
    response: BazaarSearchResponse,
}

fn read_search_evaluation() -> SearchEvaluation {
    let path = Path::new(FIXTURE_DIR).join("search_evaluation.json");
    let raw =
        fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
}

fn read_search_pages() -> SearchPagesFixture {
    let path = Path::new(FIXTURE_DIR).join("search_pages.json");
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
fn search_evaluation_fixture_meets_measurable_mrr_gate() {
    let evaluation = read_search_evaluation();
    assert_eq!(evaluation.schema_version, 1);

    let mut catalog = BazaarCatalog::default();
    for (index, candidate_file) in evaluation.candidate_files.iter().enumerate() {
        catalog
            .insert(read_candidate(candidate_file), 1_723_000_000 + index as u64)
            .unwrap_or_else(|err| panic!("insert {candidate_file}: {err}"));
    }

    let reciprocal_rank_sum = evaluation
        .cases
        .iter()
        .map(|case| {
            let response = catalog
                .search(BazaarSearchQuery {
                    query: case.query.clone(),
                    limit: Some(100),
                    ..BazaarSearchQuery::default()
                })
                .unwrap_or_else(|err| panic!("search {:?}: {err}", case.query));
            let rank = response
                .resources
                .iter()
                .position(|resource| resource.resource == case.expected_resource)
                .unwrap_or_else(|| panic!("expected resource missing for {:?}", case.query));
            1_000 / (rank + 1)
        })
        .sum::<usize>();
    let mean_reciprocal_rank_milli = reciprocal_rank_sum / evaluation.cases.len();

    assert!(
        mean_reciprocal_rank_milli >= evaluation.minimum_mean_reciprocal_rank_milli,
        "MRR milli {mean_reciprocal_rank_milli} below fixture gate {}",
        evaluation.minimum_mean_reciprocal_rank_milli
    );
}

#[test]
fn search_cursor_is_query_bound_and_ties_use_catalog_key_order() {
    let catalog = three_resource_catalog();
    let first = catalog
        .search(BazaarSearchQuery {
            query: "api".to_string(),
            limit: Some(1),
            ..BazaarSearchQuery::default()
        })
        .expect("search first page");
    assert_eq!(first.x402_version, 2);
    assert_eq!(first.pagination.limit, 1);
    assert_eq!(
        first.resources[0].resource,
        "https://api.example.com/markets/BTC"
    );
    assert!(first.partial_results);
    let first_cursor = first.pagination.cursor.expect("first continuation cursor");

    let second = catalog
        .search(BazaarSearchQuery {
            query: "api".to_string(),
            limit: Some(1),
            cursor: Some(first_cursor.clone()),
            ..BazaarSearchQuery::default()
        })
        .expect("search second page");
    assert_eq!(
        second.resources[0].resource,
        "https://api.example.com/weather/fi/helsinki"
    );
    assert!(second.partial_results);
    let second_cursor = second
        .pagination
        .cursor
        .expect("second continuation cursor");

    let third = catalog
        .search(BazaarSearchQuery {
            query: "api".to_string(),
            limit: Some(1),
            cursor: Some(second_cursor),
            ..BazaarSearchQuery::default()
        })
        .expect("search final page");
    assert_eq!(third.resources[0].resource, "https://api.example.com/mcp");
    assert!(!third.partial_results);
    assert_eq!(third.pagination.cursor, None);

    let mismatch = catalog
        .search(BazaarSearchQuery {
            query: "weather".to_string(),
            cursor: Some(first_cursor.clone()),
            ..BazaarSearchQuery::default()
        })
        .expect_err("cursor must be bound to normalized query and filters");
    assert_eq!(mismatch.code(), "invalid_search_cursor");

    let tampered = first_cursor.replacen("v1:", "v2:", 1);
    let tamper = catalog
        .search(BazaarSearchQuery {
            query: "api".to_string(),
            cursor: Some(tampered),
            ..BazaarSearchQuery::default()
        })
        .expect_err("tampered cursor must fail closed");
    assert_eq!(tamper.code(), "invalid_search_cursor");
}

#[test]
fn search_pages_fixture_locks_ranking_cursor_and_wire_parity() {
    let fixture = read_search_pages();
    assert_eq!(fixture.schema_version, 1);
    let mut catalog = BazaarCatalog::default();
    for candidate in fixture.candidates {
        catalog
            .insert(read_candidate(&candidate.file), candidate.observed_at)
            .unwrap_or_else(|err| panic!("insert {}: {err}", candidate.file));
    }

    for page in fixture.pages {
        let actual = catalog.search(page.request).expect("search fixture page");
        assert_eq!(actual, page.response);
    }
}

#[test]
fn search_filters_and_response_shape_preserve_offline_boundary() {
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
        .expect("insert MCP resource");

    for query in [
        BazaarSearchQuery {
            query: "api".to_string(),
            resource_type: Some(BazaarResourceType::Mcp),
            ..BazaarSearchQuery::default()
        },
        BazaarSearchQuery {
            query: "api".to_string(),
            pay_to: Some("GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72Q".to_string()),
            ..BazaarSearchQuery::default()
        },
        BazaarSearchQuery {
            query: "api".to_string(),
            scheme: Some("upto".to_string()),
            ..BazaarSearchQuery::default()
        },
        BazaarSearchQuery {
            query: "api".to_string(),
            network: Some("stellar:pubnet".to_string()),
            ..BazaarSearchQuery::default()
        },
    ] {
        let response = catalog.search(query).expect("filtered search");
        assert_eq!(response.resources.len(), 1);
        assert_eq!(response.resources[0].resource_type, BazaarResourceType::Mcp);
    }

    let empty = catalog
        .search(BazaarSearchQuery {
            query: "api".to_string(),
            extensions: Some("future-extension".to_string()),
            ..BazaarSearchQuery::default()
        })
        .expect("unknown well-formed extension is an empty search");
    assert!(empty.resources.is_empty());
    assert!(!empty.partial_results);
    assert_eq!(empty.pagination.cursor, None);

    let serialized = serde_json::to_value(
        catalog
            .search(BazaarSearchQuery {
                query: "weather".to_string(),
                ..BazaarSearchQuery::default()
            })
            .expect("serialize search response"),
    )
    .expect("search response JSON");
    assert!(serialized.get("resources").is_some());
    assert!(serialized.get("items").is_none());
    assert_eq!(BAZAAR_SEARCH_DEFAULT_LIMIT, 20);
    assert_eq!(serialized["pagination"]["limit"], 1);
}

#[test]
fn invalid_search_envelope_fails_closed_with_stable_codes() {
    let catalog = three_resource_catalog();
    let too_many_terms = (0..17)
        .map(|index| format!("term{index}"))
        .collect::<Vec<_>>()
        .join(" ");
    for (query, code) in [
        (BazaarSearchQuery::default(), "invalid_search_query"),
        (
            BazaarSearchQuery {
                query: "---".to_string(),
                ..BazaarSearchQuery::default()
            },
            "invalid_search_query",
        ),
        (
            BazaarSearchQuery {
                query: "x".repeat(257),
                ..BazaarSearchQuery::default()
            },
            "invalid_search_query",
        ),
        (
            BazaarSearchQuery {
                query: too_many_terms,
                ..BazaarSearchQuery::default()
            },
            "invalid_search_query",
        ),
        (
            BazaarSearchQuery {
                query: "weather".to_string(),
                limit: Some(0),
                ..BazaarSearchQuery::default()
            },
            "invalid_search_limit",
        ),
        (
            BazaarSearchQuery {
                query: "weather".to_string(),
                limit: Some(101),
                ..BazaarSearchQuery::default()
            },
            "invalid_search_limit",
        ),
        (
            BazaarSearchQuery {
                query: "weather".to_string(),
                network: Some("stellar:futurenet".to_string()),
                ..BazaarSearchQuery::default()
            },
            "invalid_search_filter",
        ),
        (
            BazaarSearchQuery {
                query: "weather".to_string(),
                cursor: Some("v1:not-a-fingerprint:1".to_string()),
                ..BazaarSearchQuery::default()
            },
            "invalid_search_cursor",
        ),
    ] {
        assert_eq!(
            catalog.search(query).expect_err("search must fail").code(),
            code
        );
    }

    assert!(
        serde_json::from_value::<BazaarSearchQuery>(
            serde_json::json!({"query": "weather", "sign": true})
        )
        .is_err(),
        "unknown authority-shaped search field must fail deserialization"
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
        "mean reciprocal rank",
        "query-bound cursor",
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
