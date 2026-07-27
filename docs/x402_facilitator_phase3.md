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

## Phase 3 Deliverables

Phase 3 must add these pieces as a separate reviewed change:

1. Real facilitator verify/settle transport behind `src/x402_facilitator.rs`
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
is selected.

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
facilitator boundary exist. It is not production x402 until real facilitator
verify/settle transport is attached behind src/x402_facilitator.rs.
```
