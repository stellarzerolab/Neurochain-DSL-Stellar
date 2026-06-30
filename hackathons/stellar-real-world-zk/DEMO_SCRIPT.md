# Demo Script (2-3 minutes)

## Before recording

- Open this package README and [ARCHITECTURE.md](ARCHITECTURE.md).
- Open a terminal at the repository root.
- Confirm no standalone localnet container is already using port `8000`.
- Do not show local environment files, identities or wallet material.
- Use the public fixtures for explanation; private policy values are not shown.
- Rehearse the proof-only recording path once from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/run_demo_rehearsal.ps1
```

## 0:00-0:25 - Problem and promise

Say:

> AI agents can prepare Stellar actions, but owners should not have to publish
> their safety policies or trust a server that merely claims an action was
> checked. NeuroChain proves that a known guardrail evaluator ran correctly
> against a private policy, and Soroban verifies the result.

Show the title and one-line description in [SUBMISSION.md](SUBMISSION.md).

## 0:25-0:55 - Architecture

Show the proof flow in [ARCHITECTURE.md](ARCHITECTURE.md).

Explain:

- The typed ActionPlan is public.
- The policy and audit nonce are private RISC Zero inputs.
- The receipt journal reveals commitments, decision, exit/reason and nullifier.
- Soroban verifies the Groth16 receipt and consumes the nullifier.

## 0:55-1:20 - Genuine proof matrix

Run the proof-only rehearsal command shown above. Point out that its readiness
stage runs:

```powershell
cargo test --manifest-path hackathons/stellar-real-world-zk/soroban/Cargo.toml --test groth16_proof
```

Point out that the test uses the pinned real verifier and covers:

- approved
- requires approval
- private allowlist block, exit `3`.

## 1:20-2:15 - Stellar localnet verification

For a live full localnet take without fetches or image pulls during recording,
run:

```powershell
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/run_demo_rehearsal.ps1 -IncludeLocalnet -OfflineLocalnet
```

Offline mode fails closed unless the pinned verifier commit, Cargo dependencies
and Quickstart image are already cached. Otherwise show the previously recorded
successful output from the direct localnet runner.

Highlight these lines:

```text
localnet_protocol=26
offline_mode=true
decision=blocked_allowlist
exit_code=3
next_step=blocked
nullifier_consumed=true
replay=contract_error_3
invalid_proof=contract_error_2
soroban_localnet_e2e=true
```

Explain that the proof is cryptographically valid, but the proven policy
decision is still a block. Then note that replay and a mutated proof are both
rejected on-chain.

## 2:15-2:40 - Privacy and safety boundary

Show the public fixture
`fixtures/groth16_blocked_exit_3.json`, its schema version and four public proof
fields. Do not scroll through the whole seal.

Say:

> The public artifact contains the seal, evaluator image ID, canonical journal
> and journal digest. It does not contain the private allowlist, salt or audit
> nonce. A valid payment or proof is never submit permission, and
> requires-approval remains a no-submit state.

## 2:40-3:00 - Close

Say:

> NeuroChain is a private-policy safety attestation layer for autonomous
> Stellar agents. It proves the entire deterministic guardrail decision, not
> just a hidden-number comparison.

End on the three demonstrated outcomes in [SUBMISSION.md](SUBMISSION.md).

## Recording fallback

If the standalone localnet startup is too slow for a clean take, record its
successful output in advance and run the fast Soroban genuine-proof test live.
Do not replace either with a mock-verifier claim.
