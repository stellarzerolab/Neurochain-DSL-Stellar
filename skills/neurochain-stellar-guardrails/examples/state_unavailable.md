# State Unavailable

Use this language when `get_guardrail_status` or another MCP v0 tool returns
`state_unavailable`.

## Agent Response

NeuroChain could not observe the requested guardrail or verification state.

- status: `state_unavailable`
- decision: `blocked` or unavailable, depending on the returned MCP result
- Stellar verification: `not_requested` or `required_on_stellar`
- attestation submitted: `false`
- transaction hash: unavailable
- nullifier consumed: `false`
- underlying action submit allowed: `false`

Do not guess the missing state. Missing host `latest_result`, unavailable
verifier configuration, or unavailable x402/facilitator state must fail closed.

## Safe Next Step

Report the missing precondition and stop. Do not retry by adding wallet
sources, secrets, signing material, submit tools, attestation tools, or
nullifier-consume tools.
