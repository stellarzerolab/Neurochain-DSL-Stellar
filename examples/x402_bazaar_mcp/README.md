# Offline Stellar Bazaar MCP search fixtures

These fixtures lock a read-only, offline MCP search contract for the existing
Stellar x402 Bazaar catalog. They do not start an MCP server, make a network
request, inspect a wallet, pay, settle, sign, execute, call an RPC submit
method, or submit an ActionPlan.

- `search_call.json` is a current MCP `2026-07-28` `tools/call` request.
- `search_result.json` is the deterministic successful MCP result envelope.
- `catalog_unavailable_result.json` proves a retryable fail-closed outcome.

The `tools/list` schema is generated and asserted directly by the Rust contract
tests. It exposes only `search_stellar_bazaar` in deterministic order.

The MCP annotations are usability hints only. The serialized `authority`
object is the explicit contract: every payment, proof, approval, settlement,
signing, wallet, shell, RPC-submit, and ActionPlan-submit capability remains
false. A separate paid-call milestone must define any service-access grant;
paid-call is intentionally absent here.
