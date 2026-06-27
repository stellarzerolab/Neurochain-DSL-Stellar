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
guest/       future RISC Zero evaluator boundary
host/        future proof generation and journal parsing boundary
contracts/   future Soroban verification and replay boundary
fixtures/    public examples of typed inputs and journal outcomes
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

The next proof milestone hashes the ActionPlan and policy preimages with
SHA-256 inside the guest/host boundary. No JSON byte representation is hashed.
This avoids whitespace, key-order and number-format ambiguity.

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
- exit `0` / `3` / `4` / `5` semantic validation
- audit nullifier preimage binding
- JSON fixture matrix and tests

Not implemented yet:

- RISC Zero guest or receipt generation
- cryptographic hashing in the guest
- Soroban receipt verifier
- replay storage
- API or submit integration

## Local checks

```powershell
cargo fmt --manifest-path hackathons/stellar-real-world-zk/shared/Cargo.toml --check
cargo test --manifest-path hackathons/stellar-real-world-zk/shared/Cargo.toml
cargo clippy --manifest-path hackathons/stellar-real-world-zk/shared/Cargo.toml --all-targets -- -D warnings
cargo test --test zk_guardrail_contract
```
