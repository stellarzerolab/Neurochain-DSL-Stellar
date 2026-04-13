# NeuroChain DSL for Stellar

NeuroChain DSL for Stellar is a Rust-based developer tool for building safer AI-assisted Stellar workflows.

The core idea is simple: natural-language intent is never turned directly into a transaction. NeuroChain classifies intent with local ONNX models, maps it into deterministic typed action templates, then applies guardrails before anything can be simulated or submitted.

This repository contains the Stellar integration layer for NeuroChain DSL.

## What It Does

`neurochain-stellar` supports one unified workflow across `.nc` scripts, an interactive REPL, and the server API:

```text
natural language / .nc command
  -> local ONNX intent classification
  -> typed ActionPlan JSON
  -> allowlist / contract policy / intent safety checks
  -> simulate
  -> preview
  -> explicit confirmation
  -> submit, only when flow mode is enabled
```

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

Main binaries:

- `neurochain-stellar` - Stellar CLI, REPL, `.nc` runner, ActionPlan builder, flow runner
- `neurochain-server` - REST API server, including `POST /api/stellar/intent-plan`
- `neurochain` - base NeuroChain DSL interpreter

Utility binaries:

- `txrep-to-action`
- `txrep-to-jsonl`
- `neurochain-stellar-demo-server`

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

## Browser Demo Quickstart

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

Demo operating model:

- REPL commands become typed `ActionPlan` objects.
- Flow is `simulate -> preview -> confirm -> submit`.
- Startup `asset_allowlist` defaults to `XLM`.
- `asset_allowlist` is a session safety filter; it does not create or mint tokens.
- Keep `XLM` in the allowlist unless you intentionally want to block XLM operations.
- In hosted demo sessions, idle timeout can clear session-local key material; re-run setup if the session expires.

## Quickstart: Plan-Only ActionPlan

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

## Quickstart: Interactive REPL

Start the Stellar REPL:

```bash
cargo run --release --bin neurochain-stellar
```

Plan-only REPL:

```bash
cargo run --release --bin neurochain-stellar -- --no-flow
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

## Testnet Flow With Confirmation

`--flow` enables simulate/preview/confirm/submit for file and intent runs.

```bash
cargo run --release --bin neurochain-stellar -- examples/stellar_actions_example.nc --flow
```

Only use `--yes` when you intentionally want to skip the final prompt, usually in tests or controlled testnet demos.

```bash
cargo run --release --bin neurochain-stellar -- examples/stellar_actions_example.nc --flow --yes
```

## Contract Invoke Example

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

## x402-lite

This repo includes an x402-lite workflow for controlled payment-required flows in REPL and `.nc` scripts.

REPL sketch:

```text
x402
x402.request to="G..." amount="1" asset_code="XLM"
x402.finalize challenge_id="last"
x402.finalize challenge_id="last"
```

The second finalize for the same challenge is blocked as replay. This gives AI-assisted payment flows an explicit challenge/finalize boundary instead of allowing repeated blind submits.

The current implementation is x402-lite, not a full x402/MPP stack. It is designed as a deterministic guardrail layer around Stellar payment actions.

## Server API

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

- `docs/stellar_actions_guide.md` - full Stellar CLI, REPL, `.nc`, flow, guardrail, and API reference
- `docs/getting_started.md` - base NeuroChain quickstart
- `docs/language.md` - `.nc` language guide
- `docs/models.md` - model pack download, verification, and release notes
- `docs/security.md` - security, CI, audit, and runtime safety notes
- `docs/troubleshooting.md` - common local development issues

## Development Checks

Recommended before pushing:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo audit --deny warnings --ignore RUSTSEC-2024-0436 \
            --ignore RUSTSEC-2025-0134 \
            --ignore RUSTSEC-2026-0097
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
