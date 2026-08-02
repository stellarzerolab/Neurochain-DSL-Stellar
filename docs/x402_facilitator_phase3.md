# X402 Facilitator Phase 3 Contract

This document defines the production boundary for attaching real x402
facilitator verify/settle support to NeuroChain. It is a product contract and
implementation checklist, not a request to add submit authority.

## Current State

x402 is already beyond a lite UI idea in this repository:

- `POST /api/x402/stellar/intent-plan` returns a typed payment and guardrail
  decision envelope
- `examples/x402_response_contract/schema.json` defines the agent/frontend
  response contract
- `examples/x402_response_contract/types.ts` and `client_adapter.ts` provide
  a frontend-facing state model
- the response matrix covers `payment_required`, `approved`,
  `requires_approval`, exit `3`, exit `4`, exit `5`, `replay_blocked`,
  `expired`, and `invalid_payment`
- safe JSONL audit can be enabled with `NC_X402_STELLAR_AUDIT_PATH`
- replay/idempotency state is separated behind `src/x402_store.rs`
- mock verification is fenced away from production
- `src/x402_facilitator.rs` contains an explicit `facilitator_verify_settle`
  boundary that currently fails closed

This is not production x402 yet. Production begins only when a real
facilitator verify/settle transport is implemented behind
`src/x402_facilitator.rs` and validated against the invariants below.

## Stellar Transport Baseline

The transport port follows the current official Stellar x402 baseline:

- x402 protocol version `2`
- Stellar scheme `exact` / `exact-v2`
- network identifiers `stellar:testnet` and `stellar:pubnet`
- facilitator operations `/supported`, `/verify`, and `/settle`
- Stellar payment payloads authorize SEP-41 token transfers through Soroban
  authorization entries

`X402FacilitatorTransport` models separate supported, verify, and settle calls
in Rust. An offline fake exercises the full state gates. A blocking reqwest
transport implements authenticated HTTPS `GET /supported` and read-only
`POST /verify`; settlement deliberately returns unavailable. The production
verifier remains fail closed because this transport is not runtime-connected
and no settlement path is enabled.

`X402FacilitatorConfig` validates the transport base URL, Stellar network,
SEP-41 asset contract, receiver StrKey, and bounded timeout before any HTTP
client can be constructed. Endpoint credentials, query strings, fragments,
plain HTTP, unknown networks, malformed StrKeys, and unsafe timeout values fail
closed. API keys are intentionally outside this configuration object.

The transport also models the official x402 v2 `/supported` wire format:
camel-case `x402Version`, version-grouped `kinds`, `signers`, and `extensions`.
Capability validation requires x402 v2, exact/exact-v2, and the configured
Stellar network. The standard response does not advertise per-asset allowlists,
so the configured SEP-41 asset remains a locally validated payment requirement
instead of being inferred from `/supported`. Missing or mismatched capability
data fails closed before verify or settle.

The offline verify orchestrator enforces the order `supported -> verify`.
Network mismatch, unavailable capability discovery, timeout, or unsupported
capability data stops before the verify call. It does not perform HTTP traffic,
settlement, signing, or ActionPlan submission.

The offline settlement gate accepts only a successful verify result and a
settlement request that exactly matches the verified network, payment payload,
payment requirements, and idempotency key. Rejected or mismatched requests stop
before the settlement transport. This gate is not connected to runtime or a
network transport.

## Hosted Facilitator Validation

The official Built on Stellar facilitator documents the testnet base URL as
`https://channels.openzeppelin.com/x402/testnet` and requires a generated API
key for facilitator use. A credential-free `GET /supported` probe on
2026-07-30 returned `HTTP 401 Unauthorized`.

No API key was generated or stored, and `/verify` and `/settle` were not
called. The approved credential boundary now reads the key only at request
time from `NC_X402_FACILITATOR_API_KEY`, creates a sensitive Authorization
header, and redacts credential state from Debug output. The key is not a field
of `X402FacilitatorConfig` and must not appear in fixtures, logs,
documentation, or source control.

The authenticated HTTPS transport now supports `/supported` and read-only
`/verify`. It disables redirects, applies the configured timeout, accepts
bounded JSON responses, and maps authentication, timeout, rate-limit, server,
content-type, and decoding failures to fail-closed transport errors. The
`/verify` builder emits the official x402 v2 `x402Version`, `paymentPayload`,
and `paymentRequirements` body, validates the selected network, SEP-41 asset,
receiver, amount, timeout, and accepted requirements locally, and preserves a
protocol-level `isValid: false` response as a payment rejection. Request
construction and response parsing are tested offline with non-secret fixtures.

No live `/verify` call has been made. The internal idempotency key remains a
NeuroChain replay input and is not added to the standard x402 v2 verify body.
`/settle` remains disabled, and the authenticated transport is not connected to
the server payment verifier.

An ignored live conformance test exercises the repository's real Rust
transport against the official Stellar testnet facilitator. It requires both
`NC_X402_LIVE_SUPPORTED_PROBE=1` and a process-local
`NC_X402_FACILITATOR_API_KEY`. Run it only after explicit approval for
credential use and a live testnet request. The test calls only `GET /supported`;
it cannot verify, settle, sign, transfer value, or submit an ActionPlan.

On 2026-07-31, an explicitly approved testnet run generated a temporary key
from the official endpoint (`HTTP 201`, JSON) and passed the ignored Rust
conformance test against the authenticated `/supported` endpoint. The key
existed only in that PowerShell process and was removed immediately afterward.
No credential value was printed or stored, and `/verify` and `/settle` were not
called.

## Phase 3 Deliverables

Phase 3 must add these pieces as a separate reviewed change:

1. Real facilitator verify/settle transport behind `src/x402_facilitator.rs`
   - authenticated verify is offline-ready but has not completed a separately
     approved live conformance call
   - settlement remains disabled and still requires implementation and review
2. Production pricing, asset, receiver, network, and facilitator endpoint
   configuration
3. Persistent replay/idempotency store policy for paid access attempts
4. Safe audit events for payment-required, finalized, replay, expired,
   invalid-payment, facilitator-unavailable, and facilitator-rejected outcomes
5. Tests proving that payment state never bypasses guardrails, ZK
   verification, approval, or submit boundaries

The protocol-neutral adapter contract is defined in
`examples/x402_facilitator_adapter/schema.json`. It locks the verify/settle
split, idempotency input, and no-submit invariant before any network transport
is selected. Its `state_transitions.json` companion locks replay and
fail-closed terminal behavior.

## Non Goals

Phase 3 must not add any of these to the default MCP or x402 path:

- underlying ActionPlan submit
- transaction signing
- wallet secret handling
- testnet or mainnet transaction broadcast
- attestation transaction submission
- audit nullifier consume
- proof generation authority
- proof verification authority beyond routing to the existing ZK/Soroban
  verification boundary
- production use of the mock x402 verifier

## Invariants

The following statements must remain true after real facilitator support is
attached:

- `payment finalized` is not guardrail approval
- `payment finalized` is not a ZK proof
- `payment finalized` is not `underlying_action_submit_allowed`
- `approved` only means the typed ActionPlan may continue to a later separate
  approval or submit flow
- `requires_approval` remains terminal no-submit in the default MCP and x402
  service path
- `blocked` remains terminal no-submit in the default MCP and x402 service
  path
- proof is not submit permission
- payment is not submit permission
- `NC_X402_STELLAR_VERIFIER=mock` fails closed in production runtimes
- `NC_X402_STELLAR_VERIFIER=facilitator` fails closed until real verify/settle
  transport is implemented
- unknown verifier modes fail closed

## Acceptance Tests

Before calling Phase 3 production-ready, the test suite must prove:

- mock verifier works only in non-production development mode
- mock verifier is unavailable when `NC_ENV`, `APP_ENV`, or `RUST_ENV` is
  `production`
- facilitator mode cannot finalize a payment until real transport exists
- replay store failures do not open access
- `invalid_payment` produces a typed fail-closed response
- paid access still runs the same deterministic guardrail evaluation
- paid access still leaves `underlying_action_submit_allowed=false`
- `requires_approval` and `blocked` never submit, sign, attest, or consume
  nullifiers

## Status Wording

Use this wording in product docs until Phase 3 is complete:

```text
x402 is beyond a lite UI idea: the paid ingress envelope, response contract,
viewer, audit path, replay store, production mock fence, and fail-closed
facilitator boundary exist. An authenticated, runtime-secret-only
/supported and read-only /verify transport is implemented but not
runtime-connected. The verify wire mapping is offline-tested; no live verify
call has been made. It is not production x402 until settlement and the reviewed
runtime integration are attached behind src/x402_facilitator.rs.
```
