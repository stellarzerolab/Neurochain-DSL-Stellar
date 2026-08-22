use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::{
    x402_bazaar::{BazaarCatalog, BazaarCatalogKey, BazaarResourceInput},
    x402_bazaar_mcp::BAZAAR_MCP_PROTOCOL_VERSION,
};

pub const BAZAAR_PAID_CALL_SCHEMA_VERSION: u32 = 1;
pub const BAZAAR_MCP_PAID_CALL_TOOL: &str = "proxy_paid_stellar_call";
const MAX_ARGUMENT_BYTES: usize = 16 * 1024;
const MAX_JSON_DEPTH: usize = 32;
const MAX_JSON_NODES: usize = 2_048;
const MAX_REQUEST_ID_BYTES: usize = 128;
const MAX_RESOURCE_KEY_BYTES: usize = 2_304;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarMcpPaidCallArguments {
    pub schema_version: u32,
    pub request_id: String,
    pub resource_key: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BazaarPaidCallBinding {
    pub schema_version: u32,
    pub request_id: String,
    pub resource_key: BazaarCatalogKey,
    pub resource_url: String,
    pub tool_name: String,
    pub network: String,
    pub arguments_digest: String,
    pub call_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BazaarPaidCallAccessDecision {
    Authorized,
    PaymentRequired,
    SettlementPending,
    SettlementRejected,
    SettlementOutcomeUnknown,
    ReplayBlocked,
    Unavailable,
}

/// Trusted runtime boundary that must atomically consume a settled-access
/// grant bound to the exact call digest. MCP arguments never implement this
/// trait and cannot supply an access decision themselves.
pub trait BazaarPaidCallAccessGate {
    fn consume_settled_access(
        &mut self,
        binding: &BazaarPaidCallBinding,
    ) -> BazaarPaidCallAccessDecision;
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct BazaarPaidCallAuthority {
    service_call_allowed: bool,
    payment_allowed: bool,
    proof_allowed: bool,
    approval_allowed: bool,
    settlement_allowed: bool,
    signing_allowed: bool,
    underlying_execution_allowed: bool,
    wallet_access_allowed: bool,
    shell_access_allowed: bool,
    rpc_submit_allowed: bool,
    action_plan_submit_allowed: bool,
}

impl BazaarPaidCallAuthority {
    fn service_call_only() -> Self {
        Self {
            service_call_allowed: true,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BazaarMcpPaidCallResult {
    pub schema_version: u32,
    pub protocol_version: String,
    pub tool: String,
    pub ok: bool,
    pub code: String,
    pub reason: String,
    pub retryable: bool,
    pub authority: BazaarPaidCallAuthority,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<BazaarPaidCallBinding>,
}

impl BazaarMcpPaidCallResult {
    fn authorized(binding: BazaarPaidCallBinding) -> Self {
        Self {
            schema_version: BAZAAR_PAID_CALL_SCHEMA_VERSION,
            protocol_version: BAZAAR_MCP_PROTOCOL_VERSION.to_string(),
            tool: BAZAAR_MCP_PAID_CALL_TOOL.to_string(),
            ok: true,
            code: "service_call_authorized".to_string(),
            reason: "Settled access was consumed for this exact named service call.".to_string(),
            retryable: false,
            authority: BazaarPaidCallAuthority::service_call_only(),
            data: Some(binding),
        }
    }

    fn rejected(code: &str, reason: &str, retryable: bool) -> Self {
        debug_assert!(!code.is_empty());
        debug_assert!(!reason.is_empty());
        Self {
            schema_version: BAZAAR_PAID_CALL_SCHEMA_VERSION,
            protocol_version: BAZAAR_MCP_PROTOCOL_VERSION.to_string(),
            tool: BAZAAR_MCP_PAID_CALL_TOOL.to_string(),
            ok: false,
            code: code.to_string(),
            reason: reason.to_string(),
            retryable,
            authority: BazaarPaidCallAuthority::default(),
            data: None,
        }
    }
}

pub fn bazaar_mcp_paid_call_tool() -> Value {
    json!({
        "name": BAZAAR_MCP_PAID_CALL_TOOL,
        "title": "Proxy One Paid Stellar Service Call",
        "description": "Authorize one exact cataloged MCP service call only after the trusted x402 runtime atomically consumes matching settled access. Payment, settlement, signing, wallet, shell, RPC submit, underlying execution, and ActionPlan submit remain outside this tool.",
        "annotations": {
            "readOnlyHint": false,
            "destructiveHint": true,
            "idempotentHint": false,
            "openWorldHint": true
        },
        "inputSchema": bazaar_mcp_paid_call_input_schema(),
        "outputSchema": bazaar_mcp_paid_call_output_schema()
    })
}

pub fn bazaar_mcp_paid_call_input_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "additionalProperties": false,
        "required": ["schemaVersion", "requestId", "resourceKey", "arguments"],
        "properties": {
            "schemaVersion": {"const": BAZAAR_PAID_CALL_SCHEMA_VERSION},
            "requestId": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_REQUEST_ID_BYTES,
                "pattern": "^[A-Za-z0-9._:-]+$"
            },
            "resourceKey": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_RESOURCE_KEY_BYTES
            },
            "arguments": {"type": "object"}
        }
    })
}

pub fn bazaar_mcp_paid_call_output_schema() -> Value {
    let false_authorities = [
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
    ];
    let mut authority_properties = Map::new();
    authority_properties.insert("serviceCallAllowed".to_string(), json!({"type": "boolean"}));
    for name in false_authorities {
        authority_properties.insert(name.to_string(), json!({"const": false}));
    }
    let mut required_authorities = vec!["serviceCallAllowed"];
    required_authorities.extend(false_authorities);

    let data_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "schemaVersion", "requestId", "resourceKey", "resourceUrl", "toolName",
            "network", "argumentsDigest", "callDigest"
        ],
        "properties": {
            "schemaVersion": {"const": BAZAAR_PAID_CALL_SCHEMA_VERSION},
            "requestId": {"type": "string"},
            "resourceKey": {"type": "string"},
            "resourceUrl": {"type": "string"},
            "toolName": {"type": "string"},
            "network": {"type": "string", "enum": ["stellar:testnet", "stellar:pubnet"]},
            "argumentsDigest": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "callDigest": {"type": "string", "pattern": "^[0-9a-f]{64}$"}
        }
    });
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "additionalProperties": false,
        "required": [
            "schemaVersion", "protocolVersion", "tool", "ok", "code", "reason",
            "retryable", "authority"
        ],
        "oneOf": [
            {
                "properties": {
                    "ok": {"const": true},
                    "authority": {
                        "properties": {"serviceCallAllowed": {"const": true}}
                    }
                },
                "required": ["data"]
            },
            {
                "properties": {
                    "ok": {"const": false},
                    "authority": {
                        "properties": {"serviceCallAllowed": {"const": false}}
                    }
                },
                "not": {"required": ["data"]}
            }
        ],
        "properties": {
            "schemaVersion": {"const": BAZAAR_PAID_CALL_SCHEMA_VERSION},
            "protocolVersion": {"const": BAZAAR_MCP_PROTOCOL_VERSION},
            "tool": {"const": BAZAAR_MCP_PAID_CALL_TOOL},
            "ok": {"type": "boolean"},
            "code": {"type": "string", "minLength": 1},
            "reason": {"type": "string", "minLength": 1},
            "retryable": {"type": "boolean"},
            "authority": {
                "type": "object",
                "additionalProperties": false,
                "required": required_authorities,
                "properties": authority_properties
            },
            "data": data_schema
        }
    })
}

pub fn execute_bazaar_mcp_paid_call(
    catalog: Option<&BazaarCatalog>,
    access_gate: Option<&mut dyn BazaarPaidCallAccessGate>,
    arguments: Value,
) -> BazaarMcpPaidCallResult {
    let argument_bytes = serde_json::to_vec(&arguments)
        .map(|encoded| encoded.len())
        .unwrap_or(usize::MAX);
    if argument_bytes > MAX_ARGUMENT_BYTES {
        return BazaarMcpPaidCallResult::rejected(
            "arguments_too_large",
            "Paid-call arguments exceed the 16384-byte offline limit.",
            false,
        );
    }

    let request = match serde_json::from_value::<BazaarMcpPaidCallArguments>(arguments) {
        Ok(request) => request,
        Err(_) => {
            return BazaarMcpPaidCallResult::rejected(
                "invalid_arguments",
                "Paid-call arguments did not match the strict input contract.",
                false,
            );
        }
    };
    if request.schema_version != BAZAAR_PAID_CALL_SCHEMA_VERSION {
        return BazaarMcpPaidCallResult::rejected(
            "unsupported_schema_version",
            "Paid-call schemaVersion must be 1.",
            false,
        );
    }
    if !is_valid_request_id(&request.request_id) {
        return BazaarMcpPaidCallResult::rejected(
            "invalid_request_id",
            "Paid-call requestId must use 1-128 safe identifier characters.",
            false,
        );
    }
    if !is_valid_resource_key(&request.resource_key) {
        return BazaarMcpPaidCallResult::rejected(
            "invalid_resource_key",
            "Paid-call resourceKey is empty, oversized, or contains control characters.",
            false,
        );
    }
    if !request.arguments.is_object()
        || !json_within_bounds(&request.arguments, MAX_JSON_DEPTH, MAX_JSON_NODES)
    {
        return BazaarMcpPaidCallResult::rejected(
            "invalid_service_arguments",
            "Paid-call service arguments must be a bounded JSON object.",
            false,
        );
    }

    let Some(catalog) = catalog else {
        return BazaarMcpPaidCallResult::rejected(
            "catalog_unavailable",
            "The local Bazaar catalog is unavailable.",
            true,
        );
    };
    let key = BazaarCatalogKey(request.resource_key.clone());
    let Some(resource) = catalog.get(&key) else {
        return BazaarMcpPaidCallResult::rejected(
            "resource_not_found",
            "The exact Bazaar resourceKey is not present in the local catalog.",
            false,
        );
    };
    let BazaarResourceInput::Mcp { tool_name } = &resource.input else {
        return BazaarMcpPaidCallResult::rejected(
            "resource_not_mcp",
            "This offline paid-call contract accepts only cataloged MCP resources.",
            false,
        );
    };

    let binding = build_call_binding(&request, resource, tool_name);
    let Some(access_gate) = access_gate else {
        return BazaarMcpPaidCallResult::rejected(
            "access_gate_unavailable",
            "The trusted settled-access gate is unavailable; no service call was authorized.",
            true,
        );
    };

    match access_gate.consume_settled_access(&binding) {
        BazaarPaidCallAccessDecision::Authorized => {
            BazaarMcpPaidCallResult::authorized(binding)
        }
        BazaarPaidCallAccessDecision::PaymentRequired => BazaarMcpPaidCallResult::rejected(
            "payment_required",
            "The exact service call has no settled access; the payment layer must handle the x402 retry loop.",
            true,
        ),
        BazaarPaidCallAccessDecision::SettlementPending => BazaarMcpPaidCallResult::rejected(
            "settlement_pending",
            "Payment verification did not yet produce settled access for this exact service call.",
            true,
        ),
        BazaarPaidCallAccessDecision::SettlementRejected => BazaarMcpPaidCallResult::rejected(
            "settlement_rejected",
            "Settlement was rejected; the service call remains unauthorized.",
            false,
        ),
        BazaarPaidCallAccessDecision::SettlementOutcomeUnknown => {
            BazaarMcpPaidCallResult::rejected(
                "settlement_outcome_unknown",
                "Settlement outcome is unknown; automatic payment retry and service call are blocked.",
                false,
            )
        }
        BazaarPaidCallAccessDecision::ReplayBlocked => BazaarMcpPaidCallResult::rejected(
            "access_replay_blocked",
            "Settled access for this exact service call was already consumed or did not match.",
            false,
        ),
        BazaarPaidCallAccessDecision::Unavailable => BazaarMcpPaidCallResult::rejected(
            "access_gate_unavailable",
            "Settled-access state is unavailable; no service call was authorized.",
            true,
        ),
    }
}

pub fn bazaar_mcp_paid_call_result(
    catalog: Option<&BazaarCatalog>,
    access_gate: Option<&mut dyn BazaarPaidCallAccessGate>,
    arguments: Value,
) -> Value {
    let result = execute_bazaar_mcp_paid_call(catalog, access_gate, arguments);
    let is_error = !result.ok;
    let structured_content =
        serde_json::to_value(result).expect("Bazaar paid-call result always serializes to JSON");
    let text = serde_json::to_string(&structured_content)
        .expect("serialized Bazaar paid-call structured content always encodes as text");
    json!({
        "resultType": "complete",
        "content": [{"type": "text", "text": text}],
        "structuredContent": structured_content,
        "isError": is_error
    })
}

fn build_call_binding(
    request: &BazaarMcpPaidCallArguments,
    resource: &crate::x402_bazaar::BazaarCatalogResource,
    tool_name: &str,
) -> BazaarPaidCallBinding {
    let arguments_digest = canonical_json_digest(&request.arguments);
    let binding_material = json!({
        "schemaVersion": BAZAAR_PAID_CALL_SCHEMA_VERSION,
        "requestId": request.request_id,
        "resourceKey": resource.key,
        "resourceUrl": resource.resource_url,
        "toolName": tool_name,
        "network": resource.payment.network,
        "payment": resource.payment,
        "argumentsDigest": arguments_digest
    });
    let call_digest = canonical_json_digest(&binding_material);
    BazaarPaidCallBinding {
        schema_version: BAZAAR_PAID_CALL_SCHEMA_VERSION,
        request_id: request.request_id.clone(),
        resource_key: resource.key.clone(),
        resource_url: resource.resource_url.clone(),
        tool_name: tool_name.to_string(),
        network: resource.payment.network.clone(),
        arguments_digest,
        call_digest,
    }
}

fn canonical_json_digest(value: &Value) -> String {
    let canonical = canonicalize_json(value);
    let encoded = serde_json::to_vec(&canonical)
        .expect("bounded JSON values always serialize for paid-call binding");
    hex::encode(Sha256::digest(encoded))
}

fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let sorted = object
                .iter()
                .map(|(key, value)| (key.clone(), canonicalize_json(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(values) => Value::Array(values.iter().map(canonicalize_json).collect()),
        other => other.clone(),
    }
}

fn json_within_bounds(value: &Value, max_depth: usize, max_nodes: usize) -> bool {
    fn visit(value: &Value, depth: usize, max_depth: usize, nodes: &mut usize) -> bool {
        *nodes = nodes.saturating_add(1);
        if depth > max_depth {
            return false;
        }
        match value {
            Value::Array(values) => values
                .iter()
                .all(|value| visit(value, depth + 1, max_depth, nodes)),
            Value::Object(object) => object
                .values()
                .all(|value| visit(value, depth + 1, max_depth, nodes)),
            _ => true,
        }
    }

    let mut nodes = 0;
    visit(value, 0, max_depth, &mut nodes) && nodes <= max_nodes
}

fn is_valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REQUEST_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn is_valid_resource_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_RESOURCE_KEY_BYTES
        && value.bytes().all(|byte| !byte.is_ascii_control())
}
