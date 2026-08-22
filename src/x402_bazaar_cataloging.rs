use std::collections::BTreeSet;

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::x402_bazaar::{
    BazaarCatalog, BazaarCatalogCandidate, BazaarCatalogError, BazaarCatalogKey, BazaarHttpMethod,
    BazaarPaymentSummary, BazaarResourceDescriptor, BazaarResourceInput,
};

pub const BAZAAR_AUTOMATIC_CATALOGING_SCHEMA_VERSION: u32 = 1;
const DRAFT_2020_12_URI: &str = "https://json-schema.org/draft/2020-12/schema";
const MAX_SCHEMA_BYTES: usize = 32 * 1024;
const MAX_INFO_BYTES: usize = 32 * 1024;
const MAX_JSON_DEPTH: usize = 32;
const MAX_JSON_NODES: usize = 4_096;
const MAX_REASON_BYTES: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarDiscoveryExtension {
    pub info: Value,
    pub schema: Value,
    #[serde(default)]
    pub route_template: Option<String>,
}

/// Offline handoff produced only after the facilitator has verified the x402
/// payment phase. It deliberately excludes the raw PaymentPayload, signatures,
/// credentials, signing capabilities, and settlement or submit authority.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarVerifiedDiscoveryInput {
    pub schema_version: u32,
    pub x402_version: u32,
    pub resource: BazaarResourceDescriptor,
    pub payment: BazaarPaymentSummary,
    #[serde(default)]
    pub bazaar: Option<BazaarDiscoveryExtension>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BazaarCatalogingDisposition {
    Accepted,
    Dropped,
    Invalid,
    Duplicate,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarCatalogingOutcome {
    pub disposition: BazaarCatalogingDisposition,
    pub code: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_key: Option<BazaarCatalogKey>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BazaarExtensionResponseStatus {
    Success,
    Processing,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarExtensionResponse {
    pub status: BazaarExtensionResponseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BazaarExtensionResponses {
    pub bazaar: BazaarExtensionResponse,
}

impl BazaarCatalogingOutcome {
    fn new(
        disposition: BazaarCatalogingDisposition,
        code: impl Into<String>,
        reason: impl Into<String>,
        catalog_key: Option<BazaarCatalogKey>,
    ) -> Self {
        let code = code.into();
        let mut reason = reason.into();
        if reason.is_empty() {
            reason = code.clone();
        }
        if reason.len() > MAX_REASON_BYTES {
            let mut boundary = MAX_REASON_BYTES;
            while !reason.is_char_boundary(boundary) {
                boundary -= 1;
            }
            reason.truncate(boundary);
        }
        Self {
            disposition,
            code,
            reason,
            catalog_key,
        }
    }

    /// Returns the spec-compatible response body later encoded into the
    /// `EXTENSION-RESPONSES` header. A missing Bazaar extension produces no
    /// Bazaar response, as required by the x402 extension contract.
    pub fn extension_responses(&self) -> Option<BazaarExtensionResponses> {
        let bazaar = match self.disposition {
            BazaarCatalogingDisposition::Accepted => BazaarExtensionResponse {
                status: BazaarExtensionResponseStatus::Success,
                rejected_reason: None,
            },
            BazaarCatalogingDisposition::Dropped => return None,
            BazaarCatalogingDisposition::Invalid
            | BazaarCatalogingDisposition::Duplicate
            | BazaarCatalogingDisposition::Unavailable => BazaarExtensionResponse {
                status: BazaarExtensionResponseStatus::Rejected,
                rejected_reason: Some(format!("{}: {}", self.code, self.reason)),
            },
        };
        Some(BazaarExtensionResponses { bazaar })
    }

    pub fn extension_responses_header_value(&self) -> Result<Option<String>, serde_json::Error> {
        self.extension_responses()
            .map(|response| serde_json::to_vec(&response).map(|json| BASE64_STANDARD.encode(json)))
            .transpose()
    }
}

/// Validates and catalogs a discovery extension without receiving a raw
/// PaymentPayload or gaining payment, settlement, signing, execution, or
/// ActionPlan-submit authority.
pub fn catalog_verified_discovery(
    catalog: Option<&mut BazaarCatalog>,
    request: BazaarVerifiedDiscoveryInput,
    observed_at_unix: u64,
) -> BazaarCatalogingOutcome {
    if request.schema_version != BAZAAR_AUTOMATIC_CATALOGING_SCHEMA_VERSION {
        return invalid_outcome(
            "unsupported_cataloging_schema_version",
            "automatic cataloging envelope schema version is unsupported",
        );
    }

    let Some(extension) = request.bazaar else {
        return BazaarCatalogingOutcome::new(
            BazaarCatalogingDisposition::Dropped,
            "bazaar_extension_missing",
            "verified discovery input did not contain the Bazaar extension",
            None,
        );
    };

    let input = match validate_and_extract_input(&extension) {
        Ok(input) => input,
        Err(failure) => return failure.into_outcome(),
    };

    let Some(catalog) = catalog else {
        return unavailable_outcome(
            "catalog_unavailable",
            "Bazaar catalog storage is unavailable",
        );
    };

    let candidate = BazaarCatalogCandidate {
        schema_version: request.schema_version,
        x402_version: request.x402_version,
        resource: request.resource,
        input,
        route_template: extension.route_template,
        payment: request.payment,
    };

    match catalog.insert(candidate, observed_at_unix) {
        Ok(resource) => BazaarCatalogingOutcome::new(
            BazaarCatalogingDisposition::Accepted,
            "cataloged",
            "discovery info passed schema validation and was cataloged",
            Some(resource.key.clone()),
        ),
        Err(BazaarCatalogError::DuplicateResource(key)) => BazaarCatalogingOutcome::new(
            BazaarCatalogingDisposition::Duplicate,
            "duplicate_resource",
            "catalog already contains the canonical resource key",
            Some(key),
        ),
        Err(error) => invalid_outcome(
            error.code(),
            format!("extracted catalog candidate failed validation: {error}"),
        ),
    }
}

#[derive(Debug)]
enum SchemaFailure {
    Invalid { code: &'static str, reason: String },
    Unavailable { code: &'static str, reason: String },
}

impl SchemaFailure {
    fn invalid(code: &'static str, reason: impl Into<String>) -> Self {
        Self::Invalid {
            code,
            reason: reason.into(),
        }
    }

    fn unavailable(code: &'static str, reason: impl Into<String>) -> Self {
        Self::Unavailable {
            code,
            reason: reason.into(),
        }
    }

    fn into_outcome(self) -> BazaarCatalogingOutcome {
        match self {
            Self::Invalid { code, reason } => invalid_outcome(code, reason),
            Self::Unavailable { code, reason } => unavailable_outcome(code, reason),
        }
    }
}

fn invalid_outcome(code: impl Into<String>, reason: impl Into<String>) -> BazaarCatalogingOutcome {
    BazaarCatalogingOutcome::new(BazaarCatalogingDisposition::Invalid, code, reason, None)
}

fn unavailable_outcome(
    code: impl Into<String>,
    reason: impl Into<String>,
) -> BazaarCatalogingOutcome {
    BazaarCatalogingOutcome::new(BazaarCatalogingDisposition::Unavailable, code, reason, None)
}

fn validate_and_extract_input(
    extension: &BazaarDiscoveryExtension,
) -> Result<BazaarResourceInput, SchemaFailure> {
    validate_json_bounds(&extension.schema, MAX_SCHEMA_BYTES, "schema")?;
    validate_json_bounds(&extension.info, MAX_INFO_BYTES, "info")?;
    validate_schema_document(&extension.schema)?;

    let input = extract_input(&extension.info)?;
    validate_discovery_schema_contract(&extension.schema, &input)?;
    validate_instance(
        &extension.info,
        &extension.schema,
        &extension.schema,
        0,
        &mut Vec::new(),
    )?;
    Ok(input)
}

fn validate_json_bounds(
    value: &Value,
    max_bytes: usize,
    label: &'static str,
) -> Result<(), SchemaFailure> {
    let encoded = serde_json::to_vec(value).map_err(|_| {
        SchemaFailure::invalid(
            "invalid_json_value",
            format!("{label} could not be encoded"),
        )
    })?;
    if encoded.len() > max_bytes {
        return Err(SchemaFailure::invalid(
            "json_value_too_large",
            format!("{label} exceeds the {max_bytes}-byte offline limit"),
        ));
    }
    let mut nodes = 0;
    validate_json_shape(value, 0, &mut nodes, label)
}

fn validate_json_shape(
    value: &Value,
    depth: usize,
    nodes: &mut usize,
    label: &'static str,
) -> Result<(), SchemaFailure> {
    if depth > MAX_JSON_DEPTH {
        return Err(SchemaFailure::invalid(
            "json_value_too_deep",
            format!("{label} exceeds the maximum JSON depth"),
        ));
    }
    *nodes += 1;
    if *nodes > MAX_JSON_NODES {
        return Err(SchemaFailure::invalid(
            "json_value_too_complex",
            format!("{label} exceeds the maximum JSON node count"),
        ));
    }
    match value {
        Value::Array(values) => {
            for value in values {
                validate_json_shape(value, depth + 1, nodes, label)?;
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                validate_json_shape(value, depth + 1, nodes, label)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_schema_document(schema: &Value) -> Result<(), SchemaFailure> {
    let root = schema.as_object().ok_or_else(|| {
        SchemaFailure::invalid("invalid_schema", "schema root must be a JSON object")
    })?;
    if root.get("$schema").and_then(Value::as_str) != Some(DRAFT_2020_12_URI) {
        return Err(SchemaFailure::invalid(
            "unsupported_schema_draft",
            "schema must declare JSON Schema Draft 2020-12",
        ));
    }
    scan_schema_references(schema)?;
    validate_supported_schema_keywords(schema, 0)
}

fn scan_schema_references(value: &Value) -> Result<(), SchemaFailure> {
    match value {
        Value::Array(values) => {
            for value in values {
                scan_schema_references(value)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                if matches!(key.as_str(), "$ref" | "$id") {
                    let reference = value.as_str().ok_or_else(|| {
                        SchemaFailure::invalid(
                            "invalid_schema_reference",
                            format!("{key} must be a string"),
                        )
                    })?;
                    if !reference.starts_with('#') {
                        return Err(SchemaFailure::invalid(
                            "external_schema_reference",
                            format!("external {key} values are forbidden"),
                        ));
                    }
                }
                scan_schema_references(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_supported_schema_keywords(schema: &Value, depth: usize) -> Result<(), SchemaFailure> {
    if depth > MAX_JSON_DEPTH {
        return Err(SchemaFailure::invalid(
            "invalid_schema",
            "schema nesting exceeds the offline validation limit",
        ));
    }
    let Value::Object(object) = schema else {
        if schema.is_boolean() {
            return Ok(());
        }
        return Err(SchemaFailure::invalid(
            "invalid_schema",
            "schema nodes must be objects or booleans",
        ));
    };

    const SUPPORTED: &[&str] = &[
        "$schema",
        "$id",
        "$ref",
        "$defs",
        "$comment",
        "title",
        "description",
        "default",
        "examples",
        "deprecated",
        "readOnly",
        "writeOnly",
        "type",
        "const",
        "enum",
        "properties",
        "required",
        "additionalProperties",
        "items",
        "minLength",
        "maxLength",
        "minimum",
        "maximum",
    ];
    for key in object.keys() {
        if !SUPPORTED.contains(&key.as_str()) {
            return Err(SchemaFailure::unavailable(
                "schema_profile_unavailable",
                format!("offline validator does not support schema keyword {key}"),
            ));
        }
    }
    validate_schema_keyword_shapes(object)?;

    for key in ["properties", "$defs"] {
        if let Some(children) = object.get(key) {
            let children = children.as_object().ok_or_else(|| {
                SchemaFailure::invalid("invalid_schema", format!("{key} must be an object"))
            })?;
            for child in children.values() {
                validate_supported_schema_keywords(child, depth + 1)?;
            }
        }
    }
    if let Some(additional) = object.get("additionalProperties") {
        if !additional.is_boolean() {
            validate_supported_schema_keywords(additional, depth + 1)?;
        }
    }
    if let Some(items) = object.get("items") {
        validate_supported_schema_keywords(items, depth + 1)?;
    }
    Ok(())
}

fn validate_schema_keyword_shapes(object: &Map<String, Value>) -> Result<(), SchemaFailure> {
    for keyword in ["$schema", "$id", "$ref"] {
        if object.get(keyword).is_some_and(|value| !value.is_string()) {
            return Err(SchemaFailure::invalid(
                "invalid_schema",
                format!("{keyword} must be a string"),
            ));
        }
    }
    if let Some(schema_type) = object.get("type") {
        let Some(schema_type) = schema_type.as_str() else {
            return Err(SchemaFailure::unavailable(
                "schema_profile_unavailable",
                "offline validator supports only a single string type keyword",
            ));
        };
        if !matches!(
            schema_type,
            "object" | "array" | "string" | "number" | "integer" | "boolean" | "null"
        ) {
            return Err(SchemaFailure::invalid(
                "invalid_schema",
                format!("unknown schema type {schema_type}"),
            ));
        }
    }
    if let Some(values) = object.get("enum") {
        if values.as_array().is_none_or(Vec::is_empty) {
            return Err(SchemaFailure::invalid(
                "invalid_schema",
                "enum must be a non-empty array",
            ));
        }
    }
    if let Some(required) = object.get("required") {
        let required = required
            .as_array()
            .ok_or_else(|| SchemaFailure::invalid("invalid_schema", "required must be an array"))?;
        let mut unique = BTreeSet::new();
        for field in required {
            let field = field.as_str().ok_or_else(|| {
                SchemaFailure::invalid("invalid_schema", "required entries must be strings")
            })?;
            if !unique.insert(field) {
                return Err(SchemaFailure::invalid(
                    "invalid_schema",
                    "required entries must be unique",
                ));
            }
        }
    }
    for keyword in ["deprecated", "readOnly", "writeOnly"] {
        if object.get(keyword).is_some_and(|value| !value.is_boolean()) {
            return Err(SchemaFailure::invalid(
                "invalid_schema",
                format!("{keyword} must be a boolean"),
            ));
        }
    }
    let min_length = schema_u64(object, "minLength")?;
    let max_length = schema_u64(object, "maxLength")?;
    if min_length
        .zip(max_length)
        .is_some_and(|(min, max)| min > max)
    {
        return Err(SchemaFailure::invalid(
            "invalid_schema",
            "minLength must not exceed maxLength",
        ));
    }
    let minimum = schema_f64(object, "minimum")?;
    let maximum = schema_f64(object, "maximum")?;
    if minimum.zip(maximum).is_some_and(|(min, max)| min > max) {
        return Err(SchemaFailure::invalid(
            "invalid_schema",
            "minimum must not exceed maximum",
        ));
    }
    Ok(())
}

fn extract_input(info: &Value) -> Result<BazaarResourceInput, SchemaFailure> {
    let input = info
        .as_object()
        .and_then(|info| info.get("input"))
        .and_then(Value::as_object)
        .ok_or_else(|| {
            SchemaFailure::invalid(
                "invalid_discovery_info",
                "discovery info must contain an input object",
            )
        })?;
    match input.get("type").and_then(Value::as_str) {
        Some("http") => extract_http_input(input),
        Some("mcp") => extract_mcp_input(input),
        _ => Err(SchemaFailure::invalid(
            "invalid_discovery_input_type",
            "discovery input type must be http or mcp",
        )),
    }
}

fn extract_http_input(input: &Map<String, Value>) -> Result<BazaarResourceInput, SchemaFailure> {
    let method = input
        .get("method")
        .cloned()
        .ok_or_else(|| {
            SchemaFailure::invalid("invalid_http_discovery", "HTTP input requires method")
        })
        .and_then(|method| {
            serde_json::from_value::<BazaarHttpMethod>(method).map_err(|_| {
                SchemaFailure::invalid(
                    "invalid_http_discovery",
                    "HTTP method must be GET, HEAD, DELETE, POST, PUT, or PATCH",
                )
            })
        })?;
    if matches!(
        method,
        BazaarHttpMethod::Post | BazaarHttpMethod::Put | BazaarHttpMethod::Patch
    ) && (!matches!(
        input.get("bodyType").and_then(Value::as_str),
        Some("json" | "form-data" | "text")
    ) || !input.contains_key("body"))
    {
        return Err(SchemaFailure::invalid(
            "invalid_http_discovery",
            "POST, PUT, and PATCH inputs require bodyType and body",
        ));
    }
    Ok(BazaarResourceInput::Http { method })
}

fn extract_mcp_input(input: &Map<String, Value>) -> Result<BazaarResourceInput, SchemaFailure> {
    let tool_name = input
        .get("toolName")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SchemaFailure::invalid("invalid_mcp_discovery", "MCP input requires toolName")
        })?;
    if !input.get("inputSchema").is_some_and(Value::is_object) {
        return Err(SchemaFailure::invalid(
            "invalid_mcp_discovery",
            "MCP input requires an inputSchema object",
        ));
    }
    if let Some(transport) = input.get("transport") {
        if !matches!(transport.as_str(), Some("streamable-http" | "sse")) {
            return Err(SchemaFailure::invalid(
                "invalid_mcp_discovery",
                "MCP transport must be streamable-http or sse",
            ));
        }
    }
    Ok(BazaarResourceInput::Mcp {
        tool_name: tool_name.to_string(),
    })
}

fn validate_discovery_schema_contract(
    schema: &Value,
    input: &BazaarResourceInput,
) -> Result<(), SchemaFailure> {
    let root = schema.as_object().expect("schema root checked");
    if root.get("type").and_then(Value::as_str) != Some("object") {
        return Err(SchemaFailure::invalid(
            "invalid_discovery_schema_contract",
            "discovery schema root must declare type object",
        ));
    }
    require_schema_field(root, "input")?;
    let input_schema = root
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get("input"))
        .ok_or_else(|| {
            SchemaFailure::invalid(
                "invalid_discovery_schema_contract",
                "discovery schema must define properties.input",
            )
        })?;
    let input_schema = resolve_local_ref(input_schema, schema)?
        .as_object()
        .ok_or_else(|| {
            SchemaFailure::invalid(
                "invalid_discovery_schema_contract",
                "input schema must resolve to an object schema",
            )
        })?;
    if input_schema.get("type").and_then(Value::as_str) != Some("object") {
        return Err(SchemaFailure::invalid(
            "invalid_discovery_schema_contract",
            "input schema must declare type object",
        ));
    }
    require_schema_field(input_schema, "type")?;
    let properties = input_schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            SchemaFailure::invalid(
                "invalid_discovery_schema_contract",
                "input schema must define properties",
            )
        })?;
    let expected_type = match input {
        BazaarResourceInput::Http { .. } => "http",
        BazaarResourceInput::Mcp { .. } => "mcp",
    };
    if properties
        .get("type")
        .and_then(Value::as_object)
        .and_then(|schema| schema.get("const"))
        .and_then(Value::as_str)
        != Some(expected_type)
    {
        return Err(SchemaFailure::invalid(
            "invalid_discovery_schema_contract",
            "input.type schema must use the matching http or mcp const",
        ));
    }

    match input {
        BazaarResourceInput::Http { method } => {
            require_schema_field(input_schema, "method")?;
            validate_http_method_schema(properties, *method)?;
            if matches!(
                method,
                BazaarHttpMethod::Post | BazaarHttpMethod::Put | BazaarHttpMethod::Patch
            ) {
                require_schema_field(input_schema, "bodyType")?;
                require_schema_field(input_schema, "body")?;
            }
        }
        BazaarResourceInput::Mcp { .. } => {
            require_schema_field(input_schema, "toolName")?;
            require_schema_field(input_schema, "inputSchema")?;
        }
    }
    Ok(())
}

fn require_schema_field(schema: &Map<String, Value>, field: &str) -> Result<(), SchemaFailure> {
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            SchemaFailure::invalid(
                "invalid_discovery_schema_contract",
                "discovery schema requires a required array",
            )
        })?;
    if required.iter().any(|value| value.as_str() == Some(field)) {
        Ok(())
    } else {
        Err(SchemaFailure::invalid(
            "invalid_discovery_schema_contract",
            format!("discovery schema must require {field}"),
        ))
    }
}

fn validate_http_method_schema(
    properties: &Map<String, Value>,
    method: BazaarHttpMethod,
) -> Result<(), SchemaFailure> {
    let allowed = properties
        .get("method")
        .and_then(Value::as_object)
        .and_then(|schema| schema.get("enum"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            SchemaFailure::invalid(
                "invalid_discovery_schema_contract",
                "HTTP method schema must declare an enum",
            )
        })?;
    let expected: &[&str] = if matches!(
        method,
        BazaarHttpMethod::Get | BazaarHttpMethod::Head | BazaarHttpMethod::Delete
    ) {
        &["GET", "HEAD", "DELETE"]
    } else {
        &["POST", "PUT", "PATCH"]
    };
    if allowed.is_empty()
        || allowed.iter().any(|value| {
            value
                .as_str()
                .is_none_or(|value| !expected.contains(&value))
        })
    {
        return Err(SchemaFailure::invalid(
            "invalid_discovery_schema_contract",
            "HTTP method enum must stay within its query or body method family",
        ));
    }
    Ok(())
}

fn validate_instance(
    instance: &Value,
    schema: &Value,
    root: &Value,
    depth: usize,
    ref_stack: &mut Vec<String>,
) -> Result<(), SchemaFailure> {
    if depth > MAX_JSON_DEPTH {
        return Err(SchemaFailure::invalid(
            "schema_info_mismatch",
            "schema evaluation exceeded the maximum depth",
        ));
    }
    if let Some(boolean) = schema.as_bool() {
        return if boolean {
            Ok(())
        } else {
            Err(SchemaFailure::invalid(
                "schema_info_mismatch",
                "boolean false schema rejected discovery info",
            ))
        };
    }
    let object = schema.as_object().ok_or_else(|| {
        SchemaFailure::invalid("invalid_schema", "schema node must be an object or boolean")
    })?;

    if let Some(reference) = object.get("$ref") {
        let reference = reference.as_str().ok_or_else(|| {
            SchemaFailure::invalid("invalid_schema_reference", "$ref must be a string")
        })?;
        if ref_stack.iter().any(|active| active == reference) {
            return Err(SchemaFailure::unavailable(
                "schema_profile_unavailable",
                "cyclic local schema references are unsupported by the offline validator",
            ));
        }
        ref_stack.push(reference.to_string());
        let resolved = resolve_pointer(root, reference)?;
        let result = validate_instance(instance, resolved, root, depth + 1, ref_stack);
        ref_stack.pop();
        result?;
    }

    if let Some(expected_type) = object.get("type") {
        let expected_type = expected_type.as_str().ok_or_else(|| {
            SchemaFailure::unavailable(
                "schema_profile_unavailable",
                "offline validator supports only a single string type keyword",
            )
        })?;
        if !instance_matches_type(instance, expected_type) {
            return Err(SchemaFailure::invalid(
                "schema_info_mismatch",
                format!("value does not match schema type {expected_type}"),
            ));
        }
    }
    if object
        .get("const")
        .is_some_and(|expected| expected != instance)
    {
        return Err(SchemaFailure::invalid(
            "schema_info_mismatch",
            "value does not match schema const",
        ));
    }
    if let Some(values) = object.get("enum") {
        let values = values
            .as_array()
            .ok_or_else(|| SchemaFailure::invalid("invalid_schema", "enum must be an array"))?;
        if values.is_empty() || !values.iter().any(|expected| expected == instance) {
            return Err(SchemaFailure::invalid(
                "schema_info_mismatch",
                "value is not a member of schema enum",
            ));
        }
    }
    validate_object_instance(instance, object, root, depth, ref_stack)?;
    validate_array_instance(instance, object, root, depth, ref_stack)?;
    validate_string_instance(instance, object)?;
    validate_number_instance(instance, object)?;
    Ok(())
}

fn validate_object_instance(
    instance: &Value,
    schema: &Map<String, Value>,
    root: &Value,
    depth: usize,
    ref_stack: &mut Vec<String>,
) -> Result<(), SchemaFailure> {
    let Some(instance) = instance.as_object() else {
        return Ok(());
    };
    if let Some(required) = schema.get("required") {
        let required = required
            .as_array()
            .ok_or_else(|| SchemaFailure::invalid("invalid_schema", "required must be an array"))?;
        let mut unique = BTreeSet::new();
        for field in required {
            let field = field.as_str().ok_or_else(|| {
                SchemaFailure::invalid("invalid_schema", "required entries must be strings")
            })?;
            if !unique.insert(field) {
                return Err(SchemaFailure::invalid(
                    "invalid_schema",
                    "required entries must be unique",
                ));
            }
            if !instance.contains_key(field) {
                return Err(SchemaFailure::invalid(
                    "schema_info_mismatch",
                    format!("required field {field} is missing"),
                ));
            }
        }
    }
    let properties = schema.get("properties").map_or(Ok(None), |value| {
        value
            .as_object()
            .map(Some)
            .ok_or_else(|| SchemaFailure::invalid("invalid_schema", "properties must be an object"))
    })?;
    for (field, value) in instance {
        if let Some(field_schema) = properties.and_then(|properties| properties.get(field)) {
            validate_instance(value, field_schema, root, depth + 1, ref_stack)?;
            continue;
        }
        match schema.get("additionalProperties") {
            Some(Value::Bool(false)) => {
                return Err(SchemaFailure::invalid(
                    "schema_info_mismatch",
                    format!("additional field {field} is forbidden"),
                ));
            }
            Some(Value::Object(_)) | Some(Value::Bool(true)) => {
                if let Some(additional) = schema.get("additionalProperties") {
                    validate_instance(value, additional, root, depth + 1, ref_stack)?;
                }
            }
            Some(_) => {
                return Err(SchemaFailure::invalid(
                    "invalid_schema",
                    "additionalProperties must be a boolean or schema",
                ));
            }
            None => {}
        }
    }
    Ok(())
}

fn validate_array_instance(
    instance: &Value,
    schema: &Map<String, Value>,
    root: &Value,
    depth: usize,
    ref_stack: &mut Vec<String>,
) -> Result<(), SchemaFailure> {
    let (Some(instance), Some(items)) = (instance.as_array(), schema.get("items")) else {
        return Ok(());
    };
    for value in instance {
        validate_instance(value, items, root, depth + 1, ref_stack)?;
    }
    Ok(())
}

fn validate_string_instance(
    instance: &Value,
    schema: &Map<String, Value>,
) -> Result<(), SchemaFailure> {
    let Some(instance) = instance.as_str() else {
        return Ok(());
    };
    let length = instance.chars().count() as u64;
    for (keyword, comparison) in [
        (
            "minLength",
            length >= schema_u64(schema, "minLength")?.unwrap_or(0),
        ),
        (
            "maxLength",
            length <= schema_u64(schema, "maxLength")?.unwrap_or(u64::MAX),
        ),
    ] {
        if !comparison {
            return Err(SchemaFailure::invalid(
                "schema_info_mismatch",
                format!("string violates {keyword}"),
            ));
        }
    }
    Ok(())
}

fn validate_number_instance(
    instance: &Value,
    schema: &Map<String, Value>,
) -> Result<(), SchemaFailure> {
    let Some(instance) = instance.as_f64() else {
        return Ok(());
    };
    for (keyword, comparison) in [
        (
            "minimum",
            instance >= schema_f64(schema, "minimum")?.unwrap_or(f64::NEG_INFINITY),
        ),
        (
            "maximum",
            instance <= schema_f64(schema, "maximum")?.unwrap_or(f64::INFINITY),
        ),
    ] {
        if !comparison {
            return Err(SchemaFailure::invalid(
                "schema_info_mismatch",
                format!("number violates {keyword}"),
            ));
        }
    }
    Ok(())
}

fn schema_u64(schema: &Map<String, Value>, keyword: &str) -> Result<Option<u64>, SchemaFailure> {
    schema
        .get(keyword)
        .map(|value| {
            value.as_u64().ok_or_else(|| {
                SchemaFailure::invalid("invalid_schema", format!("{keyword} must be a u64"))
            })
        })
        .transpose()
}

fn schema_f64(schema: &Map<String, Value>, keyword: &str) -> Result<Option<f64>, SchemaFailure> {
    schema
        .get(keyword)
        .map(|value| {
            value.as_f64().ok_or_else(|| {
                SchemaFailure::invalid("invalid_schema", format!("{keyword} must be a number"))
            })
        })
        .transpose()
}

fn instance_matches_type(instance: &Value, expected: &str) -> bool {
    match expected {
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "string" => instance.is_string(),
        "number" => instance.is_number(),
        "integer" => instance.as_i64().is_some() || instance.as_u64().is_some(),
        "boolean" => instance.is_boolean(),
        "null" => instance.is_null(),
        _ => false,
    }
}

fn resolve_local_ref<'a>(schema: &'a Value, root: &'a Value) -> Result<&'a Value, SchemaFailure> {
    schema
        .as_object()
        .and_then(|schema| schema.get("$ref"))
        .map_or(Ok(schema), |reference| {
            let reference = reference.as_str().ok_or_else(|| {
                SchemaFailure::invalid("invalid_schema_reference", "$ref must be a string")
            })?;
            resolve_pointer(root, reference)
        })
}

fn resolve_pointer<'a>(root: &'a Value, reference: &str) -> Result<&'a Value, SchemaFailure> {
    if reference == "#" {
        return Ok(root);
    }
    let pointer = reference.strip_prefix('#').ok_or_else(|| {
        SchemaFailure::invalid(
            "external_schema_reference",
            "external schema references are forbidden",
        )
    })?;
    if !pointer.starts_with('/') {
        return Err(SchemaFailure::unavailable(
            "schema_profile_unavailable",
            "offline validator supports local JSON Pointer references only",
        ));
    }
    root.pointer(pointer).ok_or_else(|| {
        SchemaFailure::invalid(
            "invalid_schema_reference",
            "local schema reference does not resolve",
        )
    })
}
