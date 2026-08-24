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
- a pure upstream `extensions` to Rust automatic-cataloging adapter that
  consumes the shared HTTP/MCP/outcome fixtures and emits deterministic
  `EXTENSION-RESPONSES` base64 without re-verifying payment;
- pure resources/search handlers that consume the same list and three-page
  ranking/cursor fixtures as Rust while leaving catalog order, ranking, and
  cursor ownership behind an injected Rust port;
- a pure MCP search/paid-call parity adapter that consumes the shared Rust call
  and result fixtures, preserves canonical structured/text output, and leaves
  exact binding plus single-use access decisions behind the Rust port without
  dispatching the service call;
- a strict machine-readable 24-case readiness validator that separates
  verified offline evidence from persistent service-boundary work, approval-
  blocked live/security work, and upstream-blocked Stellar `upto` work;
- a default-off one-shot testnet harness plus a local-only atomic state adapter
  that records only request admission and strict public ledger evidence, blocks
  duplicate/replay/restart retries, and never stores credentials or payment
  material;
- a GitHub CI gate with exact Node/pnpm versions, frozen lockfile installation,
  dependency scripts disabled, supply-chain validation, strict typecheck,
  build, and Node built-in tests;
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
the upstream port can be called. No production implementation is provided:
without one the service handler returns `settlement_state_unavailable`.
Duplicate, replay, unverified and outcome-unknown decisions remain stable
service states, and an outcome that cannot be durably recorded is never
exposed as successful. The separate `LocalTestnetStateAdapter` implements only
the approved one-shot non-production harness port. It is not connected to the
service handler or a listener and is not a production database or settlement
runtime.

The local testnet adapter writes schema-v1 records beneath the ignored
`.local-testnet-state/` directory using exclusive per-request locks and
same-directory atomic replacement. Records contain only a request digest,
reservation identifier, admission/completion timestamps, state, and strict
public evidence. Credentials, opaque signer handles, payment payloads, auth
entries, signed XDR and raw upstream responses are rejected by construction.
An interrupted `attempted` record becomes terminal `outcome_unknown` on
restart; it is never retried automatically. Corrupt state, unknown schema,
unexpected files, path traversal, symlink roots, duplicate reservations and
replays all fail closed with stable non-empty codes.

The evaluation handler consumes the shared fixtures under
`examples/x402_service_boundary/`. TypeScript checks strict envelope,
decision/exit correlation, request correlation and all-false authority fields;
Rust remains the owner of typed `ActionPlan` construction and canonical hash
validation. No Rust transport or process adapter is activated here.

`src/bazaar-automatic-cataloging.ts` accepts only an upstream verify/settle
result's `extensions` field plus a strict public resource/payment summary. It
rejects raw payload, signer, settlement, and submit fields; bounds the Bazaar
`info` and `schema`; and delegates the actual schema/catalog outcome to an
injected Rust port. Missing metadata is dropped, malformed metadata is
rejected, and unavailable or malformed ports fail closed. Accepted, dropped,
invalid, duplicate, and unavailable outcomes use the shared Rust fixture
contract and deterministic `EXTENSION-RESPONSES` encoding. There is no catalog
database or runtime port implementation in this workspace.

`src/bazaar-resources-search.ts` validates strict list/search request
envelopes, stable Rust port outcomes, x402 v2 list items, offset pagination,
query-bound cursor envelopes, and `partialResults` correlation. The shared
`examples/x402_bazaar_catalog/search_pages.json` fixture proves exact ranking
and cursor parity in both languages. TypeScript does not implement a second
ranking engine, database, HTTP route, or search service; every result preserves
the all-false authority boundary.

`src/bazaar-mcp-paid-call.ts` validates strict MCP `tools/call` envelopes and
the existing Rust structured-result contracts. Search remains read-only.
Paid-call can preserve a Rust-issued `serviceCallAllowed=true` grant only for
the exact returned binding; all payment, proof, approval, settlement, signing,
underlying execution, wallet, RPC, transaction-submit and ActionPlan-submit
flags remain false. The module exposes no listener or dispatch method and does
not reproduce the Rust access gate or digest algorithm.

## Bounded testnet harness

`src/testnet-conformance-harness.ts` is the first checkpoint of the separately
approved non-production Stellar testnet phase. Its default request is a pure
offline plan: it validates the exact `stellar:testnet` CAIP-2 identifier,
official RPC/Horizon/Friendbot endpoint allowlist, pinned testnet USDC asset,
explicit recipient and fixed 0.01 test-token amount. It creates only a public
request digest and calls no state, credential, signer, network or submit port.

Execution requires all of the following at the same time: `execute=true`, the
exact bounded-testnet confirmation, an atomic state port, an ephemeral
credential port and a canonical-client port. No concrete implementations are
provided in this checkpoint. The credential uses a symbol-keyed opaque handle
that JSON cannot serialize, and both port exceptions and malformed evidence
are replaced with stable non-secret reasons. Only a strict public evidence
envelope may leave the canonical port.

This milestone performs no live testnet call and creates no keypair. A later
checkpoint may add only the explicitly approved dedicated testnet adapters.
Pubnet/mainnet, production or existing wallets, persistent secrets, custom
accounts, general transaction submission, service dispatch, underlying
execution and ActionPlan submit remain outside the boundary.

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

The current evidence status is checked in at
`examples/x402_stellar_conformance/readiness.json`. It grants no payment,
network, credential, signing, settlement, dispatch, transaction-submit, or
ActionPlan-submit authority. CI installation may fetch only the exact frozen
dependency closure; all conformance and readiness tests run without external
Stellar or x402 service calls.
