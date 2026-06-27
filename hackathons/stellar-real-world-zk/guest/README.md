# Guest boundary

The RISC Zero guest will:

1. read a typed ActionPlan, private policy and private audit nonce
2. validate canonical ordering and required typed fields
3. hash the ActionPlan and private policy preimages
4. evaluate allowlist, contract policy, intent safety and approval threshold
5. commit only the public journal

The dependency-free evaluator is implemented as `shared::evaluate`. The guest
adapter in `src/lib.rs` calls that exact evaluator, binds canonical ActionPlan
and policy preimages through the `Sha256Provider` boundary, derives the audit
nullifier and emits encoded public journal bytes.

No cryptographic fallback is included. The future RISC Zero guest must provide
its real SHA-256 implementation and a verifier-approved image id. The test-only
digest implementation is not exported and is not cryptographic.

The guest must not sign, submit, broadcast, call x402 or contact a network.
Its image id is part of the public verification contract.
