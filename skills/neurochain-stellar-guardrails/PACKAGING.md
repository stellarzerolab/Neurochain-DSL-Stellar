# NeuroChain Stellar Guardrails Skill Packaging

This file defines the publishable-skill readiness checklist for the
`neurochain-stellar-guardrails` skill. Packaging is Phase 2 work. It starts
after the MCP v0 host path is repeatable. Do not use this checklist to claim
that the skill is already published.

## Packaging Boundary

The skill is an instruction and distribution layer for agents. It is not:

- a NeuroChain runtime dependency
- a wallet, signer, broadcaster, or transaction submitter
- an x402 payment verifier
- a ZK prover or Soroban verifier
- a path around MCP v0 no-submit rules

The runtime boundary remains:

```text
MCP host -> neurochain-mcp-v0-stdio -> NeuroChain runtime -> no automatic submit
```

## Required Package Files

Before publication, the package should include:

- `SKILL.md`
- `agents/openai.yaml`
- this `PACKAGING.md`
- `INSTALL.md`
- `RELEASE_CANDIDATE.md`
- short examples in `examples/` for:
  - `approved`
  - `requires_approval`
  - `blocked`
  - `state_unavailable`

## Publication Readiness Checklist

- [ ] MCP v0 release gate passes with `validated_by_launch=true`.
- [ ] A real MCP host or approved host-like harness can launch the stdio
      server with absolute paths.
- [ ] The skill lists only the five default read-only MCP v0 tools.
      - `plan_stellar_action`
      - `evaluate_guardrails`
      - `prove_guardrail_decision`
      - `verify_zk_on_stellar`
      - `get_guardrail_status`
- [ ] The skill explicitly excludes `submit_testnet_attestation`,
      `consume_nullifier`, `submit_underlying_action`, `sign_transaction`, and
      `configure_server`.
- [ ] Every example preserves `underlying_action_submit_allowed=false`.
- [ ] `blocked` and `requires_approval` examples stop before proof, payment,
      verification, attestation, signing, or submit.
- [ ] ZK examples distinguish local proof binding from read-only Stellar
      verification.
- [ ] x402 wording says paid service access only, not guardrail approval,
      proof verification, or submit authority.
- [ ] Raven is mentioned only as development-time context, never as a runtime
      dependency.
- [ ] No wallet secret, seed phrase, API key, private key, source alias for
      signing, or hosted service token is included.

## First Publish Candidate

The first publish candidate should be a small agent-facing package:

```text
Plan -> Evaluate -> Prove -> Verify -> Status
```

It should not include optional testnet attestation submit, nullifier consume,
real x402 facilitator settlement transport, hosted admin actions, or
underlying Stellar ActionPlan execution.

Those can be documented later as separate product surfaces with their own
approval and security review.

## Stellar Skills Community Directory

The channel-specific publish review and proposed community-directory card live
outside the installed skill package:

- `docs/stellar_skills_publish_review.md`
- `distribution/stellar-skills-community-card.json`

This keeps directory metadata and publication procedure out of the skill's
runtime instructions. The card remains `published=false` evidence until the
skill branch is merged, its direct `SKILL.md` URL is verified, and an
explicit external-publication approval is given. Opening the external directory
pull request remains a separate explicit publication decision.
