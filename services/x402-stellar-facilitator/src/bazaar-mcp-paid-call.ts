import {
  parseBazaarSearchResponse,
  type BazaarSearchResponse,
} from "./bazaar-resources-search.js";

export const BAZAAR_MCP_PARITY_SCHEMA_VERSION = 1 as const;
export const BAZAAR_MCP_PROTOCOL_VERSION = "2026-07-28" as const;
export const BAZAAR_MCP_SEARCH_TOOL = "search_stellar_bazaar" as const;
export const BAZAAR_MCP_PAID_CALL_TOOL = "proxy_paid_stellar_call" as const;

export const BAZAAR_MCP_RUNTIME_BOUNDARY = Object.freeze({
  listenerStarted: false,
  networkAccessGranted: false,
  credentialUseGranted: false,
  paymentVerificationGranted: false,
  paymentSettlementGranted: false,
  serviceDispatchGranted: false,
  walletSigningGranted: false,
  rpcSubmitGranted: false,
  transactionSubmitGranted: false,
  actionPlanSubmitGranted: false,
});

const SEARCH_ARGUMENT_KEYS = Object.freeze([
  "cursor",
  "extensions",
  "limit",
  "network",
  "payTo",
  "query",
  "schemaVersion",
  "scheme",
  "type",
]);
const PAID_CALL_ARGUMENT_KEYS = Object.freeze([
  "arguments",
  "requestId",
  "resourceKey",
  "schemaVersion",
]);
const TOOL_CALL_KEYS = Object.freeze(["id", "jsonrpc", "method", "params"]);
const TOOL_PARAMS_KEYS = Object.freeze(["_meta", "arguments", "name"]);
const SEARCH_RESULT_KEYS = Object.freeze([
  "authority",
  "code",
  "data",
  "ok",
  "protocolVersion",
  "reason",
  "retryable",
  "schemaVersion",
  "tool",
]);
const SEARCH_FAILURE_KEYS = Object.freeze(
  SEARCH_RESULT_KEYS.filter((key) => key !== "data"),
);
const PAID_RESULT_KEYS = SEARCH_RESULT_KEYS;
const PAID_FAILURE_KEYS = SEARCH_FAILURE_KEYS;
const SEARCH_AUTHORITY_KEYS = Object.freeze([
  "actionPlanSubmitAllowed",
  "approvalAllowed",
  "paymentAllowed",
  "proofAllowed",
  "rpcSubmitAllowed",
  "settlementAllowed",
  "shellAccessAllowed",
  "signingAllowed",
  "walletAccessAllowed",
]);
const PAID_AUTHORITY_KEYS = Object.freeze([
  "actionPlanSubmitAllowed",
  "approvalAllowed",
  "paymentAllowed",
  "proofAllowed",
  "rpcSubmitAllowed",
  "serviceCallAllowed",
  "settlementAllowed",
  "shellAccessAllowed",
  "signingAllowed",
  "underlyingExecutionAllowed",
  "walletAccessAllowed",
]);
const PAID_BINDING_KEYS = Object.freeze([
  "argumentsDigest",
  "callDigest",
  "network",
  "requestId",
  "resourceKey",
  "resourceUrl",
  "schemaVersion",
  "toolName",
]);

const SEARCH_RETRYABILITY = Object.freeze({
  search_completed: false,
  arguments_too_large: false,
  invalid_arguments: false,
  unsupported_schema_version: false,
  invalid_search_query: false,
  invalid_search_limit: false,
  invalid_search_filter: false,
  invalid_search_cursor: false,
  search_cursor_mismatch: false,
  catalog_unavailable: true,
});

const PAID_RETRYABILITY = Object.freeze({
  service_call_authorized: false,
  arguments_too_large: false,
  invalid_arguments: false,
  unsupported_schema_version: false,
  invalid_request_id: false,
  invalid_resource_key: false,
  invalid_service_arguments: false,
  catalog_unavailable: true,
  resource_not_found: false,
  resource_not_mcp: false,
  access_gate_unavailable: true,
  payment_required: true,
  settlement_pending: true,
  settlement_rejected: false,
  settlement_outcome_unknown: false,
  access_replay_blocked: false,
});

type SearchCode = keyof typeof SEARCH_RETRYABILITY;
type PaidCode = keyof typeof PAID_RETRYABILITY;

export interface BazaarMcpRustPort {
  search(argumentsValue: Readonly<Record<string, unknown>>): Promise<unknown>;
  paidCall(argumentsValue: Readonly<Record<string, unknown>>): Promise<unknown>;
}

export interface BazaarMcpCallResult {
  readonly resultType: "complete";
  readonly content: readonly Readonly<{ type: "text"; text: string }>[];
  readonly structuredContent: Readonly<Record<string, unknown>>;
  readonly isError: boolean;
}

interface ParsedToolCall {
  readonly argumentsValue: Readonly<Record<string, unknown>>;
}

interface PaidCallBinding {
  readonly schemaVersion: 1;
  readonly requestId: string;
  readonly resourceKey: string;
  readonly resourceUrl: string;
  readonly toolName: string;
  readonly network: "stellar:testnet" | "stellar:pubnet";
  readonly argumentsDigest: string;
  readonly callDigest: string;
}

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

function byteLength(value: unknown): number {
  try {
    return Buffer.byteLength(JSON.stringify(value), "utf8");
  } catch {
    return Number.POSITIVE_INFINITY;
  }
}

function isNonEmptyString(value: unknown, maxBytes: number): value is string {
  return (
    typeof value === "string" &&
    value.trim().length > 0 &&
    Buffer.byteLength(value, "utf8") <= maxBytes
  );
}

function jsonWithinBounds(
  value: unknown,
  maxDepth: number,
  maxNodes: number,
): boolean {
  let nodes = 0;
  function visit(candidate: unknown, depth: number): boolean {
    nodes += 1;
    if (depth > maxDepth || nodes > maxNodes) {
      return false;
    }
    if (Array.isArray(candidate)) {
      return candidate.every((item) => visit(item, depth + 1));
    }
    if (isRecord(candidate)) {
      return Object.values(candidate).every((item) => visit(item, depth + 1));
    }
    return true;
  }
  return visit(value, 0);
}

function parseToolCall(
  input: unknown,
  expectedTool: string,
  allowedArgumentKeys: readonly string[],
  maxArgumentBytes: number,
): ParsedToolCall | null {
  if (
    !isRecord(input) ||
    !hasExactKeys(input, TOOL_CALL_KEYS) ||
    input.jsonrpc !== "2.0" ||
    input.method !== "tools/call" ||
    !isNonEmptyString(input.id, 128) ||
    !isRecord(input.params) ||
    !hasExactKeys(input.params, TOOL_PARAMS_KEYS) ||
    input.params.name !== expectedTool ||
    !isRecord(input.params.arguments) ||
    !hasOnlyKeys(input.params.arguments, allowedArgumentKeys) ||
    byteLength(input.params.arguments) > maxArgumentBytes ||
    !isRecord(input.params._meta) ||
    byteLength(input.params._meta) > 4_096
  ) {
    return null;
  }
  return { argumentsValue: Object.freeze({ ...input.params.arguments }) };
}

function allFalseAuthority(
  value: unknown,
  expectedKeys: readonly string[],
): boolean {
  return (
    isRecord(value) &&
    hasExactKeys(value, expectedKeys) &&
    Object.values(value).every((grant) => grant === false)
  );
}

function isSearchCode(value: unknown): value is SearchCode {
  return typeof value === "string" && Object.hasOwn(SEARCH_RETRYABILITY, value);
}

function isPaidCode(value: unknown): value is PaidCode {
  return typeof value === "string" && Object.hasOwn(PAID_RETRYABILITY, value);
}

function parseSearchResult(
  input: unknown,
): Readonly<Record<string, unknown>> | null {
  if (
    !isRecord(input) ||
    typeof input.ok !== "boolean" ||
    !hasExactKeys(
      input,
      input.ok ? SEARCH_RESULT_KEYS : SEARCH_FAILURE_KEYS,
    ) ||
    input.schemaVersion !== BAZAAR_MCP_PARITY_SCHEMA_VERSION ||
    input.protocolVersion !== BAZAAR_MCP_PROTOCOL_VERSION ||
    input.tool !== BAZAAR_MCP_SEARCH_TOOL ||
    !isSearchCode(input.code) ||
    !isNonEmptyString(input.reason, 1_024) ||
    typeof input.retryable !== "boolean" ||
    input.retryable !== SEARCH_RETRYABILITY[input.code] ||
    !allFalseAuthority(input.authority, SEARCH_AUTHORITY_KEYS)
  ) {
    return null;
  }
  if (input.ok) {
    const data = parseBazaarSearchResponse(input.data);
    if (input.code !== "search_completed" || data === null) {
      return null;
    }
    return Object.freeze({ ...input, data });
  }
  return input.code === "search_completed" ? null : Object.freeze({ ...input });
}

function parsePaidBinding(
  input: unknown,
  request: Readonly<Record<string, unknown>>,
): PaidCallBinding | null {
  if (
    !isRecord(input) ||
    !hasExactKeys(input, PAID_BINDING_KEYS) ||
    input.schemaVersion !== BAZAAR_MCP_PARITY_SCHEMA_VERSION ||
    !isNonEmptyString(input.requestId, 128) ||
    !isNonEmptyString(input.resourceKey, 2_304) ||
    !isNonEmptyString(input.resourceUrl, 2_048) ||
    !isNonEmptyString(input.toolName, 256) ||
    (input.network !== "stellar:testnet" && input.network !== "stellar:pubnet") ||
    typeof input.argumentsDigest !== "string" ||
    !/^[0-9a-f]{64}$/.test(input.argumentsDigest) ||
    typeof input.callDigest !== "string" ||
    !/^[0-9a-f]{64}$/.test(input.callDigest) ||
    input.requestId !== request.requestId ||
    input.resourceKey !== request.resourceKey
  ) {
    return null;
  }
  return input as unknown as PaidCallBinding;
}

function parsePaidAuthority(value: unknown, serviceCallAllowed: boolean): boolean {
  if (!isRecord(value) || !hasExactKeys(value, PAID_AUTHORITY_KEYS)) {
    return false;
  }
  return PAID_AUTHORITY_KEYS.every((key) =>
    key === "serviceCallAllowed"
      ? value[key] === serviceCallAllowed
      : value[key] === false,
  );
}

function parsePaidResult(
  input: unknown,
  request: Readonly<Record<string, unknown>>,
): Readonly<Record<string, unknown>> | null {
  if (
    !isRecord(input) ||
    typeof input.ok !== "boolean" ||
    !hasExactKeys(input, input.ok ? PAID_RESULT_KEYS : PAID_FAILURE_KEYS) ||
    input.schemaVersion !== BAZAAR_MCP_PARITY_SCHEMA_VERSION ||
    input.protocolVersion !== BAZAAR_MCP_PROTOCOL_VERSION ||
    input.tool !== BAZAAR_MCP_PAID_CALL_TOOL ||
    !isPaidCode(input.code) ||
    !isNonEmptyString(input.reason, 1_024) ||
    typeof input.retryable !== "boolean" ||
    input.retryable !== PAID_RETRYABILITY[input.code]
  ) {
    return null;
  }
  if (input.ok) {
    const binding = parsePaidBinding(input.data, request);
    if (
      input.code !== "service_call_authorized" ||
      binding === null ||
      !parsePaidAuthority(input.authority, true)
    ) {
      return null;
    }
    return Object.freeze({ ...input, data: binding });
  }
  if (
    input.code === "service_call_authorized" ||
    !parsePaidAuthority(input.authority, false)
  ) {
    return null;
  }
  return Object.freeze({ ...input });
}

function canonicalize(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(canonicalize);
  }
  if (isRecord(value)) {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonicalize(value[key])]),
    );
  }
  return value;
}

function completeResult(
  structuredContent: Readonly<Record<string, unknown>>,
): BazaarMcpCallResult {
  const text = JSON.stringify(canonicalize(structuredContent));
  return Object.freeze({
    resultType: "complete",
    content: Object.freeze([Object.freeze({ type: "text", text })]),
    structuredContent,
    isError: structuredContent.ok !== true,
  });
}

function searchRejected(
  code: string,
  reason: string,
  retryable: boolean,
): BazaarMcpCallResult {
  return completeResult(
    Object.freeze({
      schemaVersion: BAZAAR_MCP_PARITY_SCHEMA_VERSION,
      protocolVersion: BAZAAR_MCP_PROTOCOL_VERSION,
      tool: BAZAAR_MCP_SEARCH_TOOL,
      ok: false,
      code,
      reason,
      retryable,
      authority: Object.freeze(
        Object.fromEntries(SEARCH_AUTHORITY_KEYS.map((key) => [key, false])),
      ),
    }),
  );
}

function paidRejected(
  code: string,
  reason: string,
  retryable: boolean,
): BazaarMcpCallResult {
  return completeResult(
    Object.freeze({
      schemaVersion: BAZAAR_MCP_PARITY_SCHEMA_VERSION,
      protocolVersion: BAZAAR_MCP_PROTOCOL_VERSION,
      tool: BAZAAR_MCP_PAID_CALL_TOOL,
      ok: false,
      code,
      reason,
      retryable,
      authority: Object.freeze(
        Object.fromEntries(PAID_AUTHORITY_KEYS.map((key) => [key, false])),
      ),
    }),
  );
}

export class PureBazaarMcpParityHandlers {
  constructor(private readonly rustPort: BazaarMcpRustPort) {}

  async handleSearchCall(input: unknown): Promise<BazaarMcpCallResult> {
    const rawArguments =
      isRecord(input) && isRecord(input.params) && isRecord(input.params.arguments)
        ? input.params.arguments
        : null;
    const call = parseToolCall(
      input,
      BAZAAR_MCP_SEARCH_TOOL,
      SEARCH_ARGUMENT_KEYS,
      4_096,
    );
    if (call === null) {
      if (rawArguments !== null && byteLength(rawArguments) > 4_096) {
        return searchRejected(
          "arguments_too_large",
          "MCP search arguments exceed the 4096-byte offline limit.",
          false,
        );
      }
      return searchRejected(
        "invalid_arguments",
        "MCP search call did not match the strict offline parity envelope.",
        false,
      );
    }
    let raw: unknown;
    try {
      raw = await this.rustPort.search(call.argumentsValue);
    } catch {
      return searchRejected(
        "catalog_unavailable",
        "The Rust Bazaar search port failed closed; no external fallback was attempted.",
        true,
      );
    }
    const result = parseSearchResult(raw);
    return result === null
      ? searchRejected(
          "search_port_invalid",
          "The Rust Bazaar search port returned no stable MCP result.",
          false,
        )
      : completeResult(result);
  }

  async handlePaidCall(input: unknown): Promise<BazaarMcpCallResult> {
    const rawArguments =
      isRecord(input) && isRecord(input.params) && isRecord(input.params.arguments)
        ? input.params.arguments
        : null;
    const call = parseToolCall(
      input,
      BAZAAR_MCP_PAID_CALL_TOOL,
      PAID_CALL_ARGUMENT_KEYS,
      16_384,
    );
    if (call === null) {
      if (rawArguments !== null && byteLength(rawArguments) > 16_384) {
        return paidRejected(
          "arguments_too_large",
          "Paid-call arguments exceed the 16384-byte offline limit.",
          false,
        );
      }
      return paidRejected(
        "invalid_arguments",
        "MCP paid-call did not match the strict offline parity envelope.",
        false,
      );
    }
    if (
      !Object.hasOwn(call.argumentsValue, "schemaVersion") ||
      !Object.hasOwn(call.argumentsValue, "requestId") ||
      !Object.hasOwn(call.argumentsValue, "resourceKey") ||
      !Object.hasOwn(call.argumentsValue, "arguments")
    ) {
      return paidRejected(
        "invalid_arguments",
        "MCP paid-call did not match the strict offline parity envelope.",
        false,
      );
    }
    if (
      !isRecord(call.argumentsValue.arguments) ||
      !jsonWithinBounds(call.argumentsValue.arguments, 32, 2_048)
    ) {
      return paidRejected(
        "invalid_service_arguments",
        "Paid-call service arguments must be a bounded JSON object.",
        false,
      );
    }
    let raw: unknown;
    try {
      raw = await this.rustPort.paidCall(call.argumentsValue);
    } catch {
      return paidRejected(
        "access_gate_unavailable",
        "The Rust settled-access port failed closed; no service call was authorized.",
        true,
      );
    }
    const result = parsePaidResult(raw, call.argumentsValue);
    return result === null
      ? paidRejected(
          "paid_call_port_invalid",
          "The Rust paid-call port returned no stable MCP result.",
          false,
        )
      : completeResult(result);
  }
}
