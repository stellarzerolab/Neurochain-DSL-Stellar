import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  buildVerifyRejectionConformanceSnapshot,
  evaluateVerifyRejectionConformance,
  VERIFY_APPROVAL_BLOCKED_CASES,
  VERIFY_REJECTION_CASE_IDS,
} from "../src/verify-rejection-conformance.js";

async function readFixture(): Promise<unknown> {
  const fixtureUrl = new URL(
    "../../fixtures/verify-rejection-v2.expected.json",
    import.meta.url,
  );
  return JSON.parse(await readFile(fixtureUrl, "utf8")) as unknown;
}

function cloneFixture(value: unknown): Record<string, unknown> {
  return JSON.parse(JSON.stringify(value)) as Record<string, unknown>;
}

test("upstream Stellar exact verify rejects every pinned pre-network case", async () => {
  const fixture = await readFixture();
  const snapshot = await buildVerifyRejectionConformanceSnapshot();

  assert.deepEqual(snapshot, fixture);
  assert.deepEqual(
    snapshot.cases.map(({ id }) => id),
    VERIFY_REJECTION_CASE_IDS,
  );
  for (const conformanceCase of snapshot.cases) {
    assert.equal(conformanceCase.response.isValid, false);
    assert.equal(
      conformanceCase.response.invalidReason,
      conformanceCase.expectedInvalidReason,
    );
    assert.ok(conformanceCase.expectedInvalidReason.trim().length > 0);
  }
  assert.deepEqual(snapshot.approvalBlocked, VERIFY_APPROVAL_BLOCKED_CASES);
  for (const blocked of snapshot.approvalBlocked) {
    assert.equal(blocked.code, "approval_blocked");
    assert.ok(blocked.reason.trim().length > 0);
  }
  assert.deepEqual(snapshot.authorityBoundary, {
    networkAccessAllowed: false,
    credentialUseAllowed: false,
    keypairCreated: false,
    signingAllowed: false,
    settlementAllowed: false,
    transactionSubmitAllowed: false,
    actionPlanSubmitAllowed: false,
    verifyMethodCalls: VERIFY_REJECTION_CASE_IDS.length,
    signerMethodCalls: 0,
    networkFetchCalls: 0,
    settleMethodCalls: 0,
  });
  assert.deepEqual(await evaluateVerifyRejectionConformance(fixture), {
    status: "ready",
    code: "verify_rejection_conformance_ready",
    reason:
      "upstream Stellar exact verify rejects every safe pre-network case with the pinned response",
  });
});

test("verify wire, approval and authority drift fail closed", async () => {
  const fixture = await readFixture();

  const packageDrift = cloneFixture(fixture);
  packageDrift.sourcePackages = {
    "@x402/core": "2.23.0",
    "@x402/stellar": "9.9.9",
  };

  const wireDrift = cloneFixture(fixture);
  const cases = wireDrift.cases as Array<Record<string, unknown>>;
  const firstResponse = cases[0]?.response as Record<string, unknown>;
  firstResponse.invalidReason = "changed_reason";

  const approvalDrift = cloneFixture(fixture);
  const approvalBlocked = approvalDrift.approvalBlocked as Array<
    Record<string, unknown>
  >;
  if (approvalBlocked[0] !== undefined) {
    approvalBlocked[0].code = "ready";
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
    [wireDrift, "verify_rejection_wire_drift"],
    [approvalDrift, "approval_boundary_drift"],
    [authorityDrift, "authority_boundary_violated"],
    [extraField, "verify_rejection_fixture_invalid"],
  ] as const;

  for (const [candidate, code] of driftCases) {
    const result = await evaluateVerifyRejectionConformance(candidate);
    assert.equal(result.status, "invalid");
    assert.equal(result.code, code);
    assert.ok(result.reason.trim().length > 0);
  }
});
