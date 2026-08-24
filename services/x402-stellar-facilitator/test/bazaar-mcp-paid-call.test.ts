import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  BAZAAR_MCP_RUNTIME_BOUNDARY,
  type BazaarMcpRustPort,
  PureBazaarMcpParityHandlers,
} from "../src/bazaar-mcp-paid-call.js";

interface Calls {
  search: number;
  paidCall: number;
  lastSearch?: unknown;
  lastPaidCall?: unknown;
}

interface OutcomeContract {
  readonly schemaVersion: 1;
  readonly outcomes: Readonly<
    Record<
      string,
      Readonly<{
        code: string;
        retryable: boolean;
        serviceCallAllowed: boolean;
      }>
    >
  >;
}

async function readFixture(directory: string, name: string): Promise<unknown> {
  const fixtureUrl = new URL(
    `../../../../examples/${directory}/${name}`,
    import.meta.url,
  );
  return JSON.parse(await readFile(fixtureUrl, "utf8")) as unknown;
}

function asRecord(value: unknown): Record<string, unknown> {
  assert.ok(value !== null && typeof value === "object" && !Array.isArray(value));
  return value as Record<string, unknown>;
}

function structuredContent(value: unknown): Record<string, unknown> {
  return asRecord(asRecord(value).structuredContent);
}

function allFalsePaidAuthority(): Record<string, false> {
  return {
    actionPlanSubmitAllowed: false,
    approvalAllowed: false,
    paymentAllowed: false,
    proofAllowed: false,
    rpcSubmitAllowed: false,
    serviceCallAllowed: false,
    settlementAllowed: false,
    shellAccessAllowed: false,
    signingAllowed: false,
    underlyingExecutionAllowed: false,
    walletAccessAllowed: false,
  };
}

function fixturePort(
  calls: Calls,
  searchResults: readonly unknown[],
  paidResults: readonly unknown[],
): BazaarMcpRustPort {
  const searchQueue = [...searchResults];
  const paidQueue = [...paidResults];
  return {
    async search(argumentsValue) {
      calls.search += 1;
      calls.lastSearch = argumentsValue;
      const next = searchQueue.shift();
      if (next === undefined) {
        throw new Error("search fixture queue exhausted");
      }
      return structuredClone(next);
    },
    async paidCall(argumentsValue) {
      calls.paidCall += 1;
      calls.lastPaidCall = argumentsValue;
      const next = paidQueue.shift();
      if (next === undefined) {
        throw new Error("paid-call fixture queue exhausted");
      }
      return structuredClone(next);
    },
  };
}

test("MCP search consumes the existing Rust call and result fixtures", async () => {
  const call = await readFixture("x402_bazaar_mcp", "search_call.json");
  const expected = await readFixture("x402_bazaar_mcp", "search_result.json");
  const calls: Calls = { search: 0, paidCall: 0 };
  const handlers = new PureBazaarMcpParityHandlers(
    fixturePort(calls, [structuredContent(expected)], []),
  );

  const actual = await handlers.handleSearchCall(call);
  assert.deepEqual(actual, expected);
  assert.deepEqual(
    calls.lastSearch,
    asRecord(asRecord(asRecord(call).params).arguments),
  );
  assert.deepEqual([calls.search, calls.paidCall], [1, 0]);
  assert.ok(Object.values(BAZAAR_MCP_RUNTIME_BOUNDARY).every((grant) => !grant));
});

test("catalog unavailability preserves the shared retryable MCP result", async () => {
  const call = await readFixture("x402_bazaar_mcp", "search_call.json");
  const expected = await readFixture(
    "x402_bazaar_mcp",
    "catalog_unavailable_result.json",
  );
  const calls: Calls = { search: 0, paidCall: 0 };
  const handlers = new PureBazaarMcpParityHandlers(
    fixturePort(calls, [structuredContent(expected)], []),
  );

  assert.deepEqual(await handlers.handleSearchCall(call), expected);
  assert.deepEqual([calls.search, calls.paidCall], [1, 0]);
});

test("paid-call preserves the Rust authorized and replay fixtures without dispatch", async () => {
  const call = await readFixture("x402_bazaar_paid_call", "paid_call.json");
  const authorized = await readFixture(
    "x402_bazaar_paid_call",
    "authorized_result.json",
  );
  const replay = await readFixture(
    "x402_bazaar_paid_call",
    "replay_result.json",
  );
  const calls: Calls = { search: 0, paidCall: 0 };
  const handlers = new PureBazaarMcpParityHandlers(
    fixturePort(
      calls,
      [],
      [structuredContent(authorized), structuredContent(replay)],
    ),
  );

  const first = await handlers.handlePaidCall(call);
  const second = await handlers.handlePaidCall(call);
  assert.deepEqual(first, authorized);
  assert.deepEqual(second, replay);
  assert.deepEqual([calls.search, calls.paidCall], [0, 2]);
  assert.deepEqual(
    calls.lastPaidCall,
    asRecord(asRecord(asRecord(call).params).arguments),
  );

  const authority = asRecord(structuredContent(first).authority);
  assert.equal(authority.serviceCallAllowed, true);
  for (const [key, value] of Object.entries(authority)) {
    if (key !== "serviceCallAllowed") {
      assert.equal(value, false, `paid-call authority leak: ${key}`);
    }
  }
});

test("all paid-call rejection outcomes retain stable Rust codes and retryability", async () => {
  const call = await readFixture("x402_bazaar_paid_call", "paid_call.json");
  const contract = (await readFixture(
    "x402_bazaar_paid_call",
    "outcome_contract.json",
  )) as OutcomeContract;
  assert.equal(contract.schemaVersion, 1);
  const rejected = Object.values(contract.outcomes).filter(
    (outcome) => !outcome.serviceCallAllowed,
  );
  const portResults = rejected.map((outcome) => ({
    authority: allFalsePaidAuthority(),
    code: outcome.code,
    ok: false,
    protocolVersion: "2026-07-28",
    reason: `Rust fixture outcome: ${outcome.code}`,
    retryable: outcome.retryable,
    schemaVersion: 1,
    tool: "proxy_paid_stellar_call",
  }));
  const calls: Calls = { search: 0, paidCall: 0 };
  const handlers = new PureBazaarMcpParityHandlers(
    fixturePort(calls, [], portResults),
  );

  for (const expected of rejected) {
    const result = await handlers.handlePaidCall(call);
    const content = structuredContent(result);
    assert.equal(content.code, expected.code);
    assert.equal(content.retryable, expected.retryable);
    assert.equal(asRecord(content.authority).serviceCallAllowed, false);
    assert.equal(result.isError, true);
  }
  assert.equal(calls.paidCall, rejected.length);
});

test("authority-shaped and malformed calls fail before the Rust ports", async () => {
  const search = asRecord(
    structuredClone(await readFixture("x402_bazaar_mcp", "search_call.json")),
  );
  const paid = asRecord(
    structuredClone(
      await readFixture("x402_bazaar_paid_call", "paid_call.json"),
    ),
  );
  const searchArguments = asRecord(asRecord(search.params).arguments);
  searchArguments.walletSigningAllowed = true;
  const paidArguments = asRecord(asRecord(paid.params).arguments);
  paidArguments.settled = true;

  const oversizedSearch = asRecord(
    structuredClone(await readFixture("x402_bazaar_mcp", "search_call.json")),
  );
  asRecord(asRecord(oversizedSearch.params).arguments).query = "x".repeat(4_097);
  const malformedServiceArguments = asRecord(
    structuredClone(
      await readFixture("x402_bazaar_paid_call", "paid_call.json"),
    ),
  );
  asRecord(asRecord(malformedServiceArguments.params).arguments).arguments = [];
  const oversizedPaidCall = asRecord(
    structuredClone(
      await readFixture("x402_bazaar_paid_call", "paid_call.json"),
    ),
  );
  asRecord(asRecord(oversizedPaidCall.params).arguments).arguments = {
    input: "x".repeat(17_000),
  };

  const calls: Calls = { search: 0, paidCall: 0 };
  const handlers = new PureBazaarMcpParityHandlers({
    async search() {
      calls.search += 1;
      throw new Error("must not be reached");
    },
    async paidCall() {
      calls.paidCall += 1;
      throw new Error("must not be reached");
    },
  });

  const searchResult = await handlers.handleSearchCall(search);
  const paidResult = await handlers.handlePaidCall(paid);
  const searchTooLarge = await handlers.handleSearchCall(oversizedSearch);
  const serviceArguments = await handlers.handlePaidCall(
    malformedServiceArguments,
  );
  const paidTooLarge = await handlers.handlePaidCall(oversizedPaidCall);
  assert.equal(structuredContent(searchResult).code, "invalid_arguments");
  assert.equal(structuredContent(paidResult).code, "invalid_arguments");
  assert.equal(structuredContent(searchTooLarge).code, "arguments_too_large");
  assert.equal(
    structuredContent(serviceArguments).code,
    "invalid_service_arguments",
  );
  assert.equal(structuredContent(paidTooLarge).code, "arguments_too_large");
  assert.deepEqual([calls.search, calls.paidCall], [0, 0]);
});

test("malformed or authority-escalating Rust results fail closed", async () => {
  const searchCall = await readFixture("x402_bazaar_mcp", "search_call.json");
  const paidCall = await readFixture("x402_bazaar_paid_call", "paid_call.json");
  const searchFixture = structuredContent(
    await readFixture("x402_bazaar_mcp", "search_result.json"),
  );
  const paidFixture = structuredContent(
    await readFixture("x402_bazaar_paid_call", "authorized_result.json"),
  );
  asRecord(searchFixture.authority).paymentAllowed = true;
  asRecord(paidFixture.data).requestId = "different-request";
  asRecord(paidFixture.authority).signingAllowed = true;

  const calls: Calls = { search: 0, paidCall: 0 };
  const handlers = new PureBazaarMcpParityHandlers(
    fixturePort(calls, [searchFixture], [paidFixture]),
  );
  const search = await handlers.handleSearchCall(searchCall);
  const paid = await handlers.handlePaidCall(paidCall);
  assert.equal(structuredContent(search).code, "search_port_invalid");
  assert.equal(structuredContent(paid).code, "paid_call_port_invalid");
  assert.equal(search.isError, true);
  assert.equal(paid.isError, true);
});
