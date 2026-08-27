import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { resolve } from "node:path";

import {
  parseOfflineReadiness,
  ReadinessValidationError,
} from "../src/readiness-status.js";

const REPO_ROOT = fileURLToPath(new URL("../../../../", import.meta.url));

async function readSharedFixture(name: string): Promise<unknown> {
  const url = new URL(
    `../../../../examples/x402_stellar_conformance/${name}`,
    import.meta.url,
  );
  return JSON.parse(await readFile(url, "utf8")) as unknown;
}

function asRecord(value: unknown): Record<string, unknown> {
  assert.ok(value !== null && typeof value === "object" && !Array.isArray(value));
  return value as Record<string, unknown>;
}

function mergePatch(target: Record<string, unknown>, patch: Record<string, unknown>): void {
  for (const [key, value] of Object.entries(patch)) {
    const current = target[key];
    if (
      value !== null &&
      typeof value === "object" &&
      !Array.isArray(value) &&
      current !== null &&
      typeof current === "object" &&
      !Array.isArray(current)
    ) {
      mergePatch(current as Record<string, unknown>, value as Record<string, unknown>);
    } else {
      target[key] = value;
    }
  }
}

function findCase(value: Record<string, unknown>, id: string): Record<string, unknown> {
  const cases = value.cases;
  assert.ok(Array.isArray(cases));
  const found = cases.find((entry) => asRecord(entry).id === id);
  assert.ok(found !== undefined, `missing case ${id}`);
  return asRecord(found);
}

function applyOperation(value: Record<string, unknown>, operation: string): void {
  const cases = value.cases;
  assert.ok(Array.isArray(cases));
  switch (operation) {
    case "mark_canonical_client_verified":
      findCase(value, "exact_canonical_client_e2e").status = "verified_offline";
      break;
    case "mark_upto_verified":
      findCase(value, "upto_stellar_upstream_spec").status = "verified_offline";
      break;
    case "mark_replay_verified":
      findCase(value, "exact_replay_reject").status = "verified_offline";
      break;
    case "remove_last_case":
      cases.pop();
      break;
    case "inject_traversal_evidence":
      findCase(value, "standard_surface").evidenceRefs = ["../private/credential.json"];
      break;
    case "replace_evidence_with_unrelated_checked_in_file":
      findCase(value, "standard_surface").evidenceRefs = [
        "docs/x402_stellar_conformance.md",
      ];
      break;
    default:
      assert.fail(`unknown readiness operation ${operation}`);
  }
}

test("machine-readable readiness records the actual 24-case offline boundary", async () => {
  const readiness = parseOfflineReadiness(await readSharedFixture("readiness.json"));
  const plan = asRecord(await readSharedFixture("plan.json"));
  assert.ok(Array.isArray(plan.cases));

  assert.deepEqual(readiness.summary, {
    totalCases: 24,
    verifiedOffline: 9,
    serviceBoundaryPending: 2,
    approvalBlocked: 11,
    upstreamBlocked: 2,
  });
  assert.equal(readiness.cases.length, 24);
  assert.deepEqual(
    readiness.cases.map(({ id }) => id),
    plan.cases.map((entry) => asRecord(entry).id),
  );
  assert.equal(
    readiness.cases.filter(({ status }) => status === "verified_offline").length,
    9,
  );
  assert.deepEqual(
    readiness.cases.find(({ id }) => id === "exact_canonical_client_e2e")?.evidenceRefs.slice(-2),
    [
      "services/x402-stellar-facilitator/fixtures/testnet-canonical-reference-v1.expected.json",
      "services/x402-stellar-facilitator/test/testnet-canonical-reference.test.ts",
    ],
  );
  assert.equal(
    readiness.cases.find(({ id }) => id === "exact_canonical_client_e2e")?.status,
    "approval_blocked",
  );
  assert.ok(Object.values(readiness.authorityBoundary).every((allowed) => !allowed));
});

test("CI runs the complete offline TypeScript readiness gate", async () => {
  const workflow = await readFile(resolve(REPO_ROOT, ".github/workflows/ci.yml"), "utf8");
  for (const required of [
    "pnpm install --frozen-lockfile --ignore-scripts",
    "pnpm run check:supply-chain",
    "pnpm run typecheck",
    "pnpm run build",
    "pnpm test",
    "pnpm run testnet:plan",
  ]) {
    assert.ok(workflow.includes(required), `CI missing ${required}`);
  }
});

test("every readiness evidence ref is a checked-in repository file", async () => {
  const readiness = parseOfflineReadiness(await readSharedFixture("readiness.json"));
  for (const entry of readiness.cases) {
    for (const evidenceRef of entry.evidenceRefs) {
      assert.equal(existsSync(resolve(REPO_ROOT, evidenceRef)), true, `${entry.id}: ${evidenceRef}`);
    }
  }
  assert.equal(
    existsSync(resolve(REPO_ROOT, readiness.packageSnapshot.licenseInventoryRef)),
    true,
  );
});

test("readiness authority, status, package, summary and evidence drift fail closed", async () => {
  const base = asRecord(await readSharedFixture("readiness.json"));
  const scenarios = await readSharedFixture("readiness_adversarial_patches.json");
  assert.ok(Array.isArray(scenarios));

  for (const rawScenario of scenarios) {
    const scenario = asRecord(rawScenario);
    const value = structuredClone(base);
    if (scenario.patch !== undefined) {
      mergePatch(value, asRecord(scenario.patch));
    }
    if (typeof scenario.operation === "string") {
      applyOperation(value, scenario.operation);
    }

    assert.throws(
      () => parseOfflineReadiness(value),
      (error: unknown) => {
        assert.ok(error instanceof ReadinessValidationError, String(scenario.name));
        assert.equal(error.code, scenario.expectedCode, String(scenario.name));
        assert.ok(error.message.length > 0);
        return true;
      },
    );
  }
});
