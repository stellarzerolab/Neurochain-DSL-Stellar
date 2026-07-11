# Public Demo Flow

This document defines the simple public walkthrough for NeuroChain after the
Stellar Real-World ZK submission. It keeps the first impression focused on the
current product direction:

```text
Plan -> Evaluate -> Prove -> Verify -> no automatic submit
```

The full CLI, REPL, `.nc`, API, x402, and local development references still
exist, but they should be treated as advanced documentation when presenting the
product.

## One Sentence

NeuroChain turns an agent, bot, script, scheduler, or backend automation request
into a typed Stellar ActionPlan, checks it against deterministic guardrails,
optionally proves the decision against private policy, verifies the proof on
Stellar, and still does not submit the underlying action automatically.

## Step 1: Plan

Start with a typed Stellar ActionPlan preview.

The demo should make clear that NeuroChain does not let free-form model output
touch signing or submit logic directly. The important output is structured:

- action label
- network
- source alias
- contract, function, asset, recipient, or amount fields when relevant
- preview logs
- submit boundary fields

This is the "what would happen" step, not execution.

## Step 2: Evaluate

Run deterministic guardrails against the ActionPlan.

The public story should show all three decision classes:

- `approved`
- `requires_approval`
- `blocked`

Keep the exit meanings stable:

- exit `3` = allowlist block
- exit `4` = contract or policy block
- exit `5` = missing, type, or confidence safety block
- exit `0` = passed

For the default public path, `requires_approval` and `blocked` remain terminal
no-submit states.

## Step 3: Prove

For ZK-enabled scenarios, show the guardrail proof artifact as evidence that a
known NeuroChain evaluator checked a typed ActionPlan against a private owner
policy.

The artifact should be described in public terms only:

- evaluator image ID
- ActionPlan hash
- policy commitment and version
- decision
- exit code and reason code
- audit or nullifier ID

Do not expose private policy rules, salts, secrets, seed phrases, private keys,
or wallet secret material.

## Step 4: Verify

Verify the proof on Stellar through the Soroban guardrail verifier path.

The default demonstration should prefer read-only verification because it
confirms the decision without changing replay/nullifier state. If a real testnet
transaction is shown, it must be an explicitly named follow-up such as
`submit_testnet_attestation`, not part of the default MCP v0 path.

The final status should preserve:

- `stellar_verification: verified_on_stellar`
- `attestation_submitted: false` for read-only verification
- `verification_transaction_submitted: false` for read-only verification
- `nullifier_consumed: false`
- `underlying_action_submit_allowed: false`

If an explicit testnet attestation is submitted for a demo video, the status may
show a transaction hash and `attestation_submitted: true`, but the underlying
ActionPlan must still remain unsubmitted.

## What To Show First

Use the shortest working path that proves the core idea:

1. Run a guided ZK demo and show `approved`, `requires_approval`, and `blocked`.
2. Show `zk status` after read-only Stellar verification.
3. Highlight the contract ID, ActionPlan hash, policy commitment, decision, and
   `underlying_action_submit_allowed: false`.
4. Optionally submit one explicit testnet attestation and open the transaction
   in a testnet explorer.
5. End by running `zk status` again and showing that proof evidence is not
   permission to execute the underlying action.

## What Stays Advanced

Move these behind advanced docs or secondary walkthroughs:

- raw CLI flag combinations
- full REPL command catalog
- `.nc` scripting reference
- API parity details
- localnet verifier build instructions
- RISC Zero build internals
- x402 challenge/finalize examples
- server deployment and operator runbooks
- nullifier consume flows
- any action that can sign, broadcast, or submit

## Default Boundary

The default public path is no-submit:

```text
payment success != submit permission
proof success != submit permission
Stellar verification != submit permission
testnet attestation != submit permission
```

Only a separately authorized execution path may ever submit an underlying
Stellar action.

## Related Docs

- [`docs/product_direction_mcp_skills.md`](product_direction_mcp_skills.md)
- [`docs/mcp_v0_tool_contract.md`](mcp_v0_tool_contract.md)
- [`docs/stellar_actions_guide.md`](stellar_actions_guide.md)
- [`docs/security.md`](security.md)
- [`hackathons/stellar-real-world-zk/ARCHITECTURE.md`](../hackathons/stellar-real-world-zk/ARCHITECTURE.md)
