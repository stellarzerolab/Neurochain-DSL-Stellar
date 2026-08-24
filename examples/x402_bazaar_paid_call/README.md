# Offline Stellar Bazaar paid-call fixtures

These fixtures define an offline, single-use paid-call contract for one exact
cataloged MCP service call. The untrusted MCP request carries only a request
identifier, canonical resource key, and bounded service arguments. It cannot
self-assert payment, settled state, wallet access, or any authority.

The future trusted x402 runtime must atomically consume settled access bound to
the exact resource, named tool, payment terms, and service-argument digest.
Only that outcome sets `serviceCallAllowed=true`. Payment, proof, approval,
settlement, signing, underlying execution, wallet, shell, RPC submit, and
ActionPlan submit remain false.

This milestone performs no dispatch, network request, payment, settlement,
signing, wallet operation, or ActionPlan submit. `paid_call.json` is an MCP
wire input fixture and `outcome_contract.json` locks the stable fail-closed
codes and retryability rules.

`authorized_result.json` and `replay_result.json` are shared Rust/TypeScript
MCP result fixtures. They lock the exact call binding, canonical digests,
single-use replay outcome, text/structured parity, and the rule that the one
successful service-call grant never becomes dispatch, signing, settlement, or
submit authority.
