import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  type BazaarDiscoveryPort,
  PureBazaarDiscoveryHandlers,
} from "../src/bazaar-resources-search.js";
import { SERVICE_HANDLER_AUTHORITY_BOUNDARY } from "../src/service-handlers.js";

interface SearchPagesFixture {
  readonly schemaVersion: 1;
  readonly candidates: readonly Readonly<{
    file: string;
    observedAt: number;
  }>[];
  readonly pages: readonly Readonly<{
    request: Readonly<Record<string, unknown>>;
    response: Readonly<Record<string, unknown>>;
  }>[];
}

interface Calls {
  list: number;
  search: number;
  lastList?: unknown;
  lastSearch?: unknown;
}

async function readFixture(name: string): Promise<unknown> {
  const fixtureUrl = new URL(
    `../../../../examples/x402_bazaar_catalog/${name}`,
    import.meta.url,
  );
  return JSON.parse(await readFile(fixtureUrl, "utf8")) as unknown;
}

function fixturePort(
  calls: Calls,
  listResponse: unknown,
  searchPages: SearchPagesFixture,
): BazaarDiscoveryPort {
  return {
    async list(query) {
      calls.list += 1;
      calls.lastList = query;
      return { status: "completed", response: structuredClone(listResponse) };
    },
    async search(query) {
      calls.search += 1;
      calls.lastSearch = query;
      const page = searchPages.pages.find(
        (candidate) =>
          JSON.stringify(candidate.request) === JSON.stringify(query),
      );
      if (page === undefined) {
        return {
          status: "rejected",
          code: "invalid_search_cursor",
          reason: "query did not match the shared cursor fixture",
        };
      }
      return { status: "completed", response: structuredClone(page.response) };
    },
  };
}

function assertNoAuthority(result: {
  readonly authorityBoundary: typeof SERVICE_HANDLER_AUTHORITY_BOUNDARY;
}) {
  assert.deepEqual(result.authorityBoundary, SERVICE_HANDLER_AUTHORITY_BOUNDARY);
  assert.ok(Object.values(result.authorityBoundary).every((grant) => !grant));
}

test("resources handler consumes the same deterministic list response as Rust", async () => {
  const listResponse = await readFixture("list_response.json");
  const searchPages = (await readFixture(
    "search_pages.json",
  )) as SearchPagesFixture;
  const calls: Calls = { list: 0, search: 0 };
  const handlers = new PureBazaarDiscoveryHandlers(
    fixturePort(calls, listResponse, searchPages),
  );

  const result = await handlers.handleResources({});
  assert.equal(result.status, "completed");
  assert.equal(result.code, "resources_completed");
  assert.deepEqual(result.response, listResponse);
  assert.deepEqual(calls.lastList, {});
  assert.deepEqual([calls.list, calls.search], [1, 0]);
  assertNoAuthority(result);
});

test("search handler consumes Rust-locked ranking and query-bound cursor pages", async () => {
  const listResponse = await readFixture("list_response.json");
  const searchPages = (await readFixture(
    "search_pages.json",
  )) as SearchPagesFixture;
  assert.equal(searchPages.schemaVersion, 1);
  assert.equal(searchPages.pages.length, 3);
  const calls: Calls = { list: 0, search: 0 };
  const handlers = new PureBazaarDiscoveryHandlers(
    fixturePort(calls, listResponse, searchPages),
  );

  for (const page of searchPages.pages) {
    const result = await handlers.handleSearch(page.request);
    assert.equal(result.status, "completed");
    assert.equal(result.code, "search_completed");
    assert.deepEqual(result.response, page.response);
    assert.deepEqual(calls.lastSearch, page.request);
    assertNoAuthority(result);
  }
  assert.deepEqual([calls.list, calls.search], [0, 3]);
});

test("strict request envelopes reject authority fields and invalid bounds before the port", async () => {
  const calls: Calls = { list: 0, search: 0 };
  const rejectingPort: BazaarDiscoveryPort = {
    async list() {
      calls.list += 1;
      throw new Error("must not be reached");
    },
    async search() {
      calls.search += 1;
      throw new Error("must not be reached");
    },
  };
  const handlers = new PureBazaarDiscoveryHandlers(rejectingPort);

  const listAuthority = await handlers.handleResources({ submit: true });
  assert.equal(listAuthority.code, "invalid_list_filter");
  const listLimit = await handlers.handleResources({ limit: 0 });
  assert.equal(listLimit.code, "invalid_list_limit");
  const listOffset = await handlers.handleResources({ offset: 1_000_001 });
  assert.equal(listOffset.code, "invalid_list_offset");

  const searchAuthority = await handlers.handleSearch({
    query: "weather",
    sign: true,
  });
  assert.equal(searchAuthority.code, "invalid_search_filter");
  const searchQuery = await handlers.handleSearch({ query: "---" });
  assert.equal(searchQuery.code, "invalid_search_query");
  const searchCursor = await handlers.handleSearch({
    query: "api",
    cursor: "v1:not-a-fingerprint:1",
  });
  assert.equal(searchCursor.code, "invalid_search_cursor");

  assert.deepEqual([calls.list, calls.search], [0, 0]);
  for (const result of [
    listAuthority,
    listLimit,
    listOffset,
    searchAuthority,
    searchQuery,
    searchCursor,
  ]) {
    assert.equal(result.status, "rejected");
    assert.equal(result.response, null);
    assert.ok(result.reason.length > 0);
    assertNoAuthority(result);
  }
});

test("Rust port rejection codes remain stable and catalog unavailability stays explicit", async () => {
  const handlers = new PureBazaarDiscoveryHandlers({
    async list() {
      return {
        status: "rejected",
        code: "invalid_list_filter",
        reason: "Rust catalog rejected the filter fingerprint",
      };
    },
    async search() {
      return {
        status: "unavailable",
        code: "catalog_unavailable",
        reason: "Rust catalog port is not configured",
      };
    },
  });

  const list = await handlers.handleResources({ network: "stellar:testnet" });
  assert.equal(list.status, "rejected");
  assert.equal(list.code, "invalid_list_filter");
  assert.equal(list.response, null);

  const search = await handlers.handleSearch({ query: "stellar" });
  assert.equal(search.status, "unavailable");
  assert.equal(search.code, "catalog_unavailable");
  assert.equal(search.response, null);
  assertNoAuthority(list);
  assertNoAuthority(search);
});

test("malformed port results and authority-shaped responses fail closed", async () => {
  const listResponse = (await readFixture(
    "list_response.json",
  )) as Record<string, unknown>;
  const searchPages = (await readFixture(
    "search_pages.json",
  )) as SearchPagesFixture;

  const malformedPort = new PureBazaarDiscoveryHandlers({
    async list() {
      return { status: "completed", response: listResponse, signer: "forbidden" };
    },
    async search() {
      return { status: "unknown", response: null };
    },
  });
  const malformedList = await malformedPort.handleResources({});
  const malformedSearch = await malformedPort.handleSearch({ query: "api" });
  assert.equal(malformedList.code, "resources_port_invalid");
  assert.equal(malformedSearch.code, "search_port_invalid");

  const escalatedSearch = structuredClone(searchPages.pages[0]?.response ?? {});
  (escalatedSearch as Record<string, unknown>).walletSigningAllowed = true;
  const responsePort = new PureBazaarDiscoveryHandlers({
    async list() {
      const escalated = structuredClone(listResponse);
      escalated.actionPlanSubmitAllowed = true;
      return { status: "completed", response: escalated };
    },
    async search() {
      return { status: "completed", response: escalatedSearch };
    },
  });
  const list = await responsePort.handleResources({});
  const search = await responsePort.handleSearch({ query: "api", limit: 1 });
  assert.equal(list.code, "resources_response_invalid");
  assert.equal(search.code, "search_response_invalid");
  assert.equal(list.response, null);
  assert.equal(search.response, null);
  assertNoAuthority(list);
  assertNoAuthority(search);
});
