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

## Canonical vocabulary

The versioned inventory is the machine-readable vocabulary source. The same
terms mean the same thing on every surface:

| Stage | Meaning | Authority boundary |
| --- | --- | --- |
| `Plan` | Convert an intent or deterministic input into a typed ActionPlan preview. | No payment, proof, approval, capability, execution or submit authority. |
| `Evaluate` | Apply deterministic policy and produce a decision. | A policy result is information, not an effect grant. |
| `Prove` | Optionally bind the ActionPlan, policy and decision to a public ZK artifact. | Proof is evidence, not approval or execution permission. |
| `Verify` | Optionally verify the proof and its bindings. | Verification does not issue a capability or submit the ActionPlan. |
| `Capability gate` | Separately evaluate an exact host-controlled capability after all required prior state. | The local reference may issue one exact service-call capability; it still exposes no dispatch, signer, wallet, RPC or submit route. |

The canonical policy decisions are:

| Decision | Meaning | Current boundary |
| --- | --- | --- |
| `not_evaluated` | A plan or access envelope exists, but deterministic policy has not run. | Terminal no-submit. |
| `approved` | Deterministic policy passed. | Not proof, human approval, capability, execution, signing or submit permission. |
| `requires_approval` | Policy passed, but a separate owner or human approval is required. | Terminal no-submit in the current core surfaces. |
| `blocked` | Policy or safety validation failed closed. | Terminal no-submit; exit `3`, `4` or `5` identifies the guardrail class where the surface exposes an exit code. |

Transport `status`, payment `state`, proof verification state and capability
`outcome` are separate fields. None should be interpreted as a policy decision
or execution authority merely because it says `ok`, `finalized`, `verified` or
`ready`.

## Cross-surface translation

| Surface | Product role | Canonical fields or representation |
| --- | --- | --- |
| CLI | One-shot and machine-readable planning. | Typed ActionPlan plus stable process exit `3` / `4` / `5`; plan-only without `--flow`. |
| REPL | Human learning and diagnostics. | The same ActionPlan and guardrail meanings; use `--no-flow` for the canonical plan-only path. |
| `.nc` | Advanced deterministic scripting. | The same plan, policy and exit semantics as CLI/REPL/API, with unsafe build-time effects separately gated. |
| MCP | Agent integration. | `decision`, `exit_code`, proof/verification fields and `underlying_action_submit_allowed=false`. |
| API | Backend integration. | `/stellar/intent-plan` exposes `ok`, `blocked`, `requires_approval`, `exit_code` and `plan`; the x402 envelope exposes `decision.status` separately from `payment.state`. |
| x402/Bazaar | Discovery, payment-state and access. | `decision` stays separate from `capability.outcome`; only the exact gate can produce `serviceCallAllowed=true`, and the reference never dispatches. |
| ZK | Optional evidence and proof verification. | `attested_decision.status` plus binding/verification state; `submit_allowed` remains false. |

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
- REPL `help` now labels itself a compatibility reference because it still
  includes advanced operator actions. `help all` calls wallet, network,
  Friendbot, policy and model configuration `Advanced operator setup`, matching
  this inventory. Command availability and behavior remain unchanged.
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

The whole-product local quickstart is the primary core onboarding evidence. It
connects the existing Bazaar/x402 reference coordinator, typed ActionPlan,
deterministic policy, bundled ZK evidence and separate exact capability gate in
one offline command. Its Verify step is local public binding validation, not
cryptographic Stellar verification. The lower-level x402 local quickstart
remains core integration evidence. The facilitator adapter, service boundary,
response contract and conformance schemas remain advanced or internal
implementation contracts. Neither quickstart establishes live canonical-client
E2E, production settlement or service dispatch.

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
