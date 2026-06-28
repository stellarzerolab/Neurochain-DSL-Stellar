# NeuroChain ZK Guardrail Attestation

This hackathon package proves that a known deterministic NeuroChain evaluator
checked a typed Stellar ActionPlan against a private owner policy. The proof
reveals the decision and its commitments, not the policy rules.

## Product flow

```text
agent intent
-> existing ContractInvoke label
-> typed ActionPlan
-> private policy + private audit nonce
-> RISC Zero guest evaluates guardrails
-> public journal + receipt
-> Soroban verifies the receipt and consumes the audit nullifier
-> approved | blocked | requires_approval
```

Payment is optional ingress. Payment or a valid proof is never submit
permission. `requires_approval` remains a no-submit boundary.

## Package layout

```text
shared/      dependency-free Rust data contract and canonical encoding
guest/       dependency-free evaluator and commitment adapter
host/        dependency-free receipt verification and journal adapter
contracts/   dependency-free Soroban-style verification and replay boundary
e2e/         fixture-only guest -> host -> contract integration tests
fixtures/    public examples of typed inputs and journal outcomes
risc0/       real RISC Zero guest, receipt generation and host verification
```

## Public and private inputs

Public ActionPlan fields:

- `schema_version`
- `intent_label`, fixed to the existing `ContractInvoke` model label
- `action_kind`, fixed to `soroban_contract_invoke`
- `contract_id`
- `function`
- typed args, sorted by arg name
- `intent_confidence_bps`

Private policy fields:

- `policy_version`
- random 32-byte `commitment_salt`
- allowed contracts
- allowed `contract:function` pairs
- allowed assets
- allowed recipients
- maximum amount in minor units
- approval threshold in minor units
- minimum intent confidence in basis points

The private audit nonce is separate from the policy. It binds one evaluation
to one public `audit_nullifier` without becoming a reusable policy secret.

Public journal fields:

- `evaluator_image_id`
- `action_plan_hash`
- `policy_commitment`
- `policy_version`
- `decision_status`
- `exit_code`
- `reason_code`
- `requires_approval`
- `audit_nullifier`

## Stable decision semantics

| Decision | Exit | Reason | Submit meaning |
| --- | ---: | --- | --- |
| `approved` | `0` | `passed` | eligible for a later, separate approval/submit flow |
| `requires_approval` | `0` | `approval_threshold` | no submit |
| `blocked` | `3` | `allowlist` | no submit |
| `blocked` | `4` | `contract_policy` | no submit |
| `blocked` | `5` | `intent_safety` | no submit |

Invalid receipt, wrong image id and replay are rejected by the Soroban
verification boundary. They map to the existing exit `4` policy boundary but
are not valid guest-produced journal decisions.

The deterministic evaluator runs guardrails in this order:

1. ActionPlan and policy shape validation
2. contract allowlist, exit `3`
3. `contract:function` policy, exit `4`
4. required typed args and confidence, exit `5`
5. asset, recipient and maximum amount policy, exit `4`
6. inclusive approval threshold (`amount >= threshold`)

An invalid private policy is an exit `4` policy failure. A missing or wrongly
typed `amount`, `asset` or `recipient` is an exit `5` intent-safety failure.

## Canonical encoding

`shared/src/lib.rs` is the contract source of truth. All integers use
big-endian encoding. Variable-length byte strings use a big-endian `u32`
length prefix. Lists use a big-endian `u32` item count. Typed args and policy
lists must be strictly lexicographically sorted and deduplicated.

Domain separators:

- `NC_ZK_ACTION_PLAN_V1\0`
- `NC_ZK_PRIVATE_POLICY_V1\0`
- `NC_ZK_PUBLIC_JOURNAL_V1\0`
- `NC_ZK_AUDIT_NULLIFIER_V1\0`

The RISC Zero guest hashes the ActionPlan and policy preimages with SHA-256.
No JSON byte representation is hashed. This avoids whitespace, key-order and
number-format ambiguity.

Typed value tags:

- `1`: address, encoded as a length-prefixed UTF-8 string
- `2`: bytes, encoded as length-prefixed raw bytes
- `3`: symbol, encoded as a length-prefixed UTF-8 string
- `4`: `u64`, encoded as eight big-endian bytes

Decision and reason numeric tags are fixed in `shared/src/lib.rs` and are used
by the future Soroban decoder.

## Current milestone

Implemented:

- shared types and canonical encoding
- existing `ContractInvoke` label binding
- private policy shape
- public journal shape
- dependency-free deterministic guardrail evaluator
- dependency-free guest input/output adapter with a required SHA-256 provider
- real RISC Zero 3.0.5 guest using the existing deterministic evaluator
- SHA-256 commitments computed inside the RISC Zero guest
- genuine Groth16 receipt generation with development mode disabled
- Stellar verifier-compatible `seal`, image ID and journal digest artifact
- serialized receipt verification through the host and contract boundaries
- approved receipt E2E with replay rejected as exit `4`
- strict public journal decoder and host receipt-verifier provider boundary
- dependency-free attestation/replay boundary with atomic nullifier consume
- fixture-only cross-crate E2E coverage for decisions, tamper and replay
- read-only local toolchain preflight for Rust, Stellar and RISC Zero
- exit `0` / `3` / `4` / `5` semantic validation
- audit nullifier preimage binding
- JSON fixture matrix and tests

Not implemented yet:

- concrete Soroban SDK contract and RISC Zero receipt verifier adapter
- persistent Soroban replay storage adapter
- API or submit integration

Dependency audit is not clean yet. The pinned RISC Zero 3.0.5 toolchain
currently brings in `RUSTSEC-2023-0071` (`rsa`, no fixed release available)
and `RUSTSEC-2025-0055` (`tracing-subscriber`) transitively, plus
unmaintained-crate warnings for `bincode`, `derivative` and `paste`. These are
toolchain/upstream constraints, not ignored findings. This milestone is a
hackathon prototype and must not be represented as production-audited.

The current `risc0/host` runner proves and verifies a real receipt locally,
serializes it, then passes it through the existing host and Soroban-style
contract boundaries. The Soroban-style boundary is dependency-free Rust and
is not yet an on-chain Soroban contract. A valid receipt only makes an
approved action eligible for a later, separate approval flow.

The runner writes `risc0/target/neurochain-zk-stellar-proof.json`. The ignored
local artifact contains only public proof material:

- `seal_hex`: Groth16 seal with the verifier-router selector prefix
- `image_id_hex`: expected 32-byte evaluator image ID
- `journal_hex`: canonical public NeuroChain journal
- `journal_digest_hex`: SHA-256 of the raw journal bytes

The private policy, commitment salt and audit nonce are not written to the
artifact. The future Soroban application contract will hash `journal_hex`,
call the verifier router with the seal/image/digest tuple, decode the journal
and atomically consume its audit nullifier.

## Local checks

The preflight script only inspects local commands and Rust targets. It never
installs tools, changes configuration or accesses a network. RISC Zero may be
native or in WSL2; the default WSL distribution is `Ubuntu`. Groth16 proving
also requires the `risc0-groth16` component and a running Docker daemon.
`-RequireReady` returns exit code `2` when a required component is absent.

```powershell
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/zk_toolchain_preflight.ps1
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/zk_toolchain_preflight.ps1 -Format Json
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/zk_toolchain_preflight.ps1 -RequireReady
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/zk_toolchain_preflight.ps1 -WslDistribution Ubuntu -RequireReady
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/run_risc0_e2e.ps1
```

RISC Zero's official readiness check is `cargo risczero --version` after an
`rzup install`. Stellar contract builds require a current Rust toolchain,
Stellar CLI and the `wasm32v1-none` target.

```powershell
cargo fmt --manifest-path hackathons/stellar-real-world-zk/shared/Cargo.toml --check
cargo test --manifest-path hackathons/stellar-real-world-zk/shared/Cargo.toml
cargo clippy --manifest-path hackathons/stellar-real-world-zk/shared/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path hackathons/stellar-real-world-zk/guest/Cargo.toml --check
cargo test --manifest-path hackathons/stellar-real-world-zk/guest/Cargo.toml
cargo clippy --manifest-path hackathons/stellar-real-world-zk/guest/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path hackathons/stellar-real-world-zk/host/Cargo.toml --check
cargo test --manifest-path hackathons/stellar-real-world-zk/host/Cargo.toml
cargo clippy --manifest-path hackathons/stellar-real-world-zk/host/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path hackathons/stellar-real-world-zk/contracts/Cargo.toml --check
cargo test --manifest-path hackathons/stellar-real-world-zk/contracts/Cargo.toml
cargo clippy --manifest-path hackathons/stellar-real-world-zk/contracts/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path hackathons/stellar-real-world-zk/e2e/Cargo.toml --check
cargo test --manifest-path hackathons/stellar-real-world-zk/e2e/Cargo.toml
cargo clippy --manifest-path hackathons/stellar-real-world-zk/e2e/Cargo.toml --all-targets -- -D warnings
cargo test --test zk_guardrail_contract
```

The RISC Zero runner uses WSL2 by default, explicitly removes
`RISC0_DEV_MODE`, and builds the host with RISC Zero's `disable-dev-mode`
feature. It also validates the generated artifact field allowlist, lengths,
hex encoding and journal digest. A successful run prints:

```text
receipt_verified=true
decision=approved
exit_code=0
next_step=eligible_for_separate_approval_flow
replay=blocked_exit_4
proof_kind=groth16
stellar_artifact=target/neurochain-zk-stellar-proof.json
stellar_artifact_valid=true
```
