import { createHash } from "node:crypto";

import { x402Facilitator } from "@x402/core/facilitator";
import type {
  PaymentPayload,
  PaymentRequirements,
  SettleResponse,
} from "@x402/core/types";
import type { FacilitatorStellarSigner } from "@x402/stellar";
import { ExactStellarScheme } from "@x402/stellar/exact/facilitator";

import {
  INERT_TEST_SIGNER_ADDRESS,
  PINNED_SOURCE_PACKAGES,
} from "./supported-conformance.js";
import {
  buildVerifyRejectionCase,
  VERIFY_EXPECTED_INVALID_REASON,
  type VerifyRejectionCaseId,
} from "./verify-rejection-conformance.js";

export const SETTLE_REJECTION_FIXTURE_SCHEMA_VERSION = 1 as const;
export const SETTLE_REJECTION_CASE_IDS = Object.freeze([
  "invalid_x402_version",
  "unsupported_scheme",
  "network_mismatch",
  "missing_transaction",
  "malformed_transaction_xdr",
  "wrong_operation",
  "unsafe_transaction_source",
  "wrong_asset",
  "wrong_function_name",
  "facilitator_is_payer",
  "wrong_recipient",
  "wrong_amount",
] as const satisfies readonly VerifyRejectionCaseId[]);
export const SETTLE_ADMISSION_CASES = Object.freeze([
  Object.freeze({
    id: "unverified",
    code: "settlement_unverified",
    reason:
      "a payment without a separately established verification state cannot enter settlement",
  }),
  Object.freeze({
    id: "duplicate",
    code: "settlement_duplicate",
    reason:
      "an idempotency key already reserved by the service boundary cannot enter settlement again",
  }),
  Object.freeze({
    id: "replay",
    code: "settlement_replay",
    reason:
      "a consumed payment digest cannot enter settlement again under a new request id",
  }),
] as const);
export const SETTLE_APPROVAL_BLOCKED_CASES = Object.freeze([
  Object.freeze({
    id: "valid_exact_settlement",
    code: "approval_blocked",
    reason:
      "a valid exact settlement requires Stellar RPC simulation, an approved signer and transaction submission",
  }),
  Object.freeze({
    id: "fee_bump_settlement",
    code: "approval_blocked",
    reason:
      "fee-bump settlement requires approved signer credentials, RPC state and transaction submission",
  }),
  Object.freeze({
    id: "canonical_client_roundtrip",
    code: "approval_blocked",
    reason:
      "canonical-client settlement conformance requires an explicitly approved network and credential test",
  }),
] as const);
export const SETTLE_SERVICE_BOUNDARY_PENDING = Object.freeze([
  Object.freeze({
    id: "persistent_idempotency_recovery",
    code: "service_boundary_pending",
    reason:
      "the upstream scheme exposes no persistent replay store; restart recovery belongs to the versioned TypeScript and Rust service boundary",
  }),
  Object.freeze({
    id: "unknown_outcome_recovery",
    code: "service_boundary_pending",
    reason:
      "outcome-unknown recovery requires durable service state and must not be inferred from an in-memory conformance fixture",
  }),
  Object.freeze({
    id: "invalid_network_exception_mapping",
    code: "service_boundary_pending",
    reason:
      "upstream settle rejects an unknown network before its internal result mapping; the pure service handler must map that exception fail-closed",
  }),
] as const);

type SettleRejectionCaseId = (typeof SETTLE_REJECTION_CASE_IDS)[number];
type SettleAdmissionCase = (typeof SETTLE_ADMISSION_CASES)[number];

interface OfflineSettleAdmissionState {
  readonly verifiedDigests: ReadonlySet<string>;
  readonly reservedRequests: ReadonlyMap<string, string>;
  readonly consumedDigests: ReadonlySet<string>;
}

const TEST_NETWORK = "stellar:testnet" as const;
const OFFLINE_RPC_SENTINEL = "https://127.0.0.1:9";

export interface SettleRejectionConformanceSnapshot {
  readonly schemaVersion: typeof SETTLE_REJECTION_FIXTURE_SCHEMA_VERSION;
  readonly sourcePackages: typeof PINNED_SOURCE_PACKAGES;
  readonly upstreamApis: readonly [
    "ExactStellarScheme.settle",
    "x402Facilitator.onBeforeSettle",
  ];
  readonly upstreamRejections: readonly {
    readonly id: SettleRejectionCaseId;
    readonly expectedErrorReason: string;
    readonly response: SettleResponse;
  }[];
  readonly admissionRejections: readonly {
    readonly id: SettleAdmissionCase["id"];
    readonly code: SettleAdmissionCase["code"];
    readonly reason: string;
    readonly upstreamError: string;
  }[];
  readonly approvalBlocked: typeof SETTLE_APPROVAL_BLOCKED_CASES;
  readonly serviceBoundaryPending: typeof SETTLE_SERVICE_BOUNDARY_PENDING;
  readonly authorityBoundary: {
    readonly networkAccessAllowed: false;
    readonly credentialUseAllowed: false;
    readonly keypairCreated: false;
    readonly custodyAllowed: false;
    readonly signingAllowed: false;
    readonly liveSettlementAllowed: false;
    readonly transactionSubmitAllowed: false;
    readonly actionPlanSubmitAllowed: false;
    readonly exactSettleMethodCalls: number;
    readonly admissionHookCalls: number;
    readonly guardedSchemeSettleCalls: number;
    readonly signerMethodCalls: number;
    readonly networkFetchCalls: number;
  };
}

export type SettleRejectionConformanceCode =
  | "settle_rejection_conformance_ready"
  | "settle_rejection_fixture_invalid"
  | "source_package_drift"
  | "settle_rejection_wire_drift"
  | "settle_admission_drift"
  | "settle_approval_boundary_drift"
  | "settle_service_boundary_drift"
  | "authority_boundary_violated"
  | "upstream_settle_execution_failed";

export interface SettleRejectionConformanceResult {
  readonly status: "ready" | "invalid";
  readonly code: SettleRejectionConformanceCode;
  readonly reason: string;
}

const FIXTURE_KEYS = Object.freeze([
  "admissionRejections",
  "approvalBlocked",
  "authorityBoundary",
  "schemaVersion",
  "serviceBoundaryPending",
  "sourcePackages",
  "upstreamApis",
  "upstreamRejections",
]);

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

export function computeOfflineSettleRequestDigest(
  payload: PaymentPayload,
  requirements: PaymentRequirements,
): string {
  return createHash("sha256")
    .update(canonicalJson({ payload, requirements }), "utf8")
    .digest("hex");
}

function invalid(
  code: Exclude<
    SettleRejectionConformanceCode,
    "settle_rejection_conformance_ready"
  >,
  reason: string,
): SettleRejectionConformanceResult {
  return Object.freeze({ status: "invalid" as const, code, reason });
}

function hasStrictFixtureEnvelope(
  value: unknown,
): value is SettleRejectionConformanceSnapshot {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  return canonicalJson(Object.keys(value).sort()) === canonicalJson(FIXTURE_KEYS);
}

function createInertSigner(onInvocation: () => never): FacilitatorStellarSigner {
  return Object.freeze({
    address: INERT_TEST_SIGNER_ADDRESS,
    signAuthEntry: async () => onInvocation(),
    signTransaction: async () => onInvocation(),
  });
}

function normalizeSettleResponse(response: SettleResponse): SettleResponse {
  return Object.fromEntries(
    Object.entries(response).filter(([, value]) => value !== undefined),
  ) as SettleResponse;
}

function createAdmissionState(
  admissionCase: SettleAdmissionCase,
  requestId: string,
  digest: string,
): OfflineSettleAdmissionState {
  switch (admissionCase.id) {
    case "unverified":
      return {
        verifiedDigests: new Set(),
        reservedRequests: new Map(),
        consumedDigests: new Set(),
      };
    case "duplicate":
      return {
        verifiedDigests: new Set([digest]),
        reservedRequests: new Map([[requestId, digest]]),
        consumedDigests: new Set(),
      };
    case "replay":
      return {
        verifiedDigests: new Set([digest]),
        reservedRequests: new Map(),
        consumedDigests: new Set([digest]),
      };
  }
}

function evaluateAdmissionState(
  state: OfflineSettleAdmissionState,
  requestId: string,
  digest: string,
): SettleAdmissionCase["code"] | undefined {
  if (!state.verifiedDigests.has(digest)) {
    return "settlement_unverified";
  }
  if (state.consumedDigests.has(digest)) {
    return "settlement_replay";
  }
  if (state.reservedRequests.has(requestId)) {
    return "settlement_duplicate";
  }
  return undefined;
}

async function runAdmissionRejection(
  admissionCase: SettleAdmissionCase,
  signer: FacilitatorStellarSigner,
  onHookInvocation: () => void,
  onSchemeSettleInvocation: () => never,
): Promise<{
  readonly id: SettleAdmissionCase["id"];
  readonly code: SettleAdmissionCase["code"];
  readonly reason: string;
  readonly upstreamError: string;
}> {
  const scheme = new ExactStellarScheme([signer], {
    rpcConfig: { url: OFFLINE_RPC_SENTINEL },
  });
  Object.defineProperty(scheme, "settle", {
    value: async () => onSchemeSettleInvocation(),
  });
  const requestId = `offline-${admissionCase.id}`;
  const [payload, requirements] = buildVerifyRejectionCase("wrong_amount");
  const digest = computeOfflineSettleRequestDigest(payload, requirements);
  const admissionState = createAdmissionState(admissionCase, requestId, digest);
  const facilitator = new x402Facilitator()
    .register(TEST_NETWORK, scheme)
    .onBeforeSettle(async ({ paymentPayload, requirements: paymentRequirements }) => {
      onHookInvocation();
      const decision = evaluateAdmissionState(
        admissionState,
        requestId,
        computeOfflineSettleRequestDigest(paymentPayload, paymentRequirements),
      );
      if (decision === undefined) {
        return undefined;
      }
      return { abort: true as const, reason: decision };
    });

  try {
    await facilitator.settle(payload, requirements);
  } catch (error: unknown) {
    const upstreamError = error instanceof Error ? error.message : String(error);
    return Object.freeze({
      id: admissionCase.id,
      code: admissionCase.code,
      reason: admissionCase.reason,
      upstreamError,
    });
  }
  throw new Error(`offline_settle_admission_${admissionCase.id}_accepted`);
}

export async function buildSettleRejectionConformanceSnapshot(): Promise<SettleRejectionConformanceSnapshot> {
  let exactSettleMethodCalls = 0;
  let admissionHookCalls = 0;
  let guardedSchemeSettleCalls = 0;
  let signerMethodCalls = 0;
  let networkFetchCalls = 0;
  const originalFetch = globalThis.fetch;
  const originalConsoleError = console.error;
  const signer = createInertSigner(() => {
    signerMethodCalls += 1;
    throw new Error("offline_settle_rejection_signer_invoked");
  });
  const scheme = new ExactStellarScheme([signer], {
    rpcConfig: { url: OFFLINE_RPC_SENTINEL },
  });

  globalThis.fetch = (async () => {
    networkFetchCalls += 1;
    throw new Error("offline_settle_rejection_network_forbidden");
  }) as typeof fetch;
  console.error = () => undefined;

  try {
    const upstreamRejections = [];
    for (const id of SETTLE_REJECTION_CASE_IDS) {
      const [payload, requirements] = buildVerifyRejectionCase(id);
      exactSettleMethodCalls += 1;
      const response = await scheme.settle(payload, requirements);
      upstreamRejections.push(
        Object.freeze({
          id,
          expectedErrorReason: VERIFY_EXPECTED_INVALID_REASON[id],
          response: normalizeSettleResponse(response),
        }),
      );
    }

    const admissionRejections = [];
    for (const admissionCase of SETTLE_ADMISSION_CASES) {
      admissionRejections.push(
        await runAdmissionRejection(
          admissionCase,
          signer,
          () => {
            admissionHookCalls += 1;
          },
          () => {
            guardedSchemeSettleCalls += 1;
            throw new Error("offline_settle_admission_scheme_invoked");
          },
        ),
      );
    }

    return Object.freeze({
      schemaVersion: SETTLE_REJECTION_FIXTURE_SCHEMA_VERSION,
      sourcePackages: PINNED_SOURCE_PACKAGES,
      upstreamApis: Object.freeze([
        "ExactStellarScheme.settle",
        "x402Facilitator.onBeforeSettle",
      ] as const),
      upstreamRejections: Object.freeze(upstreamRejections),
      admissionRejections: Object.freeze(admissionRejections),
      approvalBlocked: SETTLE_APPROVAL_BLOCKED_CASES,
      serviceBoundaryPending: SETTLE_SERVICE_BOUNDARY_PENDING,
      authorityBoundary: Object.freeze({
        networkAccessAllowed: false as const,
        credentialUseAllowed: false as const,
        keypairCreated: false as const,
        custodyAllowed: false as const,
        signingAllowed: false as const,
        liveSettlementAllowed: false as const,
        transactionSubmitAllowed: false as const,
        actionPlanSubmitAllowed: false as const,
        exactSettleMethodCalls,
        admissionHookCalls,
        guardedSchemeSettleCalls,
        signerMethodCalls,
        networkFetchCalls,
      }),
    });
  } finally {
    globalThis.fetch = originalFetch;
    console.error = originalConsoleError;
  }
}

export async function evaluateSettleRejectionConformance(
  expected: unknown,
): Promise<SettleRejectionConformanceResult> {
  if (!hasStrictFixtureEnvelope(expected)) {
    return invalid(
      "settle_rejection_fixture_invalid",
      "expected fixture must use the strict settle-rejection envelope",
    );
  }
  if (expected.schemaVersion !== SETTLE_REJECTION_FIXTURE_SCHEMA_VERSION) {
    return invalid(
      "settle_rejection_fixture_invalid",
      `expected schemaVersion ${SETTLE_REJECTION_FIXTURE_SCHEMA_VERSION}`,
    );
  }
  if (
    canonicalJson(expected.sourcePackages) !== canonicalJson(PINNED_SOURCE_PACKAGES)
  ) {
    return invalid(
      "source_package_drift",
      "fixture package versions differ from the approved direct pins",
    );
  }

  let actual: SettleRejectionConformanceSnapshot;
  try {
    actual = await buildSettleRejectionConformanceSnapshot();
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    return invalid(
      "upstream_settle_execution_failed",
      `upstream offline settle rejection execution failed: ${message || "unknown error"}`,
    );
  }

  if (
    canonicalJson(expected.upstreamRejections) !==
      canonicalJson(actual.upstreamRejections) ||
    canonicalJson(expected.upstreamApis) !== canonicalJson(actual.upstreamApis)
  ) {
    return invalid(
      "settle_rejection_wire_drift",
      "upstream settle rejection responses differ from the deterministic fixture",
    );
  }
  if (
    canonicalJson(expected.admissionRejections) !==
    canonicalJson(actual.admissionRejections)
  ) {
    return invalid(
      "settle_admission_drift",
      "unverified, duplicate or replay admission results differ from the fixture",
    );
  }
  if (
    canonicalJson(expected.approvalBlocked) !==
    canonicalJson(actual.approvalBlocked)
  ) {
    return invalid(
      "settle_approval_boundary_drift",
      "credential, RPC and live-settlement approval boundaries differ from the fixture",
    );
  }
  if (
    canonicalJson(expected.serviceBoundaryPending) !==
    canonicalJson(actual.serviceBoundaryPending)
  ) {
    return invalid(
      "settle_service_boundary_drift",
      "persistent replay and fail-closed handler responsibilities differ from the fixture",
    );
  }
  if (
    canonicalJson(expected.authorityBoundary) !==
      canonicalJson(actual.authorityBoundary) ||
    actual.authorityBoundary.exactSettleMethodCalls !==
      SETTLE_REJECTION_CASE_IDS.length ||
    actual.authorityBoundary.admissionHookCalls !==
      SETTLE_ADMISSION_CASES.length ||
    actual.authorityBoundary.guardedSchemeSettleCalls !== 0 ||
    actual.authorityBoundary.signerMethodCalls !== 0 ||
    actual.authorityBoundary.networkFetchCalls !== 0
  ) {
    return invalid(
      "authority_boundary_violated",
      "offline settle rejection must not use network, credentials, signing, custody, live settlement or submit",
    );
  }

  return Object.freeze({
    status: "ready" as const,
    code: "settle_rejection_conformance_ready" as const,
    reason:
      "upstream settle and admission hooks reject every pinned offline case before network, signer or submit",
  });
}
