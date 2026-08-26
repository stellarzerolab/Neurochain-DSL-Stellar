import {
  sanitizeTestnetCanonicalDiagnostic,
  type TestnetCanonicalDiagnostic,
  type TestnetHarnessResult,
  type TestnetStateOutcome,
} from "./testnet-conformance-harness.js";

export const TESTNET_OUTCOME_CAPTURE_POSTMORTEM_SCHEMA_VERSION = 2 as const;

const DIGEST_PATTERN = /^[0-9a-f]{64}$/u;

const LOSS_WINDOWS = Object.freeze([
  Object.freeze({
    code: "dry_run_digest_used_as_execute_expectation",
    reason:
      "a caller can discard a valid execute result by comparing it with the intentionally different dry-run digest",
  }),
]);

interface DiagnosticSummary {
  readonly stage: string;
  readonly code: string;
  readonly detailCode: string | null;
  readonly retryAllowed: false;
}

export interface TestnetOutcomeCapturePostmortem {
  readonly schemaVersion: typeof TESTNET_OUTCOME_CAPTURE_POSTMORTEM_SCHEMA_VERSION;
  readonly source: Readonly<{
    harnessSchemaVersion: number;
    boundedAttempt: number;
  }>;
  readonly digestAuthority: Readonly<{
    dryRunDigest: string;
    executeDigest: string;
    authoritativeDigest: string;
    authoritativeSource: "execute_result.plan.requestDigest";
    digestsDistinct: true;
  }>;
  readonly returnedDiagnostic: Readonly<DiagnosticSummary>;
  readonly captureBoundary: Readonly<{
    diagnosticSurface: "execute_result.diagnostic";
    publicEvidenceSurface: "execute_result.evidence";
    persistedOutcomeSurface: "state_port.finalize.outcome";
    terminalState: "outcome_unknown";
    diagnosticPersisted: true;
    persistedDiagnostic: Readonly<DiagnosticSummary>;
    rawUpstreamPersisted: false;
    credentialPersisted: false;
    retryAllowed: false;
  }>;
  readonly lossWindows: typeof LOSS_WINDOWS;
}

export class TestnetOutcomeCapturePostmortemError extends Error {
  readonly code = "testnet_outcome_capture_postmortem_invalid" as const;

  constructor() {
    super("outcome capture postmortem inputs failed closed");
    this.name = "TestnetOutcomeCapturePostmortemError";
    Object.freeze(this);
  }
}

function failClosed(): never {
  throw new TestnetOutcomeCapturePostmortemError();
}

function allAuthoritiesRemainFalse(result: TestnetHarnessResult): boolean {
  const values = Object.values(result.authorityBoundary);
  return values.length === 11 && values.every((value) => value === false);
}

function summarizeDiagnostic(
  diagnostic: TestnetCanonicalDiagnostic,
): DiagnosticSummary {
  return Object.freeze({
    stage: diagnostic.stage,
    code: diagnostic.code,
    detailCode: diagnostic.detailCode ?? null,
    retryAllowed: false as const,
  });
}

function diagnosticsMatch(
  left: TestnetCanonicalDiagnostic,
  right: TestnetCanonicalDiagnostic,
): boolean {
  return (
    left.stage === right.stage &&
    left.code === right.code &&
    left.reason === right.reason &&
    left.retryAllowed === right.retryAllowed &&
    (left.detailCode ?? null) === (right.detailCode ?? null)
  );
}

export function buildTestnetOutcomeCapturePostmortem(
  dryRunResult: TestnetHarnessResult,
  executeResult: TestnetHarnessResult,
  persistedOutcome: TestnetStateOutcome,
): TestnetOutcomeCapturePostmortem {
  const dryRunPlan = dryRunResult.plan;
  const executePlan = executeResult.plan;
  const diagnostic = executeResult.diagnostic;
  const persistedDiagnostic =
    persistedOutcome.status === "outcome_unknown"
      ? sanitizeTestnetCanonicalDiagnostic(persistedOutcome.diagnostic)
      : null;

  if (
    dryRunResult.status !== "ready" ||
    dryRunResult.code !== "testnet_harness_ready" ||
    !dryRunPlan ||
    dryRunPlan.execute ||
    dryRunResult.evidence !== null ||
    dryRunResult.diagnostic !== null ||
    executeResult.status !== "blocked" ||
    executeResult.code !== "testnet_outcome_unknown" ||
    !executePlan ||
    !executePlan.execute ||
    executeResult.evidence !== null ||
    !diagnostic ||
    diagnostic.retryAllowed !== false ||
    persistedOutcome.status !== "outcome_unknown" ||
    !persistedDiagnostic ||
    !diagnosticsMatch(diagnostic, persistedDiagnostic) ||
    !DIGEST_PATTERN.test(dryRunPlan.requestDigest) ||
    !DIGEST_PATTERN.test(executePlan.requestDigest) ||
    dryRunPlan.requestDigest === executePlan.requestDigest ||
    !allAuthoritiesRemainFalse(dryRunResult) ||
    !allAuthoritiesRemainFalse(executeResult)
  ) {
    failClosed();
  }

  return Object.freeze({
    schemaVersion: TESTNET_OUTCOME_CAPTURE_POSTMORTEM_SCHEMA_VERSION,
    source: Object.freeze({
      harnessSchemaVersion: executePlan.schemaVersion,
      boundedAttempt: executePlan.attempt,
    }),
    digestAuthority: Object.freeze({
      dryRunDigest: dryRunPlan.requestDigest,
      executeDigest: executePlan.requestDigest,
      authoritativeDigest: executePlan.requestDigest,
      authoritativeSource: "execute_result.plan.requestDigest" as const,
      digestsDistinct: true as const,
    }),
    returnedDiagnostic: summarizeDiagnostic(diagnostic),
    captureBoundary: Object.freeze({
      diagnosticSurface: "execute_result.diagnostic" as const,
      publicEvidenceSurface: "execute_result.evidence" as const,
      persistedOutcomeSurface: "state_port.finalize.outcome" as const,
      terminalState: "outcome_unknown" as const,
      diagnosticPersisted: true as const,
      persistedDiagnostic: summarizeDiagnostic(persistedDiagnostic),
      rawUpstreamPersisted: false as const,
      credentialPersisted: false as const,
      retryAllowed: false as const,
    }),
    lossWindows: LOSS_WINDOWS,
  });
}
