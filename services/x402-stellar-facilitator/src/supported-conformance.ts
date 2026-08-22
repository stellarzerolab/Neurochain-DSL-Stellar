import { x402Facilitator } from "@x402/core/facilitator";
import type { FacilitatorStellarSigner } from "@x402/stellar";
import { ExactStellarScheme } from "@x402/stellar/exact/facilitator";

export const SUPPORTED_FIXTURE_SCHEMA_VERSION = 1 as const;
export const PINNED_SOURCE_PACKAGES = Object.freeze({
  "@x402/core": "2.23.0",
  "@x402/stellar": "2.23.0",
});
export const SUPPORTED_NETWORKS = Object.freeze([
  "stellar:testnet",
  "stellar:pubnet",
] as const);

// This is the canonical all-zero Ed25519 public-key encoding. It is public,
// deterministic and has no corresponding secret in this workspace.
export const INERT_TEST_SIGNER_ADDRESS =
  "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF" as const;

export interface SupportedConformanceSnapshot {
  readonly schemaVersion: typeof SUPPORTED_FIXTURE_SCHEMA_VERSION;
  readonly sourcePackages: typeof PINNED_SOURCE_PACKAGES;
  readonly response: ReturnType<x402Facilitator["getSupported"]>;
  readonly authorityBoundary: {
    readonly networkAccessAllowed: false;
    readonly credentialUseAllowed: false;
    readonly keypairCreated: false;
    readonly signingAllowed: false;
    readonly verifyAllowed: false;
    readonly settlementAllowed: false;
    readonly transactionSubmitAllowed: false;
    readonly actionPlanSubmitAllowed: false;
    readonly signerMethodCalls: number;
    readonly verifyMethodCalls: number;
    readonly settleMethodCalls: number;
  };
}

export type SupportedConformanceCode =
  | "supported_conformance_ready"
  | "supported_fixture_invalid"
  | "source_package_drift"
  | "supported_wire_drift"
  | "authority_boundary_violated";

export interface SupportedConformanceResult {
  readonly status: "ready" | "invalid";
  readonly code: SupportedConformanceCode;
  readonly reason: string;
}

const FIXTURE_KEYS = Object.freeze([
  "authorityBoundary",
  "response",
  "schemaVersion",
  "sourcePackages",
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
  code: Exclude<SupportedConformanceCode, "supported_conformance_ready">,
  reason: string,
) {
  return Object.freeze({ status: "invalid" as const, code, reason });
}

function hasStrictFixtureEnvelope(value: unknown): value is SupportedConformanceSnapshot {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const keys = Object.keys(value).sort();
  return canonicalJson(keys) === canonicalJson(FIXTURE_KEYS);
}

function createInertSigner(onInvocation: () => never): FacilitatorStellarSigner {
  return Object.freeze({
    address: INERT_TEST_SIGNER_ADDRESS,
    signAuthEntry: async () => onInvocation(),
    signTransaction: async () => onInvocation(),
  });
}

export function buildSupportedConformanceSnapshot(): SupportedConformanceSnapshot {
  let signerMethodCalls = 0;
  let verifyMethodCalls = 0;
  let settleMethodCalls = 0;
  const signer = createInertSigner(() => {
    signerMethodCalls += 1;
    throw new Error("offline_supported_signer_invoked");
  });
  const scheme = new ExactStellarScheme([signer]);
  Object.defineProperties(scheme, {
    verify: {
      value: async () => {
        verifyMethodCalls += 1;
        throw new Error("offline_supported_verify_invoked");
      },
    },
    settle: {
      value: async () => {
        settleMethodCalls += 1;
        throw new Error("offline_supported_settle_invoked");
      },
    },
  });
  const facilitator = new x402Facilitator().register(
    [...SUPPORTED_NETWORKS],
    scheme,
  );
  const response = facilitator.getSupported();

  return Object.freeze({
    schemaVersion: SUPPORTED_FIXTURE_SCHEMA_VERSION,
    sourcePackages: PINNED_SOURCE_PACKAGES,
    response,
    authorityBoundary: Object.freeze({
      networkAccessAllowed: false,
      credentialUseAllowed: false,
      keypairCreated: false,
      signingAllowed: false,
      verifyAllowed: false,
      settlementAllowed: false,
      transactionSubmitAllowed: false,
      actionPlanSubmitAllowed: false,
      signerMethodCalls,
      verifyMethodCalls,
      settleMethodCalls,
    }),
  });
}

export function evaluateSupportedConformance(
  expected: unknown,
): SupportedConformanceResult {
  if (!hasStrictFixtureEnvelope(expected)) {
    return invalid(
      "supported_fixture_invalid",
      "expected fixture must use the strict supported-conformance envelope",
    );
  }
  if (expected.schemaVersion !== SUPPORTED_FIXTURE_SCHEMA_VERSION) {
    return invalid(
      "supported_fixture_invalid",
      `expected schemaVersion ${SUPPORTED_FIXTURE_SCHEMA_VERSION}`,
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

  const actual = buildSupportedConformanceSnapshot();
  if (canonicalJson(expected.response) !== canonicalJson(actual.response)) {
    return invalid(
      "supported_wire_drift",
      "upstream getSupported response differs from the deterministic wire fixture",
    );
  }
  if (
    canonicalJson(expected.authorityBoundary) !==
      canonicalJson(actual.authorityBoundary) ||
    actual.authorityBoundary.signerMethodCalls !== 0 ||
    actual.authorityBoundary.verifyMethodCalls !== 0 ||
    actual.authorityBoundary.settleMethodCalls !== 0
  ) {
    return invalid(
      "authority_boundary_violated",
      "supported conformance must not use network, credentials, signing, verify, settle or submit",
    );
  }

  return Object.freeze({
    status: "ready" as const,
    code: "supported_conformance_ready" as const,
    reason: "canonical upstream Stellar exact supported response matches the offline fixture",
  });
}
