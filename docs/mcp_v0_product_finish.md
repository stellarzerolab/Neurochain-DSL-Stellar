# MCP V0 Product Finish

This document defines the final product-packaging phase for NeuroChain MCP v0.
It is not a request to add submit tools. It is the last mile that turns the
working read-only guardrail runtime into a repeatable package for hosts,
agents, scripts, and backend automations.

## Phase Model

```text
Phase 1: Finish MCP v0 product package
Phase 2: Package NeuroChain Stellar Guardrails as a publishable Skill
Phase 3: Attach real x402 facilitator behind the existing boundary
```

Phase 1 is the current focus. Phase 2 and Phase 3 are separate follow-up work.

## Phase 1: MCP V0 Product Package

Goal:

```text
Plan -> Evaluate -> Prove -> Verify -> Status
```

The default path must remain read-only and no-submit.

Phase 1 is complete when:

- the MCP stdio release can be launched by a normal host configuration
- host setup examples are clear for Windows and POSIX callers
- the five default tools are described as runtime-backed, not fixture-backed
- `get_guardrail_status` explains the host's `latest_result` responsibility
- ZK proof artifacts and status fields are understandable without reading the
  whole hackathon architecture document
- x402 is documented as paid ingress only
- `submit_testnet_attestation`, nullifier consume, wallet signing, and
  underlying ActionPlan submit are documented as outside the default MCP path
- the release/conformance gate proves no default tool can sign, broadcast,
  submit, consume a nullifier, or create an attestation transaction

## Phase 2: Publishable Skill Package

The skill should be packaged only after Phase 1 is stable enough for a host to
run the MCP sequence.

Phase 2 should include:

- final skill metadata
- compact agent instructions
- install/use notes
- examples for approved, requires approval, blocked, and unavailable states
- a publishing checklist

The skill remains an instruction and distribution layer. It must not become a
runtime dependency or a path around MCP v0.

## Phase 3: Real x402 Facilitator

x402 is more than a UI idea now: the gateway, response contract, schema,
TypeScript types, viewer, audit path, replay store, mock verifier fence, and
fail-closed facilitator boundary already exist.

It is still not production x402 until real facilitator verify/settle transport
is attached behind `src/x402_facilitator.rs`.

Phase 3 should add, in a separate review:

- real facilitator verify/settle transport
- production pricing and receiver configuration
- persistent replay state policy
- safe audit events for facilitator outcomes
- tests proving x402 payment never bypasses guardrails, ZK verification,
  approval, or submit boundaries

## ZK Status

ZK is past the "lite" stage. The core already includes:

- a real RISC Zero guest
- genuine Groth16 fixture proofs
- Soroban verifier/router integration
- tamper rejection
- replay rejection
- hosted CLI demo flow
- manifest-path release tests for the Soroban package

The remaining ZK work is product polish:

- clearer artifact/status naming for agents
- a short explanation of local binding versus cryptographic verification
- explicit separation between read-only verification and testnet attestation
- continued `underlying_action_submit_allowed=false` in MCP and API views

## Current Readiness Estimate

These are planning estimates, not release claims:

| Area | Status |
| --- | --- |
| MCP v0 core | about 85% |
| ZK core | about 90% |
| x402 production | about 65% |
| publishable skill | about 45% |

## Guardrail Invariants

Do not weaken these while finishing the package:

- proof is not submit permission
- payment is not submit permission
- read-only Stellar verification leaves state unchanged
- `requires_approval` is terminal for MCP v0
- `blocked` is terminal for MCP v0
- exit `3`, `4`, and `5` keep their existing meanings
- the default MCP tool list excludes submit, signing, attestation submit,
  nullifier consume, and server administration

## Next Small Steps

1. Keep the release conformance port as the main no-submit proof.
2. Add an external host or MCP Inspector run when an approved host is available.
3. Improve the ZK artifact/status wording in MCP docs and skill examples.
4. Package the skill only after the host path is proven.
5. Add real x402 facilitator support only as Phase 3.
