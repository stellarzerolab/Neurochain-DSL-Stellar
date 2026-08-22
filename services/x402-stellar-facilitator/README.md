# NeuroChain Stellar x402 facilitator workspace

This private workspace is the offline conformance boundary for the planned
SCF x402 Facilitator with Bazaar delivery. It uses the canonical
`@x402/stellar` and `@x402/core` packages; it does not reimplement their
verification or settlement semantics.

## Current milestone

- exact direct dependency versions and deterministic `pnpm-lock.yaml`;
- machine-readable dependency and license inventory;
- offline import/build/smoke proving the upstream Stellar scheme and
  facilitator APIs are the integration owners;
- canonical in-process Stellar exact `/supported` conformance for
  `stellar:testnet` and `stellar:pubnet`, including
  `areFeesSponsored: true` and deterministic signer-family output;
- upstream `ExactStellarScheme.verify` rejection conformance for 13 malformed,
  mismatched or unsafe cases that fail before simulation, signing or network
  access;
- upstream `ExactStellarScheme.settle` rejection conformance for 12 invalid
  cases plus `x402Facilitator.onBeforeSettle` admission rejection for
  unverified, duplicate and replay states;
- pure in-process `/supported`, `/verify` and `/settle` handlers that preserve
  upstream results, map exceptions to stable fail-closed codes, and never
  create transport or execution authority;
- a strict TypeScript consumer for the same versioned NeuroChain
  `evaluation_request` and `evaluation_response` fixtures validated by Rust;
- Node built-in tests only.

## Authority boundary

This workspace currently has no HTTP or MCP listener, network adapter,
credential, keypair, credential-backed signer, wallet, settlement runtime,
service dispatch, RPC submit, transaction submit, or ActionPlan submit.
Importing an upstream API is not permission to invoke a live payment path.

The `/supported` fixture uses the canonical all-zero public Ed25519 address and
an inert signer adapter whose methods throw if called. No corresponding secret
or keypair exists in the workspace. The upstream response only reads the public
address; tests prove that no signer, verify, settle, or `fetch` call occurs.

The verify-rejection fixture calls the upstream Stellar scheme directly with
deterministic unsigned transaction envelopes. The fixture covers version,
scheme, network, malformed XDR, operation, source, asset, function, payer,
recipient and amount rejection. A blocked `fetch` sentinel and inert signer
prove that these cases stop before network access or signing. Auth-entry
structure, expiration, sub-invocations, signature status and custom
`__check_auth` remain explicitly `approval_blocked`: upstream validates those
only after RPC simulation and they are not emulated locally.

The settle-rejection fixture proves that invalid payloads return before any
network, signer or submit call. Separate upstream core admission hooks reject
unverified, duplicate and replay states before the Stellar scheme is invoked.
This is an in-memory contract fixture, not a production replay store or a live
settlement claim. The pure handler now maps unknown networks and
outcome-unknown states fail-closed and defines the persistent-state port;
production persistence and restart recovery remain `service_boundary_pending`.
Valid exact settlement, fee bump and canonical-client round trips remain
explicitly `approval_blocked`.

`src/service-handlers.ts` is a listener-free dependency-injected module. The
facilitator port owns `/supported`, `/verify` and `/settle` through upstream
`@x402/core` and `@x402/stellar`; the handler only validates the endpoint
envelope, preserves upstream response bodies and maps exceptions without
leaking raw diagnostics. Unknown networks fail closed before a facilitator
call. Every result carries an explicit all-false authority boundary.

Settlement additionally requires a `SettlementStatePort` reservation before
the upstream port can be called. No implementation is provided in this
milestone: without an atomic persistent adapter the handler returns
`settlement_state_unavailable`. Duplicate, replay, unverified and
outcome-unknown decisions remain stable service states, and an outcome that
cannot be durably recorded is never exposed as successful. This is an
interface contract, not a persistent database or settlement runtime.

The evaluation handler consumes the shared fixtures under
`examples/x402_service_boundary/`. TypeScript checks strict envelope,
decision/exit correlation, request correlation and all-false authority fields;
Rust remains the owner of typed `ActionPlan` construction and canonical hash
validation. No Rust transport or process adapter is activated here.

The separately approved follow-up milestones add only offline conformance.
Any credential, Stellar network call, signing, real settlement, long-lived
runtime, deployment, or submit still requires a new explicit approval.

## Local quality gate

After the approved first package installation:

```powershell
pnpm run check
```

The supply-chain check fails when direct versions drift, the lockfile is
missing, a runtime package has an unknown license, or the closure contains an
AGPL, GPL, LGPL, or SSPL license. Generated inventory is committed under
`supply-chain/`.

TypeScript remains strict for this workspace. `skipLibCheck` is limited to
upstream declaration files because the pinned Stellar SDK closure references
optional declaration packages that are not part of the approved direct
dependency set; no runtime validation or NeuroChain authority check is
bypassed.
