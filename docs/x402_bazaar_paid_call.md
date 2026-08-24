# Offline Stellar Bazaar paid-call contract

Date: 2026-08-22

Status: versioned offline authorization contract, fixtures, and fail-closed
Rust/TypeScript parity tests; no dispatch, payment handling, settlement
runtime, or service proxy

## Scope

`src/x402_bazaar_paid_call.rs` defines the bounded MCP tool
`proxy_paid_stellar_call`. The tool can authorize one exact cataloged MCP
service call only when a separate trusted access gate atomically consumes a
matching settled, single-use access grant.

The untrusted MCP request contains only a request identifier, the exact Bazaar
catalog resource key, and bounded JSON service arguments. It cannot contain or
self-assert a payment payload, verification result, settlement result,
authority flag, wallet reference, signing request, shell access, RPC submit,
or ActionPlan submit.

This milestone is deliberately offline. It creates and validates the call
binding and returns a machine-readable authorization result, but performs no
dispatch to the cataloged service.

## Trusted access boundary

The runtime boundary is the `BazaarPaidCallAccessGate` trait. Its implementation
must own the payment and settlement state and atomically consume access bound
to all of the following:

- schema version and caller request identifier;
- exact catalog key, resource URL, MCP tool name, and Stellar network;
- the cataloged payment terms;
- a canonical SHA-256 digest of the bounded service arguments;
- a canonical digest of the complete call binding.

Only `Authorized` produces `serviceCallAllowed: true`. The settled grant is
single-use: a repeated or mismatched binding must return
`access_replay_blocked`. Payment required, settlement pending or rejected,
unknown settlement outcome, and unavailable state all fail closed without a
service call.

The input contract accepts only cataloged MCP resources in this milestone.
HTTP route and body mapping require a separate reviewed contract.

## Authority contract

The successful result grants only permission for the named service call bound
by the returned digest. Every result explicitly keeps these capabilities
false:

- payment, proof, and approval;
- settlement and signing;
- underlying execution;
- wallet and shell access;
- RPC submit and ActionPlan submit.

This authorization does not claim that payment was performed by this module.
The future `@x402/stellar` runtime remains responsible for the protocol retry,
verification, settlement, and creation of the trusted settled-access state.
NeuroChain must not reimplement that payment core or infer service access from
an untrusted MCP argument.

## Stable outcomes

| Code | Meaning | Retryable |
| --- | --- | --- |
| `service_call_authorized` | Exact settled access was consumed | no |
| `payment_required` | No settled access exists for the binding | yes |
| `settlement_pending` | Settlement has not completed | yes |
| `settlement_rejected` | Settlement was rejected | no |
| `settlement_outcome_unknown` | Outcome is ambiguous; automatic retry is blocked | no |
| `access_replay_blocked` | Grant was consumed or binding did not match | no |
| `access_gate_unavailable` | Trusted state is unavailable | yes |

Strict validation also returns stable codes for malformed, oversized,
unsupported, missing, non-MCP, or unavailable catalog input. Every result has
a non-empty reason and deterministic structured/text parity.

## Evidence and deliberate limits

`tests/x402_bazaar_paid_call.rs` covers exact binding, canonical argument
hashing, one-shot consumption, replay blocking, all settlement states,
untrusted authority injection, bounded hostile input, unavailable dependencies,
and MCP result parity. Fixtures live in `examples/x402_bazaar_paid_call/`.

The TypeScript parity adapter accepts the same strict MCP call fixture and
delegates the exact binding and single-use access decision to an injected Rust
port. Shared `authorized_result.json` and `replay_result.json` fixtures prove
that TypeScript preserves the Rust digests, stable codes, retryability, and
canonical text/structured result. The adapter validates correlation and
authority fields but never recomputes the binding, consumes settled state, or
dispatches the authorized service call.

There is no dispatch, MCP server runtime, network access, payment verification,
settlement, wallet signing, service execution, HTTP proxy, RPC submission, or
ActionPlan submission. Wiring the trusted gate to `@x402/stellar` remains a
future dependency and runtime decision that requires explicit approval.

## Primary sources

- SCF x402 Facilitator with Bazaar RFP, section 3.3:
  <https://github.com/stellar/scf-handbook/blob/main/scf-awards/build-award/rfp-track.md#x402-facilitator-with-bazaar-discovery-support>
- MCP `2026-07-28` tools specification:
  <https://modelcontextprotocol.io/specification/2026-07-28/server/tools>
- Coinbase Bazaar MCP server flow:
  <https://docs.cdp.coinbase.com/api-reference/v2/rest-api/x402-facilitator/bazaar-mcp-server>
- Coinbase x402 client/server flow:
  <https://docs.cdp.coinbase.com/x402/core-concepts/client-server>
