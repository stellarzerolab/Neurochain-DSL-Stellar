import { createHash } from "node:crypto";

import { Asset, Networks } from "@stellar/stellar-sdk";
import {
  DEFAULT_TESTNET_HORIZON_URL,
  DEFAULT_TESTNET_RPC_URL,
  STELLAR_TESTNET_CAIP2,
  convertToTokenAmount,
  validateStellarDestinationAddress,
} from "@x402/stellar";

export const TESTNET_HARNESS_SCHEMA_VERSION = 2 as const;
export const TESTNET_HARNESS_ATTEMPT = 3 as const;
export const TESTNET_HARNESS_CONFIRMATION =
  "EXECUTE_BOUNDED_X402_TESTNET" as const;
export const OFFICIAL_TESTNET_FRIENDBOT_URL =
  "https://friendbot.stellar.org" as const;
export const NATIVE_XLM_TESTNET_ADDRESS = Asset.native().contractId(
  Networks.TESTNET,
);
export const BOUNDED_TESTNET_RECIPIENT =
  "GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5" as const;
export const BOUNDED_TESTNET_AMOUNT = convertToTokenAmount("0.01");

export const TESTNET_CREDENTIAL_HANDLE: unique symbol = Symbol(
  "testnetCredentialHandle",
);

const REQUEST_KEYS = Object.freeze([
  "amount",
  "asset",
  "attempt",
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
  readonly attempt: typeof TESTNET_HARNESS_ATTEMPT;
  readonly execute: boolean;
  readonly confirmation: string | null;
  readonly network: typeof STELLAR_TESTNET_CAIP2;
  readonly rpcUrl: typeof DEFAULT_TESTNET_RPC_URL;
  readonly horizonUrl: typeof DEFAULT_TESTNET_HORIZON_URL;
  readonly friendbotUrl: typeof OFFICIAL_TESTNET_FRIENDBOT_URL;
  readonly x402Version: 2;
  readonly scheme: "exact";
  readonly asset: string;
  readonly payTo: string;
  readonly amount: string;
}

export interface TestnetHarnessSafePlan {
  readonly schemaVersion: typeof TESTNET_HARNESS_SCHEMA_VERSION;
  readonly attempt: typeof TESTNET_HARNESS_ATTEMPT;
  readonly execute: boolean;
  readonly network: typeof STELLAR_TESTNET_CAIP2;
  readonly rpcUrl: typeof DEFAULT_TESTNET_RPC_URL;
  readonly horizonUrl: typeof DEFAULT_TESTNET_HORIZON_URL;
  readonly friendbotUrl: typeof OFFICIAL_TESTNET_FRIENDBOT_URL;
  readonly x402Version: 2;
  readonly scheme: "exact";
  readonly asset: string;
  readonly payTo: string;
  readonly amount: string;
  readonly requestDigest: string;
}

export type TestnetStateReservation =
  | Readonly<{
      status: "reserved";
      reservationId: string;
      code: string;
      reason: string;
    }>
  | Readonly<{
      status: "unavailable";
      code: string;
      reason: string;
    }>;

export type TestnetStateOutcome =
  | Readonly<{
      status: "confirmed";
      evidence: TestnetPublicEvidence;
    }>
  | Readonly<{ status: "outcome_unknown" }>;

export type TestnetStateFinalization =
  | Readonly<{
      status: "recorded";
      code: string;
      reason: string;
    }>
  | Readonly<{
      status: "unavailable";
      code: string;
      reason: string;
    }>;

export interface TestnetStatePort {
  reserve(requestDigest: string): Promise<TestnetStateReservation>;
  finalize(
    reservationId: string,
    outcome: TestnetStateOutcome,
  ): Promise<TestnetStateFinalization>;
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

const TESTNET_CANONICAL_DIAGNOSTICS = Object.freeze({
  credential_validation: Object.freeze({
    code: "testnet_canonical_credential_validation_failed",
    reason: "canonical port rejected the opaque credential boundary",
  }),
  network_allowlist: Object.freeze({
    code: "testnet_network_allowlist_failed",
    reason: "canonical port rejected a request outside the official testnet allowlist",
  }),
  supported_snapshot: Object.freeze({
    code: "testnet_supported_snapshot_failed",
    reason: "canonical supported snapshot validation failed closed",
  }),
  friendbot_funding: Object.freeze({
    code: "testnet_friendbot_funding_failed",
    reason: "official Friendbot funding failed closed",
  }),
  payer_horizon_readiness: Object.freeze({
    code: "testnet_payer_horizon_readiness_failed",
    reason: "payer account did not pass bounded Horizon readiness",
  }),
  recipient_horizon_readiness: Object.freeze({
    code: "testnet_recipient_horizon_readiness_failed",
    reason: "recipient account did not pass bounded Horizon readiness",
  }),
  payment_payload_creation: Object.freeze({
    code: "testnet_payment_payload_creation_failed",
    reason: "canonical payment payload creation failed closed",
  }),
  upstream_verify: Object.freeze({
    code: "testnet_upstream_verify_failed",
    reason: "pinned upstream canonical verify call failed closed",
  }),
  verify_result_validation: Object.freeze({
    code: "testnet_verify_result_validation_failed",
    reason: "pinned upstream verify result did not validate the bounded payer",
  }),
  canonical_port_unknown: Object.freeze({
    code: "testnet_canonical_port_unknown",
    reason: "canonical port failed outside a recognized redacted stage",
  }),
  public_evidence_validation: Object.freeze({
    code: "testnet_public_evidence_validation_failed",
    reason: "canonical port returned invalid or payer-unbound public evidence",
  }),
  state_finalization: Object.freeze({
    code: "testnet_state_finalization_failed",
    reason: "canonical public evidence could not be recorded atomically",
  }),
});

const PINNED_UPSTREAM_VERIFY_REASON_CODES = Object.freeze([
  "invalid_exact_stellar_payload_event_missing_contract_id",
  "invalid_exact_stellar_payload_event_not_transfer",
  "invalid_exact_stellar_payload_event_wrong_amount",
  "invalid_exact_stellar_payload_event_wrong_asset",
  "invalid_exact_stellar_payload_event_wrong_from",
  "invalid_exact_stellar_payload_event_wrong_to",
  "invalid_exact_stellar_payload_facilitator_in_auth",
  "invalid_exact_stellar_payload_facilitator_is_payer",
  "invalid_exact_stellar_payload_fee_exceeds_maximum",
  "invalid_exact_stellar_payload_has_subinvocations",
  "invalid_exact_stellar_payload_malformed",
  "invalid_exact_stellar_payload_missing_payer_signature",
  "invalid_exact_stellar_payload_multiple_transfers",
  "invalid_exact_stellar_payload_no_auth_entries",
  "invalid_exact_stellar_payload_no_transfer_events",
  "invalid_exact_stellar_payload_simulation_failed",
  "invalid_exact_stellar_payload_unexpected_pending_signatures",
  "invalid_exact_stellar_payload_unsafe_tx_or_op_source",
  "invalid_exact_stellar_payload_unsupported_credential_type",
  "invalid_exact_stellar_payload_wrong_amount",
  "invalid_exact_stellar_payload_wrong_asset",
  "invalid_exact_stellar_payload_wrong_function_name",
  "invalid_exact_stellar_payload_wrong_operation",
  "invalid_exact_stellar_payload_wrong_recipient",
  "invalid_exact_stellar_signature_expiration_too_far",
  "invalid_network",
  "invalid_x402_version",
  "network_mismatch",
  "unexpected_verify_error",
  "unsupported_scheme",
] as const);

const PINNED_UPSTREAM_VERIFY_REASON_SET = new Set<string>(
  PINNED_UPSTREAM_VERIFY_REASON_CODES,
);

export type PinnedUpstreamVerifyReasonCode =
  (typeof PINNED_UPSTREAM_VERIFY_REASON_CODES)[number];

export type TestnetVerifyDiagnosticDetailCode =
  | PinnedUpstreamVerifyReasonCode
  | "missing_upstream_verify_reason"
  | "unrecognized_upstream_verify_reason"
  | "verified_payer_mismatch"
  | "verify_response_malformed";

export type TestnetCanonicalStage = keyof typeof TESTNET_CANONICAL_DIAGNOSTICS;

export interface TestnetCanonicalDiagnostic {
  readonly stage: TestnetCanonicalStage;
  readonly code: string;
  readonly reason: string;
  readonly retryAllowed: false;
  readonly detailCode?: TestnetVerifyDiagnosticDetailCode;
}

function canonicalDiagnostic(
  stage: TestnetCanonicalStage,
  detailCode?: unknown,
): TestnetCanonicalDiagnostic {
  const definition = TESTNET_CANONICAL_DIAGNOSTICS[stage];
  const safeDetailCode =
    stage === "verify_result_validation" && detailCode !== undefined
      ? normalizeTestnetVerifyDiagnosticDetail(detailCode)
      : undefined;
  return Object.freeze({
    stage,
    code: definition.code,
    reason: definition.reason,
    retryAllowed: false as const,
    ...(safeDetailCode === undefined ? {} : { detailCode: safeDetailCode }),
  });
}

export function listPinnedUpstreamVerifyReasonCodes(): readonly PinnedUpstreamVerifyReasonCode[] {
  return PINNED_UPSTREAM_VERIFY_REASON_CODES;
}

export function classifyPinnedUpstreamVerifyReason(
  value: unknown,
): TestnetVerifyDiagnosticDetailCode {
  if (typeof value !== "string" || value.trim().length === 0) {
    return "missing_upstream_verify_reason";
  }
  return PINNED_UPSTREAM_VERIFY_REASON_SET.has(value)
    ? (value as PinnedUpstreamVerifyReasonCode)
    : "unrecognized_upstream_verify_reason";
}

function normalizeTestnetVerifyDiagnosticDetail(
  value: unknown,
): TestnetVerifyDiagnosticDetailCode {
  if (
    value === "verified_payer_mismatch" ||
    value === "verify_response_malformed" ||
    value === "missing_upstream_verify_reason" ||
    value === "unrecognized_upstream_verify_reason"
  ) {
    return value;
  }
  return classifyPinnedUpstreamVerifyReason(value);
}

export function listTestnetCanonicalDiagnostics(): readonly TestnetCanonicalDiagnostic[] {
  return Object.freeze(
    (Object.keys(TESTNET_CANONICAL_DIAGNOSTICS) as TestnetCanonicalStage[]).map(
      (stage) => canonicalDiagnostic(stage),
    ),
  );
}

export class TestnetCanonicalDiagnosticError extends Error {
  readonly diagnostic: TestnetCanonicalDiagnostic;

  constructor(stage: TestnetCanonicalStage, detailCode?: unknown) {
    super("canonical testnet stage failed closed");
    this.name = "TestnetCanonicalDiagnosticError";
    this.diagnostic = canonicalDiagnostic(stage, detailCode);
    Object.freeze(this);
  }
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
  | "testnet_outcome_unknown"
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
  readonly diagnostic: TestnetCanonicalDiagnostic | null;
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
  diagnostic: TestnetCanonicalDiagnostic | null = null,
): TestnetHarnessResult {
  return Object.freeze({
    status: "blocked" as const,
    code,
    reason,
    plan,
    evidence: null,
    diagnostic,
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
      "request must use the strict schema-v2 testnet harness envelope",
    );
  }
  if (
    value.schemaVersion !== TESTNET_HARNESS_SCHEMA_VERSION ||
    value.attempt !== TESTNET_HARNESS_ATTEMPT ||
    typeof value.execute !== "boolean" ||
    (value.confirmation !== null && typeof value.confirmation !== "string") ||
    value.x402Version !== 2 ||
    value.scheme !== "exact"
  ) {
    return blocked(
      "testnet_request_invalid",
      "request schemaVersion, bounded attempt, execution flag, confirmation, x402Version or scheme is invalid",
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
  if (value.asset !== NATIVE_XLM_TESTNET_ADDRESS) {
    return blocked(
      "testnet_asset_mismatch",
      "asset must match the derived Stellar testnet native-XLM SEP-41 contract",
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
    attempt: request.attempt,
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

export function sanitizeTestnetPublicEvidence(
  value: unknown,
): TestnetPublicEvidence | null {
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

async function finalizeTestnetState(
  statePort: TestnetStatePort,
  reservationId: string,
  outcome: TestnetStateOutcome,
): Promise<boolean> {
  try {
    const finalized = await statePort.finalize(reservationId, outcome);
    return (
      finalized.status === "recorded" &&
      isNonEmptyString(finalized.code) &&
      isNonEmptyString(finalized.reason)
    );
  } catch {
    return false;
  }
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
      diagnostic: null,
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
    !isNonEmptyString(reservation.reservationId) ||
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
    await finalizeTestnetState(
      boundary.statePort,
      reservation.reservationId,
      { status: "outcome_unknown" },
    );
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
    await finalizeTestnetState(
      boundary.statePort,
      reservation.reservationId,
      { status: "outcome_unknown" },
    );
    return blocked(
      "testnet_credential_invalid",
      "credential port did not return a valid opaque ephemeral testnet credential",
      plan,
    );
  }

  let rawEvidence: unknown;
  try {
    rawEvidence = await boundary.canonicalPort.run(plan, credential);
  } catch (error) {
    await finalizeTestnetState(
      boundary.statePort,
      reservation.reservationId,
      { status: "outcome_unknown" },
    );
    return blocked(
      "testnet_outcome_unknown",
      "canonical testnet conformance outcome is unknown and must not be retried automatically",
      plan,
      error instanceof TestnetCanonicalDiagnosticError
        ? error.diagnostic
        : canonicalDiagnostic("canonical_port_unknown"),
    );
  }
  const evidence = sanitizeTestnetPublicEvidence(rawEvidence);
  if (!evidence || evidence.publicAccountId !== credential.publicAccountId) {
    await finalizeTestnetState(
      boundary.statePort,
      reservation.reservationId,
      { status: "outcome_unknown" },
    );
    return blocked(
      "testnet_outcome_unknown",
      "canonical port returned invalid evidence after an attempt; the outcome must not be retried automatically",
      plan,
      canonicalDiagnostic("public_evidence_validation"),
    );
  }
  const finalized = await finalizeTestnetState(
    boundary.statePort,
    reservation.reservationId,
    { status: "confirmed", evidence },
  );
  if (!finalized) {
    return blocked(
      "testnet_outcome_unknown",
      "canonical evidence could not be recorded atomically and must not be exposed as successful",
      plan,
      canonicalDiagnostic("state_finalization"),
    );
  }

  return Object.freeze({
    status: "completed" as const,
    code: "testnet_conformance_completed" as const,
    reason:
      "bounded canonical Stellar testnet conformance completed with public redacted evidence",
    plan,
    evidence,
    diagnostic: null,
    authorityBoundary: AUTHORITY_BOUNDARY,
  });
}
