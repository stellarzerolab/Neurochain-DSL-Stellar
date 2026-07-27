# x402 Facilitator Adapter Contract

This package defines the machine-readable boundary for a future real x402
facilitator transport behind `src/x402_facilitator.rs`.

It follows the official x402 split between payment verification and payment
settlement:

- `verify` checks a `PaymentPayload` against selected `PaymentRequirements`
- `settle` is allowed only after successful verification
- both operations require an idempotency key
- payment settlement may return a payment transaction hash
- neither operation authorizes the underlying Stellar ActionPlan

The payload and requirements objects are intentionally opaque. A future
transport adapter must map the pinned x402 protocol version into these objects
without changing NeuroChain guardrail, ZK, approval, or submit semantics.

This package does not provide an HTTP endpoint, facilitator URL, credentials,
network call, signing operation, or transaction submission implementation.

Sources:

- <https://docs.x402.org/core-concepts/facilitator>
- <https://github.com/x402-foundation/x402>
