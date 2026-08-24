# NeuroChain Rust Security & Tooling Stack

Goal: keep the toolchain lean (native-first) while keeping the process strict. Less plugin sprawl, more repeatable commands and CI gates.

## 1. Development Environment (VS Code)
- Required: `rust-analyzer` (set "Check On Save" = `clippy` if possible).
- Recommended: `Even Better TOML` (for `Cargo.toml` editing).
- Optional: Snyk or another polyglot scanner if your organization already uses it; it does not replace CI-level checks.

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
            --ignore RUSTSEC-2026-0186
```

Note: `cargo test` includes AI model smoke tests (`src/ai/model/tests.rs`). These tests auto-skip if the referenced ONNX files are missing (useful if you clone without `models/`). For end-to-end validation, run the example scripts that load models (see `docs/getting_started.md` and `examples/`).

Runtime safety note (Stellar path): in addition to toolchain checks,
`neurochain-stellar` enforces runtime guardrails (allowlist, contract policy,
intent safety). Allowlist enforcement fails closed with exit `3` when the
relevant asset or contract allowlist is empty. Contract policy enforcement
fails closed with exit `4` when configured policy files are missing,
unreadable, invalid, or when a contract action has no loaded policy set. Typed
policy mismatches for Soroban invoke args (`address` / `bytes` / `symbol` /
`u64`) are treated as `slot_type_error -> Unknown -> safe no-submit` in intent
mode (blocked flow / API plan execution path).

x402 audit/store/facilitator-boundary safety note (Stellar server path): `/api/x402/stellar/intent-plan` is an access/payment gate in front of the same guardrail pipeline, not a submit path. Payment verification sits behind `src/x402_facilitator.rs`; `NC_X402_STELLAR_VERIFIER=mock` selects the local development verifier (`PAYMENT-SIGNATURE: paid:<challenge_id>`), while `NC_X402_STELLAR_VERIFIER=facilitator` emits an official Base64 x402 v2 `PAYMENT-REQUIRED` challenge and accepts a bounded v2 `PAYMENT-SIGNATURE` payload. Facilitator mode requires validated endpoint, resource, network, asset, amount, receiver, timeout, persistent store, and audit configuration. Its blocking HTTP client is constructed and dropped inside the server's blocking worker. The authenticated runtime performs only `supported -> verify`; accepted verification is persistently bound to the exact network, payload, requirements, and idempotency key through a canonical SHA-256 request digest. Repeated identical requests reuse the pending verification without another facilitator call, while binding mismatches fail closed. The offline settlement state machine permits one transition from `verified_pending_settlement` to `settlement_in_progress`; success alone finalizes the challenge. Rejection is terminal, and a transport error after the settlement attempt begins, or a process restart becomes `settlement_outcome_unknown`, which blocks automatic retry until external reconciliation. Settlement audit rows contain only identifiers, request digest, timestamps, state, and an optional public transaction hash, never payment payloads or signatures. The runtime still returns `payment_verified_settlement_required`, keeps guardrails `not_run`, and preserves `underlying_action_submit_allowed=false`. Production runtime envs (`NC_ENV`, `APP_ENV`, or `RUST_ENV` set to `production`) disable the mock verifier. `requires_approval` is an explicit no-submit approval boundary: payment can be finalized and guardrails can pass only through the development mock path today, but the response still must not sign, submit, or broadcast. If `NC_X402_STELLAR_AUDIT_PATH` is set, the server appends safe JSONL audit rows for payment-required, approved, `requires_approval`, blocked, replay, expired, invalid payment, and future settlement states. If `NC_X402_STELLAR_STORE_PATH` is set, the server persists challenge, verification binding, settlement, and replay state across restarts; if the configured store cannot be loaded, x402 requests fail closed with `state_unavailable` instead of falling back to memory. Audit rows and the store must not persist the raw `PAYMENT-SIGNATURE` header, payment payload, payment requirements, invalid payment proofs, or the mock `paid:<challenge_id>` signature material. `/api/stellar/intent-plan` accepts server-side model ids, not arbitrary client-provided `model_path` values. The authenticated facilitator client reads `NC_X402_FACILITATOR_API_KEY` only at request time, marks the Authorization header sensitive, disables redirects, bounds response size and timeout, and must never log, serialize, fixture, or store the key. One explicitly approved live testnet rejection probe confirmed the authenticated `/verify` wire path with an unsigned malformed fixture; it did not prove a valid payment. The raw HTTP `/settle` transport exists, but server runtime settlement remains disabled.

The Rust facilitator client includes an authenticated x402 v2 `/settle`
transport with bounded fail-closed response parsing. It is validated offline
behind the persistent single-attempt state machine but is not exposed by the
server runtime. No live settlement, signing, value transfer, or ActionPlan
submit is enabled by this transport implementation.

The separate `services/x402-stellar-facilitator` workspace is gated in CI
with Node 24.19.0, pnpm 11.19.0, an exact frozen lockfile, and dependency
install scripts disabled. The committed 49-package inventory rejects
AGPL/GPL/LGPL/SSPL and unknown runtime licenses; package/version, readiness
summary, evidence-path, and authority drift fail closed. The 24-case readiness
record distinguishes verified offline evidence from service-boundary,
credential/network, upstream `upto`, and independent-review blockers. Neither
the CI gate nor a `verified_offline` status grants payment, credential,
network, signing, settlement, dispatch, transaction-submit, or ActionPlan-
submit authority.

ZK guardrail attestation safety note: the hackathon Soroban contract under
`hackathons/stellar-real-world-zk/soroban/` stores an owner, verifier-router
address and evaluator image ID in its constructor. A policy commitment and
version are accepted only after the owner has authorized that exact pair.
This prevents a prover from choosing an arbitrary permissive private policy
and presenting it as owner policy. Policy authorization and revocation require
owner authentication.

The permissionless `verify` method hashes the canonical public journal inside
Soroban, calls the verifier, checks the image and authorized policy binding,
strictly decodes the journal, and returns the typed result without writing
state. The owner-authenticated `verify_and_consume` method performs the same
checks and then atomically consumes the audit nullifier. Requiring owner auth
on consume prevents a public proof from being front-run merely to burn its
nullifier. The public REPL exposes read-only verification plus the explicit
`zk.stellar.attest` transaction command. Attest is hard-limited to testnet,
requires flow mode, calls only the permissionless `verify` method, leaves the
nullifier unused, and prints the resulting transaction hash and StellarExpert
link. Its stateful consume command remains disabled in remote mode and requires
local flow plus an explicit confirmation.

A valid proof is not submit permission: `approved` is only eligible for a
separate approval flow, while `requires_approval` and blocked exit `3` / `4` /
`5` remain non-submit outcomes. Invalid proof, unauthorized policy and replay
map to the existing exit `4` client boundary. Nullifier and instance TTLs are
extended to the network maximum when accessed; deployments intended to outlive
that horizon still need an explicit state-maintenance/restore policy.

Soroban SDK tests route genuine `approved`, `requires_approval` and
private-policy allowlist-block Groth16 artifacts through the pinned real router
and verifier. The Protocol 26 localnet runner additionally proves that
read-only verification does not consume the nullifier, owner consume persists
it, replay fails and a mutated proof fails. It is a local development network,
not a testnet or mainnet claim. The optional testnet deployment script refuses
to run without an explicit `-Execute` switch and writes only secret-free
deployment metadata. The pinned Nethermind verifier repository is not audited,
so an independent security review remains required before production use.

The read-only `/api/stellar/zk-attestation/view` endpoint validates only the
public artifact bindings: canonical ActionPlan hash, journal digest, evaluator
image ID and journal semantics. It deliberately does not claim Groth16
verification. Successful inspection still returns
`cryptographically_verified=false`, `stellar_verification_required=true` and
`execution.submit_allowed=false`. Tampered plans, digests or journals fail
closed, and the endpoint uses the existing `NC_API_KEY` authentication boundary.

RustSec note: `RUSTSEC-2026-0190` was resolved by updating `anyhow 1.0.100 ->
1.0.103`. `RUSTSEC-2026-0097` was resolved by updating the transitive
`rand 0.8.5 -> 0.8.6` lockfile entry. `RUSTSEC-2026-0104` was resolved by
updating `rustls-webpki 0.103.12 -> 0.103.13`. `RUSTSEC-2026-0185` was
resolved by updating `quinn-proto 0.11.14 -> 0.11.15`. `RUSTSEC-2026-0217`
was resolved by updating the `tract` dependency family from `0.21.13` to
`0.22.3`. The temporary
`RUSTSEC-2026-0186` ignore covers `memmap2 0.9.9`, which is transitive through
`tract-onnx 0.22.3`; the advisory currently has no patched release. Keep the
ignore scoped to this advisory and remove it when the ONNX dependency can move
to a fixed `memmap2` release.

## 3. CI/CD Gatekeepers (GitHub Actions Example)
Keep audit as a separate job; combining fmt+clippy saves time.

The checked-in `.github/workflows/ci.yml` also runs the x402 TypeScript
workspace through frozen install, supply-chain/license validation, strict
typecheck, build, and Node built-in tests. Keep that job separate from runtime
deployment and credential-bearing conformance.

### Bounded x402 testnet harness

The approved non-production testnet phase starts with a default-off one-shot
TypeScript harness. Its checked-in implementation has no concrete credential,
network or submit adapter. Dry-run validation accepts only exact official
testnet endpoints and fixed safe payment metadata, then returns a public plan
digest without invoking any effectful port.

An opted-in future testnet run must supply separate atomic-state, ephemeral
credential and canonical-client ports. The credential signer is held only in
a symbol-keyed opaque field, and public results are rebuilt from a strict
allowlisted evidence schema. Raw port exceptions, unknown evidence fields and
secret-shaped inputs are never echoed. Tests replace `fetch` with a throwing
sentinel and prove the default path makes zero state, credential, canonical or
network calls.

This boundary does not authorize pubnet/mainnet, a production or existing
wallet, persistent secret storage, custom account credentials, general
transaction submission, service dispatch, underlying execution or ActionPlan
submit. Live testnet evidence is recorded only after a separately gated run
has produced the corresponding public ledger result.

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
          # Known transitive warnings are scoped and documented in docs/security.md.
          cargo audit --deny warnings \
            --ignore RUSTSEC-2024-0436 \
            --ignore RUSTSEC-2025-0134 \
            --ignore RUSTSEC-2026-0186
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
            --ignore RUSTSEC-2026-0186
```
