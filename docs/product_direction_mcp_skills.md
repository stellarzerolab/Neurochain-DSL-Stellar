# NeuroChain MCP And Skills Direction

This document locks the post-hackathon product direction for NeuroChain DSL for
Stellar. The goal is to make the runtime easier for agents, scripts, scheduled
jobs, and backend automations to use without weakening the current no-surprise
submit boundary.

## Product Thesis

NeuroChain should become a small guardrail runtime that answers one question:

> Can this typed Stellar ActionPlan proceed within the owner's policy boundary?

The intended flow is:

```text
intent or automation request
  -> typed Stellar ActionPlan
  -> deterministic guardrail evaluation
  -> optional private-policy ZK proof
  -> Stellar/Soroban verification
  -> approved | requires_approval | blocked
  -> no automatic submit
```

This is useful for AI agents, bots, scripts, schedulers, and backend automation
systems. The value is not generic chat or generic Stellar help. The value is a
deterministic enforcement layer between a requested action and chain execution.

## Current Stellar Context Sources

Checked on 2026-07-10:

- [Stellar Raven](https://raven.stellar.buzz/) is useful as a development-time
  context source for official Stellar docs, live ecosystem data, community
  intelligence, and playbooks. It exposes a remote MCP endpoint at
  `https://raven.stellar.buzz/mcp`.
- [Stellar Skills](https://skills.stellar.org/) is useful as a packaging model
  for agent instructions. It includes official skill areas for smart contracts,
  assets, RPC/Horizon APIs, agent payments, ZK proofs, and standards.

NeuroChain should use Raven and Stellar Skills as guidance and distribution
models. NeuroChain must not require Raven at runtime.

## Non-Goals

The MCP and Skills direction is deliberately narrow:

- Do not make NeuroChain a generic Stellar assistant.
- Do not make NeuroChain depend on Raven, Skills, or any external agent catalog
  at runtime.
- Do not expose wallet signing, mainnet submit, or transaction broadcast in the
  default MCP surface.
- Do not treat x402 payment as permission to execute an underlying action.
- Do not treat a valid proof as permission to execute an underlying action.
- Do not collapse `requires_approval` into `approved`.
- Do not add social posting, X/OAuth, or account-monitoring features to the
  core guardrail runtime.

## MCP V0 Shape

MCP v0 should be read-only and no-submit by default. It should expose a small
surface that can be safely called by an agent or automation runner.

The detailed contract is in
[`docs/mcp_v0_tool_contract.md`](mcp_v0_tool_contract.md).

| Tool | Purpose | Output | Safety boundary |
| --- | --- | --- | --- |
| `plan_stellar_action` | Convert intent or structured input into a typed ActionPlan. | Canonical ActionPlan preview. | No signing, simulation, or submit. |
| `evaluate_guardrails` | Run deterministic guardrails against the ActionPlan. | `approved`, `requires_approval`, or `blocked` plus exit/reason. | Blocks remain terminal for MCP v0. |
| `prove_guardrail_decision` | Produce or inspect the ZK decision artifact for supported scenarios. | Proof metadata, ActionPlan hash, policy commitment, decision. | Proof is evidence, not submit permission. |
| `verify_zk_on_stellar` | Verify the decision against Soroban in read-only mode. | Stellar verification status and contract binding. | Read-only verification leaves state unchanged. |
| `get_guardrail_status` | Return the latest local and Stellar verification status. | Status, transaction hash if available, nullifier state, submit boundary. | `underlying_action_submit_allowed` remains false. |

The following actions stay outside the MCP v0 default path:

- `submit_testnet_attestation`
- wallet signing
- mainnet submit
- stateful nullifier consume
- hosted service administration

`submit_testnet_attestation` can exist later as an explicit, separately named,
testnet-only command. It must require a clear user action and must still not
submit the underlying ActionPlan.

## Skill Shape

The first community skill candidate should be:

[`NeuroChain Stellar Guardrails`](../skills/neurochain-stellar-guardrails/SKILL.md)

The skill should teach an agent to:

- create or request typed Stellar ActionPlans instead of free-form transaction
  instructions
- call NeuroChain before attempting Stellar execution
- respect exit `3`, exit `4`, and exit `5`
- stop at `requires_approval`
- treat ZK proof verification as policy evidence, not execution permission
- keep x402 as paid ingress only
- report the ActionPlan hash, policy commitment, decision, reason, and Stellar
  verification status to the user

The skill should be short enough to load during agent work. Deep architecture,
hackathon artifacts, and operator runbooks should stay in linked docs.

## Raven Usage Rule

Raven is a helper for development context:

- use Raven to check current Stellar docs, ecosystem references, and playbooks
  when designing MCP, Skills, Soroban, x402, or ZK work
- use official Stellar docs directly when Raven is unavailable
- never place Raven in NeuroChain's runtime dependency chain
- never make guardrail decisions depend on Raven search results

The runtime must keep working from local code, local policy inputs, configured
Stellar endpoints, and explicit user/operator configuration.

## x402 Role

x402 belongs at the access boundary:

```text
paid request
  -> x402 verification/facilitation
  -> NeuroChain guardrail service access
  -> typed ActionPlan evaluation
  -> no automatic submit
```

x402 can decide whether an API call is paid. It must not decide whether an
ActionPlan can execute. The existing facilitator boundary should remain
fail-closed until real verify/settle transport, pricing, receiver config,
persistent replay state, and safe audit are implemented.

## ZK Role

ZK is the proof layer for private owner policy:

```text
typed ActionPlan
  -> private policy witness
  -> known NeuroChain evaluator
  -> public journal
  -> Soroban verification
  -> verifiable decision
```

The public artifact should continue to bind:

- evaluator image ID
- ActionPlan hash
- policy commitment and version
- decision
- exit code and reason code
- audit/nullifier ID

The private policy stays hidden. The output remains an attested decision, not
permission to submit the underlying ActionPlan.

## First Implementation Sequence

1. Publish this direction document and link it from the README.
2. Draft the MCP v0 tool contract as fixtures or documentation before adding a
   server. Done in [`docs/mcp_v0_tool_contract.md`](mcp_v0_tool_contract.md).
3. Draft the `NeuroChain Stellar Guardrails` skill as a small instruction
   package. Done in
   [`skills/neurochain-stellar-guardrails/SKILL.md`](../skills/neurochain-stellar-guardrails/SKILL.md).
4. Build a local read-only/no-submit MCP shim only after the contract is clear.
5. Keep `submit_testnet_attestation` separate and opt-in.
6. Finish x402 as optional paid ingress behind the existing fail-closed
   facilitator boundary.
7. Finish ZK status and artifact polish without weakening
   `underlying_action_submit_allowed=false`.

## Acceptance Checklist

Before calling MCP v0 or a community skill ready:

- The default path cannot sign, broadcast, or submit.
- `approved`, `requires_approval`, and `blocked` are distinct.
- Exit `3`, `4`, and `5` keep their current meanings.
- x402 payment does not bypass guardrails.
- ZK proof verification does not bypass approval.
- Raven is documented as a helper, not a runtime dependency.
- The README points users to the product direction, security model, and ZK
  architecture.
