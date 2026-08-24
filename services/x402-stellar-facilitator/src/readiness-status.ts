const EXPECTED_PACKAGES = Object.freeze({
  "@x402/core": "2.23.0",
  "@x402/stellar": "2.23.0",
  "@types/node": "24.13.3",
  typescript: "5.9.3",
});

const EXPECTED_CASE_STATUS: Readonly<Record<string, ReadinessStatus>> = Object.freeze({
  standard_surface: "verified_offline",
  supported_exact_both_networks: "verified_offline",
  supported_are_fees_sponsored: "verified_offline",
  wire_v2_payload_transaction: "verified_offline",
  exact_canonical_client_e2e: "approval_blocked",
  exact_keypair_auth: "approval_blocked",
  exact_custom_check_auth: "approval_blocked",
  exact_sep41_seven_decimals: "approval_blocked",
  exact_tampered_signature_reject: "approval_blocked",
  exact_asset_mismatch_reject: "verified_offline",
  exact_amount_mismatch_reject: "verified_offline",
  exact_recipient_mismatch_reject: "verified_offline",
  exact_expired_auth_reject: "approval_blocked",
  exact_replay_reject: "service_boundary_pending",
  exact_auth_structure_reject: "approval_blocked",
  exact_facilitator_non_custodial: "approval_blocked",
  exact_simulation_balance_change_reject: "approval_blocked",
  exact_missing_trustline_reject: "approval_blocked",
  rejections_non_null_reason: "verified_offline",
  upto_stellar_upstream_spec: "upstream_blocked",
  upto_single_use_time_bound_cap: "upstream_blocked",
  spec_drift_gate: "verified_offline",
  observability_and_audit: "service_boundary_pending",
  third_party_security_review: "approval_blocked",
});

const EXPECTED_SUMMARY = Object.freeze({
  totalCases: 24,
  verifiedOffline: 9,
  serviceBoundaryPending: 2,
  approvalBlocked: 11,
  upstreamBlocked: 2,
});

const AUTHORITY_KEYS = Object.freeze([
  "networkAccessAllowed",
  "credentialUseAllowed",
  "keypairCreationAllowed",
  "signingAllowed",
  "paymentAllowed",
  "settlementAllowed",
  "serviceDispatchAllowed",
  "transactionSubmitAllowed",
  "actionPlanSubmitAllowed",
]);

export type ReadinessStatus =
  | "verified_offline"
  | "service_boundary_pending"
  | "approval_blocked"
  | "upstream_blocked";

export interface ReadinessCase {
  readonly id: string;
  readonly status: ReadinessStatus;
  readonly evidenceRefs: readonly string[];
  readonly reason: string;
}

export interface OfflineReadinessRecord {
  readonly schemaVersion: 1;
  readonly recordedAt: string;
  readonly planRef: string;
  readonly packageSnapshot: {
    readonly node: string;
    readonly pnpm: string;
    readonly packages: Readonly<Record<string, string>>;
    readonly licenseInventoryRef: string;
  };
  readonly summary: typeof EXPECTED_SUMMARY;
  readonly cases: readonly ReadinessCase[];
  readonly authorityBoundary: Readonly<Record<string, false>>;
}

export class ReadinessValidationError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "ReadinessValidationError";
  }
}

function fail(code: string, reason: string): never {
  throw new ReadinessValidationError(code, reason);
}

function asRecord(value: unknown, code: string, label: string): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    fail(code, `${label} must be an object`);
  }
  return value as Record<string, unknown>;
}

function assertExactKeys(
  value: Record<string, unknown>,
  expected: readonly string[],
  code: string,
  label: string,
): void {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (JSON.stringify(actual) !== JSON.stringify(wanted)) {
    fail(code, `${label} fields do not match the pinned readiness contract`);
  }
}

function boundedString(value: unknown, code: string, label: string, max: number): string {
  if (typeof value !== "string" || value.trim().length === 0 || value.length > max) {
    fail(code, `${label} must be a bounded non-empty string`);
  }
  return value;
}

function parsePackageSnapshot(value: unknown): OfflineReadinessRecord["packageSnapshot"] {
  const snapshot = asRecord(value, "readiness_package_drift", "packageSnapshot");
  assertExactKeys(
    snapshot,
    ["node", "pnpm", "packages", "licenseInventoryRef"],
    "readiness_package_drift",
    "packageSnapshot",
  );
  const packages = asRecord(snapshot.packages, "readiness_package_drift", "packages");
  if (
    snapshot.node !== "24.19.0" ||
    snapshot.pnpm !== "11.19.0" ||
    JSON.stringify(packages) !== JSON.stringify(EXPECTED_PACKAGES) ||
    snapshot.licenseInventoryRef !==
      "services/x402-stellar-facilitator/supply-chain/dependency-license-inventory.json"
  ) {
    fail("readiness_package_drift", "package and license snapshot must match the approved pins");
  }
  return {
    node: snapshot.node,
    pnpm: snapshot.pnpm,
    packages: packages as Record<string, string>,
    licenseInventoryRef: snapshot.licenseInventoryRef,
  };
}

function parseSummary(value: unknown): typeof EXPECTED_SUMMARY {
  const summary = asRecord(value, "readiness_summary_mismatch", "summary");
  assertExactKeys(
    summary,
    Object.keys(EXPECTED_SUMMARY),
    "readiness_summary_mismatch",
    "summary",
  );
  if (JSON.stringify(summary) !== JSON.stringify(EXPECTED_SUMMARY)) {
    fail("readiness_summary_mismatch", "summary does not match the pinned 24-case status counts");
  }
  return EXPECTED_SUMMARY;
}

function parseEvidenceRefs(value: unknown, id: string): readonly string[] {
  if (!Array.isArray(value) || value.length === 0 || value.length > 8) {
    fail("readiness_evidence_ref_invalid", `case ${id} requires 1-8 evidence refs`);
  }
  const refs = value.map((item, index) =>
    boundedString(item, "readiness_evidence_ref_invalid", `${id}.evidenceRefs[${index}]`, 256),
  );
  const unique = new Set(refs);
  if (
    unique.size !== refs.length ||
    refs.some(
      (ref) =>
        ref.includes("\\") ||
        ref.startsWith("/") ||
        /^[A-Za-z]:/.test(ref) ||
        ref.split("/").includes("..") ||
        !/^(?:\.github|docs|examples|services|src|tests)\//.test(ref),
    )
  ) {
    fail("readiness_evidence_ref_invalid", `case ${id} contains an unsafe evidence ref`);
  }
  return refs;
}

function parseCases(value: unknown): readonly ReadinessCase[] {
  if (!Array.isArray(value) || value.length !== EXPECTED_SUMMARY.totalCases) {
    fail("readiness_case_count_mismatch", "readiness must contain exactly 24 cases");
  }
  const seen = new Set<string>();
  const cases = value.map((item, index): ReadinessCase => {
    const entry = asRecord(item, "readiness_case_mismatch", `cases[${index}]`);
    assertExactKeys(
      entry,
      ["id", "status", "evidenceRefs", "reason"],
      "readiness_case_mismatch",
      `cases[${index}]`,
    );
    const id = boundedString(entry.id, "readiness_case_mismatch", `cases[${index}].id`, 96);
    const expectedStatus = EXPECTED_CASE_STATUS[id];
    if (expectedStatus === undefined || entry.status !== expectedStatus || seen.has(id)) {
      fail("readiness_case_mismatch", `case ${id} does not match the pinned readiness status`);
    }
    seen.add(id);
    return {
      id,
      status: expectedStatus,
      evidenceRefs: parseEvidenceRefs(entry.evidenceRefs, id),
      reason: boundedString(entry.reason, "readiness_case_mismatch", `${id}.reason`, 512),
    };
  });
  if (seen.size !== Object.keys(EXPECTED_CASE_STATUS).length) {
    fail("readiness_case_mismatch", "readiness case ids are incomplete");
  }
  return cases;
}

function parseAuthority(value: unknown): Readonly<Record<string, false>> {
  const authority = asRecord(value, "readiness_authority_forbidden", "authorityBoundary");
  assertExactKeys(
    authority,
    AUTHORITY_KEYS,
    "readiness_authority_forbidden",
    "authorityBoundary",
  );
  if (Object.values(authority).some((allowed) => allowed !== false)) {
    fail("readiness_authority_forbidden", "offline readiness grants no runtime authority");
  }
  return authority as Record<string, false>;
}

export function parseOfflineReadiness(value: unknown): OfflineReadinessRecord {
  const record = asRecord(value, "readiness_envelope_invalid", "readiness");
  assertExactKeys(
    record,
    [
      "schemaVersion",
      "recordedAt",
      "planRef",
      "packageSnapshot",
      "summary",
      "cases",
      "authorityBoundary",
    ],
    "readiness_envelope_invalid",
    "readiness",
  );
  if (
    record.schemaVersion !== 1 ||
    record.recordedAt !== "2026-08-24" ||
    record.planRef !== "examples/x402_stellar_conformance/plan.json"
  ) {
    fail("readiness_envelope_invalid", "readiness envelope does not match the pinned checkpoint");
  }
  return {
    schemaVersion: 1,
    recordedAt: record.recordedAt,
    planRef: record.planRef,
    packageSnapshot: parsePackageSnapshot(record.packageSnapshot),
    summary: parseSummary(record.summary),
    cases: parseCases(record.cases),
    authorityBoundary: parseAuthority(record.authorityBoundary),
  };
}
