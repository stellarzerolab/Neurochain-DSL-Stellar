# MCP V0 No-Submit Contract Fixtures

These fixtures turn the MCP v0 contract into machine-checkable examples before
there is a dedicated MCP server implementation.

The default MCP v0 surface is read-only and no-submit:

```text
plan_stellar_action
-> evaluate_guardrails
-> prove_guardrail_decision
-> verify_zk_on_stellar
-> get_guardrail_status
-> no automatic submit
```

## Files

- `schema.json` describes the common response envelope.
- `plan_stellar_action.json` shows a typed ActionPlan preview.
- `evaluate_guardrails_approved.json` shows a passing guardrail decision.
- `evaluate_guardrails_requires_approval.json` shows a terminal no-submit
  approval boundary.
- `evaluate_guardrails_blocked_exit_4.json` shows a contract-policy block.
- `prove_guardrail_decision.json` shows a public ZK proof artifact reference.
- `verify_zk_on_stellar_read_only.json` shows read-only Soroban verification.
- `get_guardrail_status_verified.json` shows the final observational status.

## Invariants

Every fixture must preserve:

- `underlying_action_submit_allowed: false`
- `attestation_submitted: false`
- `verification_transaction_submitted: false`
- `nullifier_consumed: false`
- `transaction_hash: null`

The default MCP v0 tool list must not include:

- `submit_testnet_attestation`
- `consume_nullifier`
- `submit_underlying_action`
- `sign_transaction`
- `configure_server`

Those operations can only exist later as separately named, explicitly
confirmed, security-reviewed paths outside the default MCP v0 surface.

## Validation

The fixture package is checked by:

```text
cargo test --test mcp_v0_contract
```

The test parses every fixture and verifies the shared no-submit fields,
decision/exit consistency, and schema-level exclusions.

## Offline Runner

Use the local fixture runner when an agent, frontend, or future MCP shim needs
machine-readable sample responses without touching network, wallet, signing, or
submit paths:

```powershell
cargo run --bin neurochain-mcp-v0-fixture-runner -- --list
cargo run --bin neurochain-mcp-v0-fixture-runner -- --fixture verify_zk_on_stellar_read_only
cargo run --bin neurochain-mcp-v0-fixture-runner -- --tool evaluate_guardrails --scenario requires_approval
cargo run --bin neurochain-mcp-v0-fixture-runner -- --call-json "{\"name\":\"evaluate_guardrails\",\"arguments\":{\"scenario\":\"requires_approval\"}}"
```

The runner validates no-submit fields before printing a fixture. It is an
offline contract adapter, not a live MCP server and not a Stellar submit path.
The `--call-json` mode accepts a small MCP-style tool call shape and rejects
secret-like field names before selecting a fixture.
