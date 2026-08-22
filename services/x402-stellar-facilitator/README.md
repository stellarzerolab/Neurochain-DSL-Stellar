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
- Node built-in tests only.

## Authority boundary

This workspace currently has no HTTP or MCP listener, network adapter,
credential, keypair, signer, wallet, settlement runtime, service dispatch, RPC
submit, transaction submit, or ActionPlan submit. Importing an upstream API is
not permission to invoke a live payment path.

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
