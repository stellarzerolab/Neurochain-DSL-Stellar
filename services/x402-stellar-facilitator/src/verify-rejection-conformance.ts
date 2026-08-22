import type {
  PaymentPayload,
  PaymentRequirements,
  VerifyResponse,
} from "@x402/core/types";
import type { FacilitatorStellarSigner } from "@x402/stellar";
import { ExactStellarScheme } from "@x402/stellar/exact/facilitator";

import {
  INERT_TEST_SIGNER_ADDRESS,
  PINNED_SOURCE_PACKAGES,
} from "./supported-conformance.js";

export const VERIFY_REJECTION_FIXTURE_SCHEMA_VERSION = 1 as const;
export const VERIFY_REJECTION_CASE_IDS = Object.freeze([
  "invalid_x402_version",
  "unsupported_scheme",
  "network_mismatch",
  "invalid_network",
  "missing_transaction",
  "malformed_transaction_xdr",
  "wrong_operation",
  "unsafe_transaction_source",
  "wrong_asset",
  "wrong_function_name",
  "facilitator_is_payer",
  "wrong_recipient",
  "wrong_amount",
] as const);
export const VERIFY_APPROVAL_BLOCKED_CASES = Object.freeze([
  Object.freeze({
    id: "auth_entry_structure",
    code: "approval_blocked",
    reason:
      "upstream validates auth-entry structure only after Stellar RPC simulation",
  }),
  Object.freeze({
    id: "auth_entry_expiration",
    code: "approval_blocked",
    reason:
      "upstream derives the expiration boundary from live ledger state after simulation",
  }),
  Object.freeze({
    id: "auth_entry_sub_invocation",
    code: "approval_blocked",
    reason:
      "upstream validates auth-entry sub-invocations only after Stellar RPC simulation",
  }),
  Object.freeze({
    id: "auth_signature_status",
    code: "approval_blocked",
    reason:
      "signature-status conformance requires simulated auth entries and a separately approved signer fixture",
  }),
  Object.freeze({
    id: "custom_check_auth",
    code: "approval_blocked",
    reason:
      "custom __check_auth conformance requires approved contract-account credentials and RPC simulation",
  }),
] as const);

export type VerifyRejectionCaseId =
  (typeof VERIFY_REJECTION_CASE_IDS)[number];

const TEST_NETWORK = "stellar:testnet" as const;
const INVALID_NETWORK = "stellar:unknown" as PaymentRequirements["network"];
const TEST_PAYER =
  "GAAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQDZ7H";
const TEST_PAY_TO =
  "GABAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEJXA";
const TEST_ASSET =
  "CABQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGCK3";
const TEST_AMOUNT = "10000000";
const OFFLINE_RPC_SENTINEL = "http://127.0.0.1:9";

// These are deterministic, unsigned transaction envelopes generated from
// fixed public bytes with the transitive Stellar SDK already pinned by
// @x402/stellar@2.23.0. No keypair, secret or signature was created.
const TRANSACTION_XDR = Object.freeze({
  validShape:
    "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAAAAAAAAQAAAAEAAAAAAAAAAAAAAABqidt9AAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMAAAAIdHJhbnNmZXIAAAADAAAAEgAAAAAAAAAAAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEAAAASAAAAAAAAAAACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgAAAAoAAAAAAAAAAAAAAAAAmJaAAAAAAAAAAAAAAAAA",
  wrongOperation:
    "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAAAAAAAAQAAAAEAAAAAAAAAAAAAAABqidt9AAAAAAAAAAEAAAAAAAAACwAAAAAAAAACAAAAAAAAAAA=",
  unsafeSource:
    "AAAAAgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGQAAAAAAAAAAQAAAAEAAAAAAAAAAAAAAABqidt9AAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMAAAAIdHJhbnNmZXIAAAADAAAAEgAAAAAAAAAAAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEAAAASAAAAAAAAAAACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgAAAAoAAAAAAAAAAAAAAAAAmJaAAAAAAAAAAAAAAAAA",
  wrongAsset:
    "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAAAAAAAAQAAAAEAAAAAAAAAAAAAAABqidt9AAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAAIdHJhbnNmZXIAAAADAAAAEgAAAAAAAAAAAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEAAAASAAAAAAAAAAACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgAAAAoAAAAAAAAAAAAAAAAAmJaAAAAAAAAAAAAAAAAA",
  wrongFunction:
    "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAAAAAAAAQAAAAEAAAAAAAAAAAAAAABqidt9AAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMAAAAHYXBwcm92ZQAAAAADAAAAEgAAAAAAAAAAAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEAAAASAAAAAAAAAAACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgAAAAoAAAAAAAAAAAAAAAAAmJaAAAAAAAAAAAAAAAAA",
  facilitatorPayer:
    "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAAAAAAAAQAAAAEAAAAAAAAAAAAAAABqidt9AAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMAAAAIdHJhbnNmZXIAAAADAAAAEgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAASAAAAAAAAAAACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgAAAAoAAAAAAAAAAAAAAAAAmJaAAAAAAAAAAAAAAAAA",
  wrongRecipient:
    "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAAAAAAAAQAAAAEAAAAAAAAAAAAAAABqidt9AAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMAAAAIdHJhbnNmZXIAAAADAAAAEgAAAAAAAAAAAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEAAAASAAAAAAAAAAAFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQAAAAoAAAAAAAAAAAAAAAAAmJaAAAAAAAAAAAAAAAAA",
  wrongAmount:
    "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAAAAAAAAQAAAAEAAAAAAAAAAAAAAABqidt9AAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMAAAAIdHJhbnNmZXIAAAADAAAAEgAAAAAAAAAAAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEAAAASAAAAAAAAAAACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgAAAAoAAAAAAAAAAAAAAAAAmJaBAAAAAAAAAAAAAAAA",
});

export const VERIFY_EXPECTED_INVALID_REASON: Readonly<
  Record<VerifyRejectionCaseId, string>
> = Object.freeze({
    invalid_x402_version: "invalid_x402_version",
    unsupported_scheme: "unsupported_scheme",
    network_mismatch: "network_mismatch",
    invalid_network: "invalid_network",
    missing_transaction: "invalid_exact_stellar_payload_malformed",
    malformed_transaction_xdr: "invalid_exact_stellar_payload_malformed",
    wrong_operation: "invalid_exact_stellar_payload_wrong_operation",
    unsafe_transaction_source:
      "invalid_exact_stellar_payload_unsafe_tx_or_op_source",
    wrong_asset: "invalid_exact_stellar_payload_wrong_asset",
    wrong_function_name:
      "invalid_exact_stellar_payload_wrong_function_name",
    facilitator_is_payer:
      "invalid_exact_stellar_payload_facilitator_is_payer",
    wrong_recipient: "invalid_exact_stellar_payload_wrong_recipient",
    wrong_amount: "invalid_exact_stellar_payload_wrong_amount",
  });

export interface VerifyRejectionConformanceSnapshot {
  readonly schemaVersion: typeof VERIFY_REJECTION_FIXTURE_SCHEMA_VERSION;
  readonly sourcePackages: typeof PINNED_SOURCE_PACKAGES;
  readonly upstreamApi: "ExactStellarScheme.verify";
  readonly cases: readonly {
    readonly id: VerifyRejectionCaseId;
    readonly expectedInvalidReason: string;
    readonly response: VerifyResponse;
  }[];
  readonly approvalBlocked: typeof VERIFY_APPROVAL_BLOCKED_CASES;
  readonly authorityBoundary: {
    readonly networkAccessAllowed: false;
    readonly credentialUseAllowed: false;
    readonly keypairCreated: false;
    readonly signingAllowed: false;
    readonly settlementAllowed: false;
    readonly transactionSubmitAllowed: false;
    readonly actionPlanSubmitAllowed: false;
    readonly verifyMethodCalls: number;
    readonly signerMethodCalls: number;
    readonly networkFetchCalls: number;
    readonly settleMethodCalls: number;
  };
}

export type VerifyRejectionConformanceCode =
  | "verify_rejection_conformance_ready"
  | "verify_rejection_fixture_invalid"
  | "source_package_drift"
  | "verify_rejection_wire_drift"
  | "approval_boundary_drift"
  | "authority_boundary_violated"
  | "upstream_verify_execution_failed";

export interface VerifyRejectionConformanceResult {
  readonly status: "ready" | "invalid";
  readonly code: VerifyRejectionConformanceCode;
  readonly reason: string;
}

const FIXTURE_KEYS = Object.freeze([
  "approvalBlocked",
  "authorityBoundary",
  "cases",
  "schemaVersion",
  "sourcePackages",
  "upstreamApi",
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

function invalid(
  code: Exclude<
    VerifyRejectionConformanceCode,
    "verify_rejection_conformance_ready"
  >,
  reason: string,
): VerifyRejectionConformanceResult {
  return Object.freeze({ status: "invalid" as const, code, reason });
}

function hasStrictFixtureEnvelope(
  value: unknown,
): value is VerifyRejectionConformanceSnapshot {
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

function createRequirements(
  overrides: Partial<PaymentRequirements> = {},
): PaymentRequirements {
  return {
    scheme: "exact",
    network: TEST_NETWORK,
    asset: TEST_ASSET,
    amount: TEST_AMOUNT,
    payTo: TEST_PAY_TO,
    maxTimeoutSeconds: 300,
    extra: {},
    ...overrides,
  };
}

function createPayload(
  transaction: string,
  accepted: PaymentRequirements = createRequirements(),
): PaymentPayload {
  return {
    x402Version: 2,
    accepted,
    payload: { transaction },
  };
}

function normalizeVerifyResponse(response: VerifyResponse): VerifyResponse {
  return Object.fromEntries(
    Object.entries(response).filter(([, value]) => value !== undefined),
  ) as VerifyResponse;
}

export function buildVerifyRejectionCase(
  id: VerifyRejectionCaseId,
): readonly [PaymentPayload, PaymentRequirements] {
  const requirements = createRequirements();
  switch (id) {
    case "invalid_x402_version":
      return [
        { ...createPayload(TRANSACTION_XDR.validShape), x402Version: 1 },
        requirements,
      ];
    case "unsupported_scheme":
      return [
        createPayload(TRANSACTION_XDR.validShape),
        createRequirements({ scheme: "upto" }),
      ];
    case "network_mismatch":
      return [
        createPayload(TRANSACTION_XDR.validShape),
        createRequirements({ network: "stellar:pubnet" }),
      ];
    case "invalid_network": {
      const invalidRequirements = createRequirements({
        network: INVALID_NETWORK,
      });
      return [
        createPayload(TRANSACTION_XDR.validShape, invalidRequirements),
        invalidRequirements,
      ];
    }
    case "missing_transaction":
      return [
        { ...createPayload(TRANSACTION_XDR.validShape), payload: {} },
        requirements,
      ];
    case "malformed_transaction_xdr":
      return [createPayload("not-xdr"), requirements];
    case "wrong_operation":
      return [createPayload(TRANSACTION_XDR.wrongOperation), requirements];
    case "unsafe_transaction_source":
      return [createPayload(TRANSACTION_XDR.unsafeSource), requirements];
    case "wrong_asset":
      return [createPayload(TRANSACTION_XDR.wrongAsset), requirements];
    case "wrong_function_name":
      return [createPayload(TRANSACTION_XDR.wrongFunction), requirements];
    case "facilitator_is_payer":
      return [createPayload(TRANSACTION_XDR.facilitatorPayer), requirements];
    case "wrong_recipient":
      return [createPayload(TRANSACTION_XDR.wrongRecipient), requirements];
    case "wrong_amount":
      return [createPayload(TRANSACTION_XDR.wrongAmount), requirements];
  }
}

export async function buildVerifyRejectionConformanceSnapshot(): Promise<VerifyRejectionConformanceSnapshot> {
  let verifyMethodCalls = 0;
  let signerMethodCalls = 0;
  let networkFetchCalls = 0;
  const settleMethodCalls = 0;
  const originalFetch = globalThis.fetch;
  const originalConsoleError = console.error;
  const signer = createInertSigner(() => {
    signerMethodCalls += 1;
    throw new Error("offline_verify_rejection_signer_invoked");
  });
  const scheme = new ExactStellarScheme([signer], {
    rpcConfig: { url: OFFLINE_RPC_SENTINEL },
  });

  globalThis.fetch = (async () => {
    networkFetchCalls += 1;
    throw new Error("offline_verify_rejection_network_forbidden");
  }) as typeof fetch;
  // The malformed-XDR branch emits an upstream diagnostic before returning its
  // stable invalidReason. Keep the deterministic test output quiet without
  // changing or replacing upstream verification semantics.
  console.error = () => undefined;

  try {
    const cases = [];
    for (const id of VERIFY_REJECTION_CASE_IDS) {
      const [payload, requirements] = buildVerifyRejectionCase(id);
      verifyMethodCalls += 1;
      const response = await scheme.verify(payload, requirements);
      cases.push(
        Object.freeze({
          id,
          expectedInvalidReason: VERIFY_EXPECTED_INVALID_REASON[id],
          response: normalizeVerifyResponse(response),
        }),
      );
    }

    return Object.freeze({
      schemaVersion: VERIFY_REJECTION_FIXTURE_SCHEMA_VERSION,
      sourcePackages: PINNED_SOURCE_PACKAGES,
      upstreamApi: "ExactStellarScheme.verify" as const,
      cases: Object.freeze(cases),
      approvalBlocked: VERIFY_APPROVAL_BLOCKED_CASES,
      authorityBoundary: Object.freeze({
        networkAccessAllowed: false as const,
        credentialUseAllowed: false as const,
        keypairCreated: false as const,
        signingAllowed: false as const,
        settlementAllowed: false as const,
        transactionSubmitAllowed: false as const,
        actionPlanSubmitAllowed: false as const,
        verifyMethodCalls,
        signerMethodCalls,
        networkFetchCalls,
        settleMethodCalls,
      }),
    });
  } finally {
    globalThis.fetch = originalFetch;
    console.error = originalConsoleError;
  }
}

export async function evaluateVerifyRejectionConformance(
  expected: unknown,
): Promise<VerifyRejectionConformanceResult> {
  if (!hasStrictFixtureEnvelope(expected)) {
    return invalid(
      "verify_rejection_fixture_invalid",
      "expected fixture must use the strict verify-rejection envelope",
    );
  }
  if (expected.schemaVersion !== VERIFY_REJECTION_FIXTURE_SCHEMA_VERSION) {
    return invalid(
      "verify_rejection_fixture_invalid",
      `expected schemaVersion ${VERIFY_REJECTION_FIXTURE_SCHEMA_VERSION}`,
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

  let actual: VerifyRejectionConformanceSnapshot;
  try {
    actual = await buildVerifyRejectionConformanceSnapshot();
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    return invalid(
      "upstream_verify_execution_failed",
      `upstream offline verify rejection execution failed: ${message || "unknown error"}`,
    );
  }

  if (canonicalJson(expected.cases) !== canonicalJson(actual.cases)) {
    return invalid(
      "verify_rejection_wire_drift",
      "upstream verify rejection responses differ from the deterministic fixture",
    );
  }
  if (
    canonicalJson(expected.approvalBlocked) !==
    canonicalJson(actual.approvalBlocked)
  ) {
    return invalid(
      "approval_boundary_drift",
      "credential, RPC and auth-entry approval boundaries differ from the fixture",
    );
  }
  if (
    expected.upstreamApi !== actual.upstreamApi ||
    canonicalJson(expected.authorityBoundary) !==
      canonicalJson(actual.authorityBoundary) ||
    actual.authorityBoundary.verifyMethodCalls !==
      VERIFY_REJECTION_CASE_IDS.length ||
    actual.authorityBoundary.signerMethodCalls !== 0 ||
    actual.authorityBoundary.networkFetchCalls !== 0 ||
    actual.authorityBoundary.settleMethodCalls !== 0
  ) {
    return invalid(
      "authority_boundary_violated",
      "offline rejection conformance must not use network, credentials, signing, settlement or submit",
    );
  }

  return Object.freeze({
    status: "ready" as const,
    code: "verify_rejection_conformance_ready" as const,
    reason:
      "upstream Stellar exact verify rejects every safe pre-network case with the pinned response",
  });
}
