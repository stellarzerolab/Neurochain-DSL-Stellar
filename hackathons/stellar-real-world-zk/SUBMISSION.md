# NeuroChain ZK Guardrail Attestation

## One-line description

Prove that a known deterministic AI-agent guardrail program evaluated a typed
Stellar ActionPlan against a private owner policy, then verify that decision in
Soroban without revealing the policy.

## The problem

AI agents can prepare payments and contract calls, but an owner may not want to
publish the policy that controls those actions. A normal off-chain policy API
forces the user and the chain to trust the service that reports `approved` or
`blocked`.

NeuroChain makes the safety decision verifiable. The ActionPlan is public, the
owner policy stays private, and a RISC Zero proof attests that the committed
NeuroChain evaluator produced the published decision. Soroban verifies the
proof against an owner-authorized policy commitment. A separate owner call can
consume the audit nullifier so the same attestation cannot be reused.

## Why zero knowledge is essential

The proof is not decoration or an identity check. It proves execution of the
actual deterministic guardrail evaluator across multiple policy fields:

- contract allowlist
- contract and function policy
- asset and recipient policy
- maximum amount
- approval threshold
- required typed inputs and intent confidence.

Without the proof, Soroban would have to trust an off-chain server's claim.
Without privacy, the owner would have to publish the policy itself.

## How it works

1. NeuroChain creates a canonical typed `ContractInvoke` ActionPlan.
2. The RISC Zero guest receives the ActionPlan, private policy and private audit
   nonce.
3. The guest runs the known NeuroChain evaluator and commits a public journal.
4. A Groth16 receipt proves execution of the evaluator image.
5. The Soroban application calls the pinned verifier router and Groth16
   verifier.
6. Soroban checks the evaluator image and owner-authorized policy commitment,
   then strictly decodes the journal.
7. Permissionless `verify` returns the result without changing state;
   owner-authenticated `verify_and_consume` atomically records the nullifier.
8. The result is `approved`, `requires_approval` or `blocked` with the existing
   NeuroChain exit semantics.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the complete trust and data flow.

## Public result

The public journal binds:

- evaluator image ID
- ActionPlan hash
- private policy commitment and version
- decision status
- exit code and reason code
- approval state
- audit nullifier.

The private policy, commitment salt and audit nonce are not included in the
public proof artifact.

## Stellar integration

The implementation contains a real Soroban application contract with:

- an owner, evaluator image ID and verifier-router address
- an owner-authorized policy commitment/version registry
- SHA-256 of the canonical journal inside Soroban
- selector-based routing to the pinned RISC Zero Groth16 verifier
- strict no-allocation journal decoding in contract WASM
- repeatable read-only verification
- owner-authenticated persistent audit-nullifier consumption
- replay and invalid-proof rejection.

The full application -> router -> Groth16 verifier chain is exercised both in a
standalone Stellar Protocol 26 localnet and in an approved Stellar testnet
deployment. The secret-free [`deployments/testnet.json`](deployments/testnet.json)
manifest records the contract IDs, pinned hashes and three read-only verified
policy scenarios. No mainnet claim is made.

The existing NeuroChain CLI/REPL is the judge-facing bridge. It locally binds
the ActionPlan and journal, calls Soroban with `zk.stellar.verify <scenario>`
using `--send no`, and fails closed if the contract result differs from the
local binding. The separate `zk.stellar.consume` command is local-only and
never submits the underlying ActionPlan.

## Demonstrated scenarios

| Scenario | Proven result | Soroban next step |
| --- | --- | --- |
| valid private policy | `approved`, exit `0` | eligible only for a separate approval flow |
| private approval threshold reached | `requires_approval`, exit `0` | `RequiresApproval`, no submit |
| contract absent from private allowlist | `blocked`, exit `3`, `allowlist` | `Blocked` |
| repeatable read-only verification | same typed result, nullifier unused | no state change |
| reused audit nullifier | contract error `3` | rejected as replay |
| mutated Groth16 proof | contract error `2` | rejected as invalid attestation |

Payment or proof verification is never direct submit permission.

## Reproduce locally

Run the genuine Groth16/Soroban regression matrix:

```powershell
cargo test --manifest-path hackathons/stellar-real-world-zk/soroban/Cargo.toml --test groth16_proof
```

Run a full standalone Protocol 26 localnet scenario:

```powershell
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/run_soroban_localnet_e2e.ps1 -Scenario blocked_allowlist
```

Use the same bridge that is intended for the hosted CLI demo after configuring
a deployed contract and source alias:

```text
zk.demo blocked
zk.stellar.verify blocked
```

Generate a genuine RISC Zero Groth16 proof from private inputs:

```powershell
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/run_risc0_e2e.ps1 -Scenario requires_approval
```

The RISC Zero command needs the documented WSL2 toolchain and Docker. The
localnet runner removes its temporary identity and container after the run.

## Repository map

- [`risc0/`](risc0/) - genuine RISC Zero guest and proof host
- [`shared/`](shared/) - canonical data contract and deterministic evaluator
- [`soroban/`](soroban/) - Soroban application and verifier boundary
- [`fixtures/`](fixtures/) - public ActionPlan, journal and proof artifacts
- [`scripts/`](scripts/) - reproducible proof and Protocol 26 localnet runners
- [`DEMO_SCRIPT.md`](DEMO_SCRIPT.md) - concise video recording runbook

## Security boundaries and limitations

- No private keys, seed phrases or wallet secrets are stored in the package.
- No transaction signing or agent-controlled submit path is introduced.
- `requires_approval` is explicitly a no-submit result.
- Policy commitments can be authorized or revoked only by the configured owner.
- Stateful consume requires owner authentication; public read-only verification
  cannot burn a nullifier.
- The read-only API view validates public bindings but deliberately reports
  `cryptographically_verified=false`; cryptographic verification belongs to
  Soroban.
- The pinned verifier repository states that it is not audited. This is a
  hackathon prototype, not a production security claim.
- Persistent replay protection beyond the network maximum TTL still requires a
  maintenance and restore policy.
- Testnet evidence is limited to contract deployment, policy authorization and
  read-only proof verification; it does not submit the underlying ActionPlan.

## Differentiation

This is not a generic hidden-number range proof, payment privacy pool, identity
proof or agent-membership product. NeuroChain proves that a committed,
multi-field safety program evaluated an agent's typed on-chain action correctly
against a private policy.
