import assert from "node:assert/strict";
import test from "node:test";

import { ExactStellarScheme } from "@x402/stellar/exact/facilitator";

import { inspectOfflineUpstreamApi } from "../src/upstream-api-smoke.js";

test("imports canonical upstream facilitator APIs without network authority", () => {
  const originalFetch = globalThis.fetch;
  let fetchCalls = 0;
  globalThis.fetch = (() => {
    fetchCalls += 1;
    throw new Error("offline_smoke_network_forbidden");
  }) as typeof fetch;

  try {
    const snapshot = inspectOfflineUpstreamApi();
    assert.equal(snapshot.coreConstructor, "x402Facilitator");
    assert.equal(snapshot.stellarConstructor, "ExactStellarScheme");
    assert.deepEqual(snapshot.coreMethods, ["register", "getSupported", "verify", "settle"]);
    assert.deepEqual(snapshot.stellarMethods, ["getExtra", "getSigners", "verify", "settle"]);
    assert.equal(snapshot.emptySupportedKinds, 0);
    assert.equal(snapshot.emptySupportedExtensions, 0);
    assert.equal(snapshot.emptySupportedSignerNetworks, 0);
    assert.equal(snapshot.networkAccessAllowed, false);
    assert.equal(snapshot.credentialUseAllowed, false);
    assert.equal(snapshot.signingAllowed, false);
    assert.equal(snapshot.settlementAllowed, false);
    assert.equal(snapshot.transactionSubmitAllowed, false);
    assert.equal(snapshot.actionPlanSubmitAllowed, false);
  } finally {
    globalThis.fetch = originalFetch;
  }

  assert.equal(fetchCalls, 0);
});

test("upstream Stellar scheme refuses an empty signer set", () => {
  assert.throws(() => new ExactStellarScheme([]), /At least one signer is required/);
});
