# Offline Stellar Bazaar MCP search fixtures

These fixtures lock a read-only, offline MCP search contract for the existing
Stellar x402 Bazaar catalog. They do not start an MCP server, make a network
request, inspect a wallet, pay, settle, sign, execute, call an RPC submit
method, or submit an ActionPlan.

- `search_call.json` is a current MCP `2026-07-28` `tools/call` request.
- `search_result.json` is the deterministic successful MCP result envelope.
- `catalog_unavailable_result.json` proves a retryable fail-closed outcome.

The same fixtures are consumed by the listener-free TypeScript parity adapter.
Only the strict call arguments are handed to the injected Rust port; MCP
client metadata never becomes payment, wallet, settlement, or dispatch
authority.

The `tools/list` schema is generated and asserted directly by the Rust contract
tests. It exposes `search_stellar_bazaar` followed by the separately guarded
`proxy_paid_stellar_call` in deterministic order.

The MCP annotations are usability hints only. The serialized `authority`
object is the explicit contract: every payment, proof, approval, settlement,
signing, wallet, shell, RPC-submit, and ActionPlan-submit capability remains
false. The separate paid-call contract is the only place that can preserve an
exact single-use service-access grant, and it still performs no dispatch.
