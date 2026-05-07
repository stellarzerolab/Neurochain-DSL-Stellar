# x402 Response Contract Fixtures

These fixtures describe the stable response envelope for the NeuroChain
Stellar x402 IntentPlan Gateway:

```http
POST /api/x402/stellar/intent-plan
```

The goal is to give frontend and agent integrations a concrete contract for
the current mock x402 gateway before a real facilitator is attached.

## Common Envelope

Every response includes:

- `ok`
- `audit_id`
- `payment`
- `decision`
- `guardrails`
- `logs`

Finalized requests also include `plan`, which is the typed ActionPlan that
NeuroChain evaluated.

The machine-readable schema is in:

```text
examples/x402_response_contract/schema.json
```

## Scenario Matrix

| Fixture | HTTP | `payment.state` | `decision.status` | `decision.reason` | `guardrails.state` | `guardrails.exit_code` |
| --- | --- | --- | --- | --- | --- | --- |
| `payment_required.json` | `402` | `payment_required` | `not_evaluated` | `null` | `not_run` | `null` |
| `approved.json` | `200` | `finalized` | `approved` | `null` | `passed` | `null` |
| `blocked_exit_3_allowlist.json` | `200` | `finalized` | `blocked` | `allowlist` | `blocked` | `3` |
| `blocked_exit_4_contract_policy.json` | `200` | `finalized` | `blocked` | `contract_policy` | `blocked` | `4` |
| `blocked_exit_5_intent_safety.json` | `200` | `finalized` | `blocked` | `intent_safety` | `blocked` | `5` |
| `replay_blocked.json` | `409` | `replay_blocked` | `blocked` | `payment_replay_blocked` | `not_run` | `null` |
| `expired.json` | `402` | `expired` | `blocked` | `payment_expired` | `not_run` | `null` |

## Semantics

- x402 is an access/payment gate, not an approval decision by itself.
- Paid requests still run NeuroChain guardrails.
- Guardrail exit codes keep the project-wide meaning:
  - `3` = allowlist block
  - `4` = contract policy block
  - `5` = intent safety, low confidence, or typed slot error
- `requires_approval` is currently `false`; a real approval boundary is a
  later integration step.
- `payment_required`, `replay_blocked`, and `expired` do not run guardrails,
  so `guardrails.state` is `not_run`.

## Non-Goals

These fixtures do not model real wallet signing, real facilitator settlement,
or submit/broadcast behavior. They define the agent/frontend response contract
around payment state, decision state, guardrail state, logs, and ActionPlan
shape.
