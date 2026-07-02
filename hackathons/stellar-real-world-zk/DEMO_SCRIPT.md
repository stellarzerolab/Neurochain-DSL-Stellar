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
- Soroban verifies the Groth16 receipt and checks that the owner authorized the
  policy commitment/version.
- Read-only verification and owner-only nullifier consumption are separate.

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

## 1:20-2:15 - CLI To Soroban Verification

If `deployments/testnet.json` exists and the hosted service is configured with
that contract, use the public CLI page and run:

```text
show setup
zk.demo blocked
zk.stellar.verify blocked
zk.stellar.verify approved
zk.stellar.verify requires_approval
```

Point out the transition from `required_on_stellar` in the local view to
`verified_on_stellar` in the Soroban result. Highlight these fields:

```text
mode: read_only_verification
authorized_private_policy: verified_on_stellar
decision: blocked
exit_code: 3
nullifier_consumed: false
verification_transaction_submitted: false
underlying_action_submit_allowed: false
```

Explain that the command is safe to repeat because it uses Soroban simulation
with `--send no`. Do not demonstrate `zk.stellar.consume` in the hosted REPL;
that command is intentionally disabled there.

If testnet evidence has not been explicitly deployed, use the complete localnet
path instead and say that it is Protocol 26 localnet evidence, not testnet.

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
read_only_verified=true
authorized_policy_version=9
exit_code=3
next_step=blocked
nullifier_consumed=true
replay=contract_error_3
invalid_proof=contract_error_2
soroban_localnet_e2e=true
```

Explain that the proof is cryptographically valid and the hidden policy was
authorized by its owner, but the proven decision is still a block. The
read-only call leaves the nullifier unused; the separate owner call consumes
it. Replay and a mutated proof are both rejected on-chain.

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
