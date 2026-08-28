import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

const AUTHORITY_KEYS = [
  "actionPlanSubmitAllowed",
  "approvalAllowed",
  "paymentAllowed",
  "proofAllowed",
  "rpcSubmitAllowed",
  "serviceDispatchAllowed",
  "settlementAllowed",
  "shellAccessAllowed",
  "signingAllowed",
  "transactionSubmitAllowed",
  "underlyingExecutionAllowed",
  "walletAccessAllowed",
] as const;

function fixtureUrl(name: string): URL {
  return new URL(
    `../../../../examples/product_local_quickstart/${name}`,
    import.meta.url,
  );
}

async function readJson(name: string): Promise<unknown> {
  return JSON.parse(await readFile(fixtureUrl(name), "utf8")) as unknown;
}

function record(value: unknown): Record<string, unknown> {
  assert.ok(value !== null && typeof value === "object" && !Array.isArray(value));
  return value as Record<string, unknown>;
}

function array(value: unknown): readonly unknown[] {
  assert.ok(Array.isArray(value));
  return value;
}

function assertExactAllFalseAuthority(value: unknown): void {
  const authority = record(value);
  assert.deepEqual(Object.keys(authority).sort(), [...AUTHORITY_KEYS].sort());
  assert.ok(Object.values(authority).every((grant) => grant === false));
}

test("versioned product quickstart fixtures preserve decision and no-authority parity", async () => {
  const manifest = record(await readJson("manifest.json"));
  const report = record(await readJson("quickstart_output.json"));
  const manifestScenarios = array(manifest.scenarios).map(record);
  const reportScenarios = array(report.scenarios).map(record);

  assert.equal(manifest.schema_version, 1);
  assert.equal(report.schemaVersion, 1);
  assert.equal(report.status, "product_local_reference_ready");
  assert.equal(report.offline, true);
  assert.equal(report.credentialRequired, false);
  assert.equal(report.networkRequired, false);
  assert.equal(report.listenerRequired, false);
  assert.equal(
    report.verificationBoundary,
    "local_binding_only_cryptographic_stellar_verify_not_run",
  );
  assert.deepEqual(report.path, [
    "bazaar_discovery",
    "x402_access_state",
    "typed_action_plan",
    "deterministic_policy",
    "optional_zk_proof_artifact",
    "local_zk_binding_verify",
    "separate_exact_capability_gate",
  ]);
  assertExactAllFalseAuthority(manifest.authority);
  assertExactAllFalseAuthority(report.authorityBoundary);
  assert.equal(manifestScenarios.length, 3);
  assert.equal(reportScenarios.length, 3);

  for (const [index, expected] of manifestScenarios.entries()) {
    const actual = reportScenarios[index];
    assert.ok(actual);
    assert.equal(actual.name, expected.name);
    assert.equal(actual.outcome, expected.expected_outcome);
    assert.equal(
      record(actual.capability).code,
      expected.expected_capability_code,
    );
    assert.equal(
      record(actual.capability).accessConsumed,
      expected.expected_access_consumed,
    );
    assert.equal(record(actual.capability).serviceDispatchAllowed, false);
    assertExactAllFalseAuthority(actual.authority);

    const evidence = record(actual.zkEvidence);
    assert.equal(evidence.artifactPresent, true);
    assert.equal(evidence.actionPlanProjectionValidated, true);
    assert.equal(evidence.localBinding, "binding_validated");
    assert.equal(evidence.cryptographicallyVerified, false);
    assert.equal(evidence.stellarVerificationRequired, true);
    assert.equal(evidence.privatePolicyRevealed, false);
    assert.equal(evidence.proofReasonCode, expected.expected_proof_reason_code);
    assertExactAllFalseAuthority(evidence.authority);

    for (const field of [
      "request_fixture",
      "evaluation_response_fixture",
    ] as const) {
      assert.equal(typeof expected[field], "string");
      await access(fixtureUrl(String(expected[field])));
    }
  }

  const [approved, requiresApproval, blocked] = reportScenarios;
  assert.ok(approved);
  assert.ok(requiresApproval);
  assert.ok(blocked);
  assert.equal(record(approved.capability).gateCalls, 1);
  assert.equal(record(requiresApproval.capability).gateCalls, 0);
  assert.equal(record(blocked.capability).gateCalls, 0);
});
