# Offline x402 local reference quickstart

This quickstart packages the existing in-process reference path for an external
developer. It uses only the checked-in versioned fixtures and the existing
`run_x402_local_reference_path` coordinator:

`Bazaar discovery -> x402 access state -> typed ActionPlan -> deterministic policy -> approved/requires_approval/blocked -> exact capability gate`

## Run the integration path

Prerequisites are a Rust toolchain compatible with the repository and an
already provisioned Cargo cache. From the repository root, run:

```bash
cargo run --offline --quiet --example x402_local_reference_path
```

`--offline` makes Cargo fail instead of contacting a registry. The example
itself has no credential, keypair, environment-secret, network client,
listener, persistent store, signer, wallet, dispatcher, RPC client, or submit
function. It reads its fixtures at compile time and prints one deterministic
JSON report. A fixture, binding, ordering, outcome, capability-use, or
authority mismatch fails the command with a non-zero exit.

The expected report is versioned at
`examples/x402_local_reference_path/quickstart_output.json`. It contains one
approved, one terminal `requires_approval`, and one exit-4 blocked scenario.
All three pass through the same coordinator.

## Roles and ownership

| Role | Owns | Does not receive |
| --- | --- | --- |
| External agent or client | Discovery query, bounded intent, exact call arguments | ActionPlan choice, policy override, settled-state assertion, capability decision |
| Bazaar and x402 access layer | Bazaar discovery, paid resource identity, read-only access state | NeuroChain policy ownership, signing, execution, dispatch, submit |
| NeuroChain evaluation port | typed ActionPlan, canonical plan hash, deterministic policy decision | Payment or settlement authority, wallet/signer access, service dispatch |
| Exact capability gate | Atomic single-use release of the already bound service call after approval | General execution, dispatch, wallet, shell, RPC, transaction-submit, ActionPlan-submit |
| Host application/operator | Any separately approved future dispatch or execution integration | Implicit authority from payment, proof, approval, or this quickstart |

The quickstart uses fixture-only trusted ports: access is reported as ready,
the versioned NeuroChain evaluation response is returned once, and the exact
capability gate records whether it was called. The approved case calls it once
and exposes only `serviceCallAllowed=true`; the example performs no dispatch.
The `requires_approval` case remains terminal with no exit code, records
`approval_required`, and never calls or consumes the gate. The blocked case
records exit `4` and likewise leaves the gate untouched.

## Machine-checkable non-bypass result

For all three scenarios the `authority` object keeps all eleven fields false,
including payment, proof, approval, settlement, signing, underlying execution,
service dispatch, wallet, shell, RPC-submit, and ActionPlan-submit. The output
also records `networkRequired=false`, `credentialRequired=false`, and
`offline=true`.

This is canonical local integration evidence only. It is not live x402 E2E,
payment, settlement, signing, service dispatch, transaction submission, or a
claim that Stellar `upto` exists upstream.
