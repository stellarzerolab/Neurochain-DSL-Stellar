# Local x402 non-bypass reference path

Date: 2026-08-27

Status: complete offline reference composition; no listener, payment,
settlement, service dispatch, signing, execution, or submit path

## Product path

`src/x402_local_reference_path.rs` composes the already versioned boundaries
into one local path:

`Bazaar discovery -> x402 access state -> typed ActionPlan -> deterministic policy -> approved/blocked -> exact capability gate`

The implementation does not add a workflow engine or a second guardrail
language. `BazaarCatalog` and `search_stellar_bazaar` own discovery, a trusted
read-only port reports the current x402 access state, the trusted NeuroChain
evaluation port owns typed planning and deterministic policy, and the existing
`BazaarPaidCallAccessGate` is the only component that can atomically consume a
single exact settled-access grant.

The public reference request contains only discovery arguments, the bounded
evaluation request, and exact paid-call arguments. It cannot provide an
ActionPlan, an evaluation response, a policy override, settled state, or a
capability decision. The three inputs are bound to the same request identifier,
catalog resource key, intent text, and Stellar network before the trusted
evaluation port is called.

## Non-bypass order

The coordinator enforces these stages in order:

1. Strictly validate the versioned request and run read-only local Bazaar
   discovery.
2. Preflight the existing paid-call contract without an access gate. Only the
   expected `access_gate_unavailable` result proves that the exact cataloged
   call is structurally valid; no access is consumed.
3. Bind the discovery result, resource key, request identifier, intent text,
   service arguments, and testnet network.
4. Inspect trusted x402 access state. Any state other than
   `settled_access_ready` stops before planning or policy evaluation.
5. Ask the trusted NeuroChain port for a typed ActionPlan and deterministic
   policy decision, then validate the canonical ActionPlan hash, decision/exit
   contract, all-false authority grants, and matching request identifier.
6. An approved decision may reach the exact single-use capability gate. A
   `requires_approval` or blocked result never calls or consumes that gate.

The approved fixture reaches only `service_call_authorized`. That capability
can release the exact evaluated service call, but the module performs no dispatch.
A policy-blocked fixture remains terminal at exit `4` even when the
read-only access-state port reports settled access ready. A replayed, missing,
or unavailable exact capability also remains denied after approval.

## Authority boundary

The reference result keeps all of these false in both approved and blocked
scenarios:

- payment, proof, approval, and settlement authority;
- signing, wallet, and shell access;
- underlying execution and service dispatch;
- RPC submit and ActionPlan-submit.

The paid-call result may set only `serviceCallAllowed=true` after atomic exact
access consumption. It never turns payment, proof, policy approval, an
ActionPlan, or a ZK artifact into execution or submit authority. The local
coordinator exposes no wallet, signer, RPC client, transaction builder,
dispatcher, listener, or submit function, so an agent has no parallel route
around the capability gate.

## Evidence and limits

`tests/x402_local_reference_path.rs` runs one approved and one exit-4 blocked
scenario through the same coordinator. It also covers request binding
tampering, evaluation authority escalation, unsettled access, replay denial,
and the invariant that blocked policy never touches the capability gate. The
Node built-in parity test
`services/x402-stellar-facilitator/test/x402-local-reference-path.test.ts`
feeds the same fixtures through the existing TypeScript Bazaar and evaluation
adapters and locks the same ordering and no-dispatch result. Versioned fixtures
live in `examples/x402_local_reference_path/`.

The external-developer quickstart and role ownership table are documented in
`docs/x402_local_reference_quickstart.md`. Its single offline integration
command is:

```bash
cargo run --offline --quiet --example x402_local_reference_path
```

The command uses this coordinator directly, runs the same approved and blocked
fixtures, and checks the no-dispatch, wallet, RPC, and ActionPlan-submit
boundary before emitting its deterministic JSON report.

This is offline integration evidence, not live x402 verification or
settlement evidence. It does not modify the existing intent, policy, flow,
submit, or exit-code semantics. Network access, real service dispatch,
credentials, payment, settlement, signing, transaction submission,
ActionPlan-submit, persistent production state, and pubnet/mainnet remain
separate explicit approval gates.
