use std::{collections::VecDeque, fs, path::Path};

use neurochain::{
    x402_bazaar::{BazaarCatalog, BazaarCatalogCandidate},
    x402_bazaar_paid_call::{
        bazaar_mcp_paid_call_result, execute_bazaar_mcp_paid_call, BazaarMcpPaidCallResult,
        BazaarPaidCallAccessDecision, BazaarPaidCallAccessGate, BazaarPaidCallAuthority,
        BazaarPaidCallBinding, BAZAAR_MCP_PAID_CALL_TOOL, BAZAAR_PAID_CALL_SCHEMA_VERSION,
    },
};
use serde_json::{json, Value};

const CATALOG_FIXTURE_DIR: &str = "examples/x402_bazaar_catalog";
const PAID_CALL_FIXTURE_DIR: &str = "examples/x402_bazaar_paid_call";

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
    catalog
        .insert(read_candidate("mcp_tool.json"), 1_723_000_001)
        .expect("insert MCP resource");
    catalog
        .insert(read_candidate("http_dynamic.json"), 1_723_000_000)
        .expect("insert HTTP resource");
    catalog
}

fn fixture_arguments() -> Value {
    read_value(PAID_CALL_FIXTURE_DIR, "paid_call.json")["params"]["arguments"].clone()
}

#[derive(Debug)]
struct RecordingGate {
    decisions: VecDeque<BazaarPaidCallAccessDecision>,
    bindings: Vec<BazaarPaidCallBinding>,
}

impl RecordingGate {
    fn with(decisions: impl IntoIterator<Item = BazaarPaidCallAccessDecision>) -> Self {
        Self {
            decisions: decisions.into_iter().collect(),
            bindings: Vec::new(),
        }
    }
}

impl BazaarPaidCallAccessGate for RecordingGate {
    fn consume_settled_access(
        &mut self,
        binding: &BazaarPaidCallBinding,
    ) -> BazaarPaidCallAccessDecision {
        self.bindings.push(binding.clone());
        self.decisions
            .pop_front()
            .unwrap_or(BazaarPaidCallAccessDecision::ReplayBlocked)
    }
}

fn authority_value(result: &BazaarMcpPaidCallResult) -> Value {
    serde_json::to_value(result.authority).expect("serialize authority")
}

fn assert_no_authority(result: &BazaarMcpPaidCallResult) {
    assert_eq!(result.authority, BazaarPaidCallAuthority::default());
    let authority = authority_value(result);
    assert_eq!(authority.as_object().map(|object| object.len()), Some(11));
    assert!(authority
        .as_object()
        .expect("authority object")
        .values()
        .all(|value| value == &Value::Bool(false)));
}

#[test]
fn authorized_result_grants_only_the_exact_named_service_call() {
    let catalog = catalog();
    let mut gate = RecordingGate::with([BazaarPaidCallAccessDecision::Authorized]);
    let result = execute_bazaar_mcp_paid_call(Some(&catalog), Some(&mut gate), fixture_arguments());

    assert!(result.ok);
    assert_eq!(result.code, "service_call_authorized");
    assert!(!result.reason.is_empty());
    assert!(!result.retryable);
    let authority = authority_value(&result);
    assert_eq!(authority["serviceCallAllowed"], true);
    for forbidden in [
        "paymentAllowed",
        "proofAllowed",
        "approvalAllowed",
        "settlementAllowed",
        "signingAllowed",
        "underlyingExecutionAllowed",
        "walletAccessAllowed",
        "shellAccessAllowed",
        "rpcSubmitAllowed",
        "actionPlanSubmitAllowed",
    ] {
        assert_eq!(authority[forbidden], false, "authority leak: {forbidden}");
    }

    let binding = result.data.expect("authorized binding");
    assert_eq!(binding.request_id, "paid-call-1");
    assert_eq!(
        binding.resource_key.0,
        "mcp:https://api.example.com/mcp#plan_stellar_action"
    );
    assert_eq!(binding.resource_url, "https://api.example.com/mcp");
    assert_eq!(binding.tool_name, "plan_stellar_action");
    assert_eq!(binding.network, "stellar:testnet");
    assert_eq!(binding.arguments_digest.len(), 64);
    assert_eq!(binding.call_digest.len(), 64);
    assert!(binding
        .arguments_digest
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit()));
    assert!(binding
        .call_digest
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(gate.bindings, [binding]);
}

#[test]
fn binding_is_canonical_and_any_named_call_change_is_visible() {
    let catalog = catalog();
    let mut first_gate = RecordingGate::with([BazaarPaidCallAccessDecision::Authorized]);
    let first =
        execute_bazaar_mcp_paid_call(Some(&catalog), Some(&mut first_gate), fixture_arguments())
            .data
            .expect("first binding");

    let mut reordered = fixture_arguments();
    reordered["arguments"] = json!({
        "network": "stellar:testnet",
        "intent": "show the bounded Stellar ActionPlan"
    });
    let mut second_gate = RecordingGate::with([BazaarPaidCallAccessDecision::Authorized]);
    let second = execute_bazaar_mcp_paid_call(Some(&catalog), Some(&mut second_gate), reordered)
        .data
        .expect("second binding");
    assert_eq!(first.arguments_digest, second.arguments_digest);
    assert_eq!(first.call_digest, second.call_digest);

    let mut changed = fixture_arguments();
    changed["arguments"]["intent"] = json!("different service call");
    let mut changed_gate = RecordingGate::with([BazaarPaidCallAccessDecision::Authorized]);
    let changed = execute_bazaar_mcp_paid_call(Some(&catalog), Some(&mut changed_gate), changed)
        .data
        .expect("changed binding");
    assert_ne!(first.arguments_digest, changed.arguments_digest);
    assert_ne!(first.call_digest, changed.call_digest);
}

#[test]
fn settled_access_is_single_use_and_replay_fails_closed() {
    let catalog = catalog();
    let mut gate = RecordingGate::with([
        BazaarPaidCallAccessDecision::Authorized,
        BazaarPaidCallAccessDecision::ReplayBlocked,
    ]);
    let first = execute_bazaar_mcp_paid_call(Some(&catalog), Some(&mut gate), fixture_arguments());
    let second = execute_bazaar_mcp_paid_call(Some(&catalog), Some(&mut gate), fixture_arguments());

    assert!(first.ok);
    assert!(!second.ok);
    assert_eq!(second.code, "access_replay_blocked");
    assert!(!second.retryable);
    assert_no_authority(&second);
    assert_eq!(gate.bindings.len(), 2);
    assert_eq!(gate.bindings[0].call_digest, gate.bindings[1].call_digest);
}

#[test]
fn payment_and_settlement_states_have_stable_fail_closed_outcomes() {
    let catalog = catalog();
    let contract = read_value(PAID_CALL_FIXTURE_DIR, "outcome_contract.json");
    let cases = [
        (
            "paymentRequired",
            BazaarPaidCallAccessDecision::PaymentRequired,
        ),
        (
            "settlementPending",
            BazaarPaidCallAccessDecision::SettlementPending,
        ),
        (
            "settlementRejected",
            BazaarPaidCallAccessDecision::SettlementRejected,
        ),
        (
            "settlementOutcomeUnknown",
            BazaarPaidCallAccessDecision::SettlementOutcomeUnknown,
        ),
        ("replayBlocked", BazaarPaidCallAccessDecision::ReplayBlocked),
        ("unavailable", BazaarPaidCallAccessDecision::Unavailable),
    ];

    for (fixture_name, decision) in cases {
        let mut gate = RecordingGate::with([decision]);
        let result =
            execute_bazaar_mcp_paid_call(Some(&catalog), Some(&mut gate), fixture_arguments());
        let expected = &contract["outcomes"][fixture_name];
        assert!(!result.ok, "{fixture_name} unexpectedly authorized");
        assert_eq!(result.code, expected["code"]);
        assert_eq!(result.retryable, expected["retryable"]);
        assert!(!result.reason.is_empty());
        assert_eq!(result.data, None);
        assert_no_authority(&result);
    }
}

#[test]
fn caller_cannot_self_assert_payment_settlement_or_authority() {
    let catalog = catalog();
    for injected in [
        json!({"paymentVerified": true}),
        json!({"settled": true}),
        json!({"settlementTransactionHash": "a".repeat(64)}),
        json!({"authority": {"serviceCallAllowed": true}}),
        json!({"paymentPayload": {"signature": "forbidden"}}),
    ] {
        let mut arguments = fixture_arguments();
        let object = arguments.as_object_mut().expect("arguments object");
        object.extend(injected.as_object().expect("injected object").clone());
        let result = execute_bazaar_mcp_paid_call(Some(&catalog), None, arguments);
        assert_eq!(result.code, "invalid_arguments");
        assert_no_authority(&result);
    }
}

#[test]
fn catalog_target_and_json_bounds_fail_before_the_access_gate() {
    let catalog = catalog();
    let cases = [
        (
            json!({
                "schemaVersion": 1,
                "requestId": "paid-call-1",
                "resourceKey": "mcp:https://api.example.com/missing#tool",
                "arguments": {}
            }),
            "resource_not_found",
        ),
        (
            json!({
                "schemaVersion": 1,
                "requestId": "paid-call-1",
                "resourceKey": "http:https://api.example.com/weather/:country/:city",
                "arguments": {}
            }),
            "resource_not_mcp",
        ),
        (
            json!({
                "schemaVersion": 1,
                "requestId": "bad id",
                "resourceKey": "mcp:https://api.example.com/mcp#plan_stellar_action",
                "arguments": {}
            }),
            "invalid_request_id",
        ),
        (
            json!({
                "schemaVersion": 1,
                "requestId": "paid-call-1",
                "resourceKey": "mcp:\ninvalid",
                "arguments": {}
            }),
            "invalid_resource_key",
        ),
        (
            json!({
                "schemaVersion": 1,
                "requestId": "paid-call-1",
                "resourceKey": "mcp:https://api.example.com/mcp#plan_stellar_action",
                "arguments": []
            }),
            "invalid_service_arguments",
        ),
        (
            json!({
                "schemaVersion": 1,
                "requestId": "paid-call-1",
                "resourceKey": "mcp:https://api.example.com/mcp#plan_stellar_action",
                "arguments": {"input": "x".repeat(17_000)}
            }),
            "arguments_too_large",
        ),
    ];

    for (arguments, code) in cases {
        let mut gate = RecordingGate::with([BazaarPaidCallAccessDecision::Authorized]);
        let result = execute_bazaar_mcp_paid_call(Some(&catalog), Some(&mut gate), arguments);
        assert_eq!(result.code, code);
        assert_no_authority(&result);
        assert!(
            gate.bindings.is_empty(),
            "invalid request reached access gate"
        );
    }

    let result = execute_bazaar_mcp_paid_call(None, None, fixture_arguments());
    assert_eq!(result.code, "catalog_unavailable");
    assert!(result.retryable);
    assert_no_authority(&result);

    let result = execute_bazaar_mcp_paid_call(Some(&catalog), None, fixture_arguments());
    assert_eq!(result.code, "access_gate_unavailable");
    assert!(result.retryable);
    assert_no_authority(&result);
}

#[test]
fn mcp_result_has_structured_text_parity_without_dispatching() {
    let catalog = catalog();
    let mut gate = RecordingGate::with([BazaarPaidCallAccessDecision::Authorized]);
    let result = bazaar_mcp_paid_call_result(Some(&catalog), Some(&mut gate), fixture_arguments());
    assert_eq!(result["resultType"], "complete");
    assert_eq!(result["isError"], false);
    assert_eq!(
        result["structuredContent"]["tool"],
        BAZAAR_MCP_PAID_CALL_TOOL
    );
    assert_eq!(
        result["structuredContent"]["schemaVersion"],
        BAZAAR_PAID_CALL_SCHEMA_VERSION
    );
    let text: Value =
        serde_json::from_str(result["content"][0]["text"].as_str().expect("text content"))
            .expect("text content JSON");
    assert_eq!(text, result["structuredContent"]);
}

#[test]
fn fixtures_and_docs_lock_the_no_dispatch_authority_boundary() {
    let call = read_value(PAID_CALL_FIXTURE_DIR, "paid_call.json");
    assert_eq!(call["jsonrpc"], "2.0");
    assert_eq!(call["method"], "tools/call");
    assert_eq!(call["params"]["name"], BAZAAR_MCP_PAID_CALL_TOOL);
    assert!(call["params"]["arguments"].get("paymentPayload").is_none());

    for path in [
        "docs/x402_bazaar_paid_call.md",
        "examples/x402_bazaar_paid_call/README.md",
    ] {
        let content =
            fs::read_to_string(path).unwrap_or_else(|error| panic!("read {path}: {error}"));
        for required in [
            "offline",
            "settled",
            "single-use",
            "service call",
            "wallet",
            "ActionPlan",
            "no dispatch",
        ] {
            assert!(
                content.contains(required),
                "{path} is missing boundary term {required}"
            );
        }
    }
}
