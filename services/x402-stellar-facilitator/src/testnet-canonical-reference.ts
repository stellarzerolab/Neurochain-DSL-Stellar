import type { TestnetHarnessResult } from "./testnet-conformance-harness.js";

const DIGEST_PATTERN = /^[0-9a-f]{64}$/u;

export interface TestnetCanonicalReferenceBinding {
  readonly dryRunDigest: string;
  readonly executeDigest: string;
  readonly persistedDigest: string;
  readonly authoritativeSource: "execute_result.plan.requestDigest";
  readonly digestsDistinct: true;
}

export class TestnetCanonicalReferenceError extends Error {
  readonly code = "testnet_reference_digest_mismatch" as const;

  constructor() {
    super("canonical offline reference digest binding failed closed");
    this.name = "TestnetCanonicalReferenceError";
    Object.freeze(this);
  }
}

export function bindCanonicalReferenceDigest(
  dryRunResult: TestnetHarnessResult,
  executeResult: TestnetHarnessResult,
  persistedDigest: string,
): TestnetCanonicalReferenceBinding {
  const dryRunPlan = dryRunResult.plan;
  const executePlan = executeResult.plan;
  if (
    dryRunResult.status !== "ready" ||
    !dryRunPlan ||
    dryRunPlan.execute ||
    executeResult.status !== "completed" ||
    !executePlan ||
    !executePlan.execute ||
    !DIGEST_PATTERN.test(dryRunPlan.requestDigest) ||
    !DIGEST_PATTERN.test(executePlan.requestDigest) ||
    !DIGEST_PATTERN.test(persistedDigest) ||
    dryRunPlan.requestDigest === executePlan.requestDigest ||
    persistedDigest !== executePlan.requestDigest
  ) {
    throw new TestnetCanonicalReferenceError();
  }
  return Object.freeze({
    dryRunDigest: dryRunPlan.requestDigest,
    executeDigest: executePlan.requestDigest,
    persistedDigest,
    authoritativeSource: "execute_result.plan.requestDigest" as const,
    digestsDistinct: true as const,
  });
}
