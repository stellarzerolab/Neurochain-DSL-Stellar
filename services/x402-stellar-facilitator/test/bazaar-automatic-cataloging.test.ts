import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  type BazaarAutomaticCatalogingPort,
  type BazaarCatalogingOutcome,
  catalogUpstreamBazaarExtension,
} from "../src/bazaar-automatic-cataloging.js";
import { SERVICE_HANDLER_AUTHORITY_BOUNDARY } from "../src/service-handlers.js";

interface AutomaticFixture {
  readonly schemaVersion: 1;
  readonly x402Version: number;
  readonly resource: Record<string, unknown>;
  readonly payment: Record<string, unknown>;
  readonly bazaar: Record<string, unknown>;
}

interface OutcomeContract {
  readonly schemaVersion: 1;
  readonly outcomes: Record<
    string,
    Readonly<{ code: string; headerStatus: "success" | "rejected" | null }>
  >;
}

async function readFixture(name: string): Promise<unknown> {
  const fixtureUrl = new URL(
    `../../../../examples/x402_bazaar_cataloging/${name}`,
    import.meta.url,
  );
  return JSON.parse(await readFile(fixtureUrl, "utf8")) as unknown;
}

function splitFixture(fixture: AutomaticFixture) {
  const { bazaar, ...context } = fixture;
  return { context, extensions: { bazaar } };
}

function fixedPort(
  calls: { count: number; handoff?: unknown },
  catalogingOutcome: BazaarCatalogingOutcome,
): BazaarAutomaticCatalogingPort {
  return {
    async catalog(handoff) {
      calls.count += 1;
      calls.handoff = handoff;
      return catalogingOutcome;
    },
  };
}

function assertNoAuthority(result: {
  readonly authorityBoundary: typeof SERVICE_HANDLER_AUTHORITY_BOUNDARY;
}) {
  assert.deepEqual(result.authorityBoundary, SERVICE_HANDLER_AUTHORITY_BOUNDARY);
  assert.ok(Object.values(result.authorityBoundary).every((grant) => !grant));
}

function decodeHeader(value: string | null): unknown {
  assert.notEqual(value, null);
  return JSON.parse(Buffer.from(value ?? "", "base64").toString("utf8")) as unknown;
}

test("shared HTTP and MCP fixtures become strict Rust handoffs", async () => {
  for (const name of ["automatic_http.json", "automatic_mcp.json"] as const) {
    const fixture = (await readFixture(name)) as AutomaticFixture;
    const { context, extensions } = splitFixture(fixture);
    const calls: { count: number; handoff?: unknown } = { count: 0 };
    const result = await catalogUpstreamBazaarExtension(
      extensions,
      context,
      fixedPort(calls, {
        disposition: "accepted",
        code: "cataloged",
        reason: "discovery info passed schema validation and was cataloged",
        catalogKey: name.startsWith("automatic_http")
          ? "http:https://api.example.com/weather/:country/:city"
          : "mcp:https://api.example.com/mcp#plan_stellar_action",
      }),
    );

    assert.equal(calls.count, 1);
    assert.deepEqual(calls.handoff, fixture);
    assert.deepEqual(result.extensionResponses, {
      bazaar: { status: "success" },
    });
    assert.deepEqual(decodeHeader(result.extensionResponsesHeaderValue), {
      bazaar: { status: "success" },
    });
    assertNoAuthority(result);
  }
});

test("shared outcome contract maps accepted, dropped, invalid, duplicate and unavailable", async () => {
  const fixture = (await readFixture("automatic_http.json")) as AutomaticFixture;
  const contract = (await readFixture("outcome_contract.json")) as OutcomeContract;
  const { context, extensions } = splitFixture(fixture);
  assert.equal(contract.schemaVersion, 1);

  const accepted = await catalogUpstreamBazaarExtension(
    extensions,
    context,
    fixedPort({ count: 0 }, {
      disposition: "accepted",
      code: contract.outcomes.accepted?.code ?? "missing",
      reason: "cataloged",
      catalogKey: "http:https://api.example.com/weather/:country/:city",
    }),
  );
  assert.equal(accepted.outcome.code, "cataloged");
  assert.equal(accepted.extensionResponses?.bazaar.status, "success");

  const dropped = await catalogUpstreamBazaarExtension({}, context);
  assert.equal(dropped.outcome.disposition, "dropped");
  assert.equal(dropped.outcome.code, contract.outcomes.dropped?.code);
  assert.equal(dropped.extensionResponses, null);
  assert.equal(dropped.extensionResponsesHeaderValue, null);

  for (const disposition of ["invalid", "duplicate", "unavailable"] as const) {
    const expected = contract.outcomes[disposition];
    assert.ok(expected);
    const result = await catalogUpstreamBazaarExtension(
      extensions,
      context,
      fixedPort({ count: 0 }, {
        disposition,
        code: expected.code,
        reason: `${disposition} fixture reason`,
        ...(disposition === "duplicate"
          ? { catalogKey: "http:https://api.example.com/weather/:country/:city" }
          : {}),
      }),
    );
    assert.equal(result.outcome.code, expected.code);
    assert.equal(result.extensionResponses?.bazaar.status, "rejected");
    assert.equal(
      result.extensionResponses?.bazaar.rejectedReason,
      `${expected.code}: ${disposition} fixture reason`,
    );
    assert.deepEqual(
      decodeHeader(result.extensionResponsesHeaderValue),
      result.extensionResponses,
    );
    assertNoAuthority(result);
  }
});

test("malformed, oversized and unknown Bazaar metadata fail before the catalog port", async () => {
  const fixture = (await readFixture("automatic_http.json")) as AutomaticFixture;
  const { context, extensions } = splitFixture(fixture);
  const calls = { count: 0 };
  const port = fixedPort(calls, {
    disposition: "accepted",
    code: "cataloged",
    reason: "must not be reached",
  });

  const malformed = await catalogUpstreamBazaarExtension(
    { bazaar: { info: {} } },
    context,
    port,
  );
  assert.equal(malformed.outcome.code, "invalid_bazaar_extension");

  const oversized = await catalogUpstreamBazaarExtension(
    {
      bazaar: {
        ...extensions.bazaar,
        info: { input: { type: "http", padding: "x".repeat(33 * 1024) } },
      },
    },
    context,
    port,
  );
  assert.equal(oversized.outcome.code, "json_value_too_large");

  const unknown = await catalogUpstreamBazaarExtension(
    { bazaar: { ...extensions.bazaar, signer: "forbidden" } },
    context,
    port,
  );
  assert.equal(unknown.outcome.code, "invalid_bazaar_extension");

  const nonJson = await catalogUpstreamBazaarExtension(
    { bazaar: { ...extensions.bazaar, info: { input: new Date(0) } } },
    context,
    port,
  );
  assert.equal(nonJson.outcome.code, "invalid_json_value");
  assert.equal(calls.count, 0);
  assertNoAuthority(unknown);
});

test("unavailable and malformed catalog ports produce stable rejected wire results", async () => {
  const fixture = (await readFixture("automatic_mcp.json")) as AutomaticFixture;
  const { context, extensions } = splitFixture(fixture);

  const unavailable = await catalogUpstreamBazaarExtension(extensions, context);
  assert.equal(unavailable.outcome.disposition, "unavailable");
  assert.equal(unavailable.outcome.code, "catalog_unavailable");
  assert.equal(unavailable.extensionResponses?.bazaar.status, "rejected");
  assert.ok(
    unavailable.extensionResponses?.bazaar.rejectedReason?.startsWith(
      "catalog_unavailable:",
    ),
  );

  const malformed = await catalogUpstreamBazaarExtension(
    extensions,
    context,
    { async catalog() { return { status: "accepted", secret: "forbidden" }; } },
  );
  assert.equal(malformed.outcome.disposition, "unavailable");
  assert.equal(malformed.outcome.code, "catalog_outcome_invalid");
  assert.equal(malformed.extensionResponses?.bazaar.status, "rejected");
  assertNoAuthority(malformed);
});

test("handoff context rejects raw payload, signer and settlement authority fields", async () => {
  const fixture = (await readFixture("automatic_http.json")) as AutomaticFixture;
  const { context, extensions } = splitFixture(fixture);
  for (const forbidden of [
    { paymentPayload: {} },
    { signer: "forbidden" },
    { settlement: { success: true } },
    { actionPlanSubmitAllowed: true },
  ]) {
    const result = await catalogUpstreamBazaarExtension(
      extensions,
      { ...context, ...forbidden },
      fixedPort({ count: 0 }, {
        disposition: "accepted",
        code: "cataloged",
        reason: "must not be reached",
      }),
    );
    assert.equal(result.outcome.disposition, "invalid");
    assert.equal(result.outcome.code, "invalid_catalog_handoff_context");
    assert.equal(result.handoff, null);
    assertNoAuthority(result);
  }
});
