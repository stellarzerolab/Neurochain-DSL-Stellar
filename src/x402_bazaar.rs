use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet},
    fmt,
    net::IpAddr,
};

use serde::{Deserialize, Serialize};

pub const BAZAAR_CATALOG_SCHEMA_VERSION: u32 = 1;
pub const BAZAAR_LIST_DEFAULT_LIMIT: usize = 20;
pub const BAZAAR_LIST_MAX_LIMIT: usize = 100;
pub const BAZAAR_SEARCH_DEFAULT_LIMIT: usize = 20;
pub const BAZAAR_SEARCH_MAX_LIMIT: usize = 100;
const X402_VERSION: u32 = 2;
const MAX_RESOURCE_URL_BYTES: usize = 2_048;
const MAX_DESCRIPTION_BYTES: usize = 512;
const MAX_MIME_TYPE_BYTES: usize = 128;
const MAX_TOOL_NAME_BYTES: usize = 128;
const MAX_SERVICE_NAME_BYTES: usize = 32;
const MAX_TAG_BYTES: usize = 32;
const MAX_TAGS: usize = 5;
const MAX_ICON_URL_BYTES: usize = 2_048;
const MAX_AMOUNT_BYTES: usize = 64;
const MAX_EXTENSION_FILTER_BYTES: usize = 64;
const MAX_LIST_OFFSET: usize = 1_000_000;
const MAX_SEARCH_QUERY_BYTES: usize = 256;
const MAX_SEARCH_QUERY_TERMS: usize = 16;
const MAX_SEARCH_CURSOR_BYTES: usize = 64;
const BAZAAR_EXTENSION: &str = "bazaar";
const SEARCH_CURSOR_VERSION: &str = "v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum BazaarHttpMethod {
    Get,
    Head,
    Delete,
    Post,
    Put,
    Patch,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum BazaarResourceInput {
    Http {
        method: BazaarHttpMethod,
    },
    Mcp {
        #[serde(rename = "toolName")]
        tool_name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarServiceMetadataInput {
    #[serde(default)]
    pub service_name: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarResourceDescriptor {
    pub url: String,
    pub description: String,
    pub mime_type: String,
    #[serde(default)]
    pub service_name: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarPaymentSummary {
    pub scheme: String,
    pub network: String,
    pub amount: String,
    pub asset: String,
    pub pay_to: String,
    pub max_timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarCatalogCandidate {
    pub schema_version: u32,
    pub x402_version: u32,
    pub resource: BazaarResourceDescriptor,
    pub input: BazaarResourceInput,
    #[serde(default)]
    pub route_template: Option<String>,
    pub payment: BazaarPaymentSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarServiceMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BazaarResourceType {
    Http,
    Mcp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(transparent)]
pub struct BazaarCatalogKey(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarCatalogResource {
    pub key: BazaarCatalogKey,
    pub x402_version: u32,
    pub resource_url: String,
    pub resource_type: BazaarResourceType,
    pub input: BazaarResourceInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_template: Option<String>,
    pub description: String,
    pub mime_type: String,
    pub service_metadata: BazaarServiceMetadata,
    pub payment: BazaarPaymentSummary,
    pub last_updated: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarListQuery {
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
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarListItem {
    pub resource: String,
    #[serde(rename = "type")]
    pub resource_type: BazaarResourceType,
    pub x402_version: u32,
    pub accepts: Vec<BazaarPaymentSummary>,
    pub last_updated: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarOffsetPagination {
    pub limit: usize,
    pub offset: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarListResponse {
    pub x402_version: u32,
    pub items: Vec<BazaarListItem>,
    pub pagination: BazaarOffsetPagination,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarSearchQuery {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarCursorPagination {
    pub limit: usize,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BazaarSearchResponse {
    pub x402_version: u32,
    pub resources: Vec<BazaarListItem>,
    pub partial_results: bool,
    pub pagination: BazaarCursorPagination,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BazaarCatalogError {
    UnsupportedSchemaVersion,
    UnsupportedX402Version,
    InvalidResourceUrl,
    InvalidDescription,
    InvalidMimeType,
    InvalidToolName,
    UnexpectedMcpRouteTemplate,
    InvalidPaymentSummary,
    InvalidTimestamp,
    InvalidListLimit,
    InvalidListOffset,
    InvalidListFilter,
    InvalidSearchQuery,
    InvalidSearchLimit,
    InvalidSearchCursor,
    InvalidSearchFilter,
    DuplicateResource(BazaarCatalogKey),
}

impl BazaarCatalogError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedSchemaVersion => "unsupported_schema_version",
            Self::UnsupportedX402Version => "unsupported_x402_version",
            Self::InvalidResourceUrl => "invalid_resource_url",
            Self::InvalidDescription => "invalid_description",
            Self::InvalidMimeType => "invalid_mime_type",
            Self::InvalidToolName => "invalid_tool_name",
            Self::UnexpectedMcpRouteTemplate => "unexpected_mcp_route_template",
            Self::InvalidPaymentSummary => "invalid_payment_summary",
            Self::InvalidTimestamp => "invalid_timestamp",
            Self::InvalidListLimit => "invalid_list_limit",
            Self::InvalidListOffset => "invalid_list_offset",
            Self::InvalidListFilter => "invalid_list_filter",
            Self::InvalidSearchQuery => "invalid_search_query",
            Self::InvalidSearchLimit => "invalid_search_limit",
            Self::InvalidSearchCursor => "invalid_search_cursor",
            Self::InvalidSearchFilter => "invalid_search_filter",
            Self::DuplicateResource(_) => "duplicate_resource",
        }
    }
}

impl fmt::Display for BazaarCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateResource(key) => {
                write!(formatter, "duplicate Bazaar catalog resource: {}", key.0)
            }
            other => formatter.write_str(other.code()),
        }
    }
}

impl std::error::Error for BazaarCatalogError {}

#[derive(Debug, Default)]
pub struct BazaarCatalog {
    entries: BTreeMap<BazaarCatalogKey, BazaarCatalogResource>,
}

impl BazaarCatalog {
    pub fn insert(
        &mut self,
        candidate: BazaarCatalogCandidate,
        observed_at_unix: u64,
    ) -> Result<&BazaarCatalogResource, BazaarCatalogError> {
        let resource = normalize_candidate(candidate, observed_at_unix)?;
        match self.entries.entry(resource.key.clone()) {
            Entry::Vacant(entry) => Ok(entry.insert(resource)),
            Entry::Occupied(entry) => {
                Err(BazaarCatalogError::DuplicateResource(entry.key().clone()))
            }
        }
    }

    pub fn get(&self, key: &BazaarCatalogKey) -> Option<&BazaarCatalogResource> {
        self.entries.get(key)
    }

    pub fn list(&self, query: BazaarListQuery) -> Result<BazaarListResponse, BazaarCatalogError> {
        let normalized = normalize_list_query(query)?;
        let mut total = 0;
        let mut items = Vec::with_capacity(normalized.limit.min(self.entries.len()));
        for resource in self
            .entries
            .values()
            .filter(|resource| normalized.filters.matches(resource))
        {
            if total >= normalized.offset && items.len() < normalized.limit {
                items.push(BazaarListItem::from(resource));
            }
            total += 1;
        }

        Ok(BazaarListResponse {
            x402_version: X402_VERSION,
            items,
            pagination: BazaarOffsetPagination {
                limit: normalized.limit,
                offset: normalized.offset,
                total,
            },
        })
    }

    pub fn search(
        &self,
        query: BazaarSearchQuery,
    ) -> Result<BazaarSearchResponse, BazaarCatalogError> {
        let normalized = normalize_search_query(query)?;
        let mut ranked = self
            .entries
            .values()
            .filter(|resource| normalized.filters.matches(resource))
            .filter_map(|resource| {
                let score = search_score(resource, &normalized.terms);
                (score > 0).then_some((score, resource))
            })
            .collect::<Vec<_>>();
        ranked.sort_unstable_by(|(left_score, left), (right_score, right)| {
            right_score
                .cmp(left_score)
                .then_with(|| left.key.cmp(&right.key))
        });

        let resources = ranked
            .iter()
            .skip(normalized.offset)
            .take(normalized.limit)
            .map(|(_, resource)| BazaarListItem::from(*resource))
            .collect::<Vec<_>>();
        let page_size = resources.len();
        let next_offset = normalized.offset.saturating_add(resources.len());
        let partial_results = next_offset < ranked.len();
        let cursor =
            partial_results.then(|| encode_search_cursor(normalized.fingerprint, next_offset));

        Ok(BazaarSearchResponse {
            x402_version: X402_VERSION,
            resources,
            partial_results,
            pagination: BazaarCursorPagination {
                limit: page_size,
                cursor,
            },
        })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug)]
struct NormalizedListQuery {
    filters: NormalizedFilters,
    limit: usize,
    offset: usize,
}

#[derive(Debug)]
struct NormalizedFilters {
    resource_type: Option<BazaarResourceType>,
    pay_to: Option<String>,
    scheme: Option<String>,
    network: Option<String>,
    extensions: Option<String>,
}

impl NormalizedFilters {
    fn matches(&self, resource: &BazaarCatalogResource) -> bool {
        self.resource_type
            .is_none_or(|value| value == resource.resource_type)
            && self
                .pay_to
                .as_deref()
                .is_none_or(|value| value == resource.payment.pay_to)
            && self
                .scheme
                .as_deref()
                .is_none_or(|value| value == resource.payment.scheme)
            && self
                .network
                .as_deref()
                .is_none_or(|value| value == resource.payment.network)
            && self
                .extensions
                .as_deref()
                .is_none_or(|value| value == BAZAAR_EXTENSION)
    }

    fn fingerprint_fragment(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.resource_type.map(resource_type_name).unwrap_or(""),
            self.pay_to.as_deref().unwrap_or(""),
            self.scheme.as_deref().unwrap_or(""),
            self.network.as_deref().unwrap_or(""),
            self.extensions.as_deref().unwrap_or("")
        )
    }
}

#[derive(Debug)]
struct NormalizedSearchQuery {
    terms: Vec<String>,
    filters: NormalizedFilters,
    limit: usize,
    offset: usize,
    fingerprint: u64,
}

impl From<&BazaarCatalogResource> for BazaarListItem {
    fn from(resource: &BazaarCatalogResource) -> Self {
        Self {
            resource: resource.resource_url.clone(),
            resource_type: resource.resource_type,
            x402_version: resource.x402_version,
            accepts: vec![resource.payment.clone()],
            last_updated: resource.last_updated,
        }
    }
}

fn normalize_list_query(query: BazaarListQuery) -> Result<NormalizedListQuery, BazaarCatalogError> {
    let limit = query.limit.unwrap_or(BAZAAR_LIST_DEFAULT_LIMIT);
    if !(1..=BAZAAR_LIST_MAX_LIMIT).contains(&limit) {
        return Err(BazaarCatalogError::InvalidListLimit);
    }
    let offset = query.offset.unwrap_or(0);
    if offset > MAX_LIST_OFFSET {
        return Err(BazaarCatalogError::InvalidListOffset);
    }

    let filters = normalize_filters(
        query.resource_type,
        query.pay_to,
        query.scheme,
        query.network,
        query.extensions,
        BazaarCatalogError::InvalidListFilter,
    )?;

    Ok(NormalizedListQuery {
        filters,
        limit,
        offset,
    })
}

fn normalize_search_query(
    query: BazaarSearchQuery,
) -> Result<NormalizedSearchQuery, BazaarCatalogError> {
    if query.query.trim().is_empty()
        || query.query.len() > MAX_SEARCH_QUERY_BYTES
        || query.query.chars().any(char::is_control)
    {
        return Err(BazaarCatalogError::InvalidSearchQuery);
    }
    let terms = normalized_terms(&query.query);
    if terms.is_empty() || terms.len() > MAX_SEARCH_QUERY_TERMS {
        return Err(BazaarCatalogError::InvalidSearchQuery);
    }
    let limit = query.limit.unwrap_or(BAZAAR_SEARCH_DEFAULT_LIMIT);
    if !(1..=BAZAAR_SEARCH_MAX_LIMIT).contains(&limit) {
        return Err(BazaarCatalogError::InvalidSearchLimit);
    }
    let filters = normalize_filters(
        query.resource_type,
        query.pay_to,
        query.scheme,
        query.network,
        query.extensions,
        BazaarCatalogError::InvalidSearchFilter,
    )?;
    let fingerprint = search_fingerprint(&terms, &filters);
    let offset = query
        .cursor
        .as_deref()
        .map_or(Ok(0), |cursor| decode_search_cursor(cursor, fingerprint))?;

    Ok(NormalizedSearchQuery {
        terms,
        filters,
        limit,
        offset,
        fingerprint,
    })
}

fn normalize_filters(
    resource_type: Option<BazaarResourceType>,
    pay_to: Option<String>,
    scheme: Option<String>,
    network: Option<String>,
    extensions: Option<String>,
    invalid_error: BazaarCatalogError,
) -> Result<NormalizedFilters, BazaarCatalogError> {
    if pay_to
        .as_deref()
        .is_some_and(|value| !is_stellar_strkey(value, &['G', 'C', 'M']))
        || scheme
            .as_deref()
            .is_some_and(|value| !matches!(value, "exact" | "upto"))
        || network
            .as_deref()
            .is_some_and(|value| !matches!(value, "stellar:testnet" | "stellar:pubnet"))
        || extensions
            .as_deref()
            .is_some_and(|value| !is_extension_identifier(value))
    {
        return Err(invalid_error);
    }

    Ok(NormalizedFilters {
        resource_type,
        pay_to,
        scheme,
        network,
        extensions,
    })
}

fn search_score(resource: &BazaarCatalogResource, query_terms: &[String]) -> u32 {
    let tags = normalized_terms(&resource.service_metadata.tags.join(" "));
    let service_name = resource
        .service_metadata
        .service_name
        .as_deref()
        .map(normalized_terms)
        .unwrap_or_default();
    let tool_name = match &resource.input {
        BazaarResourceInput::Mcp { tool_name } => normalized_terms(tool_name),
        BazaarResourceInput::Http { .. } => Vec::new(),
    };
    let description = normalized_terms(&resource.description);
    let location = normalized_terms(&format!(
        "{} {}",
        resource.resource_url,
        resource.route_template.as_deref().unwrap_or("")
    ));
    let mime_type = normalized_terms(&resource.mime_type);

    let mut matched_terms = 0;
    let mut field_weight = 0;
    for term in query_terms {
        let weight = if tags.binary_search(term).is_ok() {
            10
        } else if tool_name.binary_search(term).is_ok() {
            9
        } else if service_name.binary_search(term).is_ok() {
            8
        } else if description.binary_search(term).is_ok() {
            6
        } else if location.binary_search(term).is_ok() {
            4
        } else if mime_type.binary_search(term).is_ok() {
            2
        } else {
            0
        };
        if weight > 0 {
            matched_terms += 1;
            field_weight += weight;
        }
    }
    matched_terms * 1_000 + field_weight
}

fn normalized_terms(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(str::to_lowercase)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn search_fingerprint(terms: &[String], filters: &NormalizedFilters) -> u64 {
    let canonical = format!(
        "{}|{}",
        terms.join("\u{1f}"),
        filters.fingerprint_fragment()
    );
    canonical.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn encode_search_cursor(fingerprint: u64, offset: usize) -> String {
    format!("{SEARCH_CURSOR_VERSION}:{fingerprint:016x}:{offset}")
}

fn decode_search_cursor(cursor: &str, fingerprint: u64) -> Result<usize, BazaarCatalogError> {
    if cursor.is_empty()
        || cursor.len() > MAX_SEARCH_CURSOR_BYTES
        || !cursor.is_ascii()
        || cursor.chars().any(char::is_control)
    {
        return Err(BazaarCatalogError::InvalidSearchCursor);
    }
    let mut parts = cursor.split(':');
    let version = parts.next();
    let cursor_fingerprint = parts.next();
    let offset = parts.next();
    if version != Some(SEARCH_CURSOR_VERSION) || parts.next().is_some() {
        return Err(BazaarCatalogError::InvalidSearchCursor);
    }
    let Some(cursor_fingerprint) = cursor_fingerprint else {
        return Err(BazaarCatalogError::InvalidSearchCursor);
    };
    if cursor_fingerprint.len() != 16
        || !cursor_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        || u64::from_str_radix(cursor_fingerprint, 16).ok() != Some(fingerprint)
    {
        return Err(BazaarCatalogError::InvalidSearchCursor);
    }
    let Some(offset) = offset else {
        return Err(BazaarCatalogError::InvalidSearchCursor);
    };
    if offset.is_empty() || !offset.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(BazaarCatalogError::InvalidSearchCursor);
    }
    offset
        .parse::<usize>()
        .ok()
        .filter(|value| *value <= MAX_LIST_OFFSET)
        .ok_or(BazaarCatalogError::InvalidSearchCursor)
}

const fn resource_type_name(value: BazaarResourceType) -> &'static str {
    match value {
        BazaarResourceType::Http => "http",
        BazaarResourceType::Mcp => "mcp",
    }
}

pub fn is_valid_route_template(value: &str) -> bool {
    if value.is_empty() || !value.starts_with('/') {
        return false;
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(byte, b'_' | b'/' | b':' | b'.' | b'-' | b'~' | b'%')
    }) {
        return false;
    }
    let Some(decoded) = strict_percent_decode(value) else {
        return false;
    };
    !decoded.contains("..") && !decoded.contains("://")
}

pub fn sanitize_service_metadata(input: BazaarServiceMetadataInput) -> BazaarServiceMetadata {
    let service_name = input
        .service_name
        .filter(|value| is_printable_ascii(value, MAX_SERVICE_NAME_BYTES));

    let mut seen = BTreeSet::new();
    let mut tags = Vec::new();
    for tag in input.tags {
        if !is_printable_ascii(&tag, MAX_TAG_BYTES) {
            continue;
        }
        let folded = tag.to_ascii_lowercase();
        if seen.insert(folded) {
            tags.push(tag);
            if tags.len() == MAX_TAGS {
                break;
            }
        }
    }

    let icon_url = input.icon_url.filter(|value| is_valid_icon_url(value));
    BazaarServiceMetadata {
        service_name,
        tags,
        icon_url,
    }
}

fn normalize_candidate(
    candidate: BazaarCatalogCandidate,
    observed_at_unix: u64,
) -> Result<BazaarCatalogResource, BazaarCatalogError> {
    if candidate.schema_version != BAZAAR_CATALOG_SCHEMA_VERSION {
        return Err(BazaarCatalogError::UnsupportedSchemaVersion);
    }
    if candidate.x402_version != X402_VERSION {
        return Err(BazaarCatalogError::UnsupportedX402Version);
    }
    if observed_at_unix == 0 {
        return Err(BazaarCatalogError::InvalidTimestamp);
    }

    let parsed_url = parse_resource_url(&candidate.resource.url)?;
    validate_required_text(
        &candidate.resource.description,
        MAX_DESCRIPTION_BYTES,
        BazaarCatalogError::InvalidDescription,
    )?;
    validate_required_ascii(
        &candidate.resource.mime_type,
        MAX_MIME_TYPE_BYTES,
        BazaarCatalogError::InvalidMimeType,
    )?;
    validate_payment_summary(&candidate.payment)?;

    let (resource_type, route_template, key) = match &candidate.input {
        BazaarResourceInput::Http { .. } => {
            let route_template = candidate
                .route_template
                .filter(|value| is_valid_route_template(value));
            let path = route_template
                .as_deref()
                .unwrap_or_else(|| parsed_url.path());
            let key = BazaarCatalogKey(format!(
                "http:{}{}",
                parsed_url.origin().ascii_serialization(),
                path
            ));
            (BazaarResourceType::Http, route_template, key)
        }
        BazaarResourceInput::Mcp { tool_name } => {
            if candidate.route_template.is_some() {
                return Err(BazaarCatalogError::UnexpectedMcpRouteTemplate);
            }
            validate_required_ascii(
                tool_name,
                MAX_TOOL_NAME_BYTES,
                BazaarCatalogError::InvalidToolName,
            )?;
            let key = BazaarCatalogKey(format!("mcp:{}#{tool_name}", parsed_url.as_str()));
            (BazaarResourceType::Mcp, None, key)
        }
    };

    Ok(BazaarCatalogResource {
        key,
        x402_version: candidate.x402_version,
        resource_url: candidate.resource.url,
        resource_type,
        input: candidate.input,
        route_template,
        description: candidate.resource.description,
        mime_type: candidate.resource.mime_type,
        service_metadata: sanitize_service_metadata(BazaarServiceMetadataInput {
            service_name: candidate.resource.service_name,
            tags: candidate.resource.tags,
            icon_url: candidate.resource.icon_url,
        }),
        payment: candidate.payment,
        last_updated: observed_at_unix,
    })
}

fn parse_resource_url(value: &str) -> Result<reqwest::Url, BazaarCatalogError> {
    if value.is_empty()
        || value.len() > MAX_RESOURCE_URL_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(BazaarCatalogError::InvalidResourceUrl);
    }
    let parsed = reqwest::Url::parse(value).map_err(|_| BazaarCatalogError::InvalidResourceUrl)?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err(BazaarCatalogError::InvalidResourceUrl);
    }
    Ok(parsed)
}

fn validate_payment_summary(payment: &BazaarPaymentSummary) -> Result<(), BazaarCatalogError> {
    if !matches!(
        payment.network.as_str(),
        "stellar:testnet" | "stellar:pubnet"
    ) || !matches!(payment.scheme.as_str(), "exact" | "upto")
        || payment.amount.is_empty()
        || payment.amount.len() > MAX_AMOUNT_BYTES
        || payment.amount.bytes().any(|byte| !byte.is_ascii_digit())
        || payment.amount.bytes().all(|byte| byte == b'0')
        || !is_stellar_strkey(&payment.asset, &['C'])
        || !is_stellar_strkey(&payment.pay_to, &['G', 'C', 'M'])
        || payment.max_timeout_seconds == 0
    {
        return Err(BazaarCatalogError::InvalidPaymentSummary);
    }
    Ok(())
}

fn is_extension_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_EXTENSION_FILTER_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn is_stellar_strkey(value: &str, prefixes: &[char]) -> bool {
    value.len() == 56
        && value
            .chars()
            .next()
            .is_some_and(|first| prefixes.contains(&first))
        && value
            .chars()
            .all(|character| matches!(character, 'A'..='Z' | '2'..='7'))
}

fn validate_required_text(
    value: &str,
    max_bytes: usize,
    error: BazaarCatalogError,
) -> Result<(), BazaarCatalogError> {
    if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(error);
    }
    Ok(())
}

fn validate_required_ascii(
    value: &str,
    max_bytes: usize,
    error: BazaarCatalogError,
) -> Result<(), BazaarCatalogError> {
    if !is_printable_ascii(value, max_bytes) {
        return Err(error);
    }
    Ok(())
}

fn is_printable_ascii(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
}

fn is_valid_icon_url(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_ICON_URL_BYTES || value.chars().any(char::is_control) {
        return false;
    }
    let Ok(parsed) = reqwest::Url::parse(value) else {
        return false;
    };
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if is_forbidden_icon_host(&host) {
        return false;
    }

    let Some(raw_host) = raw_url_host(value) else {
        return false;
    };
    let Some(decoded_host) = strict_percent_decode(raw_host) else {
        return false;
    };
    !is_forbidden_icon_host(decoded_host.trim_end_matches('.'))
}

fn is_forbidden_icon_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host.parse::<IpAddr>().is_ok()
        || matches!(
            host.as_str(),
            "localhost" | "localhost.localdomain" | "ip6-localhost" | "ip6-loopback"
        )
        || host.bytes().all(|byte| byte.is_ascii_digit())
        || host.strip_prefix("0x").is_some_and(|rest| {
            !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

fn raw_url_host(value: &str) -> Option<&str> {
    let authority_start = value.find("://")? + 3;
    let authority = &value[authority_start..];
    let authority_end = authority.find(['/', '?', '#']).unwrap_or(authority.len());
    let authority = &authority[..authority_end];
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    if authority.starts_with('[') {
        let end = authority.find(']')?;
        return Some(&authority[1..end]);
    }
    match authority.rsplit_once(':') {
        Some((host, port))
            if !host.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            Some(host)
        }
        _ => Some(authority),
    }
}

fn strict_percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return None;
            }
            let high = hex_value(bytes[index + 1])?;
            let low = hex_value(bytes[index + 2])?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
