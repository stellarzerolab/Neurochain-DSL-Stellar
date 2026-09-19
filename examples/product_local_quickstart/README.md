# Product local quickstart fixtures

These schema-version-1 fixtures drive the product path documented in
`docs/product_local_quickstart.md`. The product path itself is fully offline
once its locked Rust dependencies are in the local Cargo cache.

The three scenarios reuse one local Bazaar catalog entry, one typed contract
invocation and existing Groth16 proof artifacts. Only the deterministic policy
decision differs: `approved`, `requires_approval`, or `blocked`.

`quickstart_output.json` is the machine-checkable expected report.
`quickstart_output.txt` is the exact human-readable `--human` view. The JSON
report's
`cryptographicallyVerified: false` and
`stellarVerificationRequired: true` fields are intentional: this quickstart
validates the public proof journal and exact ActionPlan projection locally, but
does not perform Stellar cryptographic verification.

On a fresh machine with an empty Cargo cache, populate the locked dependencies
once:

```powershell
cargo fetch --locked
```

That dependency bootstrap may use the network. It is a Cargo setup step, not a
NeuroChain product call; skip it when the dependencies are already cached.
Then run from the repository root:

```powershell
cargo run --offline --locked --quiet --example product_local_quickstart -- --human
```

Omit `-- --human` for the unchanged machine-readable JSON output.
Use `-- --help` to discover both outputs without running the product path; its
exact successful output is locked in `quickstart_help.txt`.

No credential, network, listener, payment, settlement, signing, dispatch,
execution or submit authority is used.
