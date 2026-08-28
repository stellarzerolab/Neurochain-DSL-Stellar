# NeuroChain product surface inventory

This document is the first product-convergence checkpoint. It classifies the
existing product surfaces without deleting, renaming or changing any command,
route, guardrail, flow or exit-code behavior.

The machine-readable source is
[`examples/product_surface_inventory/v1.json`](../examples/product_surface_inventory/v1.json).
`tests/product_surface_inventory.rs` prevents the binary, API route, default MCP
tool, evidence-file and REPL help inventories from drifting silently.

## One product contract

All surfaces are different entry points to the same product story:

```text
intent
  -> typed ActionPlan
  -> deterministic policy
  -> optional ZK proof
  -> verified decision
  -> separate exact capability gate
```

x402/Bazaar owns discovery, payment state and access. NeuroChain owns the typed
ActionPlan, deterministic policy decision, ZK evidence and capability gate.
Payment, proof and approval do not grant signing, settlement, service dispatch,
underlying execution, RPC submit, transaction submit or ActionPlan submit.

## Classification rule

| Class | Meaning now | Product action |
| --- | --- | --- |
| `Core` | The shortest supported way to understand or integrate the product safely. | Keep prominent and converge terminology. |
| `Advanced` | Valid operator, scripting, server, flow or raw proof functionality. | Keep available behind explicit advanced documentation. |
| `Internal` | Conformance, fixture, hosted-demo or data-conversion tooling. | Keep out of the first-run mental model. |
| `Deprecated candidate` | A compatibility surface that duplicates or conflicts with the canonical story. | Keep unchanged until the manual review explicitly decides its future. |

`Deprecated candidate` is not a deprecation notice. It records a question for
the manual acceptance pass.

## Recommended entry points

| User | First surface | Why |
| --- | --- | --- |
| Agent or automation host | `neurochain-mcp-v0-stdio` and its five MCP tools | The smallest runtime-backed Plan -> Evaluate -> Prove -> Verify -> Status no-submit contract. |
| Integration developer | `cargo run --offline --quiet --example x402_local_reference_path` | One credential-free and network-free path through Bazaar/x402 access, policy and the separate capability gate. |
| Human learning locally | `neurochain-stellar --no-flow` | Interactive inspection without preview or submit effects. |
| Backend integrator | `POST /api/stellar/intent-plan` and `POST /api/stellar/zk-attestation/view` | The canonical plan/policy and read-only proof-binding contracts. Starting the listener remains an explicit operator action. |

## Surface roles

### CLI, REPL and `.nc`

- The core CLI role is one-shot typed ActionPlan creation from `--intent-text`
  or a checked-in input file without `--flow`.
- The core human REPL recommendation is explicit `--no-flow`.
- The current zero-argument REPL compatibility default enables flow. It is
  therefore classified as advanced, not used as the default product quickstart.
- `.nc` remains the advanced deterministic scripting surface.
- `--flow` and especially `--yes` remain advanced execution controls and are
  outside the default Plan -> Evaluate -> Prove -> Verify walkthrough.
- Every accepted `neurochain-stellar` CLI flag is listed in the versioned
  manifest and compared exactly with `parse_cli_args` by the inventory test.
- Wallet, Friendbot, network, raw Stellar CLI, allowlist/policy configuration,
  manual Stellar/Soroban actions and state-changing ZK operations remain
  advanced operator commands.
- `neurochain-agent-repl` and `eval-intent-stellar` are internal development
  utilities. They are compiled binaries, but neither belongs in the supported
  first-run product surface.

### MCP

The five default MCP tools are the core agent-facing vocabulary:

1. `plan_stellar_action`
2. `evaluate_guardrails`
3. `prove_guardrail_decision`
4. `verify_zk_on_stellar`
5. `get_guardrail_status`

The MCP v0 contract remains read-only and no-submit. Attestation submission,
nullifier consume, wallet signing and underlying ActionPlan submit are not part
of the default tool list.

### API

`/api/stellar/intent-plan` and `/api/stellar/zk-attestation/view` are the core
backend contracts. `/api/x402/stellar/intent-plan` is advanced paid ingress:
production settlement and service dispatch are not enabled by this
classification. `/api/analyze` is the advanced base-DSL endpoint. The demo
WebSocket route is internal hosted-demo transport.

### x402/Bazaar

The local reference quickstart is the core onboarding evidence. The facilitator
adapter, service boundary, response contract and conformance schemas remain
advanced or internal implementation contracts. They do not establish live
canonical-client E2E, production settlement or service dispatch.

The REPL commands `x402`, `x402.request` and `x402.finalize` are retained as a
deprecated candidate because the older x402-lite teaching flow overlaps with
the newer separate access-layer reference path. No behavior changes in this
checkpoint.

### ZK

The core product vocabulary is optional proof inspection and read-only Stellar
verification through MCP or the guided REPL scenarios. The raw RISC Zero,
Groth16, Soroban, localnet and testnet evidence package remains an advanced
reproduction surface. `zk.stellar.attest` and `zk.stellar.consume` remain
separately gated advanced operations and never grant underlying ActionPlan
submit authority.

## Manual acceptance questions

After the automated convergence pass is complete, verify these with the user
before changing compatibility behavior:

1. Should zero-argument `neurochain-stellar` remain flow-enabled, or should the
   first-run default become plan-only?
2. Should the short `help` output continue to show wallet generation,
   Friendbot bootstrap and testnet attestation, or move them only to
   `help all`?
3. Should the x402-lite REPL commands remain as advanced compatibility aliases,
   be redirected to the canonical access path, or be deprecated later?
4. Should the root README lead with MCP, the local x402 reference path or the
   human plan-only CLI after real first-run testing?

These are product choices, not defects. They stay unchanged until the manual
acceptance pass supplies evidence and explicit direction.
