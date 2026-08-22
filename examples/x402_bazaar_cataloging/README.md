# Offline automatic Bazaar cataloging fixtures

These fixtures model the bounded handoff from the `@x402/stellar` service
boundary into the Rust Bazaar catalog. Both the Rust tests and the pure
TypeScript parity adapter consume them. They are not raw
`PaymentPayload` values and contain no signatures, credentials, settlement
capability, wallet access, or ActionPlan-submit authority.

- `automatic_http.json` validates and catalogs an HTTP discovery extension
  with a canonical `routeTemplate`.
- `automatic_mcp.json` validates and catalogs an MCP tool keyed by resource URL
  plus `toolName`.
- `outcome_contract.json` locks the stable internal disposition/code mapping
  and the compatible x402 `EXTENSION-RESPONSES` status.

The offline validator implements a deliberately bounded Draft 2020-12
profile. Unsupported keywords fail closed as `schema_profile_unavailable`;
external `$ref` and `$id` values are always rejected. The TypeScript adapter
adds strict envelope/size checks and delegates schema/catalog semantics to the
Rust port; production conformance still requires a maintained full Draft
2020-12 validator and a separately approved runtime integration.
