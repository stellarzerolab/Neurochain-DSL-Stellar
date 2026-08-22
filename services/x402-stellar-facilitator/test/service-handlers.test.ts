import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import type { SettleResponse, VerifyResponse } from "@x402/core/types";

import {
  type FacilitatorPort,
  PureFacilitatorHandlers,
  SERVICE_HANDLER_AUTHORITY_BOUNDARY,
  type SettlementFinalization,
  type SettlementReservation,
  type SettlementStatePort,
} from "../src/service-handlers.js";
import { buildVerifyRejectionCase } from "../src/verify-rejection-conformance.js";

interface Calls {
  supported: number;
  verify: number;
  settle: number;
  reserve: number;
  finalize: number;
}

function standardRequest(caseId: "wrong_amount" | "invalid_network") {
  const [paymentPayload, paymentRequirements] =
    buildVerifyRejectionCase(caseId);
  return { x402Version: 2, paymentPayload, paymentRequirements };
}

function createPort(
  calls: Calls,
  options: Readonly<{
    verify?: VerifyResponse | Error;
    settle?: SettleResponse | Error;
  }> = {},
): FacilitatorPort {
  return {
    getSupported() {
      calls.supported += 1;
      return {
        kinds: [
          {
            x402Version: 2,
            scheme: "exact",
            network: "stellar:testnet",
            extra: { areFeesSponsored: true },
          },
        ],
        extensions: [],
        signers: { stellar: [] },
      };
    },
    async verify() {
      calls.verify += 1;
      if (options.verify instanceof Error) {
        throw options.verify;
      }
      return (
        options.verify ?? {
          isValid: false,
          invalidReason: "invalid_exact_stellar_payload_wrong_amount",
        }
      );
    },
    async settle() {
      calls.settle += 1;
      if (options.settle instanceof Error) {
        throw options.settle;
      }
      return (
        options.settle ?? {
          success: false,
          errorReason: "invalid_exact_stellar_payload_wrong_amount",
          transaction: "",
          network: "stellar:testnet",
        }
      );
    },
  };
}

function createCalls(): Calls {
  return { supported: 0, verify: 0, settle: 0, reserve: 0, finalize: 0 };
}

function createSettlementState(
  calls: Calls,
  reservation: SettlementReservation,
  finalization: SettlementFinalization = { status: "recorded" },
): SettlementStatePort {
  return {
    async reserve() {
      calls.reserve += 1;
      return reservation;
    },
    async finalize() {
      calls.finalize += 1;
      return finalization;
    },
  };
}

function assertNoAuthority(result: {
  readonly authorityBoundary: typeof SERVICE_HANDLER_AUTHORITY_BOUNDARY;
}) {
  assert.deepEqual(result.authorityBoundary, SERVICE_HANDLER_AUTHORITY_BOUNDARY);
  assert.ok(Object.values(result.authorityBoundary).every((grant) => !grant));
}

async function readBoundaryFixture(name: string): Promise<unknown> {
  const fixtureUrl = new URL(
    `../../../../examples/x402_service_boundary/${name}`,
    import.meta.url,
  );
  return JSON.parse(await readFile(fixtureUrl, "utf8")) as unknown;
}

test("pure supported and verify handlers delegate without granting authority", async () => {
  const calls = createCalls();
  const handlers = new PureFacilitatorHandlers(createPort(calls));

  const supported = handlers.handleSupported();
  assert.equal(supported.status, "completed");
  assert.equal(supported.code, "supported_ready");
  assert.equal(calls.supported, 1);
  assertNoAuthority(supported);

  const rejected = await handlers.handleVerify(standardRequest("wrong_amount"));
  assert.equal(rejected.status, "completed");
  assert.equal(rejected.code, "verify_rejected");
  assert.equal(
    rejected.reason,
    "invalid_exact_stellar_payload_wrong_amount",
  );
  assert.equal(rejected.response?.isValid, false);
  assert.equal(calls.verify, 1);
  assertNoAuthority(rejected);

  const malformed = {
    ...standardRequest("wrong_amount"),
    paymentSignature: "forbidden",
  };
  const invalid = await handlers.handleVerify(malformed);
  assert.equal(invalid.status, "rejected");
  assert.equal(invalid.code, "request_invalid");
  assert.equal(calls.verify, 1);
  assertNoAuthority(invalid);

  const unknownNetwork = await handlers.handleVerify(
    standardRequest("invalid_network"),
  );
  assert.equal(unknownNetwork.status, "rejected");
  assert.equal(unknownNetwork.code, "unsupported_network");
  assert.equal(calls.verify, 1);
  assertNoAuthority(unknownNetwork);
});

test("upstream verify exceptions and malformed rejection results fail closed", async () => {
  const exceptionCalls = createCalls();
  const exceptionHandlers = new PureFacilitatorHandlers(
    createPort(exceptionCalls, {
      verify: new Error("private upstream diagnostic must not cross boundary"),
    }),
  );
  const exception = await exceptionHandlers.handleVerify(
    standardRequest("wrong_amount"),
  );
  assert.equal(exception.status, "unavailable");
  assert.equal(exception.code, "upstream_verify_unavailable");
  assert.equal(exception.response, null);
  assert.ok(!exception.reason.includes("private"));
  assertNoAuthority(exception);

  const malformedCalls = createCalls();
  const malformedHandlers = new PureFacilitatorHandlers(
    createPort(malformedCalls, { verify: { isValid: false } }),
  );
  const malformed = await malformedHandlers.handleVerify(
    standardRequest("wrong_amount"),
  );
  assert.equal(malformed.status, "unavailable");
  assert.equal(malformed.code, "upstream_verify_response_invalid");
  assert.equal(malformed.response, null);
  assertNoAuthority(malformed);
});

test("settle requires durable admission and records the upstream outcome", async () => {
  const unavailableCalls = createCalls();
  const unavailableHandlers = new PureFacilitatorHandlers(
    createPort(unavailableCalls),
  );
  const unavailable = await unavailableHandlers.handleSettle(
    standardRequest("wrong_amount"),
  );
  assert.equal(unavailable.status, "unavailable");
  assert.equal(unavailable.code, "settlement_state_unavailable");
  assert.equal(unavailableCalls.settle, 0);
  assertNoAuthority(unavailable);

  const duplicateCalls = createCalls();
  const duplicateState = createSettlementState(duplicateCalls, {
    status: "rejected",
    code: "settlement_duplicate",
    reason: "the persistent request id is already reserved",
  });
  const duplicateHandlers = new PureFacilitatorHandlers(
    createPort(duplicateCalls),
    duplicateState,
  );
  const duplicate = await duplicateHandlers.handleSettle(
    standardRequest("wrong_amount"),
  );
  assert.equal(duplicate.status, "rejected");
  assert.equal(duplicate.code, "settlement_duplicate");
  assert.equal(duplicateCalls.reserve, 1);
  assert.equal(duplicateCalls.settle, 0);
  assert.equal(duplicateCalls.finalize, 0);
  assertNoAuthority(duplicate);

  const admittedCalls = createCalls();
  const admittedState = createSettlementState(admittedCalls, {
    status: "reserved",
    reservationId: "reservation-offline-001",
  });
  const admittedHandlers = new PureFacilitatorHandlers(
    createPort(admittedCalls),
    admittedState,
  );
  const rejected = await admittedHandlers.handleSettle(
    standardRequest("wrong_amount"),
  );
  assert.equal(rejected.status, "completed");
  assert.equal(rejected.code, "settle_rejected");
  assert.equal(rejected.response?.success, false);
  assert.deepEqual(
    [admittedCalls.reserve, admittedCalls.settle, admittedCalls.finalize],
    [1, 1, 1],
  );
  assertNoAuthority(rejected);
});

test("outcome-unknown never exposes an upstream settle result", async () => {
  const calls = createCalls();
  const state = createSettlementState(
    calls,
    { status: "reserved", reservationId: "reservation-offline-unknown" },
    {
      status: "unavailable",
      code: "settlement_outcome_unknown",
      reason: "the durable outcome write could not be confirmed",
    },
  );
  const handlers = new PureFacilitatorHandlers(
    createPort(calls, {
      settle: new Error("unknown upstream result with private diagnostic"),
    }),
    state,
  );

  const outcome = await handlers.handleSettle(standardRequest("wrong_amount"));
  assert.equal(outcome.status, "unavailable");
  assert.equal(outcome.code, "settlement_outcome_unknown");
  assert.equal(outcome.response, null);
  assert.ok(!outcome.reason.includes("private"));
  assert.deepEqual([calls.reserve, calls.settle, calls.finalize], [1, 1, 1]);
  assertNoAuthority(outcome);
});

test("TypeScript evaluation handler consumes the same v1 fixtures as Rust", async () => {
  const manifest = (await readBoundaryFixture("parity_manifest.json")) as {
    schema_version: number;
    request_fixture: string;
    response_fixtures: Array<{
      file: string;
      decision: string;
      exit_code: number | null;
      reason_code: string;
    }>;
    authority_grants: Record<string, boolean>;
    underlying_action_submit_allowed: boolean;
  };
  assert.equal(manifest.schema_version, 1);
  assert.ok(Object.values(manifest.authority_grants).every((grant) => !grant));
  assert.equal(manifest.underlying_action_submit_allowed, false);

  const request = (await readBoundaryFixture(
    manifest.request_fixture,
  )) as Record<string, unknown>;
  for (const expected of manifest.response_fixtures) {
    const response = (await readBoundaryFixture(expected.file)) as Record<
      string,
      unknown
    >;
    const correlatedRequest = {
      ...request,
      request_id: response.request_id,
    };
    const handlers = new PureFacilitatorHandlers(
      createPort(createCalls()),
      undefined,
      { async evaluate() { return response; } },
    );
    const evaluated = await handlers.handleEvaluation(correlatedRequest);
    assert.equal(evaluated.status, "completed");
    assert.equal(evaluated.code, "evaluation_completed");
    assert.equal(evaluated.response?.decision, expected.decision);
    assert.equal(evaluated.response?.exit_code, expected.exit_code);
    assert.equal(evaluated.response?.reason_code, expected.reason_code);
    assert.deepEqual(
      evaluated.response?.authority_grants,
      manifest.authority_grants,
    );
    assert.equal(evaluated.response?.underlying_action_submit_allowed, false);
    assertNoAuthority(evaluated);
  }
});

test("evaluation drift, authority escalation and correlation mismatch fail closed", async () => {
  const request = (await readBoundaryFixture("evaluation_request.json")) as Record<
    string,
    unknown
  >;
  const approved = (await readBoundaryFixture(
    "evaluation_approved.json",
  )) as Record<string, unknown>;

  const escalated = structuredClone(approved);
  (escalated.authority_grants as Record<string, unknown>).wallet_signing = true;
  const escalatedHandlers = new PureFacilitatorHandlers(
    createPort(createCalls()),
    undefined,
    { async evaluate() { return escalated; } },
  );
  const escalatedResult = await escalatedHandlers.handleEvaluation(request);
  assert.equal(escalatedResult.status, "rejected");
  assert.equal(escalatedResult.code, "evaluation_response_invalid");
  assert.equal(escalatedResult.response, null);
  assertNoAuthority(escalatedResult);

  const mismatch = structuredClone(approved);
  mismatch.request_id = "different-request";
  const mismatchHandlers = new PureFacilitatorHandlers(
    createPort(createCalls()),
    undefined,
    { async evaluate() { return mismatch; } },
  );
  const mismatchResult = await mismatchHandlers.handleEvaluation(request);
  assert.equal(mismatchResult.status, "rejected");
  assert.equal(mismatchResult.code, "evaluation_request_id_mismatch");
  assert.equal(mismatchResult.response, null);
  assertNoAuthority(mismatchResult);

  const unexpectedRequest = { ...request, payment_payload: {} };
  const requestHandlers = new PureFacilitatorHandlers(
    createPort(createCalls()),
    undefined,
    { async evaluate() { return approved; } },
  );
  const requestResult = await requestHandlers.handleEvaluation(unexpectedRequest);
  assert.equal(requestResult.status, "rejected");
  assert.equal(requestResult.code, "evaluation_request_invalid");
  assert.equal(requestResult.response, null);
  assertNoAuthority(requestResult);
});
