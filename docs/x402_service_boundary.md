# x402 Facilitator/Bazaar to NeuroChain boundary

Date: 2026-08-20

Status: offline v1 contract; not a deployed service

## Decision

The RFP-facing service and NeuroChain remain separate modules.

```text
x402 client / agent
        |
        v
TypeScript facilitator + Bazaar + discovery MCP
  - @x402/stellar supported / verify / settle
  - sponsored-fee and operator infrastructure
  - catalog, search, discovery metadata, paid-call orchestration
        |
        | versioned evaluation_request / evaluation_response only
        v
Rust NeuroChain guardrail/ZK runtime
  - intent -> typed ActionPlan
  - deterministic policy evaluation
  - optional proof production and read-only verification
  - approved / requires_approval / blocked
  - no wallet signing or ActionPlan submission
```

The TypeScript service must build on the Apache-2.0 `@x402/stellar` package
for the canonical facilitator protocol. The Rust runtime does not reimplement
that package's verify or settle logic. Conversely, payment verification or
settlement cannot alter a Rust guardrail result, and an `approved` Rust result
cannot authorize payment settlement, wallet signing, or transaction
submission.

## Versioned data contract

The machine-readable v1 contract is in
`examples/x402_service_boundary/schema.json`. Rust representations and
fail-closed validation are in `src/x402_service_boundary.rs`.

The inbound request contains only:

- schema and message type
- correlation `request_id`
- stable Bazaar `resource_id`
- `plan_and_evaluate`
- Stellar CAIP-2 network identifier
- bounded intent text

It deliberately cannot carry a raw `PaymentPayload`, payment signature,
Soroban auth entry, transaction envelope, secret, wallet source, model path,
policy override, caller-created ActionPlan, or submit flag. Unknown top-level
fields fail deserialization.

The outbound response contains the canonical typed ActionPlan, the existing
domain-separated ActionPlan hash, the terminal guardrail decision, exit/reason
semantics, and explicit all-false authority grants. Response validation rejects
a changed ActionPlan hash, an inconsistent decision/exit combination, any
authority escalation, or `underlying_action_submit_allowed=true`.

`stellar:pubnet` is representable for future conformance, but this contract
does not enable a pubnet call. Pubnet operation remains a separate user confirmation boundary.

## Authority ownership

| Concern | TypeScript RFP service | Rust NeuroChain runtime | Cross-boundary grant |
| --- | --- | --- | --- |
| `/supported`, `/verify`, `/settle` | Owns through `@x402/stellar` | Does not implement | None |
| Payment signatures and auth entries | Validates/forwards inside the payment module | Must not receive | None |
| Bazaar catalog/search | Owns runtime and hosting | Exposes versioned offline catalog/search contracts | Discovery data only |
| Paid service call | Owns x402 retry, verify, settle, and dispatch | Exposes an offline exact-call binding and trusted settled-access gate | One named service call only after atomic grant consumption |
| Intent -> typed ActionPlan | Requests | Owns | Result data only |
| Guardrail decision and exit semantics | Must preserve | Owns | Result data only |
| ZK proof and Soroban read-only verification | May display bounded public evidence | Owns | Evidence is not authority |
| Wallet signing and Stellar submission | Outside this v1 boundary | Outside this v1 boundary | Never |

## Lifecycle

1. The payment/Bazaar module handles x402 protocol state independently.
2. It may submit a bounded evaluation request to Rust only through this
   versioned contract.
3. Rust creates and evaluates the typed ActionPlan without payment input.
4. The response is informational and fail-closed. The caller must stop on
   `requires_approval` or `blocked`.
5. The separate offline paid-call contract may authorize one exact cataloged
   MCP service call only after trusted settled access is atomically consumed.
   It cannot infer payment or execution authority from this response.

No endpoint or transport is wired in this milestone. The exact-version
TypeScript workspace and offline package smoke are now approved and present.
A later implementation may place the TypeScript service in front of Rust only
after authentication, listener/runtime, network, and deployment choices
receive explicit approval.

The offline conformance preparation in `docs/x402_stellar_conformance.md`
adds a source-drift and evidence-coverage gate for that future service. It does
not move verify/settle ownership into Rust. The package-bootstrap workspace at
`services/x402-stellar-facilitator/` imports the canonical APIs without a
signer, listener, network call, verification, settlement, or submit.

## Primary sources

- SCF RFP: <https://github.com/stellar/scf-handbook/blob/main/scf-awards/build-award/rfp-track.md#x402-facilitator-with-bazaar-discovery-support>
- Stellar x402 documentation: <https://developers.stellar.org/docs/build/agentic-payments/x402>
- Stellar x402 reference repository: <https://github.com/stellar/x402-stellar>
- x402 specification repository: <https://github.com/x402-foundation/x402>
