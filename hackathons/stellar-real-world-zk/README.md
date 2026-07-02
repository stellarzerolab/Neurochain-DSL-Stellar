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
-> Soroban verifies the receipt and owner-authorized policy commitment
-> read-only result OR owner-only audit-nullifier consume
-> approved | blocked | requires_approval
```

Payment is optional ingress. Payment or a valid proof is never submit
permission. `requires_approval` remains a no-submit boundary.

## CLI and hosted demo bridge

The existing `neurochain-stellar` REPL now presents the complete proof story as
three explicit steps:

```text
zk.demo approved                  # local ActionPlan/journal binding
zk.stellar.verify approved        # Soroban proof check, --send no
zk.stellar.consume approved       # local owner-only replay consume
```

`zk.stellar.verify` also accepts `requires_approval`, `blocked`, or `last`. It
compares the contract's action hash, policy commitment/version, decision,
exit/reason, approval bit and nullifier with the locally bound artifact before
showing success. It is repeatable and does not write state.

`zk.stellar.consume` is disabled in the remote REPL. Locally it requires flow,
confirmation and the contract owner's source alias. It submits only the
verification/nullifier transaction, not the ActionPlan represented by the
proof.

## Submission materials

- [SUBMISSION.md](SUBMISSION.md) - judge-facing problem, ZK value, Stellar
  integration, evidence and limitations
- [ARCHITECTURE.md](ARCHITECTURE.md) - proof, trust and decision-boundary
  diagrams
- [DEMO_SCRIPT.md](DEMO_SCRIPT.md) - 2-3 minute recording runbook
- [SUBMISSION_CHECKLIST.md](SUBMISSION_CHECKLIST.md) - automated package gate
  and remaining manual submission items

## Package layout

```text
shared/      dependency-free Rust data contract and canonical encoding
guest/       dependency-free evaluator and commitment adapter
host/        dependency-free receipt verification and journal adapter
contracts/   dependency-free Soroban-style verification and replay boundary
soroban/     real Soroban SDK contract, verifier call and persistent replay state
e2e/         fixture-only guest -> host -> contract integration tests
fixtures/    public examples of typed inputs and journal outcomes
risc0/       real RISC Zero guest, receipt generation and host verification
scripts/     proof, localnet, package-gate and guarded testnet workflows
deployments/ secret-free deployment evidence created only after an approved run
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
by the Soroban decoder.

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
- real Soroban SDK application contract using the pinned verifier interface
- genuine Groth16 proof verification through the pinned verifier contract in
  the Soroban SDK test environment
- selector-based dispatch through the pinned real verifier router to the
  Groth16 verifier contract in the Soroban SDK test environment
- Protocol 26 localnet deployment and invocation through the full application
  -> router -> Groth16 verifier chain with a genuine public proof
- owner-authorized policy commitment/version registry with owner-only mutation
- permissionless read-only proof verification without nullifier consumption
- owner-authenticated consume that prevents public-proof nullifier front-running
- typed CLI/REPL bridge that fails closed if the Soroban result differs from
  the locally bound ActionPlan or journal
- genuine `requires_approval` Groth16 proof through the same router/verifier
  chain, with exit `0` and an explicit no-submit next step
- genuine private-policy allowlist block proof through the same chain, with
  decision `blocked`, exit `3` and reason `allowlist`
- one-command proof-only video rehearsal with an explicit fail-closed offline
  Protocol 26 localnet opt-in
- localnet replay rejection and cryptographically invalid proof rejection
- read-only `/api/stellar/zk-attestation/view` inspection that binds the typed
  ActionPlan to the public journal without granting submit permission
- strict no-allocation journal decoding inside Soroban WASM
- verifier call before any replay-state read or write
- persistent audit-nullifier consume with maximum network TTL extension
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

- long-lived state-maintenance/restore policy beyond the network maximum TTL
- production security audit and underlying ActionPlan submit integration
- public testnet deployment evidence; the repository includes a guarded
  deployment script but does not claim deployment without `deployments/testnet.json`

Dependency audit is not clean yet. The pinned RISC Zero 3.0.5 toolchain
currently brings in `RUSTSEC-2023-0071` (`rsa`, no fixed release available)
and `RUSTSEC-2025-0055` (`tracing-subscriber`) transitively, plus
unmaintained-crate warnings for `bincode`, `derivative` and `paste`. These are
toolchain/upstream constraints, not ignored findings. This milestone is a
hackathon prototype and must not be represented as production-audited.
The Soroban test lock has no known vulnerability advisory, but still reports
the transitive unmaintained-crate warnings `RUSTSEC-2024-0388` (`derivative`)
and `RUSTSEC-2024-0436` (`paste`).

The current `risc0/host` runner proves and verifies a real receipt locally,
serializes it, then passes it through the existing host and dependency-free
contract boundaries. The Soroban SDK application contract separately verifies
the exported proof with the pinned Nethermind Groth16 verifier contract in the
Soroban SDK test environment. A valid receipt only makes an approved action
eligible for a later, separate approval flow.

The runner accepts `-Scenario approved` (default),
`-Scenario requires_approval` or `-Scenario blocked_allowlist`. It writes the
corresponding ignored artifact under `risc0/target/`. Each local artifact
contains only public proof material:

- `seal_hex`: Groth16 seal with the verifier-router selector prefix
- `image_id_hex`: expected 32-byte evaluator image ID
- `journal_hex`: canonical public NeuroChain journal
- `journal_digest_hex`: SHA-256 of the raw journal bytes

The private policy, commitment salt and audit nonce are not written to the
artifact. The Soroban application contract hashes `journal_hex`, calls the
configured verifier address through the pinned router interface with the
seal/image/digest tuple, decodes the journal and checks the owner-authorized
policy commitment/version. Its read-only method returns the result without a
write; its owner-only method atomically consumes the audit nullifier.
`fixtures/groth16_approved.json`,
`fixtures/groth16_requires_approval.json` and
`fixtures/groth16_blocked_exit_3.json` contain the corresponding public proof
material as reproducible regression fixtures. Unit tests cover Nethermind's
testing-only mock verifier, direct genuine cryptographic verification through
the pinned Groth16 verifier contract, and selector-based dispatch through the
pinned real verifier router. The localnet runner deploys the same three-contract
chain, accepts each genuine scenario as its attested decision, verifies that
read-only calls leave the nullifier unused, persists it through the owner call,
rejects replay as contract error `3` and rejects a mutated proof as contract
error `2`.

For a caller-selected typed ActionPlan and private policy, use the private-file
mode instead of editing host source:

```powershell
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/run_risc0_e2e.ps1 `
  -InputPath C:\private\neurochain-zk-input.json `
  -CheckInput

powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/run_risc0_e2e.ps1 `
  -InputPath C:\private\neurochain-zk-input.json `
  -OutputPath C:\private\neurochain-zk-public-proof.json
```

`risc0/private_input.example.json` documents the strict input schema with a
public synthetic witness. Real commitment salts, policy rules and audit nonces
must stay outside the repository. Matching `private_input*.json` files are
ignored except for the explicit example. The generated output contains only
the public seal, image ID, journal and digest and can be inspected with
`zk.verify`, then forwarded with `zk.stellar.verify last`.

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
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/run_risc0_e2e.ps1 -Scenario requires_approval
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/run_risc0_e2e.ps1 -Scenario blocked_allowlist
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/run_soroban_localnet_e2e.ps1
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/run_soroban_localnet_e2e.ps1 -Scenario requires_approval
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/run_soroban_localnet_e2e.ps1 -Scenario blocked_allowlist
```

The Soroban runner uses the official Stellar Quickstart image in a standalone
Protocol 26 localnet. It pins the verifier source commit, checks the expected
WASM SHA-256 values, deploys the verifier, router and application contracts,
exercises read-only/consume/replay/invalid-proof paths, and removes its temporary
local identity and container in `finally`. It does not connect to testnet or
mainnet.

An optional testnet deployment script is available but is never run by the
quality gates:

```powershell
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/deploy_testnet.ps1 `
  -Source nc-zk-testnet `
  -VerifierWasm path/to/groth16_verifier.wasm `
  -RouterWasm path/to/risc0_router.wasm `
  -Execute
```

It is hard-limited to testnet, requires explicit `-Execute`, verifies pinned
WASM hashes, authorizes all bundled policy commitments, runs read-only checks,
and writes a secret-free deployment manifest. Do not run it without explicit
network-submit approval.

The read-only API view accepts `typed_action_plan.json` plus the public proof
artifact. It recomputes the canonical ActionPlan hash, validates the journal
digest, image ID and journal semantics, and exposes the attested decision. It
does not cryptographically verify the Groth16 seal: the response therefore sets
`cryptographically_verified = false`, `stellar_verification_required = true`
and `execution.submit_allowed = false`. The real cryptographic check remains
the Soroban verifier boundary demonstrated by the localnet runner.

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
cargo test --manifest-path hackathons/stellar-real-world-zk/soroban/Cargo.toml
cargo clippy --manifest-path hackathons/stellar-real-world-zk/soroban/Cargo.toml --all-targets -- -D warnings
stellar contract build --manifest-path hackathons/stellar-real-world-zk/soroban/Cargo.toml
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
