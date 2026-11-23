# Product readiness and manual acceptance

This is an evidence map, not a production-readiness certificate. It separates
what the repository can demonstrate offline from historical network evidence
and a current live check. Test output, a proof, a payment state and permission
to execute are different things.

## Verification boundaries

| Surface | Evidence | What it establishes | What it does not establish |
| --- | --- | --- | --- |
| Whole-product offline quickstart | `tests/product_local_quickstart.rs` and versioned quickstart fixtures | Discovery, access-state fixture, typed ActionPlan, policy decision, local ZK binding and an exact capability gate; all 12 authority fields remain false. | New proof generation, cryptographic verification, live payment or service dispatch. |
| Genuine Groth16 verification offline | `hackathons/stellar-real-world-zk/soroban/tests/groth16_proof.rs` | Existing genuine proofs verify through the pinned verifier, including router paths for approved, requires-approval and blocked/exit-3 decisions, in the Soroban SDK test environment. | Availability or state of any deployed contract now; production signing, execution or submission permission. |
| Historical ZK testnet record | `hackathons/stellar-real-world-zk/deployments/testnet.json` | The 2026-07-02 record identifies a three-contract testnet deployment, pinned hashes and three recorded read-only verification results. Its public bindings can be checked against the current proof fixtures offline. | A current live check, protection from network resets, a new deployment or a production security audit. |
| x402 canonical client E2E | `examples/x402_stellar_conformance/readiness.json` | Canonical offline-reference evidence is checked, while `exact_canonical_client_e2e` remains `approval_blocked`. | An authorized successful live payment/verify/settlement E2E. The three historical `outcome_unknown` attempts are not retried or reinterpreted. |
| Full manual acceptance with Ville | The sequence below and a dated project-memory acceptance record | What Ville actually tried, understood and observed across the existing product surfaces. | Completion merely because automated tests or an earlier partial UX pass succeeded. |

The historical deployment record is schema version 1, network `testnet`,
generated at `2026-07-02T13:51:25.0071028Z`, from source commit
`12ef1d45dce4dfc34bb9a911cda90c18bd61420f`. Its verifier dependency is pinned to
`e8ff6ea202db195352c0141ecc533ff649393fe4`. The manifest's
`read_only_verified=true` fields describe that recorded run; they are not a
live health signal. `tests/zk_guardrail_contract.rs` locks its structure, dated metadata,
scenario set, safety fields and proof/journal bindings without making requests.

The ZK package also documents historical hosted verification and separate
testnet attestations in its [README](../hackathons/stellar-real-world-zk/README.md)
and [submission evidence](../hackathons/stellar-real-world-zk/SUBMISSION.md).
Those are historical claims, not fresh observations from this offline check.

The whole-product quickstart intentionally continues to report
`cryptographicallyVerified=false`, `stellarVerificationRequired=true` and
`local_binding_only_cryptographic_stellar_verify_not_run`. A passing separate
Groth16 test must not change those fields. That command does not perform the
separate cryptographic test itself.

## Repeat the local checks

Use cached locked dependencies. These commands do not download dependencies,
generate a proof, require a wallet, start a listener or contact Stellar:

```powershell
cargo test --offline --locked --test product_local_quickstart --test zk_guardrail_contract
cargo test --offline --locked --release --manifest-path hackathons/stellar-real-world-zk/soroban/Cargo.toml --test groth16_proof
```

If the required cache is missing, stop and report it. Do not silently turn an
offline check into a dependency download. These tests exercise existing public
proofs; they do not repeat the historical deployment or make a new receipt.

Dependency/security review remains separate. A successful root Cargo audit
does not cover every nested ZK lockfile. In particular the RISC Zero toolchain
and Soroban package have their own documented upstream advisory/warning scope;
see the [ZK package limitations](../hackathons/stellar-real-world-zk/README.md).
No production audit or underlying ActionPlan execution integration is claimed.

### Offline audit snapshot, 2026-11-03

These results used `cargo audit --no-fetch` and advisory database commit
`6420e392` dated 2026-08-27. The database was not refreshed: these are cached
findings, not a claim that all advisories through today's date were checked.

| Lockfile scope | Result with denied warnings | Recorded findings |
| --- | --- | --- |
| Root `Cargo.lock` | Pass with three documented ignores in this snapshot | `RUSTSEC-2024-0436`, `RUSTSEC-2025-0134`, `RUSTSEC-2026-0186` were exceptions in this run. |
| Soroban `Cargo.lock` | Fails the strict warning gate | Two unmaintained warnings: `derivative` / `RUSTSEC-2024-0388`, `paste` / `RUSTSEC-2024-0436`. No vulnerability advisory was reported by this cached scan. |
| RISC Zero `Cargo.lock` | Fails the strict vulnerability/warning gate | Three advisories: `rsa` / `RUSTSEC-2023-0071`, `ruint` / `RUSTSEC-2026-0220`, `tracing-subscriber` / `RUSTSEC-2025-0055`; three unmaintained warnings: `bincode`, `derivative`, `paste`. |

The `ruint` finding is present in the current locked RISC Zero dependency tree
and was missing from the earlier package summary. This does not by itself prove
an exploitable NeuroChain path. No dependency, lockfile or ignore was changed;
remediation and a refreshed advisory scan remain separate approval decisions.

## Manual acceptance with Ville

### Local walkthrough completed on 2026-11-21

The owner ran the human and JSON quickstarts, plan-only REPL and bundled ZK
examples, one-shot CLI, `.nc` example, MCP stdio smoke client, and Bazaar/x402
reference path. The REPL, CLI and `.nc` runs produced the same 5 XLM payment
plan. The reference paths produced the expected three decisions and gate calls
`1/0/0`, with service dispatch disabled.

Codex ran the three HTTP plan-only requests with the owner's approval on a
temporary `127.0.0.1` listener. The API returned the same plan with
`approved`, `requires_approval`, and `blocked` (allowlist code 3).
`underlying_action_submit_allowed` remained `false` in all three responses.
The listener was closed and its port was verified free of listeners.

The local technical checks passed. The owner accepted the local walkthrough
on 2026-11-22 and deferred independent developer testing. The walkthrough
established local ZK binding and fixture behavior.
Live Stellar verification, live x402 settlement and ActionPlan execution
remain outside its scope.

### Walkthrough procedure

The first pass is the whole existing product, not only a new isolated example.
Record each step as passed, failed, not run or approval-blocked, with the actual
output and Ville's usability observations. Do not mark a live or interactive
step passed from an automated fixture test.

1. Read the root README, then run the human-readable quickstart and compare its
   machine-readable view. Explain the three decisions, gate calls `1/0/0` and
   `Authority boundary: ALL FALSE` from the output.
2. Start the plan-only REPL, inspect `help` and `help all`, and enter a plain-text
   intent. Zero arguments and explicit `--repl` are plan-only; `--flow` is an
   advanced opt-in, not part of this first pass.
3. In that REPL run `zk.demo approved`, `zk.demo requires_approval`,
   `zk.demo blocked` and `zk status`. Compare the guidance and local-binding
   boundary. Do not replace these with `zk.stellar.verify` or an attestation.
4. Repeat planning through one-shot CLI and a checked-in `.nc` example from the
   [Stellar actions guide](stellar_actions_guide.md), without `--flow`. Compare
   typed fields, decisions and exit/reason semantics where the surfaces expose
   them. `--yes` is not a test shortcut.
5. Review the existing [MCP/API roles and contracts](product_surface_inventory.md).
   Include the read-only/no-submit MCP stdio surface in the walkthrough. An API
   in-process test is useful evidence, but is not a manual HTTP-server test:
   starting an HTTP/MCP listener needs separate approval.
6. Review the x402/Bazaar approved, requires-approval and blocked fixture paths
   in the same coordinator. Payment/proof/approval must not enable service
   dispatch, signer, wallet, shell, RPC, transaction or ActionPlan submission.
7. List remaining advanced/live steps explicitly. A current ZK network check,
   new proof generation requiring toolchain bootstrap, credentials, wallet use,
   signing, payment, settlement, attestation, consume, execution, listeners and
   deployment retain their own approval gates. Whole-product coverage does not
   grant those permissions.

The first command for Ville is still the existing entry point:

```powershell
cargo run --offline --locked --quiet --example product_local_quickstart -- --human
```

Fix observed friction, then repeat the affected step and its regression tests.
An independent developer's usefulness assessment comes after Ville's own
acceptance round; it is not a prerequisite for starting this walkthrough.
