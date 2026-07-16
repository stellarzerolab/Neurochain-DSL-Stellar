# NeuroChain MCP V0 Tool Contract

This document defines the first no-submit MCP surface for NeuroChain DSL for
Stellar. The stdio server now connects `plan_stellar_action` to the real local
NeuroChain intent classifier and deterministic ActionPlan builder. The four
later tools remain fixture-backed while their runtime adapters are implemented
one at a time.

Machine-checkable response fixtures live in:

```text
examples/mcp_v0_no_submit_contract/
```

Validate them with:

```text
cargo test --test mcp_v0_contract
```

An offline fixture runner is available for local agent/frontend integration
tests before a live MCP server exists:

```text
cargo run --bin neurochain-mcp-v0-fixture-runner -- --list
cargo run --bin neurochain-mcp-v0-fixture-runner -- --fixture verify_zk_on_stellar_read_only
cargo run --bin neurochain-mcp-v0-fixture-runner -- --call-json "{\"name\":\"evaluate_guardrails\",\"arguments\":{\"scenario\":\"requires_approval\"}}"
```

There is also a JSON-RPC stdio server for checking the MCP-shaped boundary and
calling the first real runtime-backed planning tool:

```powershell
@(
  '{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"fixture-harness","version":"0.1.0"}}}'
  '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
  '{"jsonrpc":"2.0","id":"call-1","method":"tools/call","params":{"name":"evaluate_guardrails","arguments":{"scenario":"requires_approval"}}}'
) | cargo run --bin neurochain-mcp-v0-stdio
```

The standalone fixture runner only reads embedded fixtures and preserves the
same no-submit invariants. It does not connect to Stellar, sign, broadcast,
submit, or consume nullifiers. The `--call-json` mode is an offline MCP-style
call adapter; it rejects submit-like tool names and secret-like field names.

The stdio shim follows the same rules and returns JSON-RPC `result` or `error`
objects. Successful tool calls use the MCP `content` array and mirror the same
JSON envelope in `structuredContent`; `isError` remains `false`. Tool metadata
marks every default operation read-only, non-destructive, idempotent, and
closed-world. Its `initialize` response advertises only static tools plus an
experimental `neurochainNoSubmit` capability that makes the read-only mode,
excluded tools, and no-submit boundary machine-readable. The process reads
newline-delimited JSON-RPC messages until stdin closes. It requires
`initialize` and `notifications/initialized` before tool operations, emits no
response for notifications, and flushes one compact JSON response line per
request. Parse errors, invalid requests, unsupported methods, and invalid
parameters use the standard JSON-RPC error codes `-32700`, `-32600`, `-32601`,
and `-32602`.

For `plan_stellar_action`, an `intent_text` call loads the local
`intent_stellar` ONNX model and calls the same deterministic ActionPlan builder
used by the CLI and API. The model asset is not embedded in the executable;
configure an absolute `NC_INTENT_STELLAR_MODEL` path for MCP hosts whose working
directory is not the repository root. Calls with an explicit `scenario` or
`fixture` stay offline and deterministic for conformance testing. Calls without
either `intent_text` or an explicit fixture selector fail closed. No MCP path
calls simulation, flow execution, signing, broadcast, or submit.

A process-level client harness and MCP host configuration example live in:

```text
examples/mcp_v0_stdio_client/
```

Build both binaries and run the harness:

```bash
cargo build --bin neurochain-mcp-v0-stdio --bin neurochain-mcp-v0-client-smoke
cargo run --bin neurochain-mcp-v0-client-smoke
```

On Windows, the repeatable release gate builds both locked release binaries,
runs the conformance harness against the absolute server path, and prints
artifact SHA-256 hashes:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify_mcp_v0_release.ps1
```

The harness starts the stdio server as a child process, performs the MCP
lifecycle, discovers the five default tools, calls the `requires_approval`
fixture, and validates that submit, attestation, transaction, and nullifier
state remain disabled. Its host-neutral conformance session also checks silent
notifications and fail-closed responses for an excluded submit tool, a
secret-like argument, an unsupported method, and missing call parameters. MCP
hosts should use an absolute executable path, as shown in
`mcp_servers.json.example`, because their working directory is not a stable
runtime contract.

MCP v0 exists so an AI agent, bot, script, scheduled job, or backend automation
can ask NeuroChain for a typed policy decision without receiving a wallet,
signing, broadcast, or submit capability.

## Boundary

MCP v0 answers:

> What did NeuroChain decide about this typed Stellar ActionPlan?

MCP v0 does not answer:

> Should this agent sign and submit the underlying Stellar action now?

The default MCP surface is read-only and no-submit:

- no wallet secret input
- no wallet signing
- no transaction broadcast
- no mainnet submit
- no stateful nullifier consume
- no implicit testnet attestation transaction
- no automatic execution after a proof or payment succeeds

Any future submit-like operation must be a separate, explicitly named,
testnet-only tool with its own confirmation field. It must still not submit the
underlying ActionPlan.

## Common Response Envelope

Every MCP v0 tool should return a small envelope with stable safety fields:

```json
{
  "schema_version": 1,
  "tool": "evaluate_guardrails",
  "mode": "read_only",
  "status": "ok",
  "decision": "approved",
  "exit_code": 0,
  "reason_code": "passed",
  "action_plan_hash": "hex-string-or-null",
  "policy_commitment": "hex-string-or-null",
  "policy_version": 7,
  "stellar_verification": "not_requested",
  "attestation_submitted": false,
  "verification_transaction_submitted": false,
  "transaction_hash": null,
  "nullifier_consumed": false,
  "underlying_action_submit_allowed": false,
  "logs": []
}
```

Required safety fields:

- `decision`
- `exit_code`
- `reason_code`
- `attestation_submitted`
- `verification_transaction_submitted`
- `nullifier_consumed`
- `underlying_action_submit_allowed`

`underlying_action_submit_allowed` must be `false` for every MCP v0 tool.

## Tool 1: plan_stellar_action

Purpose: convert a user intent or structured automation request into a typed
Stellar ActionPlan preview.

Inputs:

```json
{
  "intent_text": "Invoke purchase_credits on the demo contract for 100 XLM",
  "network": "testnet",
  "source_hint": "optional-alias-only",
  "plan_mode": "preview_only"
}
```

Output additions:

```json
{
  "action_plan": {
    "schema_version": 1,
    "label": "ContractInvoke",
    "action": "soroban_contract_invoke"
  },
  "action_plan_hash": "hex-string",
  "next_recommended_tool": "evaluate_guardrails"
}
```

Safety rules:

- `source_hint` may be an alias, never a secret.
- `network` is limited to `testnet` in MCP v0.
- The classification threshold is server-controlled; clients cannot lower it.
- The tool must not simulate, sign, or submit.
- The returned `action_plan_hash` is SHA-256 over the domain-separated,
  serialized NeuroChain ActionPlan (`neurochain:mcp-v0:action-plan-json:v1`).
- Low-confidence or missing-slot plans contain an `Unknown` action and remain
  `not_evaluated`; the next `evaluate_guardrails` phase assigns exit `5`.

## Tool 2: evaluate_guardrails

Purpose: run deterministic NeuroChain guardrails against a canonical typed
ActionPlan.

Inputs:

```json
{
  "action_plan": {},
  "policy_ref": "local-policy-or-configured-service-policy",
  "evaluation_mode": "deterministic"
}
```

Output additions:

```json
{
  "decision": "approved",
  "exit_code": 0,
  "reason_code": "passed",
  "guardrails": {
    "allowlist": "passed",
    "contract_policy": "passed",
    "intent_safety": "passed"
  },
  "next_recommended_tool": "prove_guardrail_decision"
}
```

Safety rules:

- Raw private policy, salts, audit nonces, and secrets must not be returned.
- `requires_approval` is a terminal no-submit result for MCP v0.
- Exit `3`, `4`, and `5` keep the existing NeuroChain meanings:
  - `3`: allowlist block
  - `4`: contract policy block
  - `5`: intent safety, missing input, type error, or low confidence

## Tool 3: prove_guardrail_decision

Purpose: produce or inspect a proof artifact for a guardrail decision.

Inputs:

```json
{
  "action_plan": {},
  "policy_ref": "local-policy-or-configured-service-policy",
  "proof_mode": "local_or_bundled",
  "evaluator_image_id": "hex-string"
}
```

Output additions:

```json
{
  "proof_artifact_ref": "local-or-bundled-artifact-id",
  "evaluator_image_id": "hex-string",
  "journal_digest": "hex-string",
  "policy_commitment": "hex-string",
  "policy_version": 7,
  "audit_nullifier": "hex-string",
  "decision": "approved",
  "exit_code": 0,
  "reason_code": "passed",
  "next_recommended_tool": "verify_zk_on_stellar"
}
```

Safety rules:

- The proof artifact proves a decision only.
- The proof artifact must not expose private policy rules.
- A valid proof must not change `underlying_action_submit_allowed`.

## Tool 4: verify_zk_on_stellar

Purpose: verify a proof decision against the configured Soroban verifier in
read-only mode.

Inputs:

```json
{
  "proof_artifact_ref": "local-or-bundled-artifact-id",
  "contract_id": "C...",
  "network": "testnet",
  "verification_mode": "read_only"
}
```

Output additions:

```json
{
  "stellar_verification": "verified_on_stellar",
  "verification_mode": "read_only",
  "contract_id": "C...",
  "network": "testnet",
  "attestation_submitted": false,
  "verification_transaction_submitted": false,
  "transaction_hash": null,
  "nullifier_consumed": false,
  "next_recommended_tool": "get_guardrail_status"
}
```

Safety rules:

- Read-only verification must not consume a nullifier.
- Read-only verification must not submit an attestation transaction.
- Contract mismatch, unauthorized policy, invalid proof, or replay maps to a
  fail-closed exit `4` boundary.

## Tool 5: get_guardrail_status

Purpose: return the latest local and Stellar verification state for the current
MCP session or artifact.

Inputs:

```json
{
  "session_id": "optional-session-id",
  "proof_artifact_ref": "optional-artifact-id"
}
```

Output additions:

```json
{
  "local_binding": "binding_validated",
  "stellar_verification": "verified_on_stellar",
  "attestation_submitted": false,
  "verification_transaction_submitted": false,
  "transaction_hash": null,
  "nullifier_consumed": false,
  "underlying_action_submit_allowed": false
}
```

Safety rules:

- Status is observational.
- Status must not trigger a new verification, attestation, consume, or submit.
- If no Stellar verification exists yet, report `not_requested` or
  `required_on_stellar` rather than guessing.

## Explicitly Out Of V0

The following future operations are excluded from the default MCP v0 surface:

| Future operation | Why it is excluded from v0 |
| --- | --- |
| `submit_testnet_attestation` | It creates a real testnet transaction and needs an explicit user action. |
| `consume_nullifier` | It is stateful, owner-authenticated, and can affect replay state. |
| `submit_underlying_action` | It is the core execution boundary and must stay outside MCP v0. |
| `sign_transaction` | It requires wallet authority and secret/key handling. |
| `configure_server` | It is operator administration, not agent runtime work. |

If any of these are added later, they need separate documentation, explicit
input confirmation fields, tests, and a security review.

## Failure Semantics

MCP v0 should fail closed:

- malformed input -> exit `5`
- missing typed field -> exit `5`
- slot type error -> exit `5`
- allowlist violation -> exit `3`
- contract or policy violation -> exit `4`
- invalid proof -> exit `4`
- unauthorized policy commitment -> exit `4`
- replay or already-consumed nullifier -> exit `4`
- unavailable configured verifier/store -> `status: "state_unavailable"` and
  no submit capability

Errors should be explainable enough for an agent to report to the user, but
must not leak private policy contents.

## Agent Instruction Summary

An agent using MCP v0 should follow this sequence:

1. Call `plan_stellar_action`.
2. Show or inspect the typed ActionPlan preview.
3. Call `evaluate_guardrails`.
4. Stop immediately if the decision is `blocked` or `requires_approval`.
5. If proof evidence is needed, call `prove_guardrail_decision`.
6. If Stellar verification is needed, call `verify_zk_on_stellar`.
7. Call `get_guardrail_status` to report the final state.
8. Never infer submit permission from payment, proof, verification, or status.

## Acceptance Checklist

Before an implementation claims MCP v0 compatibility:

- All tools return `underlying_action_submit_allowed: false`.
- No tool accepts raw seed phrases, secret keys, private keys, or API tokens.
- No tool signs or broadcasts a transaction.
- Read-only Stellar verification leaves nullifier state unchanged.
- `submit_testnet_attestation` is not part of the default tool list.
- Exit `3`, `4`, and `5` match the existing CLI/REPL/API semantics.
- Private policy material is never returned in logs or responses.
- Raven is not required for the runtime path.
