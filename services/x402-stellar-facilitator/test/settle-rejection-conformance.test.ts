import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  buildSettleRejectionConformanceSnapshot,
  evaluateSettleRejectionConformance,
  SETTLE_ADMISSION_CASES,
  SETTLE_APPROVAL_BLOCKED_CASES,
  SETTLE_REJECTION_CASE_IDS,
  SETTLE_SERVICE_BOUNDARY_PENDING,
} from "../src/settle-rejection-conformance.js";

async function readFixture(): Promise<unknown> {
  const fixtureUrl = new URL(
    "../../fixtures/settle-rejection-v2.expected.json",
    import.meta.url,
  );
  return JSON.parse(await readFile(fixtureUrl, "utf8")) as unknown;
}

function cloneFixture(value: unknown): Record<string, unknown> {
  return JSON.parse(JSON.stringify(value)) as Record<string, unknown>;
}

test("upstream settle and admission hooks reject every pinned offline case", async () => {
  const fixture = await readFixture();
  const snapshot = await buildSettleRejectionConformanceSnapshot();

  assert.deepEqual(snapshot, fixture);
  assert.deepEqual(
    snapshot.upstreamRejections.map(({ id }) => id),
    SETTLE_REJECTION_CASE_IDS,
  );
  for (const conformanceCase of snapshot.upstreamRejections) {
    assert.equal(conformanceCase.response.success, false);
    assert.equal(conformanceCase.response.transaction, "");
    assert.equal(
      conformanceCase.response.errorReason,
      conformanceCase.expectedErrorReason,
    );
    assert.ok(conformanceCase.expectedErrorReason.trim().length > 0);
  }
  assert.deepEqual(
    snapshot.admissionRejections.map(({ id, code, reason }) => ({
      id,
      code,
      reason,
    })),
    SETTLE_ADMISSION_CASES,
  );
  for (const admission of snapshot.admissionRejections) {
    assert.equal(admission.upstreamError, `Settlement aborted: ${admission.code}`);
    assert.ok(admission.reason.trim().length > 0);
  }
  assert.deepEqual(snapshot.approvalBlocked, SETTLE_APPROVAL_BLOCKED_CASES);
  assert.deepEqual(
    snapshot.serviceBoundaryPending,
    SETTLE_SERVICE_BOUNDARY_PENDING,
  );
  assert.deepEqual(snapshot.authorityBoundary, {
    networkAccessAllowed: false,
    credentialUseAllowed: false,
    keypairCreated: false,
    custodyAllowed: false,
    signingAllowed: false,
    liveSettlementAllowed: false,
    transactionSubmitAllowed: false,
    actionPlanSubmitAllowed: false,
    exactSettleMethodCalls: SETTLE_REJECTION_CASE_IDS.length,
    admissionHookCalls: SETTLE_ADMISSION_CASES.length,
    guardedSchemeSettleCalls: 0,
    signerMethodCalls: 0,
    networkFetchCalls: 0,
  });
  assert.deepEqual(await evaluateSettleRejectionConformance(fixture), {
    status: "ready",
    code: "settle_rejection_conformance_ready",
    reason:
      "upstream settle and admission hooks reject every pinned offline case before network, signer or submit",
  });
});

test("settle wire, admission and authority drift fail closed", async () => {
  const fixture = await readFixture();

  const packageDrift = cloneFixture(fixture);
  packageDrift.sourcePackages = {
    "@x402/core": "2.23.0",
    "@x402/stellar": "9.9.9",
  };

  const wireDrift = cloneFixture(fixture);
  const upstreamRejections = wireDrift.upstreamRejections as Array<
    Record<string, unknown>
  >;
  const firstResponse = upstreamRejections[0]?.response as Record<
    string,
    unknown
  >;
  firstResponse.errorReason = "changed_reason";

  const admissionDrift = cloneFixture(fixture);
  const admissionRejections = admissionDrift.admissionRejections as Array<
    Record<string, unknown>
  >;
  if (admissionRejections[0] !== undefined) {
    admissionRejections[0].code = "changed_code";
  }

  const approvalDrift = cloneFixture(fixture);
  const approvalBlocked = approvalDrift.approvalBlocked as Array<
    Record<string, unknown>
  >;
  if (approvalBlocked[0] !== undefined) {
    approvalBlocked[0].code = "ready";
  }

  const serviceDrift = cloneFixture(fixture);
  const servicePending = serviceDrift.serviceBoundaryPending as Array<
    Record<string, unknown>
  >;
  if (servicePending[0] !== undefined) {
    servicePending[0].code = "ready";
  }

  const authorityDrift = cloneFixture(fixture);
  const authorityBoundary = authorityDrift.authorityBoundary as Record<
    string,
    unknown
  >;
  authorityBoundary.signingAllowed = true;

  const extraField = cloneFixture(fixture);
  extraField.unapproved = true;

  const driftCases = [
    [packageDrift, "source_package_drift"],
    [wireDrift, "settle_rejection_wire_drift"],
    [admissionDrift, "settle_admission_drift"],
    [approvalDrift, "settle_approval_boundary_drift"],
    [serviceDrift, "settle_service_boundary_drift"],
    [authorityDrift, "authority_boundary_violated"],
    [extraField, "settle_rejection_fixture_invalid"],
  ] as const;

  for (const [candidate, code] of driftCases) {
    const result = await evaluateSettleRejectionConformance(candidate);
    assert.equal(result.status, "invalid");
    assert.equal(result.code, code);
    assert.ok(result.reason.trim().length > 0);
  }
});
