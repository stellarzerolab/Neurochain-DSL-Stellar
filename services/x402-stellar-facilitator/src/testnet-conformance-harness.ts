import { createHash } from "node:crypto";

import {
  DEFAULT_TESTNET_HORIZON_URL,
  DEFAULT_TESTNET_RPC_URL,
  STELLAR_TESTNET_CAIP2,
  USDC_TESTNET_ADDRESS,
  convertToTokenAmount,
  validateStellarDestinationAddress,
} from "@x402/stellar";

export const TESTNET_HARNESS_SCHEMA_VERSION = 1 as const;
export const TESTNET_HARNESS_CONFIRMATION =
  "EXECUTE_BOUNDED_X402_TESTNET" as const;
export const OFFICIAL_TESTNET_FRIENDBOT_URL =
  "https://friendbot.stellar.org" as const;
export const BOUNDED_TESTNET_AMOUNT = convertToTokenAmount("0.01");

export const TESTNET_CREDENTIAL_HANDLE: unique symbol = Symbol(
  "testnetCredentialHandle",
);

const REQUEST_KEYS = Object.freeze([
  "amount",
  "asset",
  "confirmation",
  "execute",
  "friendbotUrl",
  "horizonUrl",
  "network",
  "payTo",
  "rpcUrl",
  "schemaVersion",
  "scheme",
  "x402Version",
]);

const EVIDENCE_KEYS = Object.freeze([
  "conformanceResults",
  "ledger",
  "network",
  "observedAt",
  "publicAccountId",
  "status",
  "transactionHash",
]);

const CONFORMANCE_RESULT_KEYS = Object.freeze([
  "check",
  "code",
  "reason",
  "status",
]);

const PUBLIC_CHECKS = Object.freeze({
  canonical_supported: Object.freeze({
    code: "supported_passed",
    reason: "canonical supported check passed",
  }),
  canonical_verify: Object.freeze({
    code: "verify_passed",
    reason: "canonical verify check passed",
  }),
  canonical_settlement: Object.freeze({
    code: "settlement_confirmed",
    reason: "canonical settlement was confirmed on Stellar testnet",
  }),
});

export interface TestnetHarnessRequest {
  readonly schemaVersion: typeof TESTNET_HARNESS_SCHEMA_VERSION;
  readonly execute: boolean;
  readonly confirmation: string | null;
  readonly network: typeof STELLAR_TESTNET_CAIP2;
  readonly rpcUrl: typeof DEFAULT_TESTNET_RPC_URL;
  readonly horizonUrl: typeof DEFAULT_TESTNET_HORIZON_URL;
  readonly friendbotUrl: typeof OFFICIAL_TESTNET_FRIENDBOT_URL;
  readonly x402Version: 2;
  readonly scheme: "exact";
  readonly asset: typeof USDC_TESTNET_ADDRESS;
  readonly payTo: string;
  readonly amount: string;
}

export interface TestnetHarnessSafePlan {
  readonly schemaVersion: typeof TESTNET_HARNESS_SCHEMA_VERSION;
  readonly execute: boolean;
  readonly network: typeof STELLAR_TESTNET_CAIP2;
  readonly rpcUrl: typeof DEFAULT_TESTNET_RPC_URL;
  readonly horizonUrl: typeof DEFAULT_TESTNET_HORIZON_URL;
  readonly friendbotUrl: typeof OFFICIAL_TESTNET_FRIENDBOT_URL;
  readonly x402Version: 2;
  readonly scheme: "exact";
  readonly asset: typeof USDC_TESTNET_ADDRESS;
  readonly payTo: string;
  readonly amount: string;
  readonly requestDigest: string;
}

export interface TestnetStateReservation {
  readonly status: "reserved" | "unavailable";
  readonly code: string;
  readonly reason: string;
}

export interface TestnetStatePort {
  reserve(requestDigest: string): Promise<TestnetStateReservation>;
}

export interface EphemeralTestnetCredential {
  readonly publicAccountId: string;
  readonly [TESTNET_CREDENTIAL_HANDLE]: unknown;
}

export interface TestnetCredentialPort {
  createEphemeral(): Promise<EphemeralTestnetCredential>;
}

export interface TestnetConformanceCheck {
  readonly check: string;
  readonly status: "passed" | "failed";
  readonly code: string;
  readonly reason: string;
}

export interface TestnetPublicEvidence {
  readonly network: typeof STELLAR_TESTNET_CAIP2;
  readonly publicAccountId: string;
  readonly transactionHash: string | null;
  readonly ledger: number | null;
  readonly status: "supported_verified" | "settled";
  readonly observedAt: string;
  readonly conformanceResults: readonly TestnetConformanceCheck[];
}

export interface CanonicalTestnetPort {
  run(
    plan: TestnetHarnessSafePlan,
    credential: EphemeralTestnetCredential,
  ): Promise<unknown>;
}

export interface TestnetHarnessBoundary {
  readonly expectedPayTo: string;
  readonly statePort?: TestnetStatePort;
  readonly credentialPort?: TestnetCredentialPort;
  readonly canonicalPort?: CanonicalTestnetPort;
}

export type TestnetHarnessCode =
  | "testnet_harness_ready"
  | "testnet_conformance_completed"
  | "testnet_request_invalid"
  | "testnet_execute_opt_in_required"
  | "testnet_network_forbidden"
  | "testnet_endpoint_forbidden"
  | "testnet_asset_mismatch"
  | "testnet_recipient_mismatch"
  | "testnet_amount_mismatch"
  | "testnet_authority_forbidden"
  | "testnet_state_unavailable"
  | "testnet_credential_invalid"
  | "testnet_execution_unavailable"
  | "testnet_public_evidence_invalid";

export interface TestnetHarnessAuthorityBoundary {
  readonly paymentGranted: false;
  readonly proofGranted: false;
  readonly approvalGranted: false;
  readonly serviceDispatchGranted: false;
  readonly underlyingExecutionGranted: false;
  readonly walletGeneralAuthorityGranted: false;
  readonly signingGeneralAuthorityGranted: false;
  readonly settlementGeneralAuthorityGranted: false;
  readonly transactionSubmitGeneralAuthorityGranted: false;
  readonly actionPlanSubmitGranted: false;
  readonly guardrailOverrideGranted: false;
}

export interface TestnetHarnessResult {
  readonly status: "ready" | "completed" | "blocked";
  readonly code: TestnetHarnessCode;
  readonly reason: string;
  readonly plan: TestnetHarnessSafePlan | null;
  readonly evidence: TestnetPublicEvidence | null;
  readonly authorityBoundary: TestnetHarnessAuthorityBoundary;
}

const AUTHORITY_BOUNDARY = Object.freeze({
  paymentGranted: false as const,
  proofGranted: false as const,
  approvalGranted: false as const,
  serviceDispatchGranted: false as const,
  underlyingExecutionGranted: false as const,
  walletGeneralAuthorityGranted: false as const,
  signingGeneralAuthorityGranted: false as const,
  settlementGeneralAuthorityGranted: false as const,
  transactionSubmitGeneralAuthorityGranted: false as const,
  actionPlanSubmitGranted: false as const,
  guardrailOverrideGranted: false as const,
});

function canonicalize(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(canonicalize);
  }
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, nested]) => [key, canonicalize(nested)]),
    );
  }
  return value;
}

function canonicalJson(value: unknown): string {
  return JSON.stringify(canonicalize(value));
}

function hasExactKeys(value: object, keys: readonly string[]): boolean {
  return canonicalJson(Object.keys(value).sort()) === canonicalJson(keys);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function blocked(
  code: Exclude<
    TestnetHarnessCode,
    "testnet_harness_ready" | "testnet_conformance_completed"
  >,
  reason: string,
  plan: TestnetHarnessSafePlan | null = null,
): TestnetHarnessResult {
  return Object.freeze({
    status: "blocked" as const,
    code,
    reason,
    plan,
    evidence: null,
    authorityBoundary: AUTHORITY_BOUNDARY,
  });
}

function hasStrictRequestEnvelope(
  value: unknown,
): value is Record<string, unknown> {
  return (
    value !== null &&
    typeof value === "object" &&
    !Array.isArray(value) &&
    hasExactKeys(value, REQUEST_KEYS)
  );
}

function validateRequest(
  value: unknown,
  expectedPayTo: string,
): TestnetHarnessResult | TestnetHarnessRequest {
  if (!hasStrictRequestEnvelope(value)) {
    return blocked(
      "testnet_request_invalid",
      "request must use the strict schema-v1 testnet harness envelope",
    );
  }
  if (
    value.schemaVersion !== TESTNET_HARNESS_SCHEMA_VERSION ||
    typeof value.execute !== "boolean" ||
    (value.confirmation !== null && typeof value.confirmation !== "string") ||
    value.x402Version !== 2 ||
    value.scheme !== "exact"
  ) {
    return blocked(
      "testnet_request_invalid",
      "request schemaVersion, execution flag, confirmation, x402Version or scheme is invalid",
    );
  }
  if (value.network !== STELLAR_TESTNET_CAIP2) {
    return blocked(
      "testnet_network_forbidden",
      "only the dedicated stellar:testnet network is allowed",
    );
  }
  if (
    value.rpcUrl !== DEFAULT_TESTNET_RPC_URL ||
    value.horizonUrl !== DEFAULT_TESTNET_HORIZON_URL ||
    value.friendbotUrl !== OFFICIAL_TESTNET_FRIENDBOT_URL
  ) {
    return blocked(
      "testnet_endpoint_forbidden",
      "RPC, Horizon and Friendbot must match the exact official testnet allowlist",
    );
  }
  if (value.asset !== USDC_TESTNET_ADDRESS) {
    return blocked(
      "testnet_asset_mismatch",
      "asset must match the pinned Stellar testnet USDC contract",
    );
  }
  if (
    !isNonEmptyString(expectedPayTo) ||
    !validateStellarDestinationAddress(expectedPayTo) ||
    value.payTo !== expectedPayTo
  ) {
    return blocked(
      "testnet_recipient_mismatch",
      "payTo must match the explicitly bounded valid testnet recipient",
    );
  }
  if (value.amount !== BOUNDED_TESTNET_AMOUNT) {
    return blocked(
      "testnet_amount_mismatch",
      "amount must match the fixed 0.01 test-token amount at seven decimals",
    );
  }
  if (!value.execute && value.confirmation !== null) {
    return blocked(
      "testnet_authority_forbidden",
      "dry-run requests must not carry an execution confirmation",
    );
  }
  if (value.execute && value.confirmation !== TESTNET_HARNESS_CONFIRMATION) {
    return blocked(
      "testnet_execute_opt_in_required",
      "network execution requires the exact explicit bounded-testnet confirmation",
    );
  }

  return value as unknown as TestnetHarnessRequest;
}

function buildSafePlan(request: TestnetHarnessRequest): TestnetHarnessSafePlan {
  const safeFields = Object.freeze({
    schemaVersion: request.schemaVersion,
    execute: request.execute,
    network: request.network,
    rpcUrl: request.rpcUrl,
    horizonUrl: request.horizonUrl,
    friendbotUrl: request.friendbotUrl,
    x402Version: request.x402Version,
    scheme: request.scheme,
    asset: request.asset,
    payTo: request.payTo,
    amount: request.amount,
  });
  const requestDigest = createHash("sha256")
    .update(canonicalJson(safeFields))
    .digest("hex");
  return Object.freeze({ ...safeFields, requestDigest });
}

function hasStrictConformanceResult(
  value: unknown,
): value is TestnetConformanceCheck {
  return (
    value !== null &&
    typeof value === "object" &&
    !Array.isArray(value) &&
    hasExactKeys(value, CONFORMANCE_RESULT_KEYS) &&
    isNonEmptyString((value as Record<string, unknown>).check) &&
    ((value as Record<string, unknown>).status === "passed" ||
      (value as Record<string, unknown>).status === "failed") &&
    isNonEmptyString((value as Record<string, unknown>).code) &&
    isNonEmptyString((value as Record<string, unknown>).reason)
  );
}

function sanitizeConformanceResults(
  value: unknown,
  evidenceStatus: "supported_verified" | "settled",
): readonly TestnetConformanceCheck[] | null {
  if (!Array.isArray(value) || value.length === 0) {
    return null;
  }
  const requiredChecks =
    evidenceStatus === "settled"
      ? ["canonical_supported", "canonical_verify", "canonical_settlement"]
      : ["canonical_supported", "canonical_verify"];
  if (value.length !== requiredChecks.length) {
    return null;
  }

  const sanitized: TestnetConformanceCheck[] = [];
  for (const [index, candidate] of value.entries()) {
    if (!hasStrictConformanceResult(candidate) || candidate.status !== "passed") {
      return null;
    }
    const expectedCheck = requiredChecks[index];
    if (!expectedCheck || candidate.check !== expectedCheck) {
      return null;
    }
    const allowed = PUBLIC_CHECKS[expectedCheck as keyof typeof PUBLIC_CHECKS];
    if (candidate.code !== allowed.code) {
      return null;
    }
    sanitized.push(
      Object.freeze({
        check: expectedCheck,
        status: "passed" as const,
        code: allowed.code,
        reason: allowed.reason,
      }),
    );
  }
  return Object.freeze(sanitized);
}

function parsePublicEvidence(value: unknown): TestnetPublicEvidence | null {
  if (
    value === null ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    !hasExactKeys(value, EVIDENCE_KEYS)
  ) {
    return null;
  }
  const candidate = value as Record<string, unknown>;
  if (
    candidate.network !== STELLAR_TESTNET_CAIP2 ||
    !isNonEmptyString(candidate.publicAccountId) ||
    !validateStellarDestinationAddress(candidate.publicAccountId) ||
    (candidate.status !== "supported_verified" &&
      candidate.status !== "settled") ||
    !isNonEmptyString(candidate.observedAt) ||
    Number.isNaN(Date.parse(candidate.observedAt)) ||
    new Date(candidate.observedAt).toISOString() !== candidate.observedAt
  ) {
    return null;
  }
  const conformanceResults = sanitizeConformanceResults(
    candidate.conformanceResults,
    candidate.status,
  );
  if (!conformanceResults) {
    return null;
  }
  const isSettled = candidate.status === "settled";
  if (
    (isSettled &&
      (!isNonEmptyString(candidate.transactionHash) ||
        !/^[0-9a-f]{64}$/u.test(candidate.transactionHash) ||
        !Number.isSafeInteger(candidate.ledger) ||
        (candidate.ledger as number) <= 0)) ||
    (!isSettled &&
      (candidate.transactionHash !== null || candidate.ledger !== null))
  ) {
    return null;
  }

  return Object.freeze({
    network: STELLAR_TESTNET_CAIP2,
    publicAccountId: candidate.publicAccountId,
    transactionHash: candidate.transactionHash as string | null,
    ledger: candidate.ledger as number | null,
    status: candidate.status,
    observedAt: candidate.observedAt,
    conformanceResults,
  });
}

export async function runTestnetConformanceHarness(
  requestValue: unknown,
  boundary: TestnetHarnessBoundary,
): Promise<TestnetHarnessResult> {
  const validated = validateRequest(requestValue, boundary.expectedPayTo);
  if ("authorityBoundary" in validated) {
    return validated;
  }
  const plan = buildSafePlan(validated);
  if (!validated.execute) {
    return Object.freeze({
      status: "ready" as const,
      code: "testnet_harness_ready" as const,
      reason:
        "bounded Stellar testnet plan validated offline; no credential, signing, network or submit action ran",
      plan,
      evidence: null,
      authorityBoundary: AUTHORITY_BOUNDARY,
    });
  }
  if (!boundary.statePort) {
    return blocked(
      "testnet_state_unavailable",
      "execution requires an atomic non-production state port",
      plan,
    );
  }
  if (!boundary.credentialPort || !boundary.canonicalPort) {
    return blocked(
      "testnet_execution_unavailable",
      "execution requires explicit ephemeral-credential and canonical-client ports",
      plan,
    );
  }

  let reservation: TestnetStateReservation;
  try {
    reservation = await boundary.statePort.reserve(plan.requestDigest);
  } catch {
    return blocked(
      "testnet_state_unavailable",
      "atomic non-production state reservation failed closed",
      plan,
    );
  }
  if (
    reservation.status !== "reserved" ||
    !isNonEmptyString(reservation.code) ||
    !isNonEmptyString(reservation.reason)
  ) {
    return blocked(
      "testnet_state_unavailable",
      "atomic non-production state did not reserve the bounded request",
      plan,
    );
  }

  let credential: EphemeralTestnetCredential;
  try {
    credential = await boundary.credentialPort.createEphemeral();
  } catch {
    return blocked(
      "testnet_credential_invalid",
      "ephemeral testnet credential creation failed without exposing details",
      plan,
    );
  }
  if (
    credential === null ||
    typeof credential !== "object" ||
    !isNonEmptyString(credential.publicAccountId) ||
    !validateStellarDestinationAddress(credential.publicAccountId) ||
    !(TESTNET_CREDENTIAL_HANDLE in credential) ||
    credential[TESTNET_CREDENTIAL_HANDLE] === undefined
  ) {
    return blocked(
      "testnet_credential_invalid",
      "credential port did not return a valid opaque ephemeral testnet credential",
      plan,
    );
  }

  let rawEvidence: unknown;
  try {
    rawEvidence = await boundary.canonicalPort.run(plan, credential);
  } catch {
    return blocked(
      "testnet_execution_unavailable",
      "canonical testnet conformance execution failed without exposing details",
      plan,
    );
  }
  const evidence = parsePublicEvidence(rawEvidence);
  if (!evidence || evidence.publicAccountId !== credential.publicAccountId) {
    return blocked(
      "testnet_public_evidence_invalid",
      "canonical port returned non-public, malformed, unbound or incomplete evidence",
      plan,
    );
  }

  return Object.freeze({
    status: "completed" as const,
    code: "testnet_conformance_completed" as const,
    reason:
      "bounded canonical Stellar testnet conformance completed with public redacted evidence",
    plan,
    evidence,
    authorityBoundary: AUTHORITY_BOUNDARY,
  });
}
