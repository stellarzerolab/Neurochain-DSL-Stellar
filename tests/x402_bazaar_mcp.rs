use std::{fs, path::Path};

use neurochain::{
    x402_bazaar::{BazaarCatalog, BazaarCatalogCandidate},
    x402_bazaar_mcp::{
        bazaar_mcp_call_result, bazaar_mcp_tools_list, execute_bazaar_mcp_search,
        BazaarMcpAuthority, BazaarMcpSearchResult, BAZAAR_MCP_PROTOCOL_VERSION,
        BAZAAR_MCP_SCHEMA_VERSION, BAZAAR_MCP_SEARCH_TOOL,
    },
};
use serde_json::{json, Value};

const CATALOG_FIXTURE_DIR: &str = "examples/x402_bazaar_catalog";
const MCP_FIXTURE_DIR: &str = "examples/x402_bazaar_mcp";

fn read_value(directory: &str, name: &str) -> Value {
    let path = Path::new(directory).join(name);
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn read_candidate(name: &str) -> BazaarCatalogCandidate {
    serde_json::from_value(read_value(CATALOG_FIXTURE_DIR, name))
        .unwrap_or_else(|error| panic!("parse candidate {name}: {error}"))
}

fn catalog() -> BazaarCatalog {
    let mut catalog = BazaarCatalog::default();
    for (name, observed_at) in [
        ("mcp_tool.json", 1_723_000_001),
        ("http_dynamic.json", 1_723_000_000),
        ("market_data.json", 1_723_000_002),
    ] {
        catalog
            .insert(read_candidate(name), observed_at)
            .unwrap_or_else(|error| panic!("insert {name}: {error}"));
    }
    catalog
}

fn fixture_arguments() -> Value {
    read_value(MCP_FIXTURE_DIR, "search_call.json")["params"]["arguments"].clone()
}

fn assert_no_authority(result: &BazaarMcpSearchResult) {
    assert_eq!(result.authority, BazaarMcpAuthority::default());
    let value = serde_json::to_value(result.authority).expect("serialize authority");
    let fields = value.as_object().expect("authority object");
    assert_eq!(fields.len(), 9);
    assert!(fields.values().all(|value| value == &Value::Bool(false)));
}

#[test]
fn tools_list_is_deterministic_and_search_only() {
    let actual = bazaar_mcp_tools_list();
    assert_eq!(actual, bazaar_mcp_tools_list());
    assert_eq!(actual["resultType"], "complete");
    assert_eq!(actual["tools"].as_array().map(Vec::len), Some(1));
    let tool = &actual["tools"][0];
    assert_eq!(tool["name"], BAZAAR_MCP_SEARCH_TOOL);
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    assert_eq!(tool["outputSchema"]["additionalProperties"], false);
    assert_eq!(
        tool["outputSchema"]["properties"]["authority"]["properties"]["paymentAllowed"]["const"],
        false
    );

    let tool_names = actual["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect::<Vec<_>>();
    for forbidden in [
        "paid_call",
        "settle",
        "sign_transaction",
        "wallet",
        "shell",
        "rpc_submit",
        "action_plan_submit",
    ] {
        assert!(
            !tool_names.contains(&forbidden),
            "tools/list leaked forbidden capability {forbidden}"
        );
    }
}

#[test]
fn search_call_is_structured_deterministic_and_matches_fixture() {
    let catalog = catalog();
    let actual = bazaar_mcp_call_result(Some(&catalog), fixture_arguments());
    assert_eq!(
        actual,
        read_value(MCP_FIXTURE_DIR, "search_result.json"),
        "checked-in MCP search result drifted"
    );
    assert_eq!(actual["resultType"], "complete");
    assert_eq!(actual["isError"], false);
    assert_eq!(actual["structuredContent"]["ok"], true);
    assert_eq!(
        actual["structuredContent"]["protocolVersion"],
        BAZAAR_MCP_PROTOCOL_VERSION
    );
    assert_eq!(
        actual["structuredContent"]["data"]["resources"][0]["resource"],
        "https://api.example.com/mcp"
    );

    let text: Value =
        serde_json::from_str(actual["content"][0]["text"].as_str().expect("text content"))
            .expect("text content is JSON");
    assert_eq!(text, actual["structuredContent"]);
}

#[test]
fn cursor_and_filters_delegate_to_the_existing_bazaar_search_core() {
    let catalog = catalog();
    let first = execute_bazaar_mcp_search(
        Some(&catalog),
        json!({
            "schemaVersion": 1,
            "query": "api",
            "network": "stellar:testnet",
            "extensions": "bazaar",
            "limit": 1
        }),
    );
    assert!(first.ok);
    assert_no_authority(&first);
    let first_data = first.data.expect("first page");
    assert!(first_data.partial_results);
    let cursor = first_data.pagination.cursor.expect("continuation cursor");

    let second = execute_bazaar_mcp_search(
        Some(&catalog),
        json!({
            "schemaVersion": 1,
            "query": "api",
            "network": "stellar:testnet",
            "extensions": "bazaar",
            "limit": 1,
            "cursor": cursor
        }),
    );
    assert!(second.ok);
    assert_no_authority(&second);
    let second_data = second.data.expect("second page");
    assert_eq!(second_data.resources.len(), 1);
    assert_ne!(first_data.resources, second_data.resources);
}

#[test]
fn malformed_hostile_and_oversized_arguments_fail_closed() {
    let catalog = catalog();
    let cases = [
        (json!({"query": "stellar"}), "invalid_arguments"),
        (
            json!({"schemaVersion": 2, "query": "stellar"}),
            "unsupported_schema_version",
        ),
        (
            json!({"schemaVersion": 1, "query": "stellar", "paidCall": true}),
            "invalid_arguments",
        ),
        (
            json!({"schemaVersion": 1, "query": ""}),
            "invalid_search_query",
        ),
        (
            json!({"schemaVersion": 1, "query": "stellar", "network": "eip155:1"}),
            "invalid_search_filter",
        ),
        (
            json!({"schemaVersion": 1, "query": "stellar", "cursor": "forged"}),
            "invalid_search_cursor",
        ),
        (
            json!({"schemaVersion": 1, "query": "x".repeat(4_097)}),
            "arguments_too_large",
        ),
    ];

    for (arguments, expected_code) in cases {
        let result = execute_bazaar_mcp_search(Some(&catalog), arguments);
        assert!(!result.ok, "{expected_code} unexpectedly succeeded");
        assert_eq!(result.code, expected_code);
        assert!(!result.reason.trim().is_empty());
        assert!(!result.retryable);
        assert_eq!(result.data, None);
        assert_no_authority(&result);
    }
}

#[test]
fn unavailable_catalog_is_retryable_but_never_escalates_authority() {
    let result = execute_bazaar_mcp_search(None, fixture_arguments());
    assert!(!result.ok);
    assert_eq!(result.code, "catalog_unavailable");
    assert!(result.retryable);
    assert!(!result.reason.trim().is_empty());
    assert_eq!(result.data, None);
    assert_no_authority(&result);

    let call_result = bazaar_mcp_call_result(None, fixture_arguments());
    assert_eq!(
        call_result,
        read_value(MCP_FIXTURE_DIR, "catalog_unavailable_result.json")
    );
    assert_eq!(call_result["isError"], true);
}

#[test]
fn fixture_and_docs_preserve_the_offline_no_paid_call_boundary() {
    let call = read_value(MCP_FIXTURE_DIR, "search_call.json");
    assert_eq!(call["jsonrpc"], "2.0");
    assert_eq!(call["method"], "tools/call");
    assert_eq!(call["params"]["name"], BAZAAR_MCP_SEARCH_TOOL);
    assert_eq!(
        call["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"],
        BAZAAR_MCP_PROTOCOL_VERSION
    );
    assert_eq!(
        call["params"]["arguments"]["schemaVersion"],
        BAZAAR_MCP_SCHEMA_VERSION
    );

    for path in [
        "docs/x402_bazaar_mcp.md",
        "examples/x402_bazaar_mcp/README.md",
    ] {
        let content =
            fs::read_to_string(path).unwrap_or_else(|error| panic!("read {path}: {error}"));
        for required in [
            "offline",
            "read-only",
            "paid-call",
            "wallet",
            "ActionPlan",
            "2026-07-28",
        ] {
            assert!(
                content.contains(required),
                "{path} is missing boundary term {required}"
            );
        }
    }
}
