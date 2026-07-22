# Requires Approval

Use this language when NeuroChain returns `requires_approval`.

## Agent Response

NeuroChain stopped the typed Stellar ActionPlan at the approval boundary.

- decision: `requires_approval`
- exit code: `0`
- reason: `approval_required`
- Stellar verification: `not_requested`
- attestation submitted: `false`
- nullifier consumed: `false`
- underlying action submit allowed: `false`

`requires_approval` is terminal for the default MCP v0 path. Payment, proof,
or verification must not be used to bypass this state.

## Safe Next Step

Call `get_guardrail_status` with the latest MCP `structuredContent`, report
that human or owner approval is required, and stop before proof generation,
attestation, signing, broadcast, or submit.
