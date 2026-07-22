# MCP V0 No-Submit Contract Fixtures

These fixtures keep the MCP v0 response contract machine-checkable alongside
the read-only stdio server. Explicit fixture/scenario calls remain available
for conformance tests; normal calls to all five tools use NeuroChain runtime
adapters.

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
- `prove_guardrail_decision.json` shows a locally validated public ZK artifact
  binding. It intentionally reports `cryptographically_verified: false` until
  the separate Stellar verification step.
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

## Status Vocabulary

The ZK and Stellar fields intentionally describe different boundaries:

| Field or state | Meaning in these fixtures |
| --- | --- |
| `proof_binding: "binding_validated"` | Local public artifact binding passed; the proof has not been cryptographically verified yet. |
| `cryptographically_verified: false` | Expected after `prove_guardrail_decision`; call `verify_zk_on_stellar` for read-only Soroban verification. |
| `stellar_verification: "verified_on_stellar"` | Read-only Soroban verification accepted the proof without submitting a transaction. |
| `attestation_submitted: false` | No explicit testnet attestation transaction was sent by the default MCP v0 path. |
| `nullifier_consumed: false` | No stateful replay boundary was consumed. |
| `underlying_action_submit_allowed: false` | The underlying Stellar ActionPlan still has no submit permission. |
| x402 payment fields | Payment can grant service access only; it is not proof, verification, or submit authority. |

Keep the sequence clear:

```text
Plan -> Evaluate -> Prove -> Verify -> Status -> no automatic submit
```

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

## Offline Stdio Shim

Use the stdio shim when an agent harness wants a JSON-RPC-shaped boundary
without running a live MCP server:

```powershell
@(
  '{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"fixture-harness","version":"0.1.0"}}}'
  '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
  '{"jsonrpc":"2.0","id":"call-1","method":"tools/call","params":{"name":"evaluate_guardrails","arguments":{"scenario":"requires_approval"}}}'
) | cargo run --bin neurochain-mcp-v0-stdio
```

The shim serves real local runtime calls for `plan_stellar_action`,
`evaluate_guardrails`, `prove_guardrail_decision`, `verify_zk_on_stellar`, and
`get_guardrail_status`; explicit `scenario` or `fixture` arguments retain the
embedded conformance path. The Stellar verification tool may call the
configured Stellar CLI only with `--send no`. The status tool is observational:
it normalizes the latest host-supplied `structuredContent` result or returns
`state_unavailable` when no latest result is supplied. It does not sign,
broadcast, submit, consume nullifiers, or accept secret-like fields.
`initialize` advertises the safe tool capability and an explicit experimental
`neurochainNoSubmit` boundary. The process accepts newline-delimited messages
until stdin closes, requires the initialized notification before tool calls,
and writes one compact JSON response line per request. The notification itself
correctly produces no response.
