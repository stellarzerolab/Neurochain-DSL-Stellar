# Approved

Use this language when NeuroChain returns an `approved` guardrail decision and
the host has enough evidence to continue the read-only MCP v0 sequence.

## Agent Response

NeuroChain approved the typed Stellar ActionPlan as policy evidence.

- decision: `approved`
- exit code: `0`
- reason: `passed`
- local binding: `binding_validated`
- Stellar verification: `verified_on_stellar` if the read-only verifier was
  called, otherwise `not_requested`
- nullifier consumed: `false`
- underlying action submit allowed: `false`

This does not submit the ActionPlan. A valid proof or read-only Stellar
verification is evidence only; execution still requires a separate explicit
approval and submit path outside MCP v0.

## Safe Next Step

Report the status and stop. Do not sign, broadcast, create a testnet
attestation, consume a nullifier, or submit the underlying Stellar action.
