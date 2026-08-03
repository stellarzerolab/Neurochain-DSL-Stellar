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
- facilitator mode emits an official x402 v2 `PAYMENT-REQUIRED` header and
  accepts a bounded Base64 x402 v2 `PAYMENT-SIGNATURE` payload
- the authenticated runtime performs only `supported -> verify`; accepted
  verification stops at `verified_pending_settlement`
- accepted verification is persistently bound to a canonical SHA-256 digest
  of the exact network, payment payload, requirements, and idempotency key
- an offline settlement state machine now locks single-attempt dispatch,
  terminal rejection, successful finalization, and uncertain-outcome recovery

This is not production x402 yet. Production begins only when settlement is
implemented and reviewed behind `src/x402_facilitator.rs` and the complete
paid-access lifecycle is validated against the invariants below.

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
transport implements authenticated HTTPS `GET /supported`, read-only
`POST /verify`, and state-gated `POST /settle`. Facilitator mode still
runtime-connects only the verify-only adapter through a blocking worker. It
never treats verification as settlement or permission to evaluate or execute
the underlying ActionPlan.

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

`X402FacilitatorVerifyOnlyAdapter` composes that orchestrator with the
protocol-neutral adapter contract. It accepts only verify envelopes and maps
accepted, rejected, timeout, unavailable, invalid-response, and capability
failures to typed no-submit results. It intentionally has no settle method.
The server's `FacilitatorX402PaymentVerifier` wrapper uses it for
`supported -> verify`, inspects challenge state without consuming it, and
returns `payment_verified_settlement_required` after accepted verification.
Guardrails remain `not_run`, challenge state remains unfinalized, and
`underlying_action_submit_allowed=false`.

`src/x402_store.rs` now persists a protocol-safe settlement record without the
raw payment payload or signature. The allowed lifecycle is
`verified_pending_settlement -> settlement_in_progress -> settled`.
`settlement_rejected` is terminal. A timeout or unavailable response after
dispatch is not safe evidence that no transaction was submitted, so it becomes
`settlement_outcome_unknown` and cannot be retried automatically. Loading a
store that was left in `settlement_in_progress` after a process stop performs
the same fail-closed recovery. Successful completion alone marks the challenge
finalized and stores a bounded Stellar transaction hash.

The state transition and file persistence happen while the server owns the
single challenge-store mutex. The current file store is therefore a
single-process policy; running multiple server processes against one state file
is unsupported until a storage backend with cross-process transactions is
introduced.

`src/x402_audit.rs` provides a settlement audit event format containing only
the audit and challenge identifiers, request digest, timestamps, state, and an
optional public transaction hash. It never accepts or writes the payment
payload, payment requirements, signed authorization, or credential.

The offline settlement gate accepts only a successful verify result and a
settlement request that exactly matches the verified network, payment payload,
payment requirements, and idempotency key. Rejected or mismatched requests stop
before the settlement transport. Every transport attempt must first persist the
single-use `begin_settlement` transition; success, rejection, and uncertain
transport errors are then persisted before another attempt can be considered.
This gate is not connected to the server runtime. The authenticated HTTP
settlement transport exists behind it and is validated offline, but no default
request path can call it.

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

The authenticated HTTPS transport now supports `/supported`, read-only
`/verify`, and state-gated `/settle`. It disables redirects, applies the
configured timeout, accepts bounded JSON responses, and maps authentication,
timeout, rate-limit, server, content-type, and decoding failures to fail-closed
transport errors. The request builders emit the official x402 v2
`x402Version`, `paymentPayload`, and `paymentRequirements` body, validate the
selected network, SEP-41 asset, receiver, amount, timeout, and accepted
requirements locally, and never send the internal idempotency key. The settle
parser requires the configured network, a 64-character Stellar transaction
hash on success, and an explicit error reason with no hash on rejection.
Request construction and response parsing are tested offline with non-secret
fixtures.

The internal idempotency key remains a NeuroChain replay input and is not added
to the standard x402 v2 verify or settle body. It is included in the local
request digest that binds verify to the settlement transition. Only the
authenticated verify-only adapter is connected to the server payment verifier,
and only when all facilitator runtime settings validate.

Ignored live conformance tests exercise the repository's real Rust transport
against the official Stellar testnet facilitator. The capability probe requires
`NC_X402_LIVE_SUPPORTED_PROBE=1`; the verify probe requires
`NC_X402_LIVE_VERIFY_PROBE=1`. Both require a process-local
`NC_X402_FACILITATOR_API_KEY` and explicit approval for credential use and a
live testnet request. The verify probe sends a deliberately malformed, unsigned
payment payload and requires the facilitator to return `isValid: false`; it
cannot produce a valid payment authorization. Neither test can settle, sign,
transfer value, or submit an ActionPlan.

On 2026-07-31, an explicitly approved testnet run generated a temporary key
from the official endpoint (`HTTP 201`, JSON) and passed the ignored Rust
conformance test against the authenticated `/supported` endpoint. The key
existed only in that PowerShell process and was removed immediately afterward.
No credential value was printed or stored, and `/verify` and `/settle` were not
called.

On 2026-08-02, a separately approved run generated another temporary testnet
key (`HTTP 201`, JSON) and made exactly one authenticated `POST /verify` call
through the repository's Rust transport. The facilitator rejected the unsigned
fixture with `invalid_exact_stellar_payload_malformed`, confirming the live
wire mapping, authentication, parser, and fail-closed rejection path. The key
again existed only in that PowerShell process and was removed immediately.
No `/settle`, payment, signature, transaction submission, or ActionPlan submit
occurred.

## Phase 3 Deliverables

Phase 3 must add these pieces as a separate reviewed change:

1. Real facilitator verify/settle transport behind `src/x402_facilitator.rs`
   - authenticated verify has completed a separately approved live rejection
     conformance call; a valid signed payment has not been verified
   - verify-only runtime activation now emits official x402 v2 challenge and
     payload headers and connects authenticated `supported -> verify`
   - authenticated settlement transport is implemented and offline-reviewed;
     runtime activation and a valid signed live test remain separate
2. Production pricing, asset, receiver, network, and facilitator endpoint
   configuration
3. Persistent replay/idempotency store policy for paid access attempts
   - implemented for exact verify binding, single settlement start, terminal
     rejection, successful finalization, and uncertain-outcome recovery
4. Safe audit events for payment-required, finalized, replay, expired,
   invalid-payment, facilitator-unavailable, facilitator-rejected, and future
   settlement outcomes
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
- `NC_X402_STELLAR_VERIFIER=facilitator` fails closed after verify until
  settlement runtime activation is separately implemented and reviewed
- unknown verifier modes fail closed

## Acceptance Tests

Before calling Phase 3 production-ready, the test suite must prove:

- mock verifier works only in non-production development mode
- mock verifier is unavailable when `NC_ENV`, `APP_ENV`, or `RUST_ENV` is
  `production`
- facilitator mode cannot finalize a payment until real settlement exists
- the same verified request cannot trigger a second verify or concurrent
  settlement attempt
- settlement timeout, post-dispatch unavailability, and process restart remain
  fail closed as `settlement_outcome_unknown`
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
facilitator boundary exist. Facilitator mode emits an official x402 v2
PAYMENT-REQUIRED challenge and runtime-connects authenticated supported ->
verify through a blocking worker. Accepted verification stops at
verified_pending_settlement, persists an exact request digest, and keeps
guardrails not run and underlying_action_submit_allowed=false. The persistent
settlement state machine blocks duplicate and uncertain retries, and the
authenticated HTTP /settle wire path is implemented and validated offline. It
is not connected to the default server runtime, and no live settlement has been
performed. One approved live testnet rejection probe confirmed the authenticated
/verify wire path without validating or settling a payment. Production still
requires explicit settlement runtime integration, a valid signed testnet
conformance run, and reviewed pricing and receiver configuration.
```
