# Offline @x402/stellar conformance fixtures

`plan.json` is a machine-readable preparation gate, not evidence that a
facilitator is live or conformant. A ready result does not claim package,
network, payment, or settlement conformance. It pins the official source snapshot,
requires `@x402/stellar` to retain verify/settle ownership, and separates
offline-ready cases from approval-blocked live cases and upstream-blocked
Stellar `upto` work.

`adversarial_patches.json` proves that protocol or source drift, premature
runtime claims, missing coverage, duplicate cases, premature `upto` support,
and live execution without approval fail closed.

`readiness.json` is the current machine-readable evidence status for the same
24 ids. It records 9 `verified_offline`, 2 `service_boundary_pending`, 11
`approval_blocked`, and 2 `upstream_blocked` cases. Its evidence references are
bounded repository-relative paths, and every runtime authority remains false.
`readiness_adversarial_patches.json` proves that live, persistent, or `upto`
claims cannot be promoted by changing the status text; package, summary,
evidence-path, and authority drift also fail closed in the TypeScript gate.

`schema.json` locks the strict Draft 2020-12 envelope. The Rust validator adds
the cross-field source, dependency, case-status, evidence, and exact-coverage
checks that the structural schema cannot express concisely.

This directory contains no package install, credentials, payment payload,
wallet signing, settlement, network call, transaction submission, service
dispatch, or ActionPlan submit. A ready plan grants no authority and does not
replace the canonical `@x402/stellar` package or the upstream x402 E2E suite.
