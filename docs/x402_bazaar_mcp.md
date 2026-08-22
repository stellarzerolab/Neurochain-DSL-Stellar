# Offline Stellar Bazaar MCP search contract

Date: 2026-08-22

Status: versioned read-only contract, schemas, fixtures, and fail-closed tests;
no MCP server runtime and no paid-call proxy

## Scope

`src/x402_bazaar_mcp.rs` adapts the existing offline
`BazaarCatalog::search` contract into one MCP tool named
`search_stellar_bazaar`. The module generates deterministic `tools/list`
metadata, strict Draft 2020-12 input and output schemas, a typed result with
stable codes and non-empty reasons, and the `tools/call` result envelope used
by the checked-in fixtures.

This is a separate future discovery-service contract. It does not modify or
extend the existing five-tool NeuroChain guardrail MCP v0 stdio runtime. It
does not start a process, register an HTTP route, read the network, or depend
on an MCP SDK.

The contract pins MCP `2026-07-28` because that current revision supports
stateless requests, deterministic `tools/list`, JSON Schema output contracts,
and structured tool results. The checked-in `search_call.json` includes the
per-request protocol and client metadata. Transport negotiation and
`server/discover` remain runtime work.

## Tool contract

The input requires `schemaVersion: 1` and a bounded `query`. Optional `type`,
`payTo`, `scheme`, `network`, `extensions`, `limit`, and opaque `cursor`
fields delegate to the already-tested Bazaar search core. Unknown fields fail
strict decoding. The full serialized arguments are limited to 4096 bytes
before decoding.

The result always contains:

- `schemaVersion`, `protocolVersion`, and `tool` identity;
- `ok`, stable `code`, non-empty `reason`, and `retryable`;
- an explicit `authority` object whose nine capability flags are false;
- `data` only after a successful local search.

The MCP result sets `isError` for failed tool execution and repeats the exact
serialized `structuredContent` in a text content block for client
compatibility. The declared `outputSchema` requires successful results to
contain data and failed results to omit it.

Stable local failure codes include `invalid_arguments`,
`arguments_too_large`, `unsupported_schema_version`,
`catalog_unavailable`, and the existing Bazaar codes such as
`invalid_search_query`, `invalid_search_filter`, and
`invalid_search_cursor`. Catalog unavailability is retryable; validation
failures are not. No failure invokes an external or paid fallback.

## Authority boundary

MCP annotations such as `readOnlyHint` are untrusted usability hints under the
MCP specification. NeuroChain therefore does not rely on them for safety. The
machine-readable result contract keeps all of these false on success and
failure:

- payment, proof, and approval;
- settlement and signing;
- wallet and shell access;
- RPC submit and ActionPlan submit.

Discovery returns catalog information only. It cannot turn a discovered
resource into payment permission or execution permission. A later paid-call
milestone must define one bounded named-service access grant while keeping
payment, proof, approval, settlement, signing, execution, and submission as
separate authorities. That future work still cannot add live payment, wallet
signing, or ActionPlan submission without explicit approval.

## Evidence and deliberate limits

`tests/x402_bazaar_mcp.rs` covers deterministic tool discovery, successful
structured output, text/structured parity, filters and cursor delegation,
unknown/hostile/oversized arguments, catalog unavailability, stable reasons,
and the all-false authority invariant. Fixtures live in
`examples/x402_bazaar_mcp/`.

This milestone does not implement `server/discover`, JSON-RPC dispatch,
transport authentication, catalog persistence, payment discovery/retry,
paid-call, network access, settlement, signing, wallet access, shell access,
RPC submission, or ActionPlan submission. It does not claim a production MCP
server or complete the RFP agent-facing deliverable.

## Primary sources

- SCF x402 Facilitator with Bazaar RFP, section 3.3:
  <https://github.com/stellar/scf-handbook/blob/main/scf-awards/build-award/rfp-track.md#x402-facilitator-with-bazaar-discovery-support>
- MCP `2026-07-28` tools specification:
  <https://modelcontextprotocol.io/specification/2026-07-28/server/tools>
- MCP `2026-07-28` discovery specification:
  <https://modelcontextprotocol.io/specification/2026-07-28/server/discover>
