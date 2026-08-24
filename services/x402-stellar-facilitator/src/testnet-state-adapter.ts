import {
  lstat,
  mkdir,
  open,
  readdir,
  readFile,
  realpath,
  rename,
  unlink,
} from "node:fs/promises";
import { basename, dirname, isAbsolute, join, relative, resolve } from "node:path";

import {
  sanitizeTestnetPublicEvidence,
  type TestnetPublicEvidence,
  type TestnetStateFinalization,
  type TestnetStateOutcome,
  type TestnetStatePort,
  type TestnetStateReservation,
} from "./testnet-conformance-harness.js";

export const TESTNET_STATE_SCHEMA_VERSION = 1 as const;
export const TESTNET_LOCAL_STATE_DIRECTORY = ".local-testnet-state" as const;
export const TESTNET_STATE_MAX_BYTES = 16_384 as const;

const DIGEST_PATTERN = /^[0-9a-f]{64}$/u;
const RESERVATION_PATTERN = /^tstate_([0-9a-f]{64})$/u;
const RECORD_KEYS = Object.freeze([
  "admittedAt",
  "attemptedAt",
  "completedAt",
  "evidence",
  "requestDigest",
  "reservationId",
  "schemaVersion",
  "state",
]);

let temporaryFileSequence = 0;

export type TestnetLocalState = "attempted" | "outcome_unknown" | "confirmed";

export interface TestnetLocalStateRecord {
  readonly schemaVersion: typeof TESTNET_STATE_SCHEMA_VERSION;
  readonly requestDigest: string;
  readonly reservationId: string;
  readonly state: TestnetLocalState;
  readonly admittedAt: string;
  readonly attemptedAt: string;
  readonly completedAt: string | null;
  readonly evidence: TestnetPublicEvidence | null;
}

export type TestnetStateInspection =
  | Readonly<{
      status: "recorded";
      code: "testnet_state_recorded";
      reason: string;
      record: TestnetLocalStateRecord;
    }>
  | Readonly<{
      status: "missing" | "unavailable";
      code: string;
      reason: string;
      record: null;
    }>;

export interface LocalTestnetStateAdapterOptions {
  readonly workspaceRoot: string;
  readonly now?: () => Date;
}

class TestnetStateError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "TestnetStateError";
    this.code = code;
  }
}

function canonicalize(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(canonicalize);
  }
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, nested]) => [key, canonicalize(nested)]),
    );
  }
  return value;
}

function canonicalJson(value: unknown): string {
  return JSON.stringify(canonicalize(value));
}

function hasExactKeys(value: object, keys: readonly string[]): boolean {
  return canonicalJson(Object.keys(value).sort()) === canonicalJson(keys);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function isIsoTimestamp(value: unknown): value is string {
  return (
    isNonEmptyString(value) &&
    !Number.isNaN(Date.parse(value)) &&
    new Date(value).toISOString() === value
  );
}

function errorCode(error: unknown): string | undefined {
  if (error !== null && typeof error === "object" && "code" in error) {
    const code = (error as { readonly code?: unknown }).code;
    return typeof code === "string" ? code : undefined;
  }
  return undefined;
}

function stateError(error: unknown): TestnetStateError {
  if (error instanceof TestnetStateError) {
    return error;
  }
  return new TestnetStateError(
    "testnet_state_unavailable",
    "non-production testnet state failed closed",
  );
}

function unavailableReservation(error: unknown): TestnetStateReservation {
  const mapped = stateError(error);
  return Object.freeze({
    status: "unavailable" as const,
    code: mapped.code,
    reason: mapped.message,
  });
}

function unavailableFinalization(error: unknown): TestnetStateFinalization {
  const mapped = stateError(error);
  return Object.freeze({
    status: "unavailable" as const,
    code: mapped.code,
    reason: mapped.message,
  });
}

function unavailableInspection(error: unknown): TestnetStateInspection {
  const mapped = stateError(error);
  return Object.freeze({
    status: "unavailable" as const,
    code: mapped.code,
    reason: mapped.message,
    record: null,
  });
}

function reservationIdFor(requestDigest: string): string {
  return `tstate_${requestDigest}`;
}

function digestFromReservationId(reservationId: string): string | null {
  return RESERVATION_PATTERN.exec(reservationId)?.[1] ?? null;
}

function assertDigest(requestDigest: string): void {
  if (!DIGEST_PATTERN.test(requestDigest)) {
    throw new TestnetStateError(
      "testnet_state_request_invalid",
      "request digest must be exactly 64 lowercase hexadecimal characters",
    );
  }
}

function assertSafeRecord(
  value: unknown,
  expectedDigest: string,
): asserts value is TestnetLocalStateRecord {
  if (
    value === null ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    !hasExactKeys(value, RECORD_KEYS)
  ) {
    throw new TestnetStateError(
      "testnet_state_record_invalid",
      "state record must use the strict schema-v1 public envelope",
    );
  }
  const candidate = value as Record<string, unknown>;
  if (
    candidate.schemaVersion !== TESTNET_STATE_SCHEMA_VERSION ||
    candidate.requestDigest !== expectedDigest ||
    candidate.reservationId !== reservationIdFor(expectedDigest) ||
    (candidate.state !== "attempted" &&
      candidate.state !== "outcome_unknown" &&
      candidate.state !== "confirmed") ||
    !isIsoTimestamp(candidate.admittedAt) ||
    !isIsoTimestamp(candidate.attemptedAt)
  ) {
    throw new TestnetStateError(
      "testnet_state_record_invalid",
      "state record identity, state or timestamps are invalid",
    );
  }
  if (
    candidate.state === "attempted" &&
    (candidate.completedAt !== null || candidate.evidence !== null)
  ) {
    throw new TestnetStateError(
      "testnet_state_record_invalid",
      "attempted state must not contain completion data",
    );
  }
  if (
    candidate.state === "outcome_unknown" &&
    (!isIsoTimestamp(candidate.completedAt) || candidate.evidence !== null)
  ) {
    throw new TestnetStateError(
      "testnet_state_record_invalid",
      "outcome-unknown state requires only a public completion timestamp",
    );
  }
  if (candidate.state === "confirmed") {
    const evidence = sanitizeTestnetPublicEvidence(candidate.evidence);
    if (!isIsoTimestamp(candidate.completedAt) || !evidence) {
      throw new TestnetStateError(
        "testnet_state_record_invalid",
        "confirmed state requires strict public evidence and completion time",
      );
    }
  }
}

function assertPathInside(root: string, candidate: string): void {
  const pathFromRoot = relative(root, candidate);
  if (
    pathFromRoot.length === 0 ||
    pathFromRoot.startsWith("..") ||
    isAbsolute(pathFromRoot)
  ) {
    throw new TestnetStateError(
      "testnet_state_path_forbidden",
      "state path must remain inside the dedicated local testnet state root",
    );
  }
}

export class LocalTestnetStateAdapter implements TestnetStatePort {
  readonly #workspaceRoot: string;
  readonly #now: () => Date;
  #initialization: Promise<string> | undefined;

  constructor(options: LocalTestnetStateAdapterOptions) {
    this.#workspaceRoot = options.workspaceRoot;
    this.#now = options.now ?? (() => new Date());
  }

  async reserve(requestDigest: string): Promise<TestnetStateReservation> {
    try {
      assertDigest(requestDigest);
      const stateRoot = await this.#ensureInitialized();
      return await this.#withRecordLock(stateRoot, requestDigest, async () => {
        const existing = await this.#readRecord(stateRoot, requestDigest);
        if (existing?.state === "attempted") {
          return Object.freeze({
            status: "unavailable" as const,
            code: "testnet_state_duplicate",
            reason: "the request digest already has an active testnet attempt",
          });
        }
        if (existing?.state === "outcome_unknown") {
          return Object.freeze({
            status: "unavailable" as const,
            code: "testnet_state_outcome_unknown",
            reason: "the request has an unknown prior outcome and cannot be retried",
          });
        }
        if (existing?.state === "confirmed") {
          return Object.freeze({
            status: "unavailable" as const,
            code: "testnet_state_replay",
            reason: "the request digest already has a confirmed public outcome",
          });
        }

        const timestamp = this.#timestamp();
        const record: TestnetLocalStateRecord = Object.freeze({
          schemaVersion: TESTNET_STATE_SCHEMA_VERSION,
          requestDigest,
          reservationId: reservationIdFor(requestDigest),
          state: "attempted",
          admittedAt: timestamp,
          attemptedAt: timestamp,
          completedAt: null,
          evidence: null,
        });
        await this.#writeRecord(stateRoot, record);
        return Object.freeze({
          status: "reserved" as const,
          reservationId: record.reservationId,
          code: "testnet_state_reserved",
          reason: "the bounded testnet request was reserved atomically",
        });
      });
    } catch (error) {
      return unavailableReservation(error);
    }
  }

  async finalize(
    reservationId: string,
    outcome: TestnetStateOutcome,
  ): Promise<TestnetStateFinalization> {
    try {
      const requestDigest = digestFromReservationId(reservationId);
      if (!requestDigest) {
        throw new TestnetStateError(
          "testnet_state_request_invalid",
          "reservation id is not bound to a valid request digest",
        );
      }
      const stateRoot = await this.#ensureInitialized();
      return await this.#withRecordLock(stateRoot, requestDigest, async () => {
        const existing = await this.#readRecord(stateRoot, requestDigest);
        if (!existing || existing.reservationId !== reservationId) {
          throw new TestnetStateError(
            "testnet_state_reservation_missing",
            "the testnet state reservation does not exist",
          );
        }
        if (existing.state === "outcome_unknown") {
          if (outcome.status === "outcome_unknown") {
            return Object.freeze({
              status: "recorded" as const,
              code: "testnet_state_outcome_unknown",
              reason: "the unknown outcome was already recorded",
            });
          }
          throw new TestnetStateError(
            "testnet_state_outcome_unknown",
            "an unknown outcome cannot transition to confirmed",
          );
        }
        if (existing.state === "confirmed") {
          const evidence =
            outcome.status === "confirmed"
              ? sanitizeTestnetPublicEvidence(outcome.evidence)
              : null;
          if (
            evidence &&
            canonicalJson(evidence) === canonicalJson(existing.evidence)
          ) {
            return Object.freeze({
              status: "recorded" as const,
              code: "testnet_state_confirmed",
              reason: "the same public outcome was already recorded",
            });
          }
          throw new TestnetStateError(
            "testnet_state_replay",
            "confirmed state rejects a different or unknown outcome",
          );
        }

        const completedAt = this.#timestamp();
        const next =
          outcome.status === "outcome_unknown"
            ? Object.freeze({
                ...existing,
                state: "outcome_unknown" as const,
                completedAt,
                evidence: null,
              })
            : this.#confirmedRecord(existing, outcome.evidence, completedAt);
        await this.#writeRecord(stateRoot, next);
        return Object.freeze({
          status: "recorded" as const,
          code:
            next.state === "confirmed"
              ? "testnet_state_confirmed"
              : "testnet_state_outcome_unknown",
          reason:
            next.state === "confirmed"
              ? "strict public testnet evidence was recorded atomically"
              : "the uncertain testnet outcome was recorded without retry authority",
        });
      });
    } catch (error) {
      return unavailableFinalization(error);
    }
  }

  async inspect(requestDigest: string): Promise<TestnetStateInspection> {
    try {
      assertDigest(requestDigest);
      const stateRoot = await this.#ensureInitialized();
      return await this.#withRecordLock(stateRoot, requestDigest, async () => {
        const record = await this.#readRecord(stateRoot, requestDigest);
        if (!record) {
          return Object.freeze({
            status: "missing" as const,
            code: "testnet_state_missing",
            reason: "no local non-production state exists for the request",
            record: null,
          });
        }
        return Object.freeze({
          status: "recorded" as const,
          code: "testnet_state_recorded" as const,
          reason: "strict public non-production state was loaded",
          record,
        });
      });
    } catch (error) {
      return unavailableInspection(error);
    }
  }

  #confirmedRecord(
    existing: TestnetLocalStateRecord,
    candidateEvidence: TestnetPublicEvidence,
    completedAt: string,
  ): TestnetLocalStateRecord {
    const evidence = sanitizeTestnetPublicEvidence(candidateEvidence);
    if (!evidence) {
      throw new TestnetStateError(
        "testnet_state_evidence_invalid",
        "confirmed state accepts only strict public redacted evidence",
      );
    }
    return Object.freeze({
      ...existing,
      state: "confirmed" as const,
      completedAt,
      evidence,
    });
  }

  #timestamp(): string {
    const timestamp = this.#now().toISOString();
    if (!isIsoTimestamp(timestamp)) {
      throw new TestnetStateError(
        "testnet_state_clock_invalid",
        "state clock did not return a valid UTC timestamp",
      );
    }
    return timestamp;
  }

  #ensureInitialized(): Promise<string> {
    this.#initialization ??= this.#initialize();
    return this.#initialization;
  }

  async #initialize(): Promise<string> {
    if (!isAbsolute(this.#workspaceRoot)) {
      throw new TestnetStateError(
        "testnet_state_path_forbidden",
        "workspace root must be an absolute path",
      );
    }
    const workspaceInfo = await lstat(this.#workspaceRoot).catch(() => null);
    if (!workspaceInfo?.isDirectory() || workspaceInfo.isSymbolicLink()) {
      throw new TestnetStateError(
        "testnet_state_path_forbidden",
        "workspace root must be an existing non-symlink directory",
      );
    }
    const canonicalWorkspace = await realpath(this.#workspaceRoot);
    const stateRoot = join(canonicalWorkspace, TESTNET_LOCAL_STATE_DIRECTORY);
    await mkdir(stateRoot, { recursive: true, mode: 0o700 });
    const stateRootInfo = await lstat(stateRoot);
    if (!stateRootInfo.isDirectory() || stateRootInfo.isSymbolicLink()) {
      throw new TestnetStateError(
        "testnet_state_path_forbidden",
        "local testnet state root must be a real directory, not a symlink",
      );
    }
    const canonicalStateRoot = await realpath(stateRoot);
    if (
      dirname(canonicalStateRoot) !== canonicalWorkspace ||
      basename(canonicalStateRoot) !== TESTNET_LOCAL_STATE_DIRECTORY
    ) {
      throw new TestnetStateError(
        "testnet_state_path_forbidden",
        "local state root escaped the canonical service workspace",
      );
    }

    const entries = await readdir(canonicalStateRoot, { withFileTypes: true });
    for (const entry of entries) {
      const match = /^([0-9a-f]{64})\.json$/u.exec(entry.name);
      if (!match || !entry.isFile() || entry.isSymbolicLink()) {
        throw new TestnetStateError(
          "testnet_state_root_unsafe",
          "local state root contains an unexpected, locked or unsafe entry",
        );
      }
      const requestDigest = match[1];
      if (!requestDigest) {
        throw new TestnetStateError(
          "testnet_state_root_unsafe",
          "state filename is not bound to a request digest",
        );
      }
      const record = await this.#readRecord(canonicalStateRoot, requestDigest);
      if (record?.state === "attempted") {
        await this.#withRecordLock(
          canonicalStateRoot,
          requestDigest,
          async () => {
            const current = await this.#readRecord(
              canonicalStateRoot,
              requestDigest,
            );
            if (current?.state === "attempted") {
              await this.#writeRecord(
                canonicalStateRoot,
                Object.freeze({
                  ...current,
                  state: "outcome_unknown" as const,
                  completedAt: this.#timestamp(),
                  evidence: null,
                }),
              );
            }
          },
        );
      }
    }
    return canonicalStateRoot;
  }

  async #withRecordLock<T>(
    stateRoot: string,
    requestDigest: string,
    operation: () => Promise<T>,
  ): Promise<T> {
    const lockPath = join(stateRoot, `${requestDigest}.lock`);
    assertPathInside(stateRoot, lockPath);
    let lock;
    try {
      lock = await open(lockPath, "wx", 0o600);
    } catch (error) {
      throw new TestnetStateError(
        errorCode(error) === "EEXIST"
          ? "testnet_state_locked"
          : "testnet_state_unavailable",
        errorCode(error) === "EEXIST"
          ? "the request state is locked by another process or an interrupted write"
          : "the request state lock could not be created",
      );
    }
    try {
      return await operation();
    } finally {
      await lock.close();
      await unlink(lockPath).catch(() => {
        throw new TestnetStateError(
          "testnet_state_locked",
          "the request state lock could not be removed safely",
        );
      });
    }
  }

  async #readRecord(
    stateRoot: string,
    requestDigest: string,
  ): Promise<TestnetLocalStateRecord | null> {
    const recordPath = join(stateRoot, `${requestDigest}.json`);
    assertPathInside(stateRoot, recordPath);
    const info = await lstat(recordPath).catch((error: unknown) => {
      if (errorCode(error) === "ENOENT") {
        return null;
      }
      throw error;
    });
    if (info === null) {
      return null;
    }
    if (!info.isFile() || info.isSymbolicLink() || info.size > TESTNET_STATE_MAX_BYTES) {
      throw new TestnetStateError(
        "testnet_state_record_invalid",
        "state record must be a bounded regular non-symlink file",
      );
    }
    let parsed: unknown;
    try {
      parsed = JSON.parse(await readFile(recordPath, "utf8")) as unknown;
    } catch {
      throw new TestnetStateError(
        "testnet_state_record_invalid",
        "state record is not valid bounded JSON",
      );
    }
    assertSafeRecord(parsed, requestDigest);
    const evidence =
      parsed.state === "confirmed"
        ? sanitizeTestnetPublicEvidence(parsed.evidence)
        : null;
    return Object.freeze({ ...parsed, evidence });
  }

  async #writeRecord(
    stateRoot: string,
    record: TestnetLocalStateRecord,
  ): Promise<void> {
    assertSafeRecord(record, record.requestDigest);
    const recordPath = join(stateRoot, `${record.requestDigest}.json`);
    assertPathInside(stateRoot, recordPath);
    const existing = await lstat(recordPath).catch((error: unknown) => {
      if (errorCode(error) === "ENOENT") {
        return null;
      }
      throw error;
    });
    if (existing?.isSymbolicLink() || (existing !== null && !existing.isFile())) {
      throw new TestnetStateError(
        "testnet_state_record_invalid",
        "state destination must remain a regular non-symlink file",
      );
    }

    temporaryFileSequence += 1;
    const temporaryPath = join(
      stateRoot,
      `.${record.requestDigest}.${process.pid}.${temporaryFileSequence}.tmp`,
    );
    assertPathInside(stateRoot, temporaryPath);
    const serialized = `${JSON.stringify(record, null, 2)}\n`;
    if (Buffer.byteLength(serialized, "utf8") > TESTNET_STATE_MAX_BYTES) {
      throw new TestnetStateError(
        "testnet_state_record_invalid",
        "serialized state record exceeds the bounded public schema size",
      );
    }
    let handle;
    try {
      handle = await open(temporaryPath, "wx", 0o600);
      await handle.writeFile(serialized, "utf8");
      await handle.sync();
      await handle.close();
      handle = undefined;
      await rename(temporaryPath, recordPath);
    } catch (error) {
      if (handle) {
        await handle.close().catch(() => undefined);
      }
      await unlink(temporaryPath).catch(() => undefined);
      throw stateError(error);
    }
  }
}
