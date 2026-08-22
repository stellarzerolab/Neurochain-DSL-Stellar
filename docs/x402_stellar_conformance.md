# Offline @x402/stellar conformance preparation

Date: 2026-08-22

Status: versioned offline coverage plan, source-drift gate, adversarial
fixtures, and fail-closed tests; no package install or runtime approval

## Outcome

`src/x402_stellar_conformance.rs` validates the machine-readable plan in
`examples/x402_stellar_conformance/plan.json`. A result code of
`conformance_plan_ready` means only that the planned coverage is complete and
internally consistent. It does not claim that `@x402/stellar` is installed,
that a facilitator is conformant, or that any payment settled.

The plan prepares the future official-package conformance run without
reimplementing payment verification or settlement in Rust. The upstream
`@x402/stellar` package remains the owner of verify/settle semantics. This
module only checks coverage, source assumptions, approval gates, required
evidence types, and the no authority boundary.

## Pinned source snapshot

The 2026-08-22 snapshot records these current facts:

- x402 protocol v2 and the standard `/supported`, `/verify`, and `/settle`
  surface are the target;
- the only Stellar CAIP-2 identifiers are `stellar:testnet` and
  `stellar:pubnet`;
- the network-specific Stellar `exact` specification exists and requires the
  v2 `payload: {transaction}` wire shape, SEP-41 `transfer`, signed Soroban
  address credentials, strict asset/amount/recipient binding, bounded auth
  expiration, no sub-invocations, current-ledger simulation, sponsored fees,
  and non-custodial settlement;
- the RFP additionally requires classic keypairs, custom `__check_auth`
  accounts, any SEP-41 token, seven-decimal amounts, trustline failures,
  replay/front-running resistance, non-empty rejection reasons, canonical
  client E2E, both-network transaction hashes, observability, and independent
  security review;
- the generic upstream `upto` specification exists, but no network-specific
  Stellar `upto` specification was present in the checked upstream tree.

If the protocol version, canonical sources, network identifiers, package
license, or Stellar `upto` status changes, validation returns
`spec_drift_detected` or `invalid_dependency_boundary`. A new Stellar `upto`
spec is intentionally treated as drift that requires review, not as automatic
permission to advertise or run it.

## Coverage states

The 24 required cases use three states:

- `ready`: an offline fixture, schema, reason, metrics, audit, or drift case
  can be prepared without package/runtime authority;
- `approval_blocked`: canonical-client network tests, transaction hashes,
  actual replay/trustline behavior, and an external review require a later
  explicit gate;
- `upstream_blocked`: Stellar `upto` remains a specification and upstream
  implementation contribution rather than a locally claimed feature.

Every case covers both Stellar networks. The matrix includes the standard
surface and wire shape, fee sponsorship, G- and C-account auth, SEP-41 and
seven decimals, tamper/call/expiration/replay/auth-structure/simulation/
trustline failures, non-custody, non-null reasons, `upto`, spec drift,
observability/audit, and independent review.

## Dependency and runtime proposal boundary

No dependency is installed by this milestone. The plan records the following
future decision, which requires the user's explicit approval:

- package: `@x402/stellar` under Apache-2.0;
- owner: upstream package for verify and settle, never the Rust guardrail
  runtime;
- runtime family: the official TypeScript SDK and its compatible
  `@x402/core`/transport packages, with exact stable versions selected and
  license-checked at the approval turn;
- workspace/runtime: a separate facilitator/Bazaar service boundary; the SDF
  reference repository currently documents Node.js 22+ and pnpm 10+;
- first execution: offline package tests only, followed by separately approved
  testnet canonical-client work;
- still separately gated: credentials, a live settlement, pubnet, deploy,
  wallet signing, and any transaction or ActionPlan submit.

Version selection is deliberately `approval_required`: copying a version from
an older issue would defeat the drift-control purpose. No lockfile,
`package.json`, Node workspace, or runtime process is created here.

## Authority boundary

The validation report always keeps payment verification, payment settlement,
wallet signing, network access, service dispatch, RPC submit, and ActionPlan
submit false. Conformance fixtures are evidence plans, not capabilities.

The fixtures contain no credential, payment payload, signed auth entry,
transaction XDR, private key, wallet operation, settlement, network command,
or service dispatch. Rust does not inspect or approve an actual payment and
does not replace the canonical package or upstream E2E suite.

## Evidence

- Plan: `examples/x402_stellar_conformance/plan.json`.
- Structural schema: `examples/x402_stellar_conformance/schema.json`.
- Drift/adversarial mutations:
  `examples/x402_stellar_conformance/adversarial_patches.json`.
- Tests: `tests/x402_stellar_conformance.rs`.
- Stable result codes include `conformance_plan_ready`,
  `spec_drift_detected`, `invalid_dependency_boundary`,
  `missing_conformance_case`, `duplicate_conformance_case`, and
  `conformance_case_mismatch`.

## Primary sources

- SCF x402 Facilitator with Bazaar RFP:
  <https://github.com/stellar/scf-handbook/blob/main/scf-awards/build-award/rfp-track.md#x402-facilitator-with-bazaar-discovery-support>
- Stellar x402 documentation:
  <https://developers.stellar.org/docs/build/agentic-payments/x402>
- x402 exact on Stellar specification:
  <https://github.com/x402-foundation/x402/blob/main/specs/schemes/exact/scheme_exact_stellar.md>
- Generic x402 `upto` specification:
  <https://github.com/x402-foundation/x402/blob/main/specs/schemes/upto/scheme_upto.md>
- x402 protocol and TypeScript packages:
  <https://github.com/x402-foundation/x402>
- SDF Stellar x402 reference repository:
  <https://github.com/stellar/x402-stellar>
