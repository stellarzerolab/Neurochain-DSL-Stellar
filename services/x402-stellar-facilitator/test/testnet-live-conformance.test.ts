import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import type { VerifyResponse } from "@x402/core/types";

import {
  TESTNET_CREDENTIAL_HANDLE,
  TestnetCanonicalDiagnosticError,
  type TestnetHarnessSafePlan,
} from "../src/testnet-conformance-harness.js";
import {
  createCanonicalSupportedVerifyPort,
  createEphemeralTestnetCredentialPort,
} from "../src/testnet-live-conformance.js";

const PUBLIC_ACCOUNT =
  "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
const SECRET_SENTINEL = "SSECRET_SENTINEL_MUST_NEVER_ESCAPE";

interface HarnessFixture {
  readonly request: TestnetHarnessSafePlan;
}

async function readFixture(): Promise<HarnessFixture> {
  const fixtureUrl = new URL(
    "../../fixtures/testnet-harness-v2.expected.json",
    import.meta.url,
  );
  return JSON.parse(await readFile(fixtureUrl, "utf8")) as HarnessFixture;
}

async function assertDiagnostic(
  operation: Promise<unknown>,
  stage: TestnetCanonicalDiagnosticError["diagnostic"]["stage"],
): Promise<void> {
  await assert.rejects(operation, (error: unknown) => {
    assert.ok(error instanceof TestnetCanonicalDiagnosticError);
    assert.equal(error.diagnostic.stage, stage);
    assert.ok(error.diagnostic.code.trim().length > 0);
    assert.ok(error.diagnostic.reason.trim().length > 0);
    assert.equal(error.diagnostic.retryAllowed, false);
    assert.doesNotMatch(
      JSON.stringify(error),
      new RegExp(SECRET_SENTINEL, "u"),
    );
    return true;
  });
}

function testCredentialPort() {
  return createEphemeralTestnetCredentialPort({
    createKeypairMaterial: () => ({
      publicAccountId: PUBLIC_ACCOUNT,
      secretKey: SECRET_SENTINEL,
    }),
    createSigner: (secretKey) => {
      assert.equal(secretKey, SECRET_SENTINEL);
      return {
        address: PUBLIC_ACCOUNT,
        signAuthEntry: async () => ({
          signedAuthEntry: "signed-public-auth-entry",
        }),
      };
    },
  });
}

test("ephemeral credential is one-shot, opaque and never JSON-serializes its secret", async () => {
  const port = testCredentialPort();
  const credential = await port.createEphemeral();
  assert.equal(credential.publicAccountId, PUBLIC_ACCOUNT);
  assert.ok(credential[TESTNET_CREDENTIAL_HANDLE]);
  assert.doesNotMatch(JSON.stringify(credential), new RegExp(SECRET_SENTINEL, "u"));
  await assert.rejects(port.createEphemeral(), /testnet_credential_already_created/u);
});

test("canonical adapter uses only the official bounded endpoints and returns public verify evidence", async () => {
  const fixture = await readFixture();
  const credential = await testCredentialPort().createEphemeral();
  const calls: string[] = [];
  let upstreamCalls = 0;
  const networkFetch: typeof fetch = async (input) => {
    calls.push(input instanceof Request ? input.url : input.toString());
    return new Response(null, { status: 200 });
  };
  const port = createCanonicalSupportedVerifyPort({
    networkFetch,
    now: () => new Date("2026-08-24T12:00:00.000Z"),
    runUpstreamVerify: async (_plan, signer): Promise<VerifyResponse> => {
      upstreamCalls += 1;
      assert.equal(signer.address, PUBLIC_ACCOUNT);
      return { isValid: true, payer: PUBLIC_ACCOUNT };
    },
  });

  const evidence = await port.run(fixture.request, credential);
  assert.deepEqual(calls, [
    `https://friendbot.stellar.org/?addr=${PUBLIC_ACCOUNT}`,
    `https://horizon-testnet.stellar.org/accounts/${PUBLIC_ACCOUNT}`,
    `https://horizon-testnet.stellar.org/accounts/${fixture.request.payTo}`,
  ]);
  assert.equal(upstreamCalls, 1);
  assert.deepEqual(evidence, {
    network: "stellar:testnet",
    publicAccountId: PUBLIC_ACCOUNT,
    transactionHash: null,
    ledger: null,
    status: "supported_verified",
    observedAt: "2026-08-24T12:00:00.000Z",
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
  });
  assert.doesNotMatch(JSON.stringify(evidence), new RegExp(SECRET_SENTINEL, "u"));
});

test("external network attempts and payer mismatches fail closed without secret echo", async () => {
  const fixture = await readFixture();
  const credential = await testCredentialPort().createEphemeral();
  let networkCalls = 0;
  const networkFetch: typeof fetch = async () => {
    networkCalls += 1;
    return new Response(null, { status: 200 });
  };
  const forbidden = createCanonicalSupportedVerifyPort({
    networkFetch,
    runUpstreamVerify: async () => {
      await globalThis.fetch("https://example.invalid/escape");
      return { isValid: true, payer: PUBLIC_ACCOUNT };
    },
  });
  await assertDiagnostic(
    forbidden.run(fixture.request, credential),
    "network_allowlist",
  );
  assert.equal(networkCalls, 3);

  const secondCredential = await testCredentialPort().createEphemeral();
  const mismatch = createCanonicalSupportedVerifyPort({
    networkFetch,
    runUpstreamVerify: async () => ({
      isValid: true,
      payer: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    }),
  });
  await assertDiagnostic(
    mismatch.run(fixture.request, secondCredential),
    "verify_result_validation",
  );
});

test("bounded Horizon readiness retries transient account indexing lag only", async () => {
  const fixture = await readFixture();
  const credential = await testCredentialPort().createEphemeral();
  let payerAttempts = 0;
  let waits = 0;
  const networkFetch: typeof fetch = async (input) => {
    const url = input instanceof Request ? input.url : input.toString();
    if (url.endsWith(`/accounts/${PUBLIC_ACCOUNT}`)) {
      payerAttempts += 1;
      return new Response(null, { status: payerAttempts < 3 ? 404 : 200 });
    }
    return new Response(null, { status: 200 });
  };
  const port = createCanonicalSupportedVerifyPort({
    networkFetch,
    wait: async (milliseconds) => {
      assert.equal(milliseconds, 1_000);
      waits += 1;
    },
    runUpstreamVerify: async () => ({
      isValid: true,
      payer: PUBLIC_ACCOUNT,
    }),
  });

  const evidence = await port.run(fixture.request, credential);
  assert.equal(payerAttempts, 3);
  assert.equal(waits, 2);
  assert.equal(
    (evidence as { readonly status: string }).status,
    "supported_verified",
  );
});

test("canonical adapter exposes only stable redacted failure stages", async () => {
  const fixture = await readFixture();

  const friendbotFailure = createCanonicalSupportedVerifyPort({
    networkFetch: async (input) => {
      const url = input instanceof Request ? input.url : input.toString();
      return new Response(null, {
        status: url.startsWith("https://friendbot.stellar.org/") ? 503 : 200,
      });
    },
    runUpstreamVerify: async () => ({ isValid: true, payer: PUBLIC_ACCOUNT }),
  });
  await assertDiagnostic(
    friendbotFailure.run(
      fixture.request,
      await testCredentialPort().createEphemeral(),
    ),
    "friendbot_funding",
  );

  const payerReadinessFailure = createCanonicalSupportedVerifyPort({
    networkFetch: async (input) => {
      const url = input instanceof Request ? input.url : input.toString();
      return new Response(null, {
        status: url.endsWith(`/accounts/${PUBLIC_ACCOUNT}`) ? 500 : 200,
      });
    },
    runUpstreamVerify: async () => ({ isValid: true, payer: PUBLIC_ACCOUNT }),
  });
  await assertDiagnostic(
    payerReadinessFailure.run(
      fixture.request,
      await testCredentialPort().createEphemeral(),
    ),
    "payer_horizon_readiness",
  );

  const recipientReadinessFailure = createCanonicalSupportedVerifyPort({
    networkFetch: async (input) => {
      const url = input instanceof Request ? input.url : input.toString();
      return new Response(null, {
        status: url.endsWith(`/accounts/${fixture.request.payTo}`) ? 500 : 200,
      });
    },
    runUpstreamVerify: async () => ({ isValid: true, payer: PUBLIC_ACCOUNT }),
  });
  await assertDiagnostic(
    recipientReadinessFailure.run(
      fixture.request,
      await testCredentialPort().createEphemeral(),
    ),
    "recipient_horizon_readiness",
  );

  const upstreamFailure = createCanonicalSupportedVerifyPort({
    networkFetch: async () => new Response(null, { status: 200 }),
    runUpstreamVerify: async () => {
      throw new Error(SECRET_SENTINEL);
    },
  });
  await assertDiagnostic(
    upstreamFailure.run(
      fixture.request,
      await testCredentialPort().createEphemeral(),
    ),
    "upstream_verify",
  );

  const payloadFailure = createCanonicalSupportedVerifyPort({
    networkFetch: async () => new Response(null, { status: 200 }),
    runUpstreamVerify: async () => {
      throw new TestnetCanonicalDiagnosticError("payment_payload_creation");
    },
  });
  await assertDiagnostic(
    payloadFailure.run(
      fixture.request,
      await testCredentialPort().createEphemeral(),
    ),
    "payment_payload_creation",
  );
});
