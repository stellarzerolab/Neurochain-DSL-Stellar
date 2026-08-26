import { Keypair } from "@stellar/stellar-sdk";
import type {
  PaymentPayload,
  PaymentRequirements,
  VerifyResponse,
} from "@x402/core/types";
import {
  STELLAR_TESTNET_CAIP2,
  createEd25519Signer,
  type ClientStellarSigner,
  type FacilitatorStellarSigner,
} from "@x402/stellar";
import { ExactStellarScheme as ClientExactStellarScheme } from "@x402/stellar/exact/client";
import { ExactStellarScheme as FacilitatorExactStellarScheme } from "@x402/stellar/exact/facilitator";

import {
  TESTNET_CREDENTIAL_HANDLE,
  TestnetCanonicalDiagnosticError,
  type CanonicalTestnetPort,
  type EphemeralTestnetCredential,
  type TestnetCredentialPort,
  type TestnetHarnessSafePlan,
} from "./testnet-conformance-harness.js";
import {
  INERT_TEST_SIGNER_ADDRESS,
  buildSupportedConformanceSnapshot,
} from "./supported-conformance.js";

const LIVE_CREDENTIAL_MARKER: unique symbol = Symbol(
  "liveTestnetCredentialMarker",
);

interface LiveCredentialHandle {
  readonly [LIVE_CREDENTIAL_MARKER]: true;
  readonly signer: ClientStellarSigner;
}

interface KeypairMaterial {
  readonly publicAccountId: string;
  readonly secretKey: string;
}

export interface EphemeralCredentialPortOptions {
  readonly createKeypairMaterial?: () => KeypairMaterial;
  readonly createSigner?: (
    secretKey: string,
    network: typeof STELLAR_TESTNET_CAIP2,
  ) => ClientStellarSigner;
}

export interface CanonicalSupportedVerifyOptions {
  readonly networkFetch?: typeof fetch;
  readonly now?: () => Date;
  readonly wait?: (milliseconds: number) => Promise<void>;
  readonly runUpstreamVerify?: (
    plan: TestnetHarnessSafePlan,
    signer: ClientStellarSigner,
  ) => Promise<VerifyResponse>;
}

let allowlistedFetchScopeActive = false;

function createKeypairMaterial(): KeypairMaterial {
  const keypair = Keypair.random();
  return Object.freeze({
    publicAccountId: keypair.publicKey(),
    secretKey: keypair.secret(),
  });
}

function isLiveCredentialHandle(value: unknown): value is LiveCredentialHandle {
  return (
    value !== null &&
    typeof value === "object" &&
    (value as Partial<LiveCredentialHandle>)[LIVE_CREDENTIAL_MARKER] === true &&
    typeof (value as Partial<LiveCredentialHandle>).signer?.address === "string" &&
    typeof (value as Partial<LiveCredentialHandle>).signer?.signAuthEntry ===
      "function"
  );
}

async function runCanonicalStage<T>(
  stage: ConstructorParameters<typeof TestnetCanonicalDiagnosticError>[0],
  operation: () => Promise<T> | T,
): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    if (error instanceof TestnetCanonicalDiagnosticError) {
      throw error;
    }
    throw new TestnetCanonicalDiagnosticError(stage);
  }
}

export function createEphemeralTestnetCredentialPort(
  options: EphemeralCredentialPortOptions = {},
): TestnetCredentialPort {
  let created = false;
  const materialFactory = options.createKeypairMaterial ?? createKeypairMaterial;
  const signerFactory = options.createSigner ?? createEd25519Signer;

  return Object.freeze({
    createEphemeral: async (): Promise<EphemeralTestnetCredential> => {
      if (created) {
        throw new Error(
          "testnet_credential_already_created:only one ephemeral credential is allowed per process",
        );
      }
      created = true;
      const material = materialFactory();
      const signer = signerFactory(material.secretKey, STELLAR_TESTNET_CAIP2);
      if (signer.address !== material.publicAccountId) {
        throw new Error(
          "testnet_credential_identity_mismatch:signer address differs from generated public account",
        );
      }
      const handle = Object.freeze({
        [LIVE_CREDENTIAL_MARKER]: true as const,
        signer,
      });
      return Object.freeze({
        publicAccountId: material.publicAccountId,
        [TESTNET_CREDENTIAL_HANDLE]: handle,
      });
    },
  });
}

function requestUrl(input: string | URL | Request): URL {
  return new URL(input instanceof Request ? input.url : input.toString());
}

function requestMethod(
  input: string | URL | Request,
  init?: RequestInit,
): string {
  return (init?.method ?? (input instanceof Request ? input.method : "GET"))
    .toUpperCase();
}

function hasExactSearchKeys(url: URL, expected: readonly string[]): boolean {
  return (
    JSON.stringify([...new Set(url.searchParams.keys())].sort()) ===
    JSON.stringify([...expected].sort())
  );
}

function assertAllowlistedRequest(
  plan: TestnetHarnessSafePlan,
  publicAccountId: string,
  input: string | URL | Request,
  init?: RequestInit,
): void {
  const url = requestUrl(input);
  const method = requestMethod(input, init);
  const friendbot = new URL(plan.friendbotUrl);
  const rpc = new URL(plan.rpcUrl);
  const horizon = new URL(plan.horizonUrl);

  if (url.origin === friendbot.origin) {
    if (
      url.pathname !== "/" ||
      method !== "GET" ||
      !hasExactSearchKeys(url, ["addr"]) ||
      url.searchParams.get("addr") !== publicAccountId
    ) {
      throw new TestnetCanonicalDiagnosticError("network_allowlist");
    }
    return;
  }

  if (url.origin === rpc.origin) {
    if (url.pathname !== "/" || url.search.length !== 0 || method !== "POST") {
      throw new TestnetCanonicalDiagnosticError("network_allowlist");
    }
    return;
  }

  if (url.origin === horizon.origin) {
    const accountPaths = new Set([
      `/accounts/${publicAccountId}`,
      `/accounts/${plan.payTo}`,
    ]);
    const accountRequest =
      accountPaths.has(url.pathname) &&
      url.search.length === 0 &&
      method === "GET";
    const ledgerRequest =
      url.pathname === "/ledgers" &&
      method === "GET" &&
      hasExactSearchKeys(url, ["limit", "order"]) &&
      url.searchParams.get("order") === "desc";
    if (!accountRequest && !ledgerRequest) {
      throw new TestnetCanonicalDiagnosticError("network_allowlist");
    }
    return;
  }

  throw new TestnetCanonicalDiagnosticError("network_allowlist");
}

async function withAllowlistedFetch<T>(
  plan: TestnetHarnessSafePlan,
  publicAccountId: string,
  networkFetch: typeof fetch,
  operation: () => Promise<T>,
): Promise<T> {
  if (allowlistedFetchScopeActive) {
    throw new TestnetCanonicalDiagnosticError("network_allowlist");
  }
  allowlistedFetchScopeActive = true;
  const originalFetch = globalThis.fetch;
  globalThis.fetch = ((input: string | URL | Request, init?: RequestInit) => {
    assertAllowlistedRequest(plan, publicAccountId, input, init);
    return networkFetch(input, init);
  }) as typeof fetch;
  try {
    return await operation();
  } finally {
    globalThis.fetch = originalFetch;
    allowlistedFetchScopeActive = false;
  }
}

async function requireOk(response: Response, code: string): Promise<void> {
  if (!response.ok) {
    throw new Error(`${code}:official testnet endpoint returned a non-success status`);
  }
  await response.body?.cancel().catch(() => undefined);
}

async function requireHorizonAccount(
  url: string,
  code: string,
  wait: (milliseconds: number) => Promise<void>,
): Promise<void> {
  const maximumAttempts = 8;
  for (let attempt = 1; attempt <= maximumAttempts; attempt += 1) {
    const response = await globalThis.fetch(url);
    if (response.ok) {
      await response.body?.cancel().catch(() => undefined);
      return;
    }
    const retryable =
      response.status === 404 || response.status === 429 || response.status === 503;
    await response.body?.cancel().catch(() => undefined);
    if (!retryable || attempt === maximumAttempts) {
      throw new Error(
        `${code}:funded testnet account did not become available within the bounded Horizon window`,
      );
    }
    await wait(1_000);
  }
}

function createInertFacilitatorSigner(): FacilitatorStellarSigner {
  const forbidden = async (): Promise<never> => {
    throw new Error(
      "testnet_settlement_signer_forbidden:verify-only conformance must not invoke facilitator signing",
    );
  };
  return Object.freeze({
    address: INERT_TEST_SIGNER_ADDRESS,
    signAuthEntry: forbidden,
    signTransaction: forbidden,
  });
}

async function runPinnedUpstreamVerify(
  plan: TestnetHarnessSafePlan,
  signer: ClientStellarSigner,
): Promise<VerifyResponse> {
  const requirements: PaymentRequirements = Object.freeze({
    scheme: "exact",
    network: STELLAR_TESTNET_CAIP2,
    asset: plan.asset,
    amount: plan.amount,
    payTo: plan.payTo,
    maxTimeoutSeconds: 300,
    extra: Object.freeze({ areFeesSponsored: true }),
  });
  const client = new ClientExactStellarScheme(signer, { url: plan.rpcUrl });
  const created = await runCanonicalStage("payment_payload_creation", () =>
    client.createPaymentPayload(2, requirements),
  );
  const payload: PaymentPayload = Object.freeze({
    ...created,
    accepted: requirements,
  });
  const facilitator = new FacilitatorExactStellarScheme(
    [createInertFacilitatorSigner()],
    {
      rpcConfig: { url: plan.rpcUrl },
      areFeesSponsored: true,
    },
  );
  return runCanonicalStage("upstream_verify", () =>
    facilitator.verify(payload, requirements),
  );
}

function assertCanonicalSupported(): void {
  const supported = buildSupportedConformanceSnapshot();
  const exactTestnet = supported.response.kinds.find(
    (kind) =>
      kind.x402Version === 2 &&
      kind.scheme === "exact" &&
      kind.network === STELLAR_TESTNET_CAIP2,
  );
  if (exactTestnet?.extra?.areFeesSponsored !== true) {
    throw new Error(
      "testnet_supported_invalid:pinned upstream did not advertise sponsored exact Stellar testnet support",
    );
  }
}

export function createCanonicalSupportedVerifyPort(
  options: CanonicalSupportedVerifyOptions = {},
): CanonicalTestnetPort {
  const networkFetch = options.networkFetch ?? globalThis.fetch;
  const now = options.now ?? (() => new Date());
  const wait =
    options.wait ??
    ((milliseconds: number) =>
      new Promise<void>((resolve) => setTimeout(resolve, milliseconds)));
  const runUpstreamVerify = options.runUpstreamVerify ?? runPinnedUpstreamVerify;

  return Object.freeze({
    run: async (
      plan: TestnetHarnessSafePlan,
      credential: EphemeralTestnetCredential,
    ): Promise<unknown> => {
      const handle = credential[TESTNET_CREDENTIAL_HANDLE];
      if (
        !isLiveCredentialHandle(handle) ||
        handle.signer.address !== credential.publicAccountId
      ) {
        throw new TestnetCanonicalDiagnosticError("credential_validation");
      }

      return withAllowlistedFetch(
        plan,
        credential.publicAccountId,
        networkFetch,
        async () => {
          await runCanonicalStage("supported_snapshot", assertCanonicalSupported);

          const friendbotUrl = new URL(plan.friendbotUrl);
          friendbotUrl.searchParams.set("addr", credential.publicAccountId);
          await runCanonicalStage("friendbot_funding", async () =>
            requireOk(
              await globalThis.fetch(friendbotUrl),
              "testnet_friendbot_funding_failed",
            ),
          );
          await runCanonicalStage("payer_horizon_readiness", () =>
            requireHorizonAccount(
              `${plan.horizonUrl}/accounts/${credential.publicAccountId}`,
              "testnet_payer_account_unavailable",
              wait,
            ),
          );
          await runCanonicalStage("recipient_horizon_readiness", () =>
            requireHorizonAccount(
              `${plan.horizonUrl}/accounts/${plan.payTo}`,
              "testnet_recipient_account_unavailable",
              wait,
            ),
          );

          const verify = await runCanonicalStage("upstream_verify", () =>
            runUpstreamVerify(plan, handle.signer),
          );
          if (!verify.isValid || verify.payer !== credential.publicAccountId) {
            throw new TestnetCanonicalDiagnosticError(
              "verify_result_validation",
            );
          }

          return Object.freeze({
            network: STELLAR_TESTNET_CAIP2,
            publicAccountId: credential.publicAccountId,
            transactionHash: null,
            ledger: null,
            status: "supported_verified" as const,
            observedAt: now().toISOString(),
            conformanceResults: Object.freeze([
              Object.freeze({
                check: "canonical_supported",
                status: "passed" as const,
                code: "supported_passed",
                reason: "canonical supported check passed",
              }),
              Object.freeze({
                check: "canonical_verify",
                status: "passed" as const,
                code: "verify_passed",
                reason: "canonical verify check passed",
              }),
            ]),
          });
        },
      );
    },
  });
}
