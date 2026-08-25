import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  TESTNET_CREDENTIAL_HANDLE,
  TESTNET_HARNESS_CONFIRMATION,
  runTestnetConformanceHarness,
  type CanonicalTestnetPort,
  type TestnetCredentialPort,
  type TestnetHarnessBoundary,
  type TestnetStatePort,
} from "../src/testnet-conformance-harness.js";

const PUBLIC_ACCOUNT =
  "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
const SECRET_SENTINEL = "TOP_SECRET_SENTINEL_MUST_NEVER_ESCAPE";

interface HarnessFixture {
  readonly schemaVersion: 2;
  readonly request: Record<string, unknown>;
  readonly boundary: { readonly expectedPayTo: string };
  readonly expected: { readonly status: string; readonly code: string };
}

async function readFixture(): Promise<HarnessFixture> {
  const fixtureUrl = new URL(
    "../../fixtures/testnet-harness-v2.expected.json",
    import.meta.url,
  );
  return JSON.parse(await readFile(fixtureUrl, "utf8")) as HarnessFixture;
}

function clone(value: Record<string, unknown>): Record<string, unknown> {
  return JSON.parse(JSON.stringify(value)) as Record<string, unknown>;
}

test("default harness validates a safe plan with zero credential, network or submit effects", async () => {
  const fixture = await readFixture();
  const originalFetch = globalThis.fetch;
  let fetchCalls = 0;
  let stateCalls = 0;
  let credentialCalls = 0;
  let canonicalCalls = 0;
  globalThis.fetch = (() => {
    fetchCalls += 1;
    throw new Error("offline_testnet_harness_network_forbidden");
  }) as typeof fetch;

  try {
    const result = await runTestnetConformanceHarness(fixture.request, {
      expectedPayTo: fixture.boundary.expectedPayTo,
      statePort: {
        reserve: async () => {
          stateCalls += 1;
          throw new Error("dry_run_state_invoked");
        },
        finalize: async () => {
          stateCalls += 1;
          throw new Error("dry_run_state_finalize_invoked");
        },
      },
      credentialPort: {
        createEphemeral: async () => {
          credentialCalls += 1;
          throw new Error("dry_run_credential_invoked");
        },
      },
      canonicalPort: {
        run: async () => {
          canonicalCalls += 1;
          throw new Error("dry_run_canonical_invoked");
        },
      },
    });
    assert.equal(fixture.schemaVersion, 2);
    assert.equal(result.plan?.attempt, 2);
    assert.equal(result.status, fixture.expected.status);
    assert.equal(result.code, fixture.expected.code);
    assert.ok(result.reason.trim().length > 0);
    assert.equal(result.plan?.execute, false);
    assert.match(result.plan?.requestDigest ?? "", /^[0-9a-f]{64}$/u);
    assert.equal(result.evidence, null);
    assert.deepEqual(Object.values(result.authorityBoundary), Array(11).fill(false));
    const serialized = JSON.stringify(result);
    assert.doesNotMatch(serialized, /"confirmation"\s*:/u);
    assert.doesNotMatch(serialized, new RegExp(SECRET_SENTINEL, "u"));
  } finally {
    globalThis.fetch = originalFetch;
  }

  assert.equal(fetchCalls, 0);
  assert.equal(stateCalls, 0);
  assert.equal(credentialCalls, 0);
  assert.equal(canonicalCalls, 0);
});

test("network, endpoint, asset, recipient, amount, opt-in and envelope drift fail closed", async () => {
  const fixture = await readFixture();
  const cases: ReadonlyArray<readonly [Record<string, unknown>, string]> = [
    [{ ...fixture.request, network: "stellar:pubnet" }, "testnet_network_forbidden"],
    [
      { ...fixture.request, rpcUrl: "https://rpc.example.invalid" },
      "testnet_endpoint_forbidden",
    ],
    [
      { ...fixture.request, asset: "native" },
      "testnet_asset_mismatch",
    ],
    [
      {
        ...fixture.request,
        payTo: "GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
      },
      "testnet_recipient_mismatch",
    ],
    [{ ...fixture.request, amount: "100001" }, "testnet_amount_mismatch"],
    [{ ...fixture.request, attempt: 3 }, "testnet_request_invalid"],
    [
      { ...fixture.request, execute: true, confirmation: null },
      "testnet_execute_opt_in_required",
    ],
    [
      { ...fixture.request, confirmation: TESTNET_HARNESS_CONFIRMATION },
      "testnet_authority_forbidden",
    ],
    [
      { ...fixture.request, privateKey: SECRET_SENTINEL },
      "testnet_request_invalid",
    ],
  ];

  for (const [request, code] of cases) {
    const result = await runTestnetConformanceHarness(request, {
      expectedPayTo: fixture.boundary.expectedPayTo,
    });
    assert.equal(result.status, "blocked");
    assert.equal(result.code, code);
    assert.ok(result.reason.trim().length > 0);
    assert.equal(result.evidence, null);
    assert.doesNotMatch(JSON.stringify(result), new RegExp(SECRET_SENTINEL, "u"));
  }

  const unknownField = clone(fixture.request);
  unknownField.unapproved = true;
  assert.equal(
    (
      await runTestnetConformanceHarness(unknownField, {
        expectedPayTo: fixture.boundary.expectedPayTo,
      })
    ).code,
    "testnet_request_invalid",
  );
});

test("execute path requires atomic state and opaque credential ports", async () => {
  const fixture = await readFixture();
  const request = {
    ...fixture.request,
    execute: true,
    confirmation: TESTNET_HARNESS_CONFIRMATION,
  };
  const noState = await runTestnetConformanceHarness(request, {
    expectedPayTo: fixture.boundary.expectedPayTo,
  });
  assert.equal(noState.code, "testnet_state_unavailable");

  let credentialCalls = 0;
  const unavailableState: TestnetStatePort = {
    reserve: async () => ({
      status: "unavailable",
      code: "duplicate",
      reason: "request digest is already reserved",
    }),
    finalize: async () => assert.fail("unavailable state finalized"),
  };
  const credentialPort: TestnetCredentialPort = {
    createEphemeral: async () => {
      credentialCalls += 1;
      return {
        publicAccountId: PUBLIC_ACCOUNT,
        [TESTNET_CREDENTIAL_HANDLE]: SECRET_SENTINEL,
      };
    },
  };
  const blocked = await runTestnetConformanceHarness(request, {
    expectedPayTo: fixture.boundary.expectedPayTo,
    statePort: unavailableState,
    credentialPort,
    canonicalPort: { run: async () => assert.fail("canonical port invoked") },
  });
  assert.equal(blocked.code, "testnet_state_unavailable");
  assert.equal(credentialCalls, 0);
  assert.doesNotMatch(JSON.stringify(blocked), new RegExp(SECRET_SENTINEL, "u"));
});

test("opaque credential handles and canonical errors never escape into public output", async () => {
  const fixture = await readFixture();
  const request = {
    ...fixture.request,
    execute: true,
    confirmation: TESTNET_HARNESS_CONFIRMATION,
  };
  const statePort: TestnetStatePort = {
    reserve: async () => ({
      status: "reserved",
      reservationId: "tstate_exception_case",
      code: "reserved",
      reason: "request digest reserved atomically",
    }),
    finalize: async () => ({
      status: "recorded",
      code: "outcome_unknown",
      reason: "unknown outcome recorded",
    }),
  };
  const credentialPort: TestnetCredentialPort = {
    createEphemeral: async () => ({
      publicAccountId: PUBLIC_ACCOUNT,
      [TESTNET_CREDENTIAL_HANDLE]: SECRET_SENTINEL,
    }),
  };
  const canonicalPort: CanonicalTestnetPort = {
    run: async () => {
      throw new Error(SECRET_SENTINEL);
    },
  };
  const result = await runTestnetConformanceHarness(request, {
    expectedPayTo: fixture.boundary.expectedPayTo,
    statePort,
    credentialPort,
    canonicalPort,
  });
  assert.equal(result.code, "testnet_outcome_unknown");
  assert.doesNotMatch(JSON.stringify(result), new RegExp(SECRET_SENTINEL, "u"));
});

test("only strict public evidence can leave the canonical port", async () => {
  const fixture = await readFixture();
  const request = {
    ...fixture.request,
    execute: true,
    confirmation: TESTNET_HARNESS_CONFIRMATION,
  };
  let stateCalls = 0;
  let credentialCalls = 0;
  let canonicalCalls = 0;
  const boundary: TestnetHarnessBoundary = {
    expectedPayTo: fixture.boundary.expectedPayTo,
    statePort: {
      reserve: async () => {
        stateCalls += 1;
        return {
          status: "reserved",
          reservationId: "tstate_public_evidence_case",
          code: "reserved",
          reason: "request digest reserved atomically",
        };
      },
      finalize: async () => {
        stateCalls += 1;
        return {
          status: "recorded",
          code: "recorded",
          reason: "public outcome recorded",
        };
      },
    },
    credentialPort: {
      createEphemeral: async () => {
        credentialCalls += 1;
        return {
          publicAccountId: PUBLIC_ACCOUNT,
          [TESTNET_CREDENTIAL_HANDLE]: SECRET_SENTINEL,
        };
      },
    },
    canonicalPort: {
      run: async (_plan, credential) => {
        canonicalCalls += 1;
        assert.equal(
          credential[TESTNET_CREDENTIAL_HANDLE],
          SECRET_SENTINEL,
        );
        return {
          network: "stellar:testnet",
          publicAccountId: credential.publicAccountId,
          transactionHash: null,
          ledger: null,
          status: "supported_verified",
          observedAt: "2026-08-24T09:00:00.000Z",
          conformanceResults: [
            {
              check: "canonical_supported",
              status: "passed",
              code: "supported_passed",
              reason: SECRET_SENTINEL,
            },
            {
              check: "canonical_verify",
              status: "passed",
              code: "verify_passed",
              reason: SECRET_SENTINEL,
            },
          ],
        };
      },
    },
  };
  const result = await runTestnetConformanceHarness(request, boundary);
  assert.equal(result.status, "completed");
  assert.equal(result.code, "testnet_conformance_completed");
  assert.equal(result.evidence?.status, "supported_verified");
  assert.deepEqual(result.evidence?.conformanceResults, [
    {
      check: "canonical_supported",
      status: "passed",
      code: "supported_passed",
      reason: "canonical supported check passed",
    },
    {
      check: "canonical_verify",
      status: "passed",
      code: "verify_passed",
      reason: "canonical verify check passed",
    },
  ]);
  assert.equal(stateCalls, 2);
  assert.equal(credentialCalls, 1);
  assert.equal(canonicalCalls, 1);
  assert.doesNotMatch(JSON.stringify(result), new RegExp(SECRET_SENTINEL, "u"));
  assert.deepEqual(Object.values(result.authorityBoundary), Array(11).fill(false));

  const injectedEvidenceBoundary: TestnetHarnessBoundary = {
    ...boundary,
    canonicalPort: {
      run: async () => ({
        network: "stellar:testnet",
        publicAccountId: PUBLIC_ACCOUNT,
        transactionHash: null,
        ledger: null,
        status: "supported_verified",
        observedAt: "2026-08-24T09:00:00.000Z",
        conformanceResults: [
          {
            check: "canonical_supported",
            status: "passed",
            code: "supported_passed",
            reason: "canonical client checks passed",
          },
          {
            check: "canonical_verify",
            status: "passed",
            code: "verify_passed",
            reason: "canonical client checks passed",
          },
        ],
        privateKey: SECRET_SENTINEL,
      }),
    },
  };
  const rejected = await runTestnetConformanceHarness(
    request,
    injectedEvidenceBoundary,
  );
  assert.equal(rejected.code, "testnet_outcome_unknown");
  assert.doesNotMatch(JSON.stringify(rejected), new RegExp(SECRET_SENTINEL, "u"));

  const unboundEvidenceBoundary: TestnetHarnessBoundary = {
    ...boundary,
    canonicalPort: {
      run: async () => ({
        network: "stellar:testnet",
        publicAccountId:
          "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX",
        transactionHash: null,
        ledger: null,
        status: "supported_verified",
        observedAt: "2026-08-24T09:00:00.000Z",
        conformanceResults: [
          {
            check: "canonical_supported",
            status: "passed",
            code: "supported_passed",
            reason: "canonical supported check passed",
          },
          {
            check: "canonical_verify",
            status: "passed",
            code: "verify_passed",
            reason: "canonical verify check passed",
          },
        ],
      }),
    },
  };
  const unbound = await runTestnetConformanceHarness(
    request,
    unboundEvidenceBoundary,
  );
  assert.equal(unbound.code, "testnet_outcome_unknown");
});
