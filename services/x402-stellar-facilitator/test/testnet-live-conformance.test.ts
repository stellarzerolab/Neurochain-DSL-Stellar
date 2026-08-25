import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import type { VerifyResponse } from "@x402/core/types";

import {
  TESTNET_CREDENTIAL_HANDLE,
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
  await assert.rejects(
    forbidden.run(fixture.request, credential),
    /testnet_external_endpoint_forbidden/u,
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
  await assert.rejects(
    mismatch.run(fixture.request, secondCredential),
    /testnet_canonical_verify_failed/u,
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
