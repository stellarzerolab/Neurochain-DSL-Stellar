import type { SettleResponse, VerifyResponse } from "@x402/core/types";

import { SERVICE_HANDLER_AUTHORITY_BOUNDARY } from "./service-handlers.js";

export const BAZAAR_AUTOMATIC_CATALOGING_SCHEMA_VERSION = 1 as const;

const MAX_JSON_BYTES = 32 * 1024;
const MAX_JSON_DEPTH = 32;
const MAX_JSON_NODES = 4_096;
const MAX_REASON_BYTES = 512;

export type UpstreamExtensions =
  | VerifyResponse["extensions"]
  | SettleResponse["extensions"];

export interface BazaarResourceDescriptor {
  readonly url: string;
  readonly description: string;
  readonly mimeType: string;
  readonly serviceName?: string;
  readonly tags: readonly string[];
  readonly iconUrl?: string;
}

export interface BazaarPaymentSummary {
  readonly scheme: string;
  readonly network: string;
  readonly amount: string;
  readonly asset: string;
  readonly payTo: string;
  readonly maxTimeoutSeconds: number;
}

export interface BazaarCatalogHandoffContext {
  readonly schemaVersion: typeof BAZAAR_AUTOMATIC_CATALOGING_SCHEMA_VERSION;
  readonly x402Version: number;
  readonly resource: BazaarResourceDescriptor;
  readonly payment: BazaarPaymentSummary;
}

export interface BazaarDiscoveryExtension {
  readonly info: unknown;
  readonly schema: unknown;
  readonly routeTemplate?: string;
}

export interface BazaarVerifiedDiscoveryInput
  extends BazaarCatalogHandoffContext {
  readonly bazaar: BazaarDiscoveryExtension;
}

export type BazaarCatalogingDisposition =
  | "accepted"
  | "dropped"
  | "invalid"
  | "duplicate"
  | "unavailable";

export interface BazaarCatalogingOutcome {
  readonly disposition: BazaarCatalogingDisposition;
  readonly code: string;
  readonly reason: string;
  readonly catalogKey?: string;
}

export interface BazaarExtensionResponses {
  readonly bazaar: Readonly<{
    status: "success" | "rejected";
    rejectedReason?: string;
  }>;
}

export interface BazaarAutomaticCatalogingPort {
  catalog(request: BazaarVerifiedDiscoveryInput): Promise<unknown>;
}

export interface BazaarAutomaticCatalogingResult {
  readonly schemaVersion: typeof BAZAAR_AUTOMATIC_CATALOGING_SCHEMA_VERSION;
  readonly outcome: BazaarCatalogingOutcome;
  readonly extensionResponses: BazaarExtensionResponses | null;
  readonly extensionResponsesHeaderValue: string | null;
  readonly handoff: BazaarVerifiedDiscoveryInput | null;
  readonly authorityBoundary: typeof SERVICE_HANDLER_AUTHORITY_BOUNDARY;
}

interface Parsed<T> {
  readonly ok: true;
  readonly value: T;
}

interface ParseFailure {
  readonly ok: false;
  readonly outcome: BazaarCatalogingOutcome;
}

type ParseResult<T> = Parsed<T> | ParseFailure;

const CONTEXT_KEYS = Object.freeze([
  "payment",
  "resource",
  "schemaVersion",
  "x402Version",
]);
const RESOURCE_KEYS = Object.freeze([
  "description",
  "iconUrl",
  "mimeType",
  "serviceName",
  "tags",
  "url",
]);
const RESOURCE_REQUIRED_KEYS = Object.freeze([
  "description",
  "mimeType",
  "tags",
  "url",
]);
const PAYMENT_KEYS = Object.freeze([
  "amount",
  "asset",
  "maxTimeoutSeconds",
  "network",
  "payTo",
  "scheme",
]);
const EXTENSION_KEYS = Object.freeze(["info", "routeTemplate", "schema"]);
const EXTENSION_REQUIRED_KEYS = Object.freeze(["info", "schema"]);
const OUTCOME_KEYS = Object.freeze([
  "catalogKey",
  "code",
  "disposition",
  "reason",
]);
const OUTCOME_REQUIRED_KEYS = Object.freeze([
  "code",
  "disposition",
  "reason",
]);
const DISPOSITIONS = Object.freeze([
  "accepted",
  "dropped",
  "invalid",
  "duplicate",
  "unavailable",
] as const);

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function isPlainJsonObject(value: unknown): value is Record<string, unknown> {
  if (!isRecord(value)) {
    return false;
  }
  const prototype = Object.getPrototypeOf(value) as unknown;
  return (
    (prototype === Object.prototype || prototype === null) &&
    Object.getOwnPropertySymbols(value).length === 0
  );
}

function hasAllowedKeys(
  value: Record<string, unknown>,
  allowed: readonly string[],
  required: readonly string[],
): boolean {
  const actual = Object.keys(value);
  return (
    actual.every((key) => allowed.includes(key)) &&
    required.every((key) => Object.hasOwn(value, key))
  );
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

function isBoundedString(
  value: unknown,
  maxBytes: number,
  allowEmpty = false,
): value is string {
  return (
    typeof value === "string" &&
    (allowEmpty || value.trim().length > 0) &&
    Buffer.byteLength(value, "utf8") <= maxBytes
  );
}

function outcome(
  disposition: BazaarCatalogingDisposition,
  code: string,
  reason: string,
  catalogKey?: string,
): BazaarCatalogingOutcome {
  const boundedReason = truncateUtf8(reason.trim() || code, MAX_REASON_BYTES);
  return catalogKey === undefined
    ? Object.freeze({ disposition, code, reason: boundedReason })
    : Object.freeze({ disposition, code, reason: boundedReason, catalogKey });
}

function truncateUtf8(value: string, maxBytes: number): string {
  if (Buffer.byteLength(value, "utf8") <= maxBytes) {
    return value;
  }
  let end = value.length;
  while (end > 0 && Buffer.byteLength(value.slice(0, end), "utf8") > maxBytes) {
    end -= 1;
  }
  return value.slice(0, end);
}

function parseContext(input: unknown): ParseResult<BazaarCatalogHandoffContext> {
  if (!isRecord(input) || !hasExactKeys(input, CONTEXT_KEYS)) {
    return {
      ok: false,
      outcome: outcome(
        "invalid",
        "invalid_catalog_handoff_context",
        "automatic cataloging requires the strict public v1 handoff context",
      ),
    };
  }
  if (input.schemaVersion !== BAZAAR_AUTOMATIC_CATALOGING_SCHEMA_VERSION) {
    return {
      ok: false,
      outcome: outcome(
        "invalid",
        "unsupported_cataloging_schema_version",
        "automatic cataloging envelope schema version is unsupported",
      ),
    };
  }
  if (
    !Number.isSafeInteger(input.x402Version) ||
    Number(input.x402Version) < 0 ||
    Number(input.x402Version) > 0xffff_ffff
  ) {
    return {
      ok: false,
      outcome: outcome(
        "invalid",
        "invalid_catalog_handoff_context",
        "automatic cataloging x402 version is invalid",
      ),
    };
  }

  const resource = parseResource(input.resource);
  const payment = parsePayment(input.payment);
  if (resource === null || payment === null) {
    return {
      ok: false,
      outcome: outcome(
        "invalid",
        "invalid_catalog_handoff_context",
        "handoff resource or payment summary failed the strict public schema",
      ),
    };
  }
  return {
    ok: true,
    value: Object.freeze({
      schemaVersion: BAZAAR_AUTOMATIC_CATALOGING_SCHEMA_VERSION,
      x402Version: Number(input.x402Version),
      resource,
      payment,
    }),
  };
}

function parseResource(input: unknown): BazaarResourceDescriptor | null {
  if (
    !isRecord(input) ||
    !hasAllowedKeys(input, RESOURCE_KEYS, RESOURCE_REQUIRED_KEYS) ||
    !isBoundedString(input.url, 2_048) ||
    !isBoundedString(input.description, 4_096, true) ||
    !isBoundedString(input.mimeType, 256) ||
    !Array.isArray(input.tags) ||
    input.tags.length > 64 ||
    input.tags.some((tag) => !isBoundedString(tag, 128)) ||
    (input.serviceName !== undefined &&
      !isBoundedString(input.serviceName, 256)) ||
    (input.iconUrl !== undefined && !isBoundedString(input.iconUrl, 2_048))
  ) {
    return null;
  }
  return Object.freeze({
    url: input.url,
    description: input.description,
    mimeType: input.mimeType,
    ...(input.serviceName === undefined
      ? {}
      : { serviceName: input.serviceName }),
    tags: Object.freeze([...input.tags]) as readonly string[],
    ...(input.iconUrl === undefined ? {} : { iconUrl: input.iconUrl }),
  });
}

function parsePayment(input: unknown): BazaarPaymentSummary | null {
  if (
    !isRecord(input) ||
    !hasExactKeys(input, PAYMENT_KEYS) ||
    !isBoundedString(input.scheme, 64) ||
    !isBoundedString(input.network, 128) ||
    !isBoundedString(input.amount, 128) ||
    !isBoundedString(input.asset, 256) ||
    !isBoundedString(input.payTo, 256) ||
    !Number.isSafeInteger(input.maxTimeoutSeconds) ||
    Number(input.maxTimeoutSeconds) < 0
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

function validateJsonBounds(
  value: unknown,
  label: "info" | "schema",
): BazaarCatalogingOutcome | null {
  let encoded: string;
  try {
    encoded = JSON.stringify(value);
  } catch {
    return outcome(
      "invalid",
      "invalid_json_value",
      `${label} could not be encoded`,
    );
  }
  if (encoded === undefined) {
    return outcome(
      "invalid",
      "invalid_json_value",
      `${label} could not be encoded`,
    );
  }
  if (Buffer.byteLength(encoded, "utf8") > MAX_JSON_BYTES) {
    return outcome(
      "invalid",
      "json_value_too_large",
      `${label} exceeds the ${MAX_JSON_BYTES}-byte offline limit`,
    );
  }

  const stack: Array<Readonly<{ value: unknown; depth: number }>> = [
    { value, depth: 0 },
  ];
  let nodes = 0;
  while (stack.length > 0) {
    const current = stack.pop();
    if (current === undefined) {
      break;
    }
    nodes += 1;
    if (nodes > MAX_JSON_NODES) {
      return outcome(
        "invalid",
        "json_value_too_complex",
        `${label} exceeds the ${MAX_JSON_NODES}-node offline limit`,
      );
    }
    if (current.depth > MAX_JSON_DEPTH) {
      return outcome(
        "invalid",
        "json_value_too_deep",
        `${label} exceeds the ${MAX_JSON_DEPTH}-level offline limit`,
      );
    }
    if (Array.isArray(current.value)) {
      for (const child of current.value) {
        stack.push({ value: child, depth: current.depth + 1 });
      }
    } else if (isPlainJsonObject(current.value)) {
      for (const child of Object.values(current.value)) {
        stack.push({ value: child, depth: current.depth + 1 });
      }
    } else if (
      current.value !== null &&
      typeof current.value !== "string" &&
      typeof current.value !== "boolean" &&
      !(typeof current.value === "number" && Number.isFinite(current.value))
    ) {
      return outcome(
        "invalid",
        "invalid_json_value",
        `${label} contains a value outside the JSON data model`,
      );
    }
  }
  return null;
}

function parseBazaarExtension(input: unknown): ParseResult<BazaarDiscoveryExtension> {
  if (
    !isRecord(input) ||
    !hasAllowedKeys(input, EXTENSION_KEYS, EXTENSION_REQUIRED_KEYS) ||
    (input.routeTemplate !== undefined &&
      !isBoundedString(input.routeTemplate, 2_048))
  ) {
    return {
      ok: false,
      outcome: outcome(
        "invalid",
        "invalid_bazaar_extension",
        "Bazaar metadata must use the strict info, schema and optional routeTemplate envelope",
      ),
    };
  }
  const schemaBounds = validateJsonBounds(input.schema, "schema");
  if (schemaBounds !== null) {
    return { ok: false, outcome: schemaBounds };
  }
  const infoBounds = validateJsonBounds(input.info, "info");
  if (infoBounds !== null) {
    return { ok: false, outcome: infoBounds };
  }
  return {
    ok: true,
    value: Object.freeze({
      info: structuredClone(input.info),
      schema: structuredClone(input.schema),
      ...(input.routeTemplate === undefined
        ? {}
        : { routeTemplate: input.routeTemplate }),
    }),
  };
}

function parseOutcome(input: unknown): BazaarCatalogingOutcome | null {
  if (
    !isRecord(input) ||
    !hasAllowedKeys(input, OUTCOME_KEYS, OUTCOME_REQUIRED_KEYS) ||
    !DISPOSITIONS.some((value) => value === input.disposition) ||
    !isBoundedString(input.code, 128) ||
    !isBoundedString(input.reason, MAX_REASON_BYTES) ||
    (input.catalogKey !== undefined &&
      !isBoundedString(input.catalogKey, 2_048))
  ) {
    return null;
  }
  const keyRequired =
    input.disposition === "accepted" || input.disposition === "duplicate";
  if (keyRequired !== (input.catalogKey !== undefined)) {
    return null;
  }
  return outcome(
    input.disposition as BazaarCatalogingDisposition,
    input.code,
    input.reason,
    input.catalogKey as string | undefined,
  );
}

function buildResult(
  catalogingOutcome: BazaarCatalogingOutcome,
  handoff: BazaarVerifiedDiscoveryInput | null,
): BazaarAutomaticCatalogingResult {
  const extensionResponses = buildExtensionResponses(catalogingOutcome);
  return Object.freeze({
    schemaVersion: BAZAAR_AUTOMATIC_CATALOGING_SCHEMA_VERSION,
    outcome: catalogingOutcome,
    extensionResponses,
    extensionResponsesHeaderValue:
      extensionResponses === null
        ? null
        : Buffer.from(JSON.stringify(extensionResponses), "utf8").toString(
            "base64",
          ),
    handoff,
    authorityBoundary: SERVICE_HANDLER_AUTHORITY_BOUNDARY,
  });
}

function buildExtensionResponses(
  catalogingOutcome: BazaarCatalogingOutcome,
): BazaarExtensionResponses | null {
  switch (catalogingOutcome.disposition) {
    case "accepted":
      return Object.freeze({ bazaar: Object.freeze({ status: "success" }) });
    case "dropped":
      return null;
    case "invalid":
    case "duplicate":
    case "unavailable":
      return Object.freeze({
        bazaar: Object.freeze({
          status: "rejected",
          rejectedReason: `${catalogingOutcome.code}: ${catalogingOutcome.reason}`,
        }),
      });
  }
}

/**
 * Maps only an upstream verify/settle result's `extensions` field into the
 * versioned Rust automatic-cataloging handoff. The caller remains responsible
 * for invoking this function only after the upstream payment phase succeeded.
 * This module cannot verify, settle, dispatch, sign, or submit anything.
 */
export async function catalogUpstreamBazaarExtension(
  extensions: UpstreamExtensions,
  context: unknown,
  catalogPort?: BazaarAutomaticCatalogingPort,
): Promise<BazaarAutomaticCatalogingResult> {
  const parsedContext = parseContext(context);
  if (!parsedContext.ok) {
    return buildResult(parsedContext.outcome, null);
  }
  if (extensions === undefined) {
    return buildResult(
      outcome(
        "dropped",
        "bazaar_extension_missing",
        "upstream result did not contain the Bazaar extension",
      ),
      null,
    );
  }
  if (!isRecord(extensions)) {
    return buildResult(
      outcome(
        "invalid",
        "invalid_bazaar_extensions",
        "upstream extensions must be an object",
      ),
      null,
    );
  }
  if (!Object.hasOwn(extensions, "bazaar")) {
    return buildResult(
      outcome(
        "dropped",
        "bazaar_extension_missing",
        "upstream result did not contain the Bazaar extension",
      ),
      null,
    );
  }
  const parsedExtension = parseBazaarExtension(extensions.bazaar);
  if (!parsedExtension.ok) {
    return buildResult(parsedExtension.outcome, null);
  }

  const handoff: BazaarVerifiedDiscoveryInput = Object.freeze({
    ...parsedContext.value,
    bazaar: parsedExtension.value,
  });
  if (catalogPort === undefined) {
    return buildResult(
      outcome(
        "unavailable",
        "catalog_unavailable",
        "Bazaar automatic-cataloging port is unavailable",
      ),
      handoff,
    );
  }

  let rawOutcome: unknown;
  try {
    rawOutcome = await catalogPort.catalog(handoff);
  } catch {
    return buildResult(
      outcome(
        "unavailable",
        "catalog_unavailable",
        "Bazaar automatic-cataloging port failed closed",
      ),
      handoff,
    );
  }
  const catalogingOutcome = parseOutcome(rawOutcome);
  if (catalogingOutcome === null) {
    return buildResult(
      outcome(
        "unavailable",
        "catalog_outcome_invalid",
        "Bazaar automatic-cataloging port returned no stable outcome",
      ),
      handoff,
    );
  }
  return buildResult(catalogingOutcome, handoff);
}
