# Offline automatic Bazaar cataloging fixtures

These fixtures model the bounded handoff from a future `@x402/stellar`
facilitator service into the Rust Bazaar catalog. They are not raw
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
external `$ref` and `$id` values are always rejected. Production conformance
still requires the future TypeScript facilitator's full maintained validator.
