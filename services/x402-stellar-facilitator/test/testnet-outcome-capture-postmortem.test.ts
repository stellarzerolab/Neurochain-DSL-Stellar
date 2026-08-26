import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  TESTNET_CREDENTIAL_HANDLE,
  TESTNET_HARNESS_CONFIRMATION,
  TestnetCanonicalDiagnosticError,
  runTestnetConformanceHarness,
  type TestnetStateOutcome,
} from "../src/testnet-conformance-harness.js";
import {
  TestnetOutcomeCapturePostmortemError,
  buildTestnetOutcomeCapturePostmortem,
} from "../src/testnet-outcome-capture-postmortem.js";

const PUBLIC_ACCOUNT =
  "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
const SECRET_SENTINEL = "TOP_SECRET_SENTINEL_MUST_NEVER_ESCAPE";

interface HarnessFixture {
  readonly request: Record<string, unknown>;
  readonly boundary: { readonly expectedPayTo: string };
}

async function readJsonFixture(path: string): Promise<unknown> {
  return JSON.parse(
    await readFile(new URL(`../../fixtures/${path}`, import.meta.url), "utf8"),
  ) as unknown;
}

test("postmortem locks the execute digest and exposes the current capture gap without network", async () => {
  const fixture = (await readJsonFixture(
    "testnet-harness-v3.expected.json",
  )) as HarnessFixture;
  const expected = await readJsonFixture(
    "testnet-outcome-capture-postmortem-v1.expected.json",
  );
  const originalFetch = globalThis.fetch;
  let fetchCalls = 0;
  let persistedOutcome: TestnetStateOutcome | null = null;
  globalThis.fetch = (() => {
    fetchCalls += 1;
    throw new Error("offline_postmortem_network_forbidden");
  }) as typeof fetch;

  try {
    const dryRunResult = await runTestnetConformanceHarness(fixture.request, {
      expectedPayTo: fixture.boundary.expectedPayTo,
    });
    const executeResult = await runTestnetConformanceHarness(
      {
        ...fixture.request,
        execute: true,
        confirmation: TESTNET_HARNESS_CONFIRMATION,
      },
      {
        expectedPayTo: fixture.boundary.expectedPayTo,
        statePort: {
          reserve: async (requestDigest) => ({
            status: "reserved",
            reservationId: `tstate_${requestDigest}`,
            code: "testnet_state_reserved",
            reason: "offline postmortem reserved the execute digest",
          }),
          finalize: async (_reservationId, outcome) => {
            persistedOutcome = outcome;
            return {
              status: "recorded",
              code: "testnet_state_outcome_unknown",
              reason: "offline postmortem observed terminal state capture",
            };
          },
        },
        credentialPort: {
          createEphemeral: async () => ({
            publicAccountId: PUBLIC_ACCOUNT,
            [TESTNET_CREDENTIAL_HANDLE]: SECRET_SENTINEL,
          }),
        },
        canonicalPort: {
          run: async () => {
            throw new TestnetCanonicalDiagnosticError("upstream_verify");
          },
        },
      },
    );

    assert.ok(persistedOutcome);
    const postmortem = buildTestnetOutcomeCapturePostmortem(
      dryRunResult,
      executeResult,
      persistedOutcome,
    );
    assert.deepEqual(postmortem, expected);
    assert.equal(
      postmortem.digestAuthority.authoritativeDigest,
      executeResult.plan?.requestDigest,
    );
    assert.notEqual(
      postmortem.digestAuthority.authoritativeDigest,
      dryRunResult.plan?.requestDigest,
    );
    assert.equal(postmortem.captureBoundary.diagnosticPersisted, false);
    assert.doesNotMatch(
      JSON.stringify(postmortem),
      new RegExp(SECRET_SENTINEL, "u"),
    );
  } finally {
    globalThis.fetch = originalFetch;
  }

  assert.equal(fetchCalls, 0);
});

test("postmortem rejects missing diagnostics and non-terminal state capture", async () => {
  const fixture = (await readJsonFixture(
    "testnet-harness-v3.expected.json",
  )) as HarnessFixture;
  const dryRunResult = await runTestnetConformanceHarness(fixture.request, {
    expectedPayTo: fixture.boundary.expectedPayTo,
  });
  const invalidExecuteResult = Object.freeze({
    ...dryRunResult,
    status: "blocked" as const,
    code: "testnet_outcome_unknown" as const,
    plan: Object.freeze({
      ...dryRunResult.plan!,
      execute: true,
      requestDigest:
        "88f097a735d9df8aab8759a9b1dac2b7a984167ad5849acd6cabee9ca19204a3",
    }),
  });

  assert.throws(
    () =>
      buildTestnetOutcomeCapturePostmortem(
        dryRunResult,
        invalidExecuteResult,
        { status: "outcome_unknown" },
      ),
    (error: unknown) => {
      assert.ok(error instanceof TestnetOutcomeCapturePostmortemError);
      assert.equal(error.code, "testnet_outcome_capture_postmortem_invalid");
      assert.doesNotMatch(JSON.stringify(error), new RegExp(SECRET_SENTINEL, "u"));
      return true;
    },
  );
});
