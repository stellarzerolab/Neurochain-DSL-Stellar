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
- a successful settle outcome is explicitly `settled`
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

- only `verified` may proceed to one settlement attempt
- a repeated settled idempotency key is replay-blocked
- rejected, unavailable, and expired requests remain fail-closed
- unknown state fails closed as unavailable
- no transition grants underlying ActionPlan submit authority

`supported_stellar_exact_v2.json` is an offline `/supported` capability
fixture. A facilitator config must match x402 v2, exact/exact-v2, the configured
Stellar network, and the configured SEP-41 asset before verify or settle can be
enabled.

The Rust offline orchestrator requires this handshake before verify and
preserves unavailable and timeout errors as fail-closed outcomes. No verify
call occurs when capability discovery or validation fails.

This package does not provide an HTTP endpoint, facilitator URL, credentials,
network call, signing operation, or transaction submission implementation.

Sources:

- <https://docs.x402.org/core-concepts/facilitator>
- <https://github.com/x402-foundation/x402>
