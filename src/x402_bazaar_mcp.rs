use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::x402_bazaar::{
    BazaarCatalog, BazaarResourceType, BazaarSearchQuery, BazaarSearchResponse,
};

pub const BAZAAR_MCP_SCHEMA_VERSION: u32 = 1;
pub const BAZAAR_MCP_PROTOCOL_VERSION: &str = "2026-07-28";
pub const BAZAAR_MCP_SEARCH_TOOL: &str = "search_stellar_bazaar";
const MAX_ARGUMENT_BYTES: usize = 4_096;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarMcpSearchArguments {
    pub schema_version: u32,
    pub query: String,
    #[serde(rename = "type", default)]
    pub resource_type: Option<BazaarResourceType>,
    #[serde(default)]
    pub pay_to: Option<String>,
    #[serde(default)]
    pub scheme: Option<String>,
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub extensions: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub cursor: Option<String>,
}

impl From<BazaarMcpSearchArguments> for BazaarSearchQuery {
    fn from(arguments: BazaarMcpSearchArguments) -> Self {
        Self {
            query: arguments.query,
            resource_type: arguments.resource_type,
            pay_to: arguments.pay_to,
            scheme: arguments.scheme,
            network: arguments.network,
            extensions: arguments.extensions,
            limit: arguments.limit,
            cursor: arguments.cursor,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct BazaarMcpAuthority {
    payment_allowed: bool,
    proof_allowed: bool,
    approval_allowed: bool,
    settlement_allowed: bool,
    signing_allowed: bool,
    wallet_access_allowed: bool,
    shell_access_allowed: bool,
    rpc_submit_allowed: bool,
    action_plan_submit_allowed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BazaarMcpSearchResult {
    pub schema_version: u32,
    pub protocol_version: String,
    pub tool: String,
    pub ok: bool,
    pub code: String,
    pub reason: String,
    pub retryable: bool,
    pub authority: BazaarMcpAuthority,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<BazaarSearchResponse>,
}

impl BazaarMcpSearchResult {
    fn success(data: BazaarSearchResponse) -> Self {
        Self {
            schema_version: BAZAAR_MCP_SCHEMA_VERSION,
            protocol_version: BAZAAR_MCP_PROTOCOL_VERSION.to_string(),
            tool: BAZAAR_MCP_SEARCH_TOOL.to_string(),
            ok: true,
            code: "search_completed".to_string(),
            reason: "Bazaar search completed with read-only discovery authority.".to_string(),
            retryable: false,
            authority: BazaarMcpAuthority::default(),
            data: Some(data),
        }
    }

    fn rejected(code: &str, reason: impl Into<String>, retryable: bool) -> Self {
        let reason = reason.into();
        debug_assert!(!reason.trim().is_empty());
        Self {
            schema_version: BAZAAR_MCP_SCHEMA_VERSION,
            protocol_version: BAZAAR_MCP_PROTOCOL_VERSION.to_string(),
            tool: BAZAAR_MCP_SEARCH_TOOL.to_string(),
            ok: false,
            code: code.to_string(),
            reason,
            retryable,
            authority: BazaarMcpAuthority::default(),
            data: None,
        }
    }
}

pub fn bazaar_mcp_tools_list() -> Value {
    json!({
        "resultType": "complete",
        "tools": [bazaar_mcp_search_tool()],
        "ttlMs": 300_000,
        "cacheScope": "public"
    })
}

pub fn bazaar_mcp_search_tool() -> Value {
    json!({
        "name": BAZAAR_MCP_SEARCH_TOOL,
        "title": "Stellar Bazaar Resource Search",
        "description": "Search the local Stellar x402 Bazaar catalog. Read-only discovery only: this tool cannot pay, settle, sign, access a wallet or shell, call RPC submission, or submit an ActionPlan.",
        "annotations": {
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false
        },
        "inputSchema": bazaar_mcp_search_input_schema(),
        "outputSchema": bazaar_mcp_search_output_schema()
    })
}

pub fn bazaar_mcp_search_input_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "additionalProperties": false,
        "required": ["schemaVersion", "query"],
        "properties": {
            "schemaVersion": {"const": BAZAAR_MCP_SCHEMA_VERSION},
            "query": {"type": "string", "minLength": 1, "maxLength": 256},
            "type": {"type": "string", "enum": ["http", "mcp"]},
            "payTo": {"type": "string"},
            "scheme": {"type": "string", "enum": ["exact", "upto"]},
            "network": {"type": "string", "enum": ["stellar:testnet", "stellar:pubnet"]},
            "extensions": {"type": "string", "minLength": 1, "maxLength": 64},
            "limit": {"type": "integer", "minimum": 1, "maximum": 100},
            "cursor": {"type": "string", "minLength": 1, "maxLength": 64}
        }
    })
}

pub fn bazaar_mcp_search_output_schema() -> Value {
    let authority_properties = [
        "paymentAllowed",
        "proofAllowed",
        "approvalAllowed",
        "settlementAllowed",
        "signingAllowed",
        "walletAccessAllowed",
        "shellAccessAllowed",
        "rpcSubmitAllowed",
        "actionPlanSubmitAllowed",
    ];
    let payment_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "scheme", "network", "amount", "asset", "payTo", "maxTimeoutSeconds"
        ],
        "properties": {
            "scheme": {"type": "string", "enum": ["exact", "upto"]},
            "network": {"type": "string", "enum": ["stellar:testnet", "stellar:pubnet"]},
            "amount": {"type": "string"},
            "asset": {"type": "string"},
            "payTo": {"type": "string"},
            "maxTimeoutSeconds": {"type": "integer", "minimum": 1}
        }
    });
    let resource_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["resource", "type", "x402Version", "accepts", "lastUpdated"],
        "properties": {
            "resource": {"type": "string"},
            "type": {"type": "string", "enum": ["http", "mcp"]},
            "x402Version": {"const": 2},
            "accepts": {
                "type": "array",
                "minItems": 1,
                "maxItems": 1,
                "items": payment_schema
            },
            "lastUpdated": {"type": "integer", "minimum": 1}
        }
    });
    let data_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["x402Version", "resources", "partialResults", "pagination"],
        "properties": {
            "x402Version": {"const": 2},
            "resources": {"type": "array", "items": resource_schema},
            "partialResults": {"type": "boolean"},
            "pagination": {
                "type": "object",
                "additionalProperties": false,
                "required": ["limit", "cursor"],
                "properties": {
                    "limit": {"type": "integer", "minimum": 0, "maximum": 100},
                    "cursor": {"type": ["string", "null"]}
                }
            }
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
            {"properties": {"ok": {"const": true}}, "required": ["data"]},
            {
                "properties": {"ok": {"const": false}},
                "not": {"required": ["data"]}
            }
        ],
        "properties": {
            "schemaVersion": {"const": BAZAAR_MCP_SCHEMA_VERSION},
            "protocolVersion": {"const": BAZAAR_MCP_PROTOCOL_VERSION},
            "tool": {"const": BAZAAR_MCP_SEARCH_TOOL},
            "ok": {"type": "boolean"},
            "code": {"type": "string", "minLength": 1},
            "reason": {"type": "string", "minLength": 1},
            "retryable": {"type": "boolean"},
            "authority": {
                "type": "object",
                "additionalProperties": false,
                "required": authority_properties,
                "properties": {
                    "paymentAllowed": {"const": false},
                    "proofAllowed": {"const": false},
                    "approvalAllowed": {"const": false},
                    "settlementAllowed": {"const": false},
                    "signingAllowed": {"const": false},
                    "walletAccessAllowed": {"const": false},
                    "shellAccessAllowed": {"const": false},
                    "rpcSubmitAllowed": {"const": false},
                    "actionPlanSubmitAllowed": {"const": false}
                }
            },
            "data": data_schema
        }
    })
}

pub fn execute_bazaar_mcp_search(
    catalog: Option<&BazaarCatalog>,
    arguments: Value,
) -> BazaarMcpSearchResult {
    let argument_bytes = serde_json::to_vec(&arguments)
        .map(|encoded| encoded.len())
        .unwrap_or(usize::MAX);
    if argument_bytes > MAX_ARGUMENT_BYTES {
        return BazaarMcpSearchResult::rejected(
            "arguments_too_large",
            "MCP search arguments exceed the 4096-byte offline limit.",
            false,
        );
    }

    let arguments = match serde_json::from_value::<BazaarMcpSearchArguments>(arguments) {
        Ok(arguments) => arguments,
        Err(_) => {
            return BazaarMcpSearchResult::rejected(
                "invalid_arguments",
                "MCP search arguments did not match the strict input contract.",
                false,
            );
        }
    };
    if arguments.schema_version != BAZAAR_MCP_SCHEMA_VERSION {
        return BazaarMcpSearchResult::rejected(
            "unsupported_schema_version",
            format!("MCP search schemaVersion must be {BAZAAR_MCP_SCHEMA_VERSION}."),
            false,
        );
    }

    let Some(catalog) = catalog else {
        return BazaarMcpSearchResult::rejected(
            "catalog_unavailable",
            "The local Bazaar catalog is unavailable; no payment or external fallback was attempted.",
            true,
        );
    };

    match catalog.search(arguments.into()) {
        Ok(response) => BazaarMcpSearchResult::success(response),
        Err(error) => BazaarMcpSearchResult::rejected(
            error.code(),
            format!("Bazaar search rejected the request: {}.", error.code()),
            false,
        ),
    }
}

pub fn bazaar_mcp_call_result(catalog: Option<&BazaarCatalog>, arguments: Value) -> Value {
    let result = execute_bazaar_mcp_search(catalog, arguments);
    let is_error = !result.ok;
    let structured_content =
        serde_json::to_value(result).expect("Bazaar MCP result always serializes to JSON");
    let text = serde_json::to_string(&structured_content)
        .expect("serialized Bazaar MCP structured content always encodes as text");
    json!({
        "resultType": "complete",
        "content": [{"type": "text", "text": text}],
        "structuredContent": structured_content,
        "isError": is_error
    })
}
