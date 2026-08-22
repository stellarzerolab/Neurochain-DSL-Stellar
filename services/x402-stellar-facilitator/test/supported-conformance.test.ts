import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  buildSupportedConformanceSnapshot,
  evaluateSupportedConformance,
  INERT_TEST_SIGNER_ADDRESS,
  PINNED_SOURCE_PACKAGES,
  SUPPORTED_NETWORKS,
} from "../src/supported-conformance.js";

async function readFixture(): Promise<unknown> {
  const fixtureUrl = new URL(
    "../../fixtures/supported-v2.expected.json",
    import.meta.url,
  );
  return JSON.parse(await readFile(fixtureUrl, "utf8")) as unknown;
}

async function readManifest(): Promise<{
  readonly dependencies: Readonly<Record<string, string>>;
}> {
  const manifestUrl = new URL("../../package.json", import.meta.url);
  return JSON.parse(await readFile(manifestUrl, "utf8")) as {
    readonly dependencies: Readonly<Record<string, string>>;
  };
}

function cloneFixture(value: unknown): Record<string, unknown> {
  return JSON.parse(JSON.stringify(value)) as Record<string, unknown>;
}

test("canonical upstream Stellar exact /supported matches the offline fixture", async () => {
  const originalFetch = globalThis.fetch;
  let fetchCalls = 0;
  globalThis.fetch = (() => {
    fetchCalls += 1;
    throw new Error("offline_supported_network_forbidden");
  }) as typeof fetch;

  try {
    const fixture = await readFixture();
    const manifest = await readManifest();
    const snapshot = buildSupportedConformanceSnapshot();
    assert.deepEqual(snapshot, fixture);
    assert.deepEqual(snapshot.sourcePackages, PINNED_SOURCE_PACKAGES);
    assert.deepEqual(manifest.dependencies, PINNED_SOURCE_PACKAGES);
    assert.deepEqual(
      snapshot.response.kinds.map(({ network }) => network),
      SUPPORTED_NETWORKS,
    );
    assert.deepEqual(
      snapshot.response.kinds.map(({ scheme, extra }) => ({ scheme, extra })),
      [
        { scheme: "exact", extra: { areFeesSponsored: true } },
        { scheme: "exact", extra: { areFeesSponsored: true } },
      ],
    );
    assert.deepEqual(snapshot.response.signers, {
      "stellar:*": [INERT_TEST_SIGNER_ADDRESS],
    });
    assert.equal(snapshot.authorityBoundary.signerMethodCalls, 0);
    assert.equal(snapshot.authorityBoundary.verifyMethodCalls, 0);
    assert.equal(snapshot.authorityBoundary.settleMethodCalls, 0);
    assert.deepEqual(evaluateSupportedConformance(fixture), {
      status: "ready",
      code: "supported_conformance_ready",
      reason:
        "canonical upstream Stellar exact supported response matches the offline fixture",
    });
  } finally {
    globalThis.fetch = originalFetch;
  }

  assert.equal(fetchCalls, 0);
});

test("package, wire and authority drift fail closed with stable non-empty reasons", async () => {
  const fixture = await readFixture();

  const packageDrift = cloneFixture(fixture);
  packageDrift.sourcePackages = {
    "@x402/core": "2.23.0",
    "@x402/stellar": "9.9.9",
  };

  const wireDrift = cloneFixture(fixture);
  const response = wireDrift.response as { kinds: Array<Record<string, unknown>> };
  response.kinds[0] = { ...response.kinds[0], network: "stellar:unknown" };

  const authorityDrift = cloneFixture(fixture);
  const authorityBoundary = authorityDrift.authorityBoundary as Record<
    string,
    unknown
  >;
  authorityBoundary.signingAllowed = true;

  const extraField = cloneFixture(fixture);
  extraField.unapproved = true;

  const cases = [
    [packageDrift, "source_package_drift"],
    [wireDrift, "supported_wire_drift"],
    [authorityDrift, "authority_boundary_violated"],
    [extraField, "supported_fixture_invalid"],
  ] as const;

  for (const [candidate, code] of cases) {
    const result = evaluateSupportedConformance(candidate);
    assert.equal(result.status, "invalid");
    assert.equal(result.code, code);
    assert.ok(result.reason.trim().length > 0);
  }
});
