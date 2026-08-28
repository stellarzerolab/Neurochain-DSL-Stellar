# NeuroChain product local quickstart

This is the shortest fully offline path through the existing product layers:

```text
Bazaar discovery -> x402 access state -> typed ActionPlan -> deterministic policy
-> optional ZK proof artifact -> local binding verification -> exact capability gate
```

Run one command from the repository root:

```powershell
cargo run --offline --quiet --example product_local_quickstart
```

The command reads only checked-in fixtures. It needs no credential, keypair,
network connection, listener or persistent store.

## What each layer owns

| Layer | Role in this quickstart |
| --- | --- |
| Bazaar | Finds the exact local MCP resource and its x402 payment metadata. |
| x402 access state | Supplies a read-only `settled_access_ready` fixture state; it does not settle a payment. |
| NeuroChain ActionPlan | Carries the typed `soroban_contract_invoke` action produced by the planning boundary. |
| Deterministic policy | Returns exactly one of `approved`, `requires_approval` or `blocked`. |
| Optional ZK evidence | Reuses an existing Groth16 artifact and public journal for the same typed action and decision. |
| Local Verify step | Validates journal integrity, ActionPlan projection and decision/exit parity before the capability gate. |
| Capability gate | May consume one exact service-call capability only for `approved`; it never dispatches the call. |

The runtime ActionPlan hash and the ZK typed-plan hash use different existing
canonicalization domains. The adapter therefore validates the contract,
function and every typed argument exactly instead of pretending the two hashes
are interchangeable. The versioned expected hashes are:

- runtime ActionPlan: `86911159cf10da6bb306e7edb0db91ec09bcec401413d1f5a25cbcb9b5faddfb`
- ZK typed ActionPlan: `a008efa4f3ecbdf88b9bcc3ed4c7672994136f16074e8fddd6bb8192ea7970cd`

## Outcomes

| Scenario | Policy decision | Capability gate calls | Service-call capability | Dispatch |
| --- | --- | ---: | --- | --- |
| `approved` | `approved` | 1 | exact, single-use capability only | false |
| `requires_approval` | `requires_approval` | 0 | false | false |
| `blocked` | `blocked`, exit 3 `allowlist` | 0 | false | false |

Every report contains an exact all-false authority boundary for payment, proof,
approval, settlement, signing, underlying execution, service dispatch, wallet,
shell, RPC submit, transaction submit and ActionPlan-submit. A proof artifact,
payment state or policy approval does not grant any of those authorities.

## Verification boundary

This command does **not** generate a proof and does **not** cryptographically
verify one on Stellar. It validates the bundled artifact's public binding with
the existing read-only inspector and reports both:

```json
{
  "cryptographicallyVerified": false,
  "stellarVerificationRequired": true,
  "verificationBoundary": "local_binding_only_cryptographic_stellar_verify_not_run"
}
```

The separate Stellar verification path remains an advanced, separately gated
operation. Production payment, settlement, service dispatch and underlying
ActionPlan execution remain out of scope.

For the lower-level x402-only integration, see
`docs/x402_local_reference_quickstart.md`. For all public surfaces and their
Core/Advanced/Internal roles, see `docs/product_surface_inventory.md`.
