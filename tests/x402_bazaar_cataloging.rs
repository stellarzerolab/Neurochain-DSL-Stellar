use std::{fs, path::Path};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use neurochain::{
    x402_bazaar::{BazaarCatalog, BazaarCatalogKey},
    x402_bazaar_cataloging::{
        catalog_verified_discovery, BazaarCatalogingDisposition, BazaarExtensionResponseStatus,
        BazaarVerifiedDiscoveryInput,
    },
};
use serde::Deserialize;
use serde_json::{json, Value};

const FIXTURE_DIR: &str = "examples/x402_bazaar_cataloging";

fn read_request(name: &str) -> BazaarVerifiedDiscoveryInput {
    let path = Path::new(FIXTURE_DIR).join(name);
    let raw =
        fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
}

fn decoded_header(outcome: &neurochain::x402_bazaar_cataloging::BazaarCatalogingOutcome) -> Value {
    let encoded = outcome
        .extension_responses_header_value()
        .expect("serialize extension responses")
        .expect("outcome emits extension responses");
    let decoded = BASE64_STANDARD
        .decode(encoded)
        .expect("decode base64 header");
    serde_json::from_slice(&decoded).expect("parse extension responses JSON")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OutcomeContract {
    schema_version: u32,
    outcomes: OutcomeCases,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OutcomeCases {
    accepted: ExpectedOutcome,
    dropped: ExpectedOutcome,
    invalid: ExpectedOutcome,
    duplicate: ExpectedOutcome,
    unavailable: ExpectedOutcome,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExpectedOutcome {
    code: String,
    header_status: Option<String>,
}

fn read_outcome_contract() -> OutcomeContract {
    let path = Path::new(FIXTURE_DIR).join("outcome_contract.json");
    let raw =
        fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
}

#[test]
fn automatic_http_cataloging_validates_schema_and_emits_success_header() {
    let mut catalog = BazaarCatalog::default();
    let outcome = catalog_verified_discovery(
        Some(&mut catalog),
        read_request("automatic_http.json"),
        1_723_000_000,
    );

    assert_eq!(outcome.disposition, BazaarCatalogingDisposition::Accepted);
    assert_eq!(outcome.code, "cataloged");
    assert!(!outcome.reason.is_empty());
    assert_eq!(
        outcome.catalog_key,
        Some(BazaarCatalogKey(
            "http:https://api.example.com/weather/:country/:city".to_string()
        ))
    );
    assert_eq!(catalog.len(), 1);
    assert_eq!(
        decoded_header(&outcome),
        json!({"bazaar": {"status": "success"}})
    );
}

#[test]
fn automatic_mcp_cataloging_uses_url_and_tool_name_identity() {
    let mut catalog = BazaarCatalog::default();
    let outcome = catalog_verified_discovery(
        Some(&mut catalog),
        read_request("automatic_mcp.json"),
        1_723_000_001,
    );

    assert_eq!(outcome.disposition, BazaarCatalogingDisposition::Accepted);
    assert_eq!(
        outcome.catalog_key,
        Some(BazaarCatalogKey(
            "mcp:https://api.example.com/mcp#plan_stellar_action".to_string()
        ))
    );
    assert_eq!(catalog.len(), 1);
}

#[test]
fn schema_info_mismatch_and_external_reference_fail_closed() {
    let mut mismatch = read_request("automatic_http.json");
    let info = &mut mismatch.bazaar.as_mut().expect("Bazaar extension").info;
    info["input"]["queryParams"]["city"] = json!(42);
    let outcome = catalog_verified_discovery(Some(&mut BazaarCatalog::default()), mismatch, 1);
    assert_eq!(outcome.disposition, BazaarCatalogingDisposition::Invalid);
    assert_eq!(outcome.code, "schema_info_mismatch");
    let rejected = decoded_header(&outcome);
    assert_eq!(rejected["bazaar"]["status"], "rejected");
    assert!(rejected["bazaar"]["rejectedReason"]
        .as_str()
        .is_some_and(|reason| reason.starts_with("schema_info_mismatch:")));

    let mut external = read_request("automatic_http.json");
    external.bazaar.as_mut().expect("Bazaar extension").schema["properties"]["input"] =
        json!({"$ref": "https://attacker.example/schema.json"});
    let outcome = catalog_verified_discovery(Some(&mut BazaarCatalog::default()), external, 1);
    assert_eq!(outcome.disposition, BazaarCatalogingDisposition::Invalid);
    assert_eq!(outcome.code, "external_schema_reference");
}

#[test]
fn malformed_oversized_and_unknown_schema_profile_never_catalog() {
    let mut malformed = read_request("automatic_http.json");
    malformed.bazaar.as_mut().expect("Bazaar extension").info["input"] = json!({
        "type": "http"
    });
    let mut catalog = BazaarCatalog::default();
    let outcome = catalog_verified_discovery(Some(&mut catalog), malformed, 1);
    assert_eq!(outcome.disposition, BazaarCatalogingDisposition::Invalid);
    assert_eq!(outcome.code, "invalid_http_discovery");
    assert!(catalog.is_empty());

    let mut oversized = read_request("automatic_http.json");
    oversized.bazaar.as_mut().expect("Bazaar extension").info["output"]["example"] =
        json!({"payload": "x".repeat(33 * 1024)});
    let outcome = catalog_verified_discovery(Some(&mut catalog), oversized, 1);
    assert_eq!(outcome.disposition, BazaarCatalogingDisposition::Invalid);
    assert_eq!(outcome.code, "json_value_too_large");
    assert!(catalog.is_empty());

    let mut unsupported = read_request("automatic_http.json");
    unsupported
        .bazaar
        .as_mut()
        .expect("Bazaar extension")
        .schema["allOf"] = json!([]);
    let outcome = catalog_verified_discovery(Some(&mut catalog), unsupported, 1);
    assert_eq!(
        outcome.disposition,
        BazaarCatalogingDisposition::Unavailable
    );
    assert_eq!(outcome.code, "schema_profile_unavailable");
    assert!(catalog.is_empty());
}

#[test]
fn duplicate_unavailable_and_missing_extension_have_stable_outcomes() {
    let contract = read_outcome_contract();
    assert_eq!(contract.schema_version, 1);
    let mut catalog = BazaarCatalog::default();
    let request = read_request("automatic_http.json");
    let accepted = catalog_verified_discovery(Some(&mut catalog), request.clone(), 1);
    let duplicate = catalog_verified_discovery(Some(&mut catalog), request.clone(), 2);
    let unavailable = catalog_verified_discovery(None, request.clone(), 3);
    let mut missing = request;
    missing.bazaar = None;
    let dropped = catalog_verified_discovery(Some(&mut catalog), missing, 4);

    let cases = [
        (&accepted, &contract.outcomes.accepted),
        (&duplicate, &contract.outcomes.duplicate),
        (&unavailable, &contract.outcomes.unavailable),
        (&dropped, &contract.outcomes.dropped),
    ];
    for (actual, expected) in cases {
        assert_eq!(actual.code, expected.code);
        assert!(!actual.reason.is_empty());
        let status = actual
            .extension_responses()
            .map(|responses| match responses.bazaar.status {
                BazaarExtensionResponseStatus::Success => "success",
                BazaarExtensionResponseStatus::Processing => "processing",
                BazaarExtensionResponseStatus::Rejected => "rejected",
            });
        assert_eq!(status, expected.header_status.as_deref());
    }
    assert_eq!(
        duplicate.disposition,
        BazaarCatalogingDisposition::Duplicate
    );
    assert_eq!(
        unavailable.disposition,
        BazaarCatalogingDisposition::Unavailable
    );
    assert_eq!(dropped.disposition, BazaarCatalogingDisposition::Dropped);
    assert_eq!(
        dropped
            .extension_responses_header_value()
            .expect("serialize dropped"),
        None
    );

    let mut invalid = read_request("automatic_http.json");
    invalid.bazaar.as_mut().expect("Bazaar extension").info["input"]["queryParams"]["city"] =
        json!(42);
    let invalid = catalog_verified_discovery(Some(&mut catalog), invalid, 5);
    assert_eq!(invalid.code, contract.outcomes.invalid.code);
}

#[test]
fn automatic_input_rejects_authority_shaped_unknown_fields() {
    let mut value = serde_json::to_value(read_request("automatic_http.json")).expect("to JSON");
    value["sign"] = json!(true);
    assert!(serde_json::from_value::<BazaarVerifiedDiscoveryInput>(value).is_err());
}

#[test]
fn docs_and_fixtures_lock_cataloging_authority_boundary() {
    let docs = fs::read_to_string("docs/x402_bazaar_cataloging.md").expect("read docs");
    for required in [
        "does not receive a raw `PaymentPayload`",
        "external `$ref` and `$id`",
        "schema_profile_unavailable",
        "`success | processing | rejected`",
        "does not grant payment, settlement, signing, execution, or ActionPlan-submit authority",
    ] {
        assert!(docs.contains(required), "docs missing boundary: {required}");
    }

    let readme = fs::read_to_string("examples/x402_bazaar_cataloging/README.md")
        .expect("read fixture README");
    assert!(readme.contains("not raw"));
    assert!(readme.contains("`PaymentPayload` values"));
    assert!(!readme.contains("live settlement is active"));
}
