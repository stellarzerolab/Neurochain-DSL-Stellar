# NeuroChain DSL for Stellar

NeuroChain DSL for Stellar is a Rust-based developer tool for building safer AI-assisted Stellar workflows.

The core idea is simple: natural-language intent is never turned directly into a transaction. NeuroChain classifies intent with local ONNX models, maps it into deterministic typed action templates, then applies guardrails before anything can be simulated or submitted.

This repository contains the Stellar integration layer for NeuroChain DSL.

## Start Here: Offline Product Path

Run the whole local product path before choosing an integration surface:

```powershell
cargo run --offline --quiet --example product_local_quickstart
```

The checked-in scenarios run one coordinator through:

```text
Bazaar discovery -> x402 access state -> typed ActionPlan (Plan)
-> deterministic policy (Evaluate) -> optional ZK proof artifact
-> local binding Verify -> separate exact capability gate
```

The machine-readable report includes `approved`, `requires_approval`, and
`blocked`. Only `approved` reaches the exact single-use service-call capability
gate, and even then service dispatch remains false. The quickstart needs no
credential, keypair, listener, or network call and grants no payment, proof,
approval, settlement, signing, execution, wallet, shell, RPC, transaction
submit, or ActionPlan-submit authority.

The bundled Groth16 artifacts are real evidence fixtures, but this local run
checks their public binding only. It reports `cryptographicallyVerified=false`
and `stellarVerificationRequired=true`; it does not claim a live Stellar
verification.

After this first run, choose the surface that matches the caller:

| Caller | Surface | Default role |
| --- | --- | --- |
| Script or CI | `neurochain-stellar` one-shot CLI | plan-only machine JSON unless `--flow` is explicit |
| Human | `neurochain-stellar --no-flow` REPL | learning and diagnostics |
| AI agent | `neurochain-mcp-v0-stdio` | read-only/no-submit MCP integration |
| Backend | `POST /api/stellar/intent-plan` | typed service integration |
| Deterministic program | `.nc` | advanced scripting |

See [`docs/product_local_quickstart.md`](docs/product_local_quickstart.md) for
the exact evidence boundary and
[`docs/product_surface_inventory.md`](docs/product_surface_inventory.md) for
the Core/Advanced/Internal classification. The remaining sections are product
architecture, advanced operation, or reproducible evidence—not competing first
steps.

## Advanced Evidence: ZK

The repository now includes **NeuroChain ZK Guardrail Attestation**, a complete
RISC Zero and Soroban proof path for private owner policies:

- a known RISC Zero guest runs the deterministic NeuroChain guardrail evaluator
- the private policy can enforce contract, function, asset, recipient, amount,
  confidence, and approval-threshold rules
- the public journal binds the evaluator image ID, ActionPlan hash, policy
  commitment/version, decision, exit/reason, and audit nullifier
- the Soroban application contract verifies a genuine Groth16 receipt through
  the pinned verifier router and accepts only owner-authorized policy
  commitment/version pairs
- repeatable read-only verification is separate from the owner-authenticated
  nullifier consume that prevents replay
- genuine proofs cover `approved`, `requires_approval`, and private-policy
  allowlist block with exit `3`
- a standalone Protocol 26 localnet demonstrates verification, persistent
  replay rejection, and invalid-proof rejection

A valid proof is not submit permission. `requires_approval` remains a
no-submit state, and the read-only API view always reports
`submit_allowed=false`.

The Stellar REPL exposes the local binding boundary through
`zk.demo approved`, `zk.demo requires_approval`, `zk.demo blocked`, and
`zk status`. Local command-line sessions can inspect caller-selected JSON with
`zk.verify`; the public WebSocket REPL disables arbitrary file access and keeps
only the bundled demo scenarios available. Once a deployed contract ID is
configured, `zk.stellar.verify <scenario>` performs repeatable cryptographic
Soroban verification with no state change. The separate local-only
`zk.stellar.consume <scenario>` requires flow, confirmation and owner auth,
stores the replay nullifier, and still never submits the underlying ActionPlan.

The repository includes a testnet-only deployment script, but does not claim a
testnet deployment until a successful authorized run creates
`hackathons/stellar-real-world-zk/deployments/testnet.json`.

Start with the public package:

- [`hackathons/stellar-real-world-zk/README.md`](hackathons/stellar-real-world-zk/README.md)
- [`hackathons/stellar-real-world-zk/SUBMISSION.md`](hackathons/stellar-real-world-zk/SUBMISSION.md)
- [`hackathons/stellar-real-world-zk/ARCHITECTURE.md`](hackathons/stellar-real-world-zk/ARCHITECTURE.md)

Fresh clone build note: the repository root is the main NeuroChain CLI crate.
To reproduce the ZK/Soroban evidence directly without pulling the whole CLI
build path first, run the ZK package commands with `--manifest-path` instead of
starting with a root `cargo build`.

Run the repository evidence gate:

```powershell
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/check_submission_package.ps1 -RunTests
```

Run the genuine Groth16/Soroban regression matrix directly:

```powershell
cargo test --release --manifest-path hackathons/stellar-real-world-zk/soroban/Cargo.toml
```

Run the complete local recording rehearsal without verifier fetches, Cargo
downloads, or Docker image pulls after prerequisites are cached:

```powershell
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/run_demo_rehearsal.ps1 -IncludeLocalnet -OfflineLocalnet
```

## Product Scope

The current direction is one narrow guardrail-and-capability product exposed
through role-specific surfaces. MCP and Skills serve AI agents, the CLI and
REPL serve local users, `.nc` serves deterministic scripts, and the API serves
backend integrations. They share one product shape:

```text
Plan -> Evaluate -> optional Prove -> Verify -> separate capability gate
```

Raven and Stellar Skills are useful development-time guidance and packaging
models, but NeuroChain must not depend on them at runtime.

See [`docs/product_direction_mcp_skills.md`](docs/product_direction_mcp_skills.md).
The current last-mile packaging phase is
[`docs/mcp_v0_product_finish.md`](docs/mcp_v0_product_finish.md).
The completion audit for the MCP/Skill last-mile objective is
[`docs/mcp_skill_completion_audit.md`](docs/mcp_skill_completion_audit.md).
For the public walkthrough, start with
[`docs/public_demo_flow.md`](docs/public_demo_flow.md).

## Advanced Evidence: MCP And Skill Release Status

The MCP v0 path is a runtime-backed read-only guardrail surface:

```text
Plan -> Evaluate -> Prove -> Verify -> Status -> no automatic submit
```

The default MCP tools are `plan_stellar_action`, `evaluate_guardrails`,
`prove_guardrail_decision`, `verify_zk_on_stellar`, and
`get_guardrail_status`. They do not sign, broadcast, submit an attestation,
consume a nullifier, or submit the underlying ActionPlan.

The `neurochain-stellar-guardrails` skill is an internal release candidate,
not a published package. It is an instruction and distribution layer for MCP
hosts, not a NeuroChain runtime dependency or submit surface.

Run the combined release candidate gate:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify_guardrails_skill_release_candidate.ps1
```

The gate must report `status=passed`, `release_candidate=true`,
`published=false`, `runtime_dependency=false`, `submit_surface=false`,
MCP `mode=read_only_no_submit`, and `validated_by_launch=true`.

Current boundary status:

- ZK is beyond a lite demo: the core includes a real RISC Zero guest, genuine
  Groth16 fixture proofs, Soroban verifier/router integration, tamper
  rejection, replay rejection, and hosted CLI proof evidence.
- x402 is beyond a lite UI idea: the paid ingress envelope, response contract,
  schema/types, viewer, audit/replay boundaries, production mock fence, and
  fail-closed facilitator boundary exist. Facilitator mode emits an official
  x402 v2 `PAYMENT-REQUIRED` challenge and can run authenticated
  `supported -> verify` without settling or executing an ActionPlan. The same
  authenticated transport now implements the official x402 v2 `/settle` wire
  path behind a persistent single-attempt state machine, with request and
  response behavior validated offline.
- x402 is not production until settlement is explicitly runtime-gated, tested
  with a valid signed testnet payment, and reviewed with production pricing and
  receiver configuration. No live settlement is enabled by default.

## Product Architecture

Every surface uses the same product stages:

```text
Intent -> typed ActionPlan -> deterministic policy -> optional ZK proof
-> verified decision -> separate exact capability gate
```

Payment, proof, approval, settlement, a service-call capability, signing,
execution, and submission are separate authorities. The advanced CLI/REPL/`.nc`
flow can simulate, preview, confirm, and submit only when flow mode is explicit;
it is not the default product quickstart.

Supported Stellar actions include:

- testnet funding via Friendbot
- account balance queries
- account creation
- trustline creation
- XLM and issued-asset payments
- transaction status checks
- Soroban contract deploy plans
- Soroban contract invokes
- x402-lite payment-required challenge/finalize flows

## Safety Model

NeuroChain is intentionally conservative.

- File and `--intent-text` runs are plan-only unless `--flow` is passed.
- REPL starts with flow enabled by default, but still shows preview and asks for confirmation before submit.
- Use `--no-flow` for plan-only REPL sessions.
- Use `--yes` only for controlled testnet automation; it skips the final prompt.
- Secret keys should not be written into files or docs. Use Stellar CLI key aliases such as `wallet: nc-testnet`.

Hard block exit codes are stable:

| Exit code | Meaning |
|---|---|
| `3` | allowlist block |
| `4` | contract policy block |
| `5` | intent safety block, low confidence, slot missing, or slot type error |

Typed Soroban policy mismatches such as `address`, `bytes`, `symbol`, and `u64` errors are downgraded into safe no-submit blocks.

### The 3 / 4 / 5 Guardrail Contract

The most important runtime promise is that unsafe execution stops before submit and reports a stable block class:

- **3 = allowlist protection**
  - Blocks assets, contracts, or functions outside the active session allowlist when `allowlist_enforce` is enabled.
  - Example: if only `XLM` is allowed, an issued-asset trustline/payment is blocked before submit.
- **4 = contract policy protection**
  - Blocks contract calls that violate a configured policy when `contract_policy_enforce` is enabled.
  - Example: wrong function or missing required Soroban invoke args stops before chain execution.
- **5 = intent safety protection**
  - Blocks unknown, low-confidence, slot-missing, or slot-type-error intent plans.
  - Example: a vague or invalid natural-language prompt becomes a safe no-submit result instead of a guessed transaction.

These same codes are used across CLI, REPL, `.nc` scripts, and `/api/stellar/intent-plan`, so demo behavior and automated tests speak the same language.

## Repository Binaries

Core product binaries:

- `neurochain-stellar` - Stellar one-shot CLI, REPL and `.nc` runner; use
  `--no-flow` for the recommended plan-only human path
- `neurochain-mcp-v0-stdio` - default agent-facing read-only/no-submit MCP
  runtime

Advanced integration binaries:

- `neurochain-server` - long-lived REST API host, including
  `POST /api/stellar/intent-plan`
- `neurochain` - base non-Stellar NeuroChain DSL interpreter

Internal development and conformance binaries:

- `neurochain-stellar-demo-server`
- `neurochain-agent-repl`
- `eval-intent-stellar`
- `neurochain-mcp-v0-client-smoke`
- `neurochain-mcp-v0-fixture-runner`
- `txrep-to-action`
- `txrep-to-jsonl`

The complete classification and drift-checked source inventory is in
[`docs/product_surface_inventory.md`](docs/product_surface_inventory.md).

## Prerequisites

Install:

- Rust + Cargo via `rustup`
- Stellar CLI (`stellar`) for Stellar Classic and Soroban operations
- `cosign` for model pack verification, if using the fetch scripts
- platform build tools:
  - Windows: Visual Studio Build Tools / Community with Desktop development with C++
  - Linux/WSL: `build-essential` + `pkg-config`
  - macOS: Xcode Command Line Tools

The default network for examples and docs is `testnet`.

## Model Pack

Binary ONNX model files are distributed separately through GitHub Releases. The repo tracks metadata and README files under `models/`, but not the large model binaries.

Clone the repository first:

```bash
git clone https://github.com/stellarzerolab/Neurochain-DSL-Stellar.git
cd Neurochain-DSL-Stellar
```

Current model pack metadata is in:

- `models/manifest.json`
- `models/README.md`

Download and verify models:

```bash
bash scripts/fetch_models.sh
```

Windows PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/fetch_models.ps1
```

The Stellar model pack should provide these paths after extraction:

```text
models/distilbert-sst2/model.onnx
models/toxic_quantized/model.onnx
models/factcheck/model.onnx
models/intent/model.onnx
models/intent_macro/model.onnx
models/intent_stellar/model.onnx
```

See `docs/models.md` for release and verification details.

## Advanced: Hosted Browser Demo

The browser-based CLI demo uses a server-side `neurochain-stellar` REPL. You do not need local binary commands in that mode; type REPL commands into the demo input.

Run this first:

```text
help
help all
show setup
setup testnet
wallet_bootstrap: demo-boot
show setup
```

What this proves:

- `help` and `help all` show the available command surface.
- `setup testnet` applies the testnet Horizon/Friendbot baseline.
- `wallet_bootstrap` creates a wallet alias and funds it on testnet.
- `show setup` confirms the active network, wallet/source, flow mode, allowlist, policy mode, and x402 mode.

ZK demo commands, after the hosted service has configured a deployed
`NC_ZK_GUARDRAIL_CONTRACT` and simulation source:

```text
zk.demo approved
zk.stellar.verify approved
zk.stellar.verify requires_approval
zk.stellar.verify blocked
```

`zk.stellar.verify` invokes Soroban with `--send no`. The hosted REPL disables
`zk.stellar.consume`, so public users cannot change replay state.

Demo operating model:

- REPL commands become typed `ActionPlan` objects.
- Flow is `simulate -> preview -> confirm -> submit`.
- Startup `asset_allowlist` defaults to `XLM`.
- `asset_allowlist` is a session safety filter; it does not create or mint tokens.
- Keep `XLM` in the allowlist unless you intentionally want to block XLM operations.
- In hosted demo sessions, idle timeout can clear session-local key material; re-run setup if the session expires.

## Core Usage: One-Shot Plan-Only ActionPlan

Plan-only mode is the safest first run. It builds JSON but does not simulate or submit.

```bash
cargo run --release --bin neurochain-stellar -- examples/intent_stellar_smoke.nc
```

Direct natural-language intent:

```bash
cargo run --release --bin neurochain-stellar -- --intent-text "Transfer 5 XLM to G..."
```

With intent debugging:

```bash
cargo run --release --bin neurochain-stellar -- --intent-text "Transfer 5 XLM to G..." --debug
```

## Core Usage: Plan-Only Interactive REPL

Start the canonical plan-only REPL:

```bash
cargo run --release --bin neurochain-stellar -- --no-flow
```

The zero-argument compatibility REPL remains available, but it starts with flow
enabled and is therefore an advanced operator path:

```bash
cargo run --release --bin neurochain-stellar
```

Useful REPL setup commands:

```text
network: testnet
wallet: nc-testnet
AI: "models/intent_stellar/model.onnx"
asset_allowlist: XLM
allowlist_enforce
contract_policy_enforce
help
help all
```

## Advanced: Testnet Flow With Confirmation

`--flow` enables simulate/preview/confirm/submit for file and intent runs.

```bash
cargo run --release --bin neurochain-stellar -- examples/stellar_actions_example.nc --flow
```

Only use `--yes` when you intentionally want to skip the final prompt, usually in tests or controlled testnet demos.

```bash
cargo run --release --bin neurochain-stellar -- examples/stellar_actions_example.nc --flow --yes
```

## Advanced: Contract Invoke Example

Plan-only:

```bash
cargo run --release --bin neurochain-stellar -- examples/soroban_hello_invoke.nc
```

Flow mode:

```bash
cargo run --release --bin neurochain-stellar -- examples/soroban_hello_invoke.nc --flow
```

For policy-controlled Soroban invokes, see:

- `contracts/hello/policy.json`
- `contracts/CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ/policy.json`
- `docs/stellar_actions_guide.md`

## Advanced: x402 Paid Ingress

This repo includes controlled x402 payment-required flows in REPL and `.nc`
scripts, plus a server-side paid ingress envelope for Stellar intent planning.

To inspect only the lower-level offline Bazaar discovery -> typed ActionPlan ->
policy -> requires_approval/approved/blocked -> capability gate boundary, with
no dispatch, run:

```powershell
cargo run --offline --quiet --example x402_local_reference_path
```

REPL sketch:

```text
x402
x402.request to="G..." amount="1" asset_code="XLM"
x402.finalize challenge_id="last"
x402.finalize challenge_id="last"
```

The second finalize for the same challenge is blocked as replay. This gives
AI-assisted payment flows an explicit challenge/finalize boundary instead of
allowing repeated blind submits.

x402 is paid service access only. Payment is not guardrail approval, proof
verification, attestation, nullifier consume, or submit permission. The current
product boundary is documented in
[`docs/x402_facilitator_phase3.md`](docs/x402_facilitator_phase3.md).

## Advanced: Server API

Start the API server:

```bash
PORT=8081 NC_MODELS_DIR=models cargo run --release --bin neurochain-server
```

Optional API key:

```bash
NC_API_KEY="your-secret-key" PORT=8081 NC_MODELS_DIR=models cargo run --release --bin neurochain-server
```

Stellar endpoint:

```http
POST /api/stellar/intent-plan
```

The response includes:

- `plan`
- `blocked`
- `exit_code`
- `logs`

The endpoint uses the same intent core and guardrail behavior as CLI, REPL, and `.nc` scripts.

## Documentation

Start here:

- `docs/product_local_quickstart.md` - one-command whole-product offline path and exact verification boundary
- `docs/product_surface_inventory.md` - canonical Core / Advanced / Internal product surface map and manual review questions
- `docs/stellar_actions_guide.md` - full Stellar CLI, REPL, `.nc`, flow, guardrail, and API reference
- `docs/getting_started.md` - base NeuroChain quickstart
- `docs/language.md` - `.nc` language guide
- `docs/mcp_v0_tool_contract.md` - no-submit MCP v0 tool contract
- `docs/models.md` - model pack download, verification, and release notes
- `docs/product_direction_mcp_skills.md` - MCP and Skills product direction
- `docs/public_demo_flow.md` - simplified Plan -> Evaluate -> Prove -> Verify public demo path
- `docs/x402_local_reference_quickstart.md` - one-command offline Bazaar/x402 -> ActionPlan -> policy -> capability quickstart
- `docs/security.md` - security, CI, audit, and runtime safety notes
- `docs/troubleshooting.md` - common local development issues
- `examples/mcp_v0_no_submit_contract/` - machine-checkable MCP v0 no-submit fixtures
- `examples/mcp_v0_stdio_client/` - stdio host config and process-level no-submit smoke client
- `examples/x402_local_reference_path/` - versioned approved/requires_approval/blocked fixtures for the local non-bypass reference path
- `skills/neurochain-stellar-guardrails/SKILL.md` - no-submit Stellar guardrail skill draft

## Development Checks

Recommended before pushing:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo audit --deny warnings --ignore RUSTSEC-2024-0436 \
            --ignore RUSTSEC-2025-0134 \
            --ignore RUSTSEC-2026-0186
```

Focused Stellar guardrail/parity tests:

```bash
cargo test --test flow_cli --test stellar_repl --test stellar_script --test server_analyze
```

## Project Positioning

This is not a generic autonomous trading agent and it does not rely on free-form transaction generation.

NeuroChain DSL is a deterministic execution layer for AI-assisted Stellar workflows:

- lightweight local ONNX models classify user intent
- typed templates construct the only allowed action shapes
- guardrails decide whether execution is allowed
- submit is explicit and observable
- unsafe or low-confidence plans stop before submit

## License

Apache-2.0. See `LICENSE`.

Redistributions must retain `LICENSE` and `NOTICE`.

Model files may have additional third-party license or attribution requirements. See `models/LICENSE` and `models/THIRD_PARTY_NOTICES.md`.

## Branding And Trademarks

The Apache-2.0 license does not grant rights to use the NeuroChain DSL or StellarZeroLab names, logos, or branding to imply endorsement or official affiliation.

If you fork this project, use your own name and branding for your fork or release.

Copyright 2026 StellarZeroLab.
