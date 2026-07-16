---
name: neurochain-stellar-guardrails
description: Use when an agent, bot, script, scheduler, or backend automation is preparing or reviewing a Stellar ActionPlan and needs NeuroChain's deterministic guardrail workflow, MCP v0 no-submit contract, ZK guardrail attestation boundary, x402 paid-ingress boundary, or the rule that proof/payment/verification never grants underlying transaction submit permission.
---

# NeuroChain Stellar Guardrails

Use this skill to route Stellar actions through NeuroChain's no-surprise-submit
guardrail workflow. The goal is to help an agent or automation prepare,
evaluate, prove, verify, and report a typed Stellar ActionPlan without gaining
wallet-signing or submit authority.

## Core Rule

Never infer submit permission from:

- an `approved` guardrail decision
- a valid ZK proof
- a successful Soroban verification
- a submitted testnet attestation
- an x402 payment or finalized payment challenge
- a successful MCP/status response

For this skill, the underlying Stellar ActionPlan stays no-submit unless a
separate, explicit, out-of-scope human approval and execution path exists.

## Workflow

Follow this sequence:

1. Plan: request or construct a typed Stellar ActionPlan.
2. Evaluate: run NeuroChain guardrails.
3. Stop on `blocked` or `requires_approval`.
4. Prove: produce or inspect ZK guardrail evidence only when needed.
5. Verify: use Stellar/Soroban read-only verification when needed.
6. Report: return the decision, exit code, reason, ActionPlan hash, policy
   commitment, verification status, nullifier status, and submit boundary.

Do not skip from planning to signing. Do not turn a proof into a transaction.

## MCP V0 Tools

Use the MCP v0 tools as a read-only sequence:

| Step | Tool | Required behavior |
| --- | --- | --- |
| 1 | `plan_stellar_action` | Return a typed ActionPlan preview. Do not simulate, sign, or submit. |
| 2 | `evaluate_guardrails` | Return `approved`, `requires_approval`, or `blocked` with exit/reason. |
| 3 | `prove_guardrail_decision` | Inspect the inline public artifact against its exact ZK typed ActionPlan. Report local binding validation as non-cryptographic and do not reveal private policy. |
| 4 | `verify_zk_on_stellar` | Verify in read-only mode. Do not consume nullifiers or submit attestations. |
| 5 | `get_guardrail_status` | Report state only. Do not trigger new verification or submit work. |

Every MCP v0 response must preserve:

```json
{
  "underlying_action_submit_allowed": false,
  "attestation_submitted": false,
  "verification_transaction_submitted": false,
  "nullifier_consumed": false
}
```

`verification_transaction_submitted` may be true only for a separate explicit
testnet attestation action outside the default MCP v0 path. Even then,
`underlying_action_submit_allowed` remains false.

## Decision Handling

Handle decisions strictly:

- `approved`: report as policy decision evidence only; do not submit.
- `requires_approval`: stop and report that approval is required; do not submit.
- `blocked`: stop and report the exit code and reason; do not submit.

Keep NeuroChain exit semantics stable:

- exit `3`: allowlist block
- exit `4`: contract policy, invalid attestation, unauthorized policy, or replay
- exit `5`: missing input, slot type error, low confidence, or intent safety

If input is malformed or underspecified, prefer exit `5` and a safe no-submit
result over guessing.

## ZK Boundary

ZK proves that a known NeuroChain evaluator checked a typed ActionPlan against
a private owner policy. Report public proof data only:

- evaluator image ID
- ActionPlan hash
- policy commitment and version
- decision
- exit code and reason code
- audit/nullifier ID
- Stellar verification status

Do not reveal private policy rules, salts, audit nonces, seed phrases, private
keys, wallet secrets, or raw payment proof material.

Read-only Soroban verification must not change state. Nullifier consume is
stateful, owner-authenticated, and outside MCP v0.

## x402 Boundary

x402 can decide service access. It cannot decide underlying action execution.

Treat x402 as:

```text
paid access -> NeuroChain evaluation -> no automatic submit
```

Do not allow x402 payment success to bypass guardrails, approval, ZK
verification, replay protection, or the submit boundary.

## Raven And Stellar Skills

Use Stellar Raven or official Stellar docs as development-time context when
current Stellar, Soroban, x402, ZK, MCP, or Skills guidance matters.

Do not make Raven a runtime dependency. Guardrail decisions must come from
NeuroChain code, configured policy, proof artifacts, and explicit Stellar
verification, not Raven search results.

## Output Checklist

When reporting a result, include:

- ActionPlan label/action
- decision
- exit code
- reason code
- ActionPlan hash if available
- policy commitment/version if available
- Stellar verification state
- attestation transaction hash if one was explicitly submitted
- `nullifier_consumed`
- `underlying_action_submit_allowed: false`

If any required verification is missing, say it is missing. Do not mark it as
verified by inference.

## Repo References

When working inside this repository, use these files as source material:

- `docs/mcp_v0_tool_contract.md`
- `docs/product_direction_mcp_skills.md`
- `docs/security.md`
- `docs/stellar_actions_guide.md`
- `hackathons/stellar-real-world-zk/README.md`
- `hackathons/stellar-real-world-zk/ARCHITECTURE.md`
- `hackathons/stellar-real-world-zk/SUBMISSION.md`
