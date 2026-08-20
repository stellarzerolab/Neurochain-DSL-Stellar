# x402 service boundary fixtures

These fixtures define the offline v1 data contract between a future
TypeScript `@x402/stellar` facilitator/Bazaar service and the Rust NeuroChain
guardrail runtime.

- `evaluation_request.json` contains only a resource request and intent. It
  deliberately excludes `PaymentPayload`, signatures, auth entries,
  transaction envelopes, wallet material, policy overrides, and model paths.
- the three response fixtures lock `approved`, `requires_approval`, and
  `blocked` decision semantics to the existing typed ActionPlan hash.
- every response explicitly grants no payment, settlement, override, signing,
  or Stellar submission authority.

The package is offline contract evidence. It does not expose an HTTP route,
install `@x402/stellar`, run settlement, or enable pubnet.

See `docs/x402_service_boundary.md` for ownership and trust boundaries.
