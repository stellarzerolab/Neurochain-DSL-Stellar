import assert from "node:assert/strict";
import {
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

import {
  TESTNET_CREDENTIAL_HANDLE,
  TESTNET_HARNESS_CONFIRMATION,
  TestnetCanonicalDiagnosticError,
  runTestnetConformanceHarness,
  type TestnetCanonicalDiagnostic,
  type TestnetPublicEvidence,
} from "../src/testnet-conformance-harness.js";
import {
  LocalTestnetStateAdapter,
  TESTNET_LEGACY_STATE_SCHEMA_VERSION,
  TESTNET_LOCAL_STATE_DIRECTORY,
  TESTNET_STATE_SCHEMA_VERSION,
} from "../src/testnet-state-adapter.js";

const DIGEST_A = "a".repeat(64);
const DIGEST_B = "b".repeat(64);
const PUBLIC_ACCOUNT =
  "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
const SECRET_SENTINEL = "TOP_SECRET_SENTINEL_MUST_NEVER_PERSIST";

interface StateFixture {
  readonly schemaVersion: 2;
  readonly legacySchemaVersion: 1;
  readonly localDirectory: string;
  readonly requestDigest: string;
  readonly reservationId: string;
  readonly storedFields: readonly string[];
  readonly states: readonly string[];
  readonly transitions: ReadonlyArray<{
    readonly operation: string;
    readonly from: string | null;
    readonly to: string;
    readonly code: string;
  }>;
  readonly forbiddenFields: readonly string[];
}

async function readStateFixture(): Promise<StateFixture> {
  const fixtureUrl = new URL(
    "../../fixtures/testnet-state-v2.expected.json",
    import.meta.url,
  );
  return JSON.parse(await readFile(fixtureUrl, "utf8")) as StateFixture;
}

function diagnostic(
  stage: ConstructorParameters<typeof TestnetCanonicalDiagnosticError>[0] =
    "upstream_verify",
): TestnetCanonicalDiagnostic {
  return new TestnetCanonicalDiagnosticError(stage).diagnostic;
}

async function readHarnessFixture(): Promise<{
  readonly request: Record<string, unknown>;
  readonly boundary: { readonly expectedPayTo: string };
}> {
  const fixtureUrl = new URL(
    "../../fixtures/testnet-harness-v3.expected.json",
    import.meta.url,
  );
  return JSON.parse(await readFile(fixtureUrl, "utf8")) as {
    readonly request: Record<string, unknown>;
    readonly boundary: { readonly expectedPayTo: string };
  };
}

async function withTemporaryWorkspace(
  run: (workspaceRoot: string) => Promise<void>,
): Promise<void> {
  const workspaceRoot = await mkdtemp(
    join(tmpdir(), "neurochain-x402-testnet-state-"),
  );
  try {
    await run(resolve(workspaceRoot));
  } finally {
    assert.ok(resolve(workspaceRoot).startsWith(resolve(tmpdir())));
    await rm(workspaceRoot, { recursive: true, force: true });
  }
}

function fixedClock(...timestamps: readonly string[]): () => Date {
  let index = 0;
  return () => {
    const timestamp = timestamps[Math.min(index, timestamps.length - 1)];
    index += 1;
    if (!timestamp) {
      throw new Error("test clock requires at least one timestamp");
    }
    return new Date(timestamp);
  };
}

function settledEvidence(): TestnetPublicEvidence {
  return {
    network: "stellar:testnet",
    publicAccountId: PUBLIC_ACCOUNT,
    transactionHash: "c".repeat(64),
    ledger: 123456,
    status: "settled",
    observedAt: "2026-08-24T10:10:00.000Z",
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
      {
        check: "canonical_settlement",
        status: "passed",
        code: "settlement_confirmed",
        reason: SECRET_SENTINEL,
      },
    ],
  };
}

test("state fixture and ignored local directory lock the public storage boundary", async () => {
  const fixture = await readStateFixture();
  const ignore = await readFile(
    new URL("../../../../.gitignore", import.meta.url),
    "utf8",
  );
  assert.equal(fixture.schemaVersion, TESTNET_STATE_SCHEMA_VERSION);
  assert.equal(fixture.localDirectory, TESTNET_LOCAL_STATE_DIRECTORY);
  assert.equal(fixture.requestDigest, DIGEST_A);
  assert.equal(fixture.reservationId, `tstate_${DIGEST_A}`);
  assert.deepEqual(fixture.states, [
    "attempted",
    "outcome_unknown",
    "confirmed",
  ]);
  assert.match(
    ignore,
    /\/services\/x402-stellar-facilitator\/\.local-testnet-state\//u,
  );
  assert.deepEqual(
    fixture.transitions.map(({ code }) => code),
    [
      "testnet_state_reserved",
      "testnet_state_duplicate",
      "testnet_state_outcome_unknown",
      "testnet_state_outcome_unknown",
      "testnet_state_confirmed",
      "testnet_state_replay",
    ],
  );
});

test("harness dry-run never initializes the injected local state adapter", async () => {
  await withTemporaryWorkspace(async (workspaceRoot) => {
    const fixture = await readHarnessFixture();
    const adapter = new LocalTestnetStateAdapter({ workspaceRoot });
    const result = await runTestnetConformanceHarness(fixture.request, {
      expectedPayTo: fixture.boundary.expectedPayTo,
      statePort: adapter,
    });
    assert.equal(result.code, "testnet_harness_ready");
    await assert.rejects(
      lstat(join(workspaceRoot, TESTNET_LOCAL_STATE_DIRECTORY)),
      (error: unknown) =>
        error !== null &&
        typeof error === "object" &&
        "code" in error &&
        (error as { readonly code: string }).code === "ENOENT",
    );
  });
});

test("reserve is atomic, bounded and duplicate-safe", async () => {
  await withTemporaryWorkspace(async (workspaceRoot) => {
    const fixture = await readStateFixture();
    const adapter = new LocalTestnetStateAdapter({
      workspaceRoot,
      now: fixedClock("2026-08-24T10:00:00.000Z"),
    });
    const reserved = await adapter.reserve(DIGEST_A);
    assert.deepEqual(reserved, {
      status: "reserved",
      reservationId: `tstate_${DIGEST_A}`,
      code: "testnet_state_reserved",
      reason: "the bounded testnet request was reserved atomically",
    });
    const duplicate = await adapter.reserve(DIGEST_A);
    assert.equal(duplicate.status, "unavailable");
    assert.equal(duplicate.code, "testnet_state_duplicate");
    assert.ok(duplicate.reason.trim().length > 0);

    const inspection = await adapter.inspect(DIGEST_A);
    assert.equal(inspection.status, "recorded");
    assert.equal(inspection.record?.state, "attempted");
    const stateRoot = join(workspaceRoot, TESTNET_LOCAL_STATE_DIRECTORY);
    const raw = await readFile(join(stateRoot, `${DIGEST_A}.json`), "utf8");
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    assert.deepEqual(Object.keys(parsed).sort(), [...fixture.storedFields].sort());
    for (const forbidden of fixture.forbiddenFields) {
      assert.doesNotMatch(raw, new RegExp(forbidden, "iu"));
    }
    assert.deepEqual(await readdir(stateRoot), [`${DIGEST_A}.json`]);
  });
});

test("concurrent reservation has one winner and never leaves a lock or temp file", async () => {
  await withTemporaryWorkspace(async (workspaceRoot) => {
    const adapter = new LocalTestnetStateAdapter({
      workspaceRoot,
      now: fixedClock("2026-08-24T10:01:00.000Z"),
    });
    const results = await Promise.all(
      Array.from({ length: 12 }, async () => adapter.reserve(DIGEST_B)),
    );
    assert.equal(
      results.filter(({ status }) => status === "reserved").length,
      1,
    );
    for (const result of results.filter(
      ({ status }) => status === "unavailable",
    )) {
      assert.ok(
        result.code === "testnet_state_locked" ||
          result.code === "testnet_state_duplicate",
      );
      assert.ok(result.reason.trim().length > 0);
    }
    assert.deepEqual(
      await readdir(join(workspaceRoot, TESTNET_LOCAL_STATE_DIRECTORY)),
      [`${DIGEST_B}.json`],
    );
  });
});

test("restart converts an interrupted attempt to outcome unknown and blocks retry", async () => {
  await withTemporaryWorkspace(async (workspaceRoot) => {
    const firstProcess = new LocalTestnetStateAdapter({
      workspaceRoot,
      now: fixedClock("2026-08-24T10:02:00.000Z"),
    });
    const reserved = await firstProcess.reserve(DIGEST_A);
    assert.equal(reserved.status, "reserved");

    const restarted = new LocalTestnetStateAdapter({
      workspaceRoot,
      now: fixedClock("2026-08-24T10:03:00.000Z"),
    });
    const recovered = await restarted.inspect(DIGEST_A);
    assert.equal(recovered.status, "recorded");
    assert.equal(recovered.record?.state, "outcome_unknown");
    assert.equal(recovered.record?.evidence, null);
    assert.equal(
      recovered.record?.schemaVersion === TESTNET_STATE_SCHEMA_VERSION
        ? recovered.record.diagnostic?.stage
        : null,
      "canonical_port_unknown",
    );
    const retry = await restarted.reserve(DIGEST_A);
    assert.equal(retry.status, "unavailable");
    assert.equal(retry.code, "testnet_state_outcome_unknown");

    if (reserved.status !== "reserved") {
      assert.fail("reservation unexpectedly unavailable");
    }
    const lateFinalize = await firstProcess.finalize(reserved.reservationId, {
      status: "confirmed",
      evidence: settledEvidence(),
    });
    assert.equal(lateFinalize.status, "unavailable");
    assert.equal(lateFinalize.code, "testnet_state_outcome_unknown");
  });
});

test("confirmed state stores only normalized public evidence and blocks replay", async () => {
  await withTemporaryWorkspace(async (workspaceRoot) => {
    const adapter = new LocalTestnetStateAdapter({
      workspaceRoot,
      now: fixedClock(
        "2026-08-24T10:04:00.000Z",
        "2026-08-24T10:11:00.000Z",
      ),
    });
    const reserved = await adapter.reserve(DIGEST_A);
    if (reserved.status !== "reserved") {
      assert.fail("reservation unexpectedly unavailable");
    }
    const evidence = settledEvidence();
    const finalized = await adapter.finalize(reserved.reservationId, {
      status: "confirmed",
      evidence,
    });
    assert.equal(finalized.status, "recorded");
    assert.equal(finalized.code, "testnet_state_confirmed");

    const inspection = await adapter.inspect(DIGEST_A);
    assert.equal(inspection.status, "recorded");
    assert.equal(inspection.record?.state, "confirmed");
    assert.equal(inspection.record?.evidence?.transactionHash, "c".repeat(64));
    assert.deepEqual(
      inspection.record?.evidence?.conformanceResults.map(({ reason }) => reason),
      [
        "canonical supported check passed",
        "canonical verify check passed",
        "canonical settlement was confirmed on Stellar testnet",
      ],
    );
    const raw = await readFile(
      join(
        workspaceRoot,
        TESTNET_LOCAL_STATE_DIRECTORY,
        `${DIGEST_A}.json`,
      ),
      "utf8",
    );
    assert.doesNotMatch(raw, new RegExp(SECRET_SENTINEL, "u"));
    const replay = await adapter.reserve(DIGEST_A);
    assert.equal(replay.status, "unavailable");
    assert.equal(replay.code, "testnet_state_replay");
    const idempotent = await adapter.finalize(reserved.reservationId, {
      status: "confirmed",
      evidence,
    });
    assert.equal(idempotent.status, "recorded");
    assert.equal(idempotent.code, "testnet_state_confirmed");
  });
});

test("explicit outcome unknown atomically stores only an allowlisted diagnostic", async () => {
  await withTemporaryWorkspace(async (workspaceRoot) => {
    const adapter = new LocalTestnetStateAdapter({
      workspaceRoot,
      now: fixedClock(
        "2026-08-24T10:05:00.000Z",
        "2026-08-24T10:06:00.000Z",
      ),
    });
    const reserved = await adapter.reserve(DIGEST_A);
    if (reserved.status !== "reserved") {
      assert.fail("reservation unexpectedly unavailable");
    }
    const finalized = await adapter.finalize(reserved.reservationId, {
      status: "outcome_unknown",
      diagnostic: diagnostic(),
    });
    assert.equal(finalized.status, "recorded");
    assert.equal(finalized.code, "testnet_state_outcome_unknown");
    const inspection = await adapter.inspect(DIGEST_A);
    assert.equal(inspection.record?.state, "outcome_unknown");
    assert.equal(inspection.record?.evidence, null);
    assert.equal(
      inspection.record?.schemaVersion === TESTNET_STATE_SCHEMA_VERSION
        ? inspection.record.diagnostic?.stage
        : null,
      "upstream_verify",
    );
    const raw = await readFile(
      join(
        workspaceRoot,
        TESTNET_LOCAL_STATE_DIRECTORY,
        `${DIGEST_A}.json`,
      ),
      "utf8",
    );
    assert.doesNotMatch(raw, new RegExp(SECRET_SENTINEL, "u"));
    const retry = await adapter.reserve(DIGEST_A);
    assert.equal(retry.code, "testnet_state_outcome_unknown");
  });
});

test("unknown diagnostics fail closed without persisting raw or secret data", async () => {
  await withTemporaryWorkspace(async (workspaceRoot) => {
    const adapter = new LocalTestnetStateAdapter({
      workspaceRoot,
      now: fixedClock("2026-08-24T10:13:00.000Z"),
    });
    const reserved = await adapter.reserve(DIGEST_A);
    if (reserved.status !== "reserved") {
      assert.fail("reservation unexpectedly unavailable");
    }
    const result = await adapter.finalize(reserved.reservationId, {
      status: "outcome_unknown",
      diagnostic: {
        stage: "upstream_verify",
        code: "unknown_raw_code",
        reason: SECRET_SENTINEL,
        retryAllowed: false,
      } as TestnetCanonicalDiagnostic,
    });
    assert.equal(result.status, "unavailable");
    assert.equal(result.code, "testnet_state_diagnostic_invalid");
    assert.doesNotMatch(JSON.stringify(result), new RegExp(SECRET_SENTINEL, "u"));
    const raw = await readFile(
      join(
        workspaceRoot,
        TESTNET_LOCAL_STATE_DIRECTORY,
        `${DIGEST_A}.json`,
      ),
      "utf8",
    );
    assert.doesNotMatch(raw, new RegExp(SECRET_SENTINEL, "u"));
    assert.equal((JSON.parse(raw) as { readonly state: string }).state, "attempted");
  });
});

test("terminal diagnostic mismatch fails closed and preserves the first capture", async () => {
  await withTemporaryWorkspace(async (workspaceRoot) => {
    const adapter = new LocalTestnetStateAdapter({
      workspaceRoot,
      now: fixedClock(
        "2026-08-24T10:14:00.000Z",
        "2026-08-24T10:15:00.000Z",
      ),
    });
    const reserved = await adapter.reserve(DIGEST_A);
    if (reserved.status !== "reserved") {
      assert.fail("reservation unexpectedly unavailable");
    }
    const first = await adapter.finalize(reserved.reservationId, {
      status: "outcome_unknown",
      diagnostic: diagnostic("upstream_verify"),
    });
    assert.equal(first.status, "recorded");
    const mismatch = await adapter.finalize(reserved.reservationId, {
      status: "outcome_unknown",
      diagnostic: diagnostic("canonical_port_unknown"),
    });
    assert.equal(mismatch.status, "unavailable");
    assert.equal(mismatch.code, "testnet_state_diagnostic_mismatch");
    const inspection = await adapter.inspect(DIGEST_A);
    assert.equal(
      inspection.record?.schemaVersion === TESTNET_STATE_SCHEMA_VERSION
        ? inspection.record.diagnostic?.stage
        : null,
      "upstream_verify",
    );
  });
});

test("legacy schema-v1 terminal records remain readable and byte-identical", async () => {
  await withTemporaryWorkspace(async (workspaceRoot) => {
    const stateRoot = join(workspaceRoot, TESTNET_LOCAL_STATE_DIRECTORY);
    await mkdir(stateRoot, { recursive: true });
    const legacy = JSON.stringify({
      schemaVersion: TESTNET_LEGACY_STATE_SCHEMA_VERSION,
      requestDigest: DIGEST_A,
      reservationId: `tstate_${DIGEST_A}`,
      state: "outcome_unknown",
      admittedAt: "2026-08-24T10:16:00.000Z",
      attemptedAt: "2026-08-24T10:16:00.000Z",
      completedAt: "2026-08-24T10:17:00.000Z",
      evidence: null,
    });
    const recordPath = join(stateRoot, `${DIGEST_A}.json`);
    await writeFile(recordPath, legacy, "utf8");
    const adapter = new LocalTestnetStateAdapter({ workspaceRoot });
    const inspection = await adapter.inspect(DIGEST_A);
    assert.equal(inspection.status, "recorded");
    assert.equal(inspection.record?.schemaVersion, 1);
    assert.equal(await readFile(recordPath, "utf8"), legacy);
  });
});

test("partial state writes fail closed without diagnostic inference", async () => {
  await withTemporaryWorkspace(async (workspaceRoot) => {
    const stateRoot = join(workspaceRoot, TESTNET_LOCAL_STATE_DIRECTORY);
    await mkdir(stateRoot, { recursive: true });
    await writeFile(
      join(stateRoot, `${DIGEST_A}.json`),
      `{"schemaVersion":2,"requestDigest":"${DIGEST_A}"`,
      "utf8",
    );
    const adapter = new LocalTestnetStateAdapter({ workspaceRoot });
    const result = await adapter.inspect(DIGEST_A);
    assert.equal(result.status, "unavailable");
    assert.equal(result.code, "testnet_state_record_invalid");
    assert.doesNotMatch(JSON.stringify(result), /upstream_verify|canonical_port_unknown/u);
  });
});

test("invalid digests, corrupt records and unexpected root entries fail closed", async () => {
  await withTemporaryWorkspace(async (workspaceRoot) => {
    const adapter = new LocalTestnetStateAdapter({ workspaceRoot });
    const traversal = await adapter.reserve("../outside.json");
    assert.equal(traversal.status, "unavailable");
    assert.equal(traversal.code, "testnet_state_request_invalid");
    await assert.rejects(
      lstat(join(workspaceRoot, TESTNET_LOCAL_STATE_DIRECTORY)),
    );

    const stateRoot = join(workspaceRoot, TESTNET_LOCAL_STATE_DIRECTORY);
    await mkdir(stateRoot, { recursive: true });
    await writeFile(join(stateRoot, `${DIGEST_A}.json`), "{not-json", "utf8");
    const corrupt = new LocalTestnetStateAdapter({ workspaceRoot });
    const result = await corrupt.inspect(DIGEST_A);
    assert.equal(result.status, "unavailable");
    assert.equal(result.code, "testnet_state_record_invalid");
    assert.ok(result.reason.trim().length > 0);
  });

  await withTemporaryWorkspace(async (workspaceRoot) => {
    const stateRoot = join(workspaceRoot, TESTNET_LOCAL_STATE_DIRECTORY);
    await mkdir(stateRoot, { recursive: true });
    await writeFile(join(stateRoot, "unexpected.secret"), SECRET_SENTINEL, "utf8");
    const adapter = new LocalTestnetStateAdapter({ workspaceRoot });
    const result = await adapter.inspect(DIGEST_A);
    assert.equal(result.status, "unavailable");
    assert.equal(result.code, "testnet_state_root_unsafe");
    assert.doesNotMatch(JSON.stringify(result), new RegExp(SECRET_SENTINEL, "u"));
  });
});

test("symlinked state roots fail closed instead of escaping the workspace", async (t) => {
  await withTemporaryWorkspace(async (workspaceRoot) => {
    const externalTarget = await mkdtemp(
      join(tmpdir(), "neurochain-x402-external-state-"),
    );
    try {
      const stateRoot = join(workspaceRoot, TESTNET_LOCAL_STATE_DIRECTORY);
      try {
        await symlink(
          externalTarget,
          stateRoot,
          process.platform === "win32" ? "junction" : "dir",
        );
      } catch (error) {
        const code =
          error !== null && typeof error === "object" && "code" in error
            ? (error as { readonly code: unknown }).code
            : undefined;
        if (code === "EPERM" || code === "EACCES") {
          t.skip("local OS policy does not permit symlink creation");
          return;
        }
        throw error;
      }
      const adapter = new LocalTestnetStateAdapter({ workspaceRoot });
      const result = await adapter.inspect(DIGEST_A);
      assert.equal(result.status, "unavailable");
      assert.equal(result.code, "testnet_state_path_forbidden");
    } finally {
      assert.ok(resolve(externalTarget).startsWith(resolve(tmpdir())));
      await rm(externalTarget, { recursive: true, force: true });
    }
  });
});

test("execute harness records successful public evidence through the local adapter", async () => {
  await withTemporaryWorkspace(async (workspaceRoot) => {
    const fixture = await readHarnessFixture();
    const adapter = new LocalTestnetStateAdapter({
      workspaceRoot,
      now: fixedClock(
        "2026-08-24T10:07:00.000Z",
        "2026-08-24T10:12:00.000Z",
      ),
    });
    const request = {
      ...fixture.request,
      execute: true,
      confirmation: TESTNET_HARNESS_CONFIRMATION,
    };
    const result = await runTestnetConformanceHarness(request, {
      expectedPayTo: fixture.boundary.expectedPayTo,
      statePort: adapter,
      credentialPort: {
        createEphemeral: async () => ({
          publicAccountId: PUBLIC_ACCOUNT,
          [TESTNET_CREDENTIAL_HANDLE]: SECRET_SENTINEL,
        }),
      },
      canonicalPort: {
        run: async () => settledEvidence(),
      },
    });
    assert.equal(result.code, "testnet_conformance_completed");
    assert.equal(result.evidence?.status, "settled");
    assert.doesNotMatch(JSON.stringify(result), new RegExp(SECRET_SENTINEL, "u"));
    const inspection = await adapter.inspect(result.plan?.requestDigest ?? "");
    assert.equal(inspection.record?.state, "confirmed");
    assert.equal(inspection.record?.evidence?.transactionHash, "c".repeat(64));
  });
});
