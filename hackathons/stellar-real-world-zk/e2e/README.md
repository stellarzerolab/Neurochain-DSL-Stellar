# Provider-boundary E2E fixtures

This crate tests the complete dependency-free boundary chain:

```text
typed ActionPlan + private policy
-> guest adapter
-> public journal + fixture proof
-> host verification boundary
-> Soroban-style verification + atomic replay boundary
```

The digest and proof providers exist only inside `tests/pipeline.rs`. They are
deterministic fixture mechanisms, not SHA-256, RISC Zero proofs or production
fallbacks. They bind the expected image id and exact journal bytes so that the
integration tests can prove tamper rejection and replay ordering across all
four crates before the real toolchain adapters are installed.

Covered outcomes:

- `approved` is only eligible for a separate approval flow
- `requires_approval` remains no-submit
- blocked exit `3`, `4` and `5` remain blocked
- a journal changed after fixture proof creation is rejected before nullifier
  consumption
- the same valid nullifier is accepted once and then rejected as exit `4`
  with reason `replay`
- private policy values do not appear in public journal bytes
