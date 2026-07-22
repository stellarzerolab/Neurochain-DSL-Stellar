# NeuroChain Stellar Guardrails Skill Examples

These examples show how an agent should report common MCP v0 outcomes when
using the `neurochain-stellar-guardrails` skill.

They are not new runtime fixtures and they are not submit recipes. The
machine-checkable MCP response fixtures remain in:

```text
examples/mcp_v0_no_submit_contract/
```

Use these examples to keep the agent-facing language short and safe:

- [approved](approved.md)
- [requires_approval](requires_approval.md)
- [blocked](blocked.md)
- [state_unavailable](state_unavailable.md)

All examples preserve:

```json
{
  "underlying_action_submit_allowed": false,
  "attestation_submitted": false,
  "verification_transaction_submitted": false,
  "nullifier_consumed": false
}
```

The default sequence remains:

```text
Plan -> Evaluate -> Prove -> Verify -> Status -> no automatic submit
```
