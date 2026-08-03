# x402 Facilitator Adapter Contract

This package defines the machine-readable boundary for a future real x402
facilitator transport behind `src/x402_facilitator.rs`.

It follows the official x402 split between payment verification and payment
settlement:

- `verify` checks a `PaymentPayload` against selected `PaymentRequirements`
- `settle` is allowed only after successful verification
- both operations require an idempotency key
- verify outcomes are explicitly `verified`, `rejected`, or `unavailable`
- facilitator rejection and unavailability fail closed without settlement
- settlement uses explicit `verified_pending_settlement`,
  `settlement_in_progress`, `settled`, `settlement_rejected`, and
  `settlement_outcome_unknown` states
- payment settlement may return a payment transaction hash
- neither operation authorizes the underlying Stellar ActionPlan

The payload and requirements objects are intentionally opaque. A future
transport adapter must map the pinned x402 protocol version into these objects
without changing NeuroChain guardrail, ZK, approval, or submit semantics.
The Rust mapping functions in `src/x402_facilitator.rs` round-trip the verify
and settle fixtures through an offline transport and reject operation, version,
idempotency, or network mismatches before transport execution.

`state_transitions.json` locks the idempotency and replay behavior expected from
that transport:

- only `verified_pending_settlement` may proceed to one settlement attempt
- the exact verified request is bound by a canonical SHA-256 digest
- repeated requests cannot dispatch a second settlement while one is in flight
- timeout, post-dispatch unavailability, or restart becomes
  `settlement_outcome_unknown` and requires external reconciliation
- a repeated settled idempotency key is replay-blocked
- rejected, unavailable, and expired requests remain fail-closed
- unknown state fails closed as unavailable
- no transition grants underlying ActionPlan submit authority

`supported_stellar_exact_v2.json` is an offline fixture for the official x402
v2 `/supported` shape: camel-case `x402Version`, `kinds`, `signers`, and
`extensions`. A facilitator config must match x402 v2, exact/exact-v2, and the
configured Stellar network before verify or settle can be enabled. The
configured SEP-41 asset remains locally validated because `/supported` does not
advertise per-asset allowlists.

The Rust offline orchestrator requires this handshake before verify and
preserves unavailable and timeout errors as fail-closed outcomes. No verify
call occurs when capability discovery or validation fails.

`X402FacilitatorVerifyOnlyAdapter` is the runtime verify-only adapter.
It accepts only a `verify` envelope, performs the offline-tested capability
handshake, and maps accepted, rejected, timeout, unavailable, invalid-response,
and capability failures into this no-submit contract. It has no settle method.
The server wrapper selects it in facilitator mode, performs read-only challenge
inspection, and persists accepted verification at
`verified_pending_settlement`. An identical retry reuses that state without a
second facilitator verify call; a mismatched request fails closed.
It does not finalize challenge state, run guardrails, or authorize the
underlying ActionPlan.

Offline settlement requires an accepted verify result plus an exact match of
network, payment payload, payment requirements, and idempotency key. Rejected
or mismatched requests do not reach the settlement transport.

The Rust module now contains authenticated HTTPS `GET /supported`, read-only
`POST /verify`, and state-gated `POST /settle` transports. The credential is
injected at runtime, marked sensitive, and never stored in this fixture package.
Both POST requests use the official x402 v2 wire names and validate the exact
selected requirements before request construction. The adapter fixtures remain
protocol-neutral; a real HTTP request must carry a complete official
`PaymentPayload` and `PaymentRequirements`, not fixture references.

The HTTP mapping and parser are tested offline. One separately approved live
testnet `/verify` rejection probe confirmed the unsigned malformed request
path; it did not validate or settle a payment. The server now emits official
Base64 x402 v2 `PAYMENT-REQUIRED` data and accepts bounded v2
`PAYMENT-SIGNATURE` data in facilitator mode. The server remains verify-only and
does not expose the settlement transport. This package provides no signing
operation, automatic payment submission, or underlying ActionPlan submission.

Sources:

- <https://developers.stellar.org/docs/build/agentic-payments/x402/built-on-stellar>
- <https://github.com/OpenZeppelin/relayer-plugin-x402-facilitator>
- <https://docs.x402.org/core-concepts/facilitator>
- <https://github.com/x402-foundation/x402>
