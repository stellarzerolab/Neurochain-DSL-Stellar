# 🛡️ NeuroChain Rust Security & Tooling Stack

Goal: keep the toolchain lean (native-first) while keeping the process strict. Less plugin sprawl, more repeatable commands and CI gates.

## 1. Development Environment (VS Code)
- Required: `rust-analyzer` (set “Check On Save” = `clippy` if possible).
- Recommended: `Even Better TOML` (for `Cargo.toml` editing).
- Optional: Snyk or another polyglot scanner if your organization already uses it – it does not replace CI-level checks.

## 2. Local Workflow (The Local Loop)
Install audit once:
```bash
cargo install cargo-audit
```
Before committing:
```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo audit --deny warnings --ignore RUSTSEC-2024-0436 \
            --ignore RUSTSEC-2025-0134 \
            --ignore RUSTSEC-2026-0097
```

Note: `cargo test` includes AI model smoke tests (`src/ai/model/tests.rs`). These tests auto-skip if the referenced ONNX files are missing (useful if you clone without `models/`). For end-to-end validation, run the example scripts that load models (see `docs/getting_started.md` and `examples/`).

Runtime safety note (Stellar path): in addition to toolchain checks, `neurochain-stellar` enforces runtime guardrails (allowlist, contract policy, intent safety). Typed policy mismatches for Soroban invoke args (`address` / `bytes` / `symbol` / `u64`) are treated as `slot_type_error -> Unknown -> safe no-submit` in intent mode (blocked flow / API plan execution path).

x402 audit safety note (Stellar server path): `/api/x402/stellar/intent-plan` is an access/payment gate in front of the same guardrail pipeline, not a submit path. If `NC_X402_STELLAR_AUDIT_PATH` is set, the server appends safe JSONL audit rows for payment-required, finalized, blocked, replay, expired, and invalid payment states. Audit rows must not store the raw `PAYMENT-SIGNATURE` header or the mock `paid:<challenge_id>` signature material.

RustSec note: `RUSTSEC-2026-0097` is currently transitive (`rand 0.8.5` via `tokenizers`/`tract`/`axum` stack) and is tracked in the ignore list until upstream-compatible updates are available.

## 3. CI/CD Gatekeepers (GitHub Actions Example)
Keep audit as a separate job; combining fmt+clippy saves time.

```yaml
name: Security & Quality

on: [push, pull_request]

jobs:
  lint-fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: rustup component add clippy rustfmt
      - name: Format
        run: cargo fmt --check
      - name: Clippy
        run: cargo clippy -- -D warnings

  tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo test

  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install cargo-audit
        run: cargo install cargo-audit
      - name: Run Audit
        run: |
          # Known unmaintained warnings via transitive deps.
          cargo audit --deny warnings \
            --ignore RUSTSEC-2024-0436 \
            --ignore RUSTSEC-2025-0134 \
            --ignore RUSTSEC-2026-0097
```

## 4. Supply Chain Hardening (Later)
- `cargo deny`: license policy, banned crates, duplicate-version checks. Recommendation: enable a baseline config (`licenses` + `bans` + `sources` + `duplicates`) for critical parts.
- Release assets (recommended for public GitHub releases): publish `SHA256SUMS` and sign it (Sigstore/cosign keyless). This repo includes `.github/workflows/release_sha256sums.yml` to generate + upload `SHA256SUMS`, `SHA256SUMS.sig`, and `SHA256SUMS.pem` for a release. User-facing verification steps are in `docs/models.md`.

## Summary
1) Editor: `rust-analyzer` warns while you type.  
2) Dev: run `fmt + clippy + test + audit` before pushing.  
3) CI: enforce the same gates to block vulnerable/warning builds.  
4) Growing project: add `cargo deny` for supply-chain hardening.
5) Public releases: ship signed `SHA256SUMS` for release assets.

```
# Install tools (once):
rustup component add clippy rustfmt
cargo install cargo-audit

# Same set as CI runs:
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo audit --deny warnings --ignore RUSTSEC-2024-0436 \
            --ignore RUSTSEC-2025-0134 \
            --ignore RUSTSEC-2026-0097
```
