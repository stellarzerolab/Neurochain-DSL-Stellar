import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

import {
  TESTNET_CREDENTIAL_HANDLE,
  TESTNET_HARNESS_CONFIRMATION,
  runTestnetConformanceHarness,
  type CanonicalTestnetPort,
  type TestnetHarnessResult,
  type TestnetStatePort,
} from "../src/testnet-conformance-harness.js";
import {
  assertCanonicalSupported,
  mapCanonicalVerifyResultToPublicEvidence,
} from "../src/testnet-live-conformance.js";
import {
  TestnetCanonicalReferenceError,
  bindCanonicalReferenceDigest,
} from "../src/testnet-canonical-reference.js";
import {
  LocalTestnetStateAdapter,
  TESTNET_STATE_SCHEMA_VERSION,
  type TestnetStateInspection,
} from "../src/testnet-state-adapter.js";

const PUBLIC_ACCOUNT =
  "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
const WRONG_PAYER =
  "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
const RAW_SECRET_SENTINEL = "RAW_UPSTREAM_SECRET_MUST_NEVER_ESCAPE";

interface ReferenceCase {
  readonly id: string;
  readonly resultStatus: string;
  readonly resultCode: string;
  readonly state: string;
  readonly diagnosticStage: string | null;
  readonly detailCode: string | null;
  readonly evidenceStatus: string | null;
}

interface ReferenceFixture {
  readonly schemaVersion: 1;
  readonly sourcePackages: {
    readonly stellar: "2.23.0";
    readonly core: "2.23.0";
  };
  readonly mode: "offline_injected_ports";
  readonly observedAt: string;
  readonly publicAccountId: string;
  readonly binding: Record<string, unknown>;
  readonly cases: readonly ReferenceCase[];
  readonly effectBoundary: Record<string, number>;
  readonly forbiddenFields: readonly string[];
}

interface HarnessFixture {
  readonly request: Record<string, unknown>;
  readonly boundary: { readonly expectedPayTo: string };
}

async function readJsonFixture(path: string): Promise<unknown> {
  return JSON.parse(
    await readFile(new URL(`../../fixtures/${path}`, import.meta.url), "utf8"),
  ) as unknown;
}

async function withTemporaryWorkspace<T>(
  run: (workspaceRoot: string) => Promise<T>,
): Promise<T> {
  const workspaceRoot = await mkdtemp(
    join(tmpdir(), "neurochain-x402-reference-"),
  );
  try {
    return await run(resolve(workspaceRoot));
  } finally {
    assert.ok(resolve(workspaceRoot).startsWith(resolve(tmpdir())));
    await rm(workspaceRoot, { recursive: true, force: true });
  }
}

function executeRequest(fixture: HarnessFixture): Record<string, unknown> {
  return {
    ...fixture.request,
    execute: true,
    confirmation: TESTNET_HARNESS_CONFIRMATION,
  };
}

function referenceCredentialPort() {
  return {
    createEphemeral: async () => ({
      publicAccountId: PUBLIC_ACCOUNT,
      [TESTNET_CREDENTIAL_HANDLE]: Object.freeze({ reference: true }),
    }),
  };
}

function canonicalPort(
  upstreamResult: unknown,
  observedAt: string,
): CanonicalTestnetPort {
  return {
    run: async () => {
      assertCanonicalSupported();
      return mapCanonicalVerifyResultToPublicEvidence(
        upstreamResult,
        PUBLIC_ACCOUNT,
        observedAt,
      );
    },
  };
}

function summarize(
  id: string,
  result: TestnetHarnessResult,
  inspection: TestnetStateInspection | null,
): ReferenceCase {
  const authorities = Object.values(result.authorityBoundary);
  assert.equal(authorities.length, 11);
  assert.ok(authorities.every((granted) => granted === false));
  return {
    id,
    resultStatus: result.status,
    resultCode: result.code,
    state:
      inspection?.status === "recorded"
        ? inspection.record.state
        : inspection?.status ?? "unavailable",
    diagnosticStage: result.diagnostic?.stage ?? null,
    detailCode: result.diagnostic?.detailCode ?? null,
    evidenceStatus: result.evidence?.status ?? null,
  };
}

test("canonical offline reference locks result-wrapper and schema-v2 state parity", async () => {
  const reference = (await readJsonFixture(
    "testnet-canonical-reference-v1.expected.json",
  )) as ReferenceFixture;
  const harness = (await readJsonFixture(
    "testnet-harness-v3.expected.json",
  )) as HarnessFixture;
  const packageJson = JSON.parse(
    await readFile(new URL("../../package.json", import.meta.url), "utf8"),
  ) as { readonly dependencies: Record<string, string> };
  assert.equal(
    packageJson.dependencies["@x402/stellar"],
    reference.sourcePackages.stellar,
  );
  assert.equal(
    packageJson.dependencies["@x402/core"],
    reference.sourcePackages.core,
  );
  assert.equal(reference.mode, "offline_injected_ports");
  assert.equal(reference.publicAccountId, PUBLIC_ACCOUNT);

  const originalFetch = globalThis.fetch;
  let externalNetworkCalls = 0;
  globalThis.fetch = (() => {
    externalNetworkCalls += 1;
    throw new Error("offline_reference_network_forbidden");
  }) as typeof fetch;

  try {
    const dryRun = await runTestnetConformanceHarness(harness.request, {
      expectedPayTo: harness.boundary.expectedPayTo,
    });

    const valid = await withTemporaryWorkspace(async (workspaceRoot) => {
      const statePort = new LocalTestnetStateAdapter({ workspaceRoot });
      const result = await runTestnetConformanceHarness(executeRequest(harness), {
        expectedPayTo: harness.boundary.expectedPayTo,
        statePort,
        credentialPort: referenceCredentialPort(),
        canonicalPort: canonicalPort(
          { isValid: true, payer: PUBLIC_ACCOUNT },
          reference.observedAt,
        ),
      });
      const inspection = await statePort.inspect(result.plan?.requestDigest ?? "");
      assert.equal(inspection.status, "recorded");
      assert.equal(inspection.record?.schemaVersion, TESTNET_STATE_SCHEMA_VERSION);
      assert.equal(
        inspection.record?.schemaVersion === TESTNET_STATE_SCHEMA_VERSION
          ? inspection.record.diagnostic
          : undefined,
        null,
      );
      return { result, inspection };
    });

    const invalidInputs: readonly {
      readonly id: string;
      readonly upstreamResult: unknown;
    }[] = [
      { id: "malformed_verify_result", upstreamResult: null },
      {
        id: "unknown_verify_reason",
        upstreamResult: {
          isValid: false,
          invalidReason: RAW_SECRET_SENTINEL,
          invalidMessage: RAW_SECRET_SENTINEL,
        },
      },
      {
        id: "payer_mismatch",
        upstreamResult: { isValid: true, payer: WRONG_PAYER },
      },
    ];
    const cases: ReferenceCase[] = [
      summarize("valid_result_wrapper", valid.result, valid.inspection),
    ];

    for (const invalid of invalidInputs) {
      const outcome = await withTemporaryWorkspace(async (workspaceRoot) => {
        const statePort = new LocalTestnetStateAdapter({ workspaceRoot });
        const result = await runTestnetConformanceHarness(
          executeRequest(harness),
          {
            expectedPayTo: harness.boundary.expectedPayTo,
            statePort,
            credentialPort: referenceCredentialPort(),
            canonicalPort: canonicalPort(
              invalid.upstreamResult,
              reference.observedAt,
            ),
          },
        );
        const inspection = await statePort.inspect(
          result.plan?.requestDigest ?? "",
        );
        assert.equal(inspection.status, "recorded");
        assert.equal(inspection.record?.state, "outcome_unknown");
        if (
          inspection.record?.schemaVersion === TESTNET_STATE_SCHEMA_VERSION
        ) {
          assert.equal(
            inspection.record.diagnostic?.stage,
            result.diagnostic?.stage,
          );
          assert.equal(
            inspection.record.diagnostic?.detailCode ?? null,
            result.diagnostic?.detailCode ?? null,
          );
        } else {
          assert.fail("reference state did not use schema-v2");
        }
        return { result, inspection };
      });
      assert.doesNotMatch(
        JSON.stringify(outcome),
        new RegExp(RAW_SECRET_SENTINEL, "u"),
      );
      cases.push(summarize(invalid.id, outcome.result, outcome.inspection));
    }

    let finalizedOutcomes = 0;
    const unavailableState: TestnetStatePort = {
      reserve: async (requestDigest) => ({
        status: "reserved",
        reservationId: `tstate_${requestDigest}`,
        code: "testnet_state_reserved",
        reason: "offline reference reserved the request",
      }),
      finalize: async () => {
        finalizedOutcomes += 1;
        return {
          status: "unavailable",
          code: "testnet_state_capture_unavailable",
          reason: "offline reference forced capture unavailable",
        };
      },
    };
    const captureUnavailable = await runTestnetConformanceHarness(
      executeRequest(harness),
      {
        expectedPayTo: harness.boundary.expectedPayTo,
        statePort: unavailableState,
        credentialPort: referenceCredentialPort(),
        canonicalPort: canonicalPort(
          { isValid: true, payer: PUBLIC_ACCOUNT },
          reference.observedAt,
        ),
      },
    );
    assert.equal(finalizedOutcomes, 1);
    cases.push(summarize("capture_unavailable", captureUnavailable, null));

    const binding = bindCanonicalReferenceDigest(
      dryRun,
      valid.result,
      valid.inspection.record?.requestDigest ?? "",
    );
    assert.deepEqual(binding, reference.binding);
    assert.throws(
      () =>
        bindCanonicalReferenceDigest(
          dryRun,
          valid.result,
          dryRun.plan?.requestDigest ?? "",
        ),
      (error: unknown) => {
        assert.ok(error instanceof TestnetCanonicalReferenceError);
        assert.equal(error.code, "testnet_reference_digest_mismatch");
        cases.push({
          id: "digest_mismatch",
          resultStatus: "blocked",
          resultCode: error.code,
          state: "confirmed",
          diagnosticStage: null,
          detailCode: null,
          evidenceStatus: null,
        });
        return true;
      },
    );

    assert.deepEqual(cases, reference.cases);
    assert.equal(externalNetworkCalls, 0);
    assert.deepEqual(reference.effectBoundary, {
      externalNetworkCalls: 0,
      keypairsGenerated: 0,
      signingCalls: 0,
      submitCalls: 0,
      settlementCalls: 0,
      serviceDispatchCalls: 0,
      actionPlanSubmitCalls: 0,
    });
    const publicSnapshot = JSON.stringify({ binding, cases });
    assert.doesNotMatch(publicSnapshot, new RegExp(RAW_SECRET_SENTINEL, "u"));
    for (const forbidden of reference.forbiddenFields) {
      assert.doesNotMatch(publicSnapshot, new RegExp(`"${forbidden}"`, "u"));
    }
  } finally {
    globalThis.fetch = originalFetch;
  }
});
