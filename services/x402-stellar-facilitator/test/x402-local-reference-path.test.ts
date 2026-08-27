import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  BAZAAR_MCP_RUNTIME_BOUNDARY,
  type BazaarMcpRustPort,
  PureBazaarMcpParityHandlers,
} from "../src/bazaar-mcp-paid-call.js";
import {
  type FacilitatorPort,
  type NeuroChainEvaluationPort,
  PureFacilitatorHandlers,
  SERVICE_HANDLER_AUTHORITY_BOUNDARY,
} from "../src/service-handlers.js";

interface ReferenceRequest {
  readonly schema_version: 1;
  readonly discovery_arguments: Readonly<Record<string, unknown>>;
  readonly evaluation_request: Readonly<Record<string, unknown>>;
  readonly paid_call_arguments: Readonly<Record<string, unknown>>;
}

interface ScenarioResult {
  readonly events: readonly string[];
  readonly decision: string;
  readonly serviceCallAllowed: boolean;
  readonly serviceDispatchAllowed: false;
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

function toolCall(
  id: string,
  name: string,
  argumentsValue: Readonly<Record<string, unknown>>,
): Readonly<Record<string, unknown>> {
  return {
    jsonrpc: "2.0",
    id,
    method: "tools/call",
    params: {
      name,
      arguments: argumentsValue,
      _meta: {
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {
          name: "offline-reference-path",
          version: "1.0.0",
        },
        "io.modelcontextprotocol/clientCapabilities": {},
      },
    },
  };
}

function unusedFacilitator(): FacilitatorPort {
  return {
    getSupported() {
      throw new Error("reference path must not query supported");
    },
    async verify() {
      throw new Error("reference path must not verify payment");
    },
    async settle() {
      throw new Error("reference path must not settle payment");
    },
  };
}

function paidCallAuthorized(
  request: ReferenceRequest,
): Readonly<Record<string, unknown>> {
  const paid = request.paid_call_arguments;
  return {
    authority: {
      actionPlanSubmitAllowed: false,
      approvalAllowed: false,
      paymentAllowed: false,
      proofAllowed: false,
      rpcSubmitAllowed: false,
      serviceCallAllowed: true,
      settlementAllowed: false,
      shellAccessAllowed: false,
      signingAllowed: false,
      underlyingExecutionAllowed: false,
      walletAccessAllowed: false,
    },
    code: "service_call_authorized",
    data: {
      argumentsDigest: "a".repeat(64),
      callDigest: "b".repeat(64),
      network: "stellar:testnet",
      requestId: paid.requestId,
      resourceKey: paid.resourceKey,
      resourceUrl: "https://api.example.com/mcp",
      schemaVersion: 1,
      toolName: "plan_stellar_action",
    },
    ok: true,
    protocolVersion: "2026-07-28",
    reason: "Settled access was consumed for this exact named service call.",
    retryable: false,
    schemaVersion: 1,
    tool: "proxy_paid_stellar_call",
  };
}

async function runScenario(
  requestName: string,
  responseName: string,
): Promise<ScenarioResult> {
  const request = (await readFixture(
    "x402_local_reference_path",
    requestName,
  )) as ReferenceRequest;
  const evaluationResponse = await readFixture(
    "x402_local_reference_path",
    responseName,
  );
  const searchFixture = structuredContent(
    await readFixture("x402_bazaar_mcp", "search_result.json"),
  );
  const events: string[] = [];
  let paidCalls = 0;
  const rustPort: BazaarMcpRustPort = {
    async search() {
      events.push("bazaar_discovery");
      return structuredClone(searchFixture);
    },
    async paidCall() {
      events.push("capability_gate");
      paidCalls += 1;
      return paidCallAuthorized(request);
    },
  };
  const evaluationPort: NeuroChainEvaluationPort = {
    async evaluate() {
      events.push("typed_action_plan_and_policy");
      return structuredClone(evaluationResponse);
    },
  };

  const bazaar = new PureBazaarMcpParityHandlers(rustPort);
  const discovery = await bazaar.handleSearchCall(
    toolCall(
      String(asRecord(request.evaluation_request).request_id),
      "search_stellar_bazaar",
      request.discovery_arguments,
    ),
  );
  assert.equal(discovery.isError, false);
  assert.equal(structuredContent(discovery).code, "search_completed");

  events.push("settled_access_ready");
  const facilitator = new PureFacilitatorHandlers(
    unusedFacilitator(),
    undefined,
    evaluationPort,
  );
  const evaluation = await facilitator.handleEvaluation(
    request.evaluation_request,
  );
  assert.equal(evaluation.status, "completed");
  assert.equal(evaluation.code, "evaluation_completed");
  assert.ok(evaluation.response !== null);

  let serviceCallAllowed = false;
  if (evaluation.response.decision === "approved") {
    const capability = await bazaar.handlePaidCall(
      toolCall(
        String(asRecord(request.evaluation_request).request_id),
        "proxy_paid_stellar_call",
        request.paid_call_arguments,
      ),
    );
    assert.equal(capability.isError, false);
    serviceCallAllowed =
      asRecord(structuredContent(capability).authority).serviceCallAllowed === true;
  }

  assert.equal(paidCalls, evaluation.response.decision === "approved" ? 1 : 0);
  assert.ok(Object.values(BAZAAR_MCP_RUNTIME_BOUNDARY).every((grant) => !grant));
  assert.ok(
    Object.values(SERVICE_HANDLER_AUTHORITY_BOUNDARY).every((grant) => !grant),
  );
  return {
    events,
    decision: evaluation.response.decision,
    serviceCallAllowed,
    serviceDispatchAllowed: false,
  };
}

test("approved fixture follows discovery, access, policy, then exact capability", async () => {
  const result = await runScenario(
    "approved_request.json",
    "approved_evaluation_response.json",
  );
  assert.deepEqual(result.events, [
    "bazaar_discovery",
    "settled_access_ready",
    "typed_action_plan_and_policy",
    "capability_gate",
  ]);
  assert.equal(result.decision, "approved");
  assert.equal(result.serviceCallAllowed, true);
  assert.equal(result.serviceDispatchAllowed, false);
});

test("blocked fixture never reaches the capability gate", async () => {
  const result = await runScenario(
    "blocked_request.json",
    "blocked_evaluation_response.json",
  );
  assert.deepEqual(result.events, [
    "bazaar_discovery",
    "settled_access_ready",
    "typed_action_plan_and_policy",
  ]);
  assert.equal(result.decision, "blocked");
  assert.equal(result.serviceCallAllowed, false);
  assert.equal(result.serviceDispatchAllowed, false);
});

test("evaluation authority escalation fails before capability access", async () => {
  const request = (await readFixture(
    "x402_local_reference_path",
    "approved_request.json",
  )) as ReferenceRequest;
  const escalated = asRecord(
    structuredClone(
      await readFixture(
        "x402_local_reference_path",
        "approved_evaluation_response.json",
      ),
    ),
  );
  asRecord(escalated.authority_grants).wallet_signing = true;
  let paidCalls = 0;
  const bazaar = new PureBazaarMcpParityHandlers({
    async search() {
      return structuredContent(
        await readFixture("x402_bazaar_mcp", "search_result.json"),
      );
    },
    async paidCall() {
      paidCalls += 1;
      return paidCallAuthorized(request);
    },
  });
  const discovery = await bazaar.handleSearchCall(
    toolCall("tampered", "search_stellar_bazaar", request.discovery_arguments),
  );
  assert.equal(discovery.isError, false);

  const facilitator = new PureFacilitatorHandlers(
    unusedFacilitator(),
    undefined,
    { async evaluate() { return escalated; } },
  );
  const evaluation = await facilitator.handleEvaluation(request.evaluation_request);
  assert.equal(evaluation.status, "rejected");
  assert.equal(evaluation.code, "evaluation_response_invalid");
  assert.equal(evaluation.response, null);
  assert.equal(paidCalls, 0);
});
