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

`schema.json` locks the strict Draft 2020-12 envelope. The Rust validator adds
the cross-field source, dependency, case-status, evidence, and exact-coverage
checks that the structural schema cannot express concisely.

This directory contains no package install, credentials, payment payload,
wallet signing, settlement, network call, transaction submission, service
dispatch, or ActionPlan submit. A ready plan grants no authority and does not
replace the canonical `@x402/stellar` package or the upstream x402 E2E suite.
