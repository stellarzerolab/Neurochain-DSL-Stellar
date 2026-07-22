# Blocked

Use this language when NeuroChain returns `blocked`.

## Agent Response

NeuroChain blocked the typed Stellar ActionPlan before execution.

- decision: `blocked`
- exit code: `3`, `4`, or `5`
- reason: report the returned reason code
- Stellar verification: `not_requested`
- attestation submitted: `false`
- nullifier consumed: `false`
- underlying action submit allowed: `false`

Exit meanings stay stable:

- `3`: allowlist block
- `4`: contract policy, invalid attestation, unauthorized policy, or replay
- `5`: missing input, type error, low confidence, or intent safety

## Safe Next Step

Call `get_guardrail_status` with the latest MCP `structuredContent`, report
the block, and stop. Do not ask another tool to sign, broadcast, consume a
nullifier, submit a testnet attestation, or execute the underlying action.
