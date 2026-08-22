# Offline @x402/stellar conformance preparation

Date: 2026-08-22

Status: versioned offline coverage plan plus approved exact-version package
bootstrap, supply-chain gate, upstream API smoke, and canonical offline
Stellar exact `/supported` conformance; no live runtime

## Outcome

`src/x402_stellar_conformance.rs` validates the machine-readable preparation plan in
`examples/x402_stellar_conformance/plan.json`. A result code of
`conformance_plan_ready` means only that the planned coverage is complete and
internally consistent. That historical plan result does not claim an
installed package, a conformant facilitator, or a settled payment.

The separately approved workspace at
`services/x402-stellar-facilitator/` now pins and installs the canonical
packages for offline conformance. Its lockfile, license inventory, and smoke
tests are the authoritative evidence for package-bootstrap state. They still
do not claim a listener, credential, signer, verification, settlement, or
network conformance.

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

## Approved package bootstrap boundary

The user approved this exact offline-only package boundary on 2026-08-22:

- workspace: `services/x402-stellar-facilitator/`;
- runtime dependencies: `@x402/stellar@2.23.0` and `@x402/core@2.23.0`;
- development dependencies: `typescript@5.9.3` and
  `@types/node@24.13.3`;
- package manager: `pnpm@11.19.0`; checked runtime: Node.js 24.19.0;
- deterministic lockfile installation with dependency install scripts
  disabled;
- a machine-readable 49-package closure: 42 MIT, 5 Apache-2.0 and 2
  BSD-3-Clause packages, with no AGPL/GPL/LGPL/SSPL or unknown runtime
  license;
- upstream API smoke imports `ExactStellarScheme` and `x402Facilitator`,
  checks their required prototype surface, and deliberately refuses to create
  a signer or call verify/settle.

The upstream package remains the owner of verify and settle, never the Rust
guardrail runtime. Credentials, a credential-backed signer, any
RPC/Horizon/facilitator call, live settlement, pubnet, deployment, wallet
signing, transaction submit, and ActionPlan submit remain separate approval
gates.

The offline `/supported` fixture registers `ExactStellarScheme` in the upstream
`x402Facilitator` for `stellar:testnet` and `stellar:pubnet`. It uses only the
canonical all-zero public Ed25519 address and an inert adapter whose signing
methods throw if invoked. `getSupported()` reads the public address and returns
`exact`, x402 version 2 and `areFeesSponsored: true` for both networks without
calling signing, verify, settle, RPC, Horizon, facilitator transport or submit.

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
- Package manifest and lockfile:
  `services/x402-stellar-facilitator/package.json` and `pnpm-lock.yaml`.
- Supply-chain gate and committed inventory:
  `services/x402-stellar-facilitator/scripts/check-supply-chain.mjs` and
  `supply-chain/dependency-license-inventory.json`.
- Offline upstream API smoke:
  `services/x402-stellar-facilitator/src/upstream-api-smoke.ts` and its Node
  built-in tests.
- Canonical `/supported` fixture and drift gate:
  `services/x402-stellar-facilitator/fixtures/supported-v2.expected.json`,
  `src/supported-conformance.ts` and its Node built-in tests.
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
