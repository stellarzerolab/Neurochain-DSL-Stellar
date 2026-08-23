import { SERVICE_HANDLER_AUTHORITY_BOUNDARY } from "./service-handlers.js";

export const BAZAAR_DISCOVERY_PARITY_SCHEMA_VERSION = 1 as const;

type DiscoveryOperation = "resources" | "search";
type DiscoveryStatus = "completed" | "rejected" | "unavailable";

export type BazaarResourceType = "http" | "mcp";

export interface BazaarPaymentSummary {
  readonly scheme: string;
  readonly network: string;
  readonly amount: string;
  readonly asset: string;
  readonly payTo: string;
  readonly maxTimeoutSeconds: number;
}

export interface BazaarListQuery {
  readonly type?: BazaarResourceType;
  readonly payTo?: string;
  readonly scheme?: string;
  readonly network?: string;
  readonly extensions?: string;
  readonly limit?: number;
  readonly offset?: number;
}

export interface BazaarSearchQuery {
  readonly query: string;
  readonly type?: BazaarResourceType;
  readonly payTo?: string;
  readonly scheme?: string;
  readonly network?: string;
  readonly extensions?: string;
  readonly limit?: number;
  readonly cursor?: string;
}

export interface BazaarListItem {
  readonly resource: string;
  readonly type: BazaarResourceType;
  readonly x402Version: 2;
  readonly accepts: readonly [BazaarPaymentSummary];
  readonly lastUpdated: number;
}

export interface BazaarListResponse {
  readonly x402Version: 2;
  readonly items: readonly BazaarListItem[];
  readonly pagination: Readonly<{
    limit: number;
    offset: number;
    total: number;
  }>;
}

export interface BazaarSearchResponse {
  readonly x402Version: 2;
  readonly resources: readonly BazaarListItem[];
  readonly partialResults: boolean;
  readonly pagination: Readonly<{
    limit: number;
    cursor: string | null;
  }>;
}

export type BazaarDiscoveryPortCode =
  | "invalid_list_limit"
  | "invalid_list_offset"
  | "invalid_list_filter"
  | "invalid_search_query"
  | "invalid_search_limit"
  | "invalid_search_cursor"
  | "invalid_search_filter"
  | "catalog_unavailable";

export type BazaarDiscoveryPortResult =
  | Readonly<{ status: "completed"; response: unknown }>
  | Readonly<{
      status: "rejected" | "unavailable";
      code: BazaarDiscoveryPortCode;
      reason: string;
    }>;

export interface BazaarDiscoveryPort {
  list(query: BazaarListQuery): Promise<unknown>;
  search(query: BazaarSearchQuery): Promise<unknown>;
}

export interface BazaarDiscoveryResult<T> {
  readonly schemaVersion: typeof BAZAAR_DISCOVERY_PARITY_SCHEMA_VERSION;
  readonly operation: DiscoveryOperation;
  readonly status: DiscoveryStatus;
  readonly code: string;
  readonly reason: string;
  readonly response: T | null;
  readonly authorityBoundary: typeof SERVICE_HANDLER_AUTHORITY_BOUNDARY;
}

interface Parsed<T> {
  readonly ok: true;
  readonly value: T;
}

interface ParseFailure {
  readonly ok: false;
  readonly code: string;
  readonly reason: string;
}

type ParseResult<T> = Parsed<T> | ParseFailure;

const LIST_KEYS = Object.freeze([
  "extensions",
  "limit",
  "network",
  "offset",
  "payTo",
  "scheme",
  "type",
]);
const SEARCH_KEYS = Object.freeze([
  "cursor",
  "extensions",
  "limit",
  "network",
  "payTo",
  "query",
  "scheme",
  "type",
]);
const FILTER_KEYS = Object.freeze([
  "extensions",
  "network",
  "payTo",
  "scheme",
  "type",
]);
const PAYMENT_KEYS = Object.freeze([
  "amount",
  "asset",
  "maxTimeoutSeconds",
  "network",
  "payTo",
  "scheme",
]);
const LIST_ITEM_KEYS = Object.freeze([
  "accepts",
  "lastUpdated",
  "resource",
  "type",
  "x402Version",
]);
const LIST_RESPONSE_KEYS = Object.freeze([
  "items",
  "pagination",
  "x402Version",
]);
const LIST_PAGINATION_KEYS = Object.freeze(["limit", "offset", "total"]);
const SEARCH_RESPONSE_KEYS = Object.freeze([
  "pagination",
  "partialResults",
  "resources",
  "x402Version",
]);
const SEARCH_PAGINATION_KEYS = Object.freeze(["cursor", "limit"]);
const PORT_COMPLETED_KEYS = Object.freeze(["response", "status"]);
const PORT_FAILURE_KEYS = Object.freeze(["code", "reason", "status"]);
const LIST_PORT_CODES = Object.freeze([
  "invalid_list_limit",
  "invalid_list_offset",
  "invalid_list_filter",
  "catalog_unavailable",
] as const);
const SEARCH_PORT_CODES = Object.freeze([
  "invalid_search_query",
  "invalid_search_limit",
  "invalid_search_cursor",
  "invalid_search_filter",
  "catalog_unavailable",
] as const);

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function hasExactKeys(
  value: Record<string, unknown>,
  expected: readonly string[],
): boolean {
  const actual = Object.keys(value).sort();
  return (
    actual.length === expected.length &&
    actual.every((key, index) => key === expected[index])
  );
}

function hasOnlyKeys(
  value: Record<string, unknown>,
  allowed: readonly string[],
): boolean {
  return Object.keys(value).every((key) => allowed.includes(key));
}

function isNonEmptyBoundedString(value: unknown, maxBytes: number): value is string {
  return (
    typeof value === "string" &&
    value.trim().length > 0 &&
    Buffer.byteLength(value, "utf8") <= maxBytes
  );
}

function hasOwnValue(value: Record<string, unknown>, key: string): boolean {
  return Object.hasOwn(value, key) && value[key] !== undefined;
}

function parseFilters(
  input: Record<string, unknown>,
): ParseResult<Pick<BazaarListQuery, "type" | "payTo" | "scheme" | "network" | "extensions">> {
  for (const key of FILTER_KEYS) {
    if (Object.hasOwn(input, key) && input[key] === undefined) {
      return { ok: false, code: "invalid_filter", reason: `${key} must not be undefined` };
    }
  }
  if (
    (hasOwnValue(input, "type") &&
      input.type !== "http" &&
      input.type !== "mcp") ||
    (hasOwnValue(input, "payTo") &&
      (typeof input.payTo !== "string" ||
        !/^[GCM][A-Z2-7]{55}$/.test(input.payTo))) ||
    (hasOwnValue(input, "scheme") &&
      input.scheme !== "exact" &&
      input.scheme !== "upto") ||
    (hasOwnValue(input, "network") &&
      input.network !== "stellar:testnet" &&
      input.network !== "stellar:pubnet") ||
    (hasOwnValue(input, "extensions") &&
      (typeof input.extensions !== "string" ||
        !/^[A-Za-z0-9._-]{1,64}$/.test(input.extensions)))
  ) {
    return {
      ok: false,
      code: "invalid_filter",
      reason: "discovery filters failed the strict Rust-compatible profile",
    };
  }
  return {
    ok: true,
    value: Object.freeze({
      ...(input.type === undefined ? {} : { type: input.type as BazaarResourceType }),
      ...(input.payTo === undefined ? {} : { payTo: input.payTo as string }),
      ...(input.scheme === undefined ? {} : { scheme: input.scheme as string }),
      ...(input.network === undefined ? {} : { network: input.network as string }),
      ...(input.extensions === undefined
        ? {}
        : { extensions: input.extensions as string }),
    }),
  };
}

function parseListQuery(input: unknown): ParseResult<BazaarListQuery> {
  if (!isRecord(input) || !hasOnlyKeys(input, LIST_KEYS)) {
    return {
      ok: false,
      code: "invalid_list_filter",
      reason: "resources request must use the strict query envelope",
    };
  }
  if (
    Object.hasOwn(input, "limit") &&
    (!Number.isInteger(input.limit) || Number(input.limit) < 1 || Number(input.limit) > 100)
  ) {
    return { ok: false, code: "invalid_list_limit", reason: "limit must be an integer from 1 to 100" };
  }
  if (
    Object.hasOwn(input, "offset") &&
    (!Number.isInteger(input.offset) || Number(input.offset) < 0 || Number(input.offset) > 1_000_000)
  ) {
    return { ok: false, code: "invalid_list_offset", reason: "offset must be an integer from 0 to 1000000" };
  }
  const filters = parseFilters(input);
  if (!filters.ok) {
    return { ok: false, code: "invalid_list_filter", reason: filters.reason };
  }
  return {
    ok: true,
    value: Object.freeze({
      ...filters.value,
      ...(input.limit === undefined ? {} : { limit: Number(input.limit) }),
      ...(input.offset === undefined ? {} : { offset: Number(input.offset) }),
    }),
  };
}

function normalizedTerms(value: string): readonly string[] {
  return Object.freeze(
    [...new Set(value.split(/[^\p{L}\p{N}]+/u).filter(Boolean).map((term) => term.toLowerCase()))].sort(),
  );
}

function isCursor(value: unknown): value is string {
  return (
    typeof value === "string" &&
    Buffer.byteLength(value, "utf8") <= 64 &&
    /^v1:[0-9a-f]{16}:[0-9]+$/.test(value)
  );
}

function parseSearchQuery(input: unknown): ParseResult<BazaarSearchQuery> {
  if (!isRecord(input) || !hasOnlyKeys(input, SEARCH_KEYS)) {
    return {
      ok: false,
      code: "invalid_search_filter",
      reason: "search request must use the strict query envelope",
    };
  }
  const query = input.query;
  if (
    typeof query !== "string" ||
    query.trim().length === 0 ||
    Buffer.byteLength(query, "utf8") > 256 ||
    /\p{Cc}/u.test(query)
  ) {
    return { ok: false, code: "invalid_search_query", reason: "query must be 1-256 bytes without control characters" };
  }
  const terms = normalizedTerms(query);
  if (terms.length < 1 || terms.length > 16) {
    return { ok: false, code: "invalid_search_query", reason: "query must produce 1-16 unique alphanumeric terms" };
  }
  if (
    Object.hasOwn(input, "limit") &&
    (!Number.isInteger(input.limit) || Number(input.limit) < 1 || Number(input.limit) > 100)
  ) {
    return { ok: false, code: "invalid_search_limit", reason: "limit must be an integer from 1 to 100" };
  }
  if (Object.hasOwn(input, "cursor") && !isCursor(input.cursor)) {
    return { ok: false, code: "invalid_search_cursor", reason: "cursor must use the bounded v1 envelope" };
  }
  const filters = parseFilters(input);
  if (!filters.ok) {
    return { ok: false, code: "invalid_search_filter", reason: filters.reason };
  }
  return {
    ok: true,
    value: Object.freeze({
      query,
      ...filters.value,
      ...(input.limit === undefined ? {} : { limit: Number(input.limit) }),
      ...(input.cursor === undefined ? {} : { cursor: input.cursor as string }),
    }),
  };
}

function parsePayment(input: unknown): BazaarPaymentSummary | null {
  if (
    !isRecord(input) ||
    !hasExactKeys(input, PAYMENT_KEYS) ||
    (input.scheme !== "exact" && input.scheme !== "upto") ||
    (input.network !== "stellar:testnet" && input.network !== "stellar:pubnet") ||
    typeof input.amount !== "string" ||
    !/^[1-9][0-9]{0,63}$/.test(input.amount) ||
    typeof input.asset !== "string" ||
    !/^C[A-Z2-7]{55}$/.test(input.asset) ||
    typeof input.payTo !== "string" ||
    !/^[GCM][A-Z2-7]{55}$/.test(input.payTo) ||
    !Number.isSafeInteger(input.maxTimeoutSeconds) ||
    Number(input.maxTimeoutSeconds) < 1
  ) {
    return null;
  }
  return Object.freeze({
    scheme: input.scheme,
    network: input.network,
    amount: input.amount,
    asset: input.asset,
    payTo: input.payTo,
    maxTimeoutSeconds: Number(input.maxTimeoutSeconds),
  });
}

function parseListItem(input: unknown): BazaarListItem | null {
  if (
    !isRecord(input) ||
    !hasExactKeys(input, LIST_ITEM_KEYS) ||
    !isNonEmptyBoundedString(input.resource, 2_048) ||
    (input.type !== "http" && input.type !== "mcp") ||
    input.x402Version !== 2 ||
    !Array.isArray(input.accepts) ||
    input.accepts.length !== 1 ||
    !Number.isSafeInteger(input.lastUpdated) ||
    Number(input.lastUpdated) < 1
  ) {
    return null;
  }
  const payment = parsePayment(input.accepts[0]);
  if (payment === null) {
    return null;
  }
  return Object.freeze({
    resource: input.resource,
    type: input.type,
    x402Version: 2,
    accepts: Object.freeze([payment]) as readonly [BazaarPaymentSummary],
    lastUpdated: Number(input.lastUpdated),
  });
}

function parseItems(input: unknown, maxItems: number): readonly BazaarListItem[] | null {
  if (!Array.isArray(input) || input.length > maxItems) {
    return null;
  }
  const items: BazaarListItem[] = [];
  for (const rawItem of input) {
    const item = parseListItem(rawItem);
    if (item === null) {
      return null;
    }
    items.push(item);
  }
  return Object.freeze(items);
}

function parseListResponse(input: unknown): BazaarListResponse | null {
  if (
    !isRecord(input) ||
    !hasExactKeys(input, LIST_RESPONSE_KEYS) ||
    input.x402Version !== 2 ||
    !isRecord(input.pagination) ||
    !hasExactKeys(input.pagination, LIST_PAGINATION_KEYS) ||
    !Number.isInteger(input.pagination.limit) ||
    Number(input.pagination.limit) < 1 ||
    Number(input.pagination.limit) > 100 ||
    !Number.isInteger(input.pagination.offset) ||
    Number(input.pagination.offset) < 0 ||
    Number(input.pagination.offset) > 1_000_000 ||
    !Number.isInteger(input.pagination.total) ||
    Number(input.pagination.total) < 0
  ) {
    return null;
  }
  const limit = Number(input.pagination.limit);
  const offset = Number(input.pagination.offset);
  const total = Number(input.pagination.total);
  const available = Math.max(total - Math.min(offset, total), 0);
  const items = parseItems(input.items, limit);
  if (items === null || items.length > Math.min(limit, available)) {
    return null;
  }
  return Object.freeze({
    x402Version: 2,
    items,
    pagination: Object.freeze({
      limit,
      offset,
      total,
    }),
  });
}

function parseSearchResponse(input: unknown): BazaarSearchResponse | null {
  if (
    !isRecord(input) ||
    !hasExactKeys(input, SEARCH_RESPONSE_KEYS) ||
    input.x402Version !== 2 ||
    typeof input.partialResults !== "boolean" ||
    !isRecord(input.pagination) ||
    !hasExactKeys(input.pagination, SEARCH_PAGINATION_KEYS) ||
    !Number.isInteger(input.pagination.limit) ||
    Number(input.pagination.limit) < 0 ||
    Number(input.pagination.limit) > 100 ||
    (input.pagination.cursor !== null && !isCursor(input.pagination.cursor))
  ) {
    return null;
  }
  const resources = parseItems(input.resources, 100);
  if (
    resources === null ||
    resources.length !== Number(input.pagination.limit) ||
    (input.partialResults && input.pagination.cursor === null) ||
    (!input.partialResults && input.pagination.cursor !== null)
  ) {
    return null;
  }
  return Object.freeze({
    x402Version: 2,
    resources,
    partialResults: input.partialResults,
    pagination: Object.freeze({
      limit: Number(input.pagination.limit),
      cursor: input.pagination.cursor as string | null,
    }),
  });
}

function parsePortResult(
  input: unknown,
  operation: DiscoveryOperation,
): BazaarDiscoveryPortResult | null {
  if (!isRecord(input) || typeof input.status !== "string") {
    return null;
  }
  if (input.status === "completed") {
    return hasExactKeys(input, PORT_COMPLETED_KEYS)
      ? { status: "completed", response: input.response }
      : null;
  }
  if (
    (input.status !== "rejected" && input.status !== "unavailable") ||
    !hasExactKeys(input, PORT_FAILURE_KEYS) ||
    !isNonEmptyBoundedString(input.reason, 512) ||
    typeof input.code !== "string"
  ) {
    return null;
  }
  const allowed = operation === "resources" ? LIST_PORT_CODES : SEARCH_PORT_CODES;
  if (!allowed.some((code) => code === input.code)) {
    return null;
  }
  if ((input.code === "catalog_unavailable") !== (input.status === "unavailable")) {
    return null;
  }
  return {
    status: input.status,
    code: input.code as BazaarDiscoveryPortCode,
    reason: input.reason,
  };
}

function result<T>(
  operation: DiscoveryOperation,
  status: DiscoveryStatus,
  code: string,
  reason: string,
  response: T | null,
): BazaarDiscoveryResult<T> {
  return Object.freeze({
    schemaVersion: BAZAAR_DISCOVERY_PARITY_SCHEMA_VERSION,
    operation,
    status,
    code,
    reason,
    response,
    authorityBoundary: SERVICE_HANDLER_AUTHORITY_BOUNDARY,
  });
}

export class PureBazaarDiscoveryHandlers {
  constructor(private readonly port: BazaarDiscoveryPort) {}

  async handleResources(input: unknown): Promise<BazaarDiscoveryResult<BazaarListResponse>> {
    const query = parseListQuery(input);
    if (!query.ok) {
      return result<BazaarListResponse>("resources", "rejected", query.code, query.reason, null);
    }
    let raw: unknown;
    try {
      raw = await this.port.list(query.value);
    } catch {
      return result<BazaarListResponse>("resources", "unavailable", "resources_unavailable", "resources port failed closed", null);
    }
    const portResult = parsePortResult(raw, "resources");
    if (portResult === null) {
      return result<BazaarListResponse>("resources", "unavailable", "resources_port_invalid", "resources port returned no stable result", null);
    }
    if (portResult.status !== "completed") {
      return result<BazaarListResponse>("resources", portResult.status, portResult.code, portResult.reason, null);
    }
    const response = parseListResponse(portResult.response);
    return response === null
      ? result<BazaarListResponse>("resources", "unavailable", "resources_response_invalid", "resources response failed the Rust-compatible wire schema", null)
      : result<BazaarListResponse>("resources", "completed", "resources_completed", "resources response passed strict offline parity", response);
  }

  async handleSearch(input: unknown): Promise<BazaarDiscoveryResult<BazaarSearchResponse>> {
    const query = parseSearchQuery(input);
    if (!query.ok) {
      return result<BazaarSearchResponse>("search", "rejected", query.code, query.reason, null);
    }
    let raw: unknown;
    try {
      raw = await this.port.search(query.value);
    } catch {
      return result<BazaarSearchResponse>("search", "unavailable", "search_unavailable", "search port failed closed", null);
    }
    const portResult = parsePortResult(raw, "search");
    if (portResult === null) {
      return result<BazaarSearchResponse>("search", "unavailable", "search_port_invalid", "search port returned no stable result", null);
    }
    if (portResult.status !== "completed") {
      return result<BazaarSearchResponse>("search", portResult.status, portResult.code, portResult.reason, null);
    }
    const response = parseSearchResponse(portResult.response);
    return response === null
      ? result<BazaarSearchResponse>("search", "unavailable", "search_response_invalid", "search response failed the Rust-compatible wire schema", null)
      : result<BazaarSearchResponse>("search", "completed", "search_completed", "search response passed strict offline parity", response);
  }
}
