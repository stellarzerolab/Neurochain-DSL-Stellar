# Soroban attestation and replay boundary

This crate defines the dependency-free contract logic that the Soroban adapter
must preserve:

1. verify a non-empty proof against the expected evaluator image id and exact
   journal bytes
2. strictly decode and validate the public journal
3. require the journal image id to match the allowlisted image id
4. atomically consume the `audit_nullifier`
5. preserve the guest decision without upgrading its submit meaning

The nullifier interface intentionally exposes only `consume_if_unused`. It
does not expose a check-then-insert sequence that could permit concurrent
replay. An unavailable nullifier store fails closed.

Invalid proof, malformed journal, wrong image id and unavailable state map to
exit `4` with `invalid_attestation`. A consumed nullifier maps to exit `4` with
`replay`. These are contract-level rejections, not guest-produced decisions.

An accepted `approved` journal is only eligible for a later, separate approval
flow. `requires_approval` and all accepted blocked exit `3` / `4` / `5` journals
remain no-submit boundaries.

This is not yet a deployed Soroban contract or a real RISC Zero verifier. The
`ProofVerifier` and `NullifierStore` traits are explicit integration points for
those production adapters; no permissive fallback is included.
