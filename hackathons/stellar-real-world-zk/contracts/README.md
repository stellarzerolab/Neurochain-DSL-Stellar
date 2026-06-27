# Soroban boundary

The Soroban contract will verify the RISC Zero receipt against an allowlisted
evaluator image id, decode the public journal and reject a previously consumed
`audit_nullifier`.

Invalid attestation, wrong image id and replay are exit `4` policy failures.
`requires_approval` remains no-submit and cannot be upgraded to `approved` by
the verifier contract.
