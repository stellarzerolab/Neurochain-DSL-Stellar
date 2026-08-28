# Product local quickstart fixtures

These schema-version-1 fixtures drive the one-command, fully offline product
reference path documented in `docs/product_local_quickstart.md`.

The three scenarios reuse one local Bazaar catalog entry, one typed contract
invocation and existing Groth16 proof artifacts. Only the deterministic policy
decision differs: `approved`, `requires_approval`, or `blocked`.

`quickstart_output.json` is the machine-checkable expected report. Its
`cryptographicallyVerified: false` and
`stellarVerificationRequired: true` fields are intentional: this quickstart
validates the public proof journal and exact ActionPlan projection locally, but
does not perform Stellar cryptographic verification.

Run from the repository root:

```powershell
cargo run --offline --quiet --example product_local_quickstart
```

No credential, network, listener, payment, settlement, signing, dispatch,
execution or submit authority is used.
