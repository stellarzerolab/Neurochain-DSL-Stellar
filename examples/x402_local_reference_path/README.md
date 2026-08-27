# Local x402 non-bypass reference path

These versioned fixtures exercise one in-process product path:

`Bazaar discovery -> x402 access state -> typed ActionPlan -> deterministic policy -> approved/blocked -> exact capability gate`

The JSON request cannot provide its own ActionPlan, policy decision, payment
state, or capability. Tests inject a trusted read-only access-state port and a
trusted NeuroChain evaluation port. Only the existing single-use
`BazaarPaidCallAccessGate` can consume exact settled access after an approved
policy result.

The approved scenario reaches `service_call_authorized`; this releases only
the exact already-evaluated service call. There is no dispatch of the service or
grant payment, proof, approval, settlement, signing, underlying execution,
wallet, shell, RPC-submit, or ActionPlan-submit authority. The blocked
scenario proves that an exit-4 contract-policy decision never calls or
consumes the capability gate even when the read-only access state says that
settled access is ready.

Everything here is offline and local. The fixtures contain no credential,
signature, payment payload, transaction envelope, network command, listener,
service dispatch, or submit path.

From the repository root, an external developer can run both scenarios through
the existing coordinator with one network-disabled command:

```bash
cargo run --offline --quiet --example x402_local_reference_path
```

The command compares the actual result with the manifest expectations and
prints the machine-checkable shape locked by `quickstart_output.json`. The
approved scenario reaches the exact capability gate once with no dispatch. The
blocked scenario leaves that gate untouched. All signing, underlying
execution, service-dispatch, wallet, shell, RPC-submit, and ActionPlan-submit
fields remain false.
