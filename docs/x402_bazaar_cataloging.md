# Offline x402 Bazaar automatic cataloging

Date: 2026-08-22

Status: bounded offline adapter and outcome contract; no facilitator or HTTP runtime

## Purpose and authority boundary

`src/x402_bazaar_cataloging.rs` connects an already verified facilitator-side
discovery handoff to the existing offline Bazaar catalog. The adapter does not receive a raw `PaymentPayload`, payment signature, credential, wallet handle,
or transaction. It does not verify or settle a payment and does not grant payment, settlement, signing, execution, or ActionPlan-submit authority.

The future TypeScript `@x402/stellar` service remains responsible for the
standard facilitator protocol and for proving that the handoff occurs only
after the intended verify/settle policy. Rust receives only:

- the resource descriptor and already extracted payment summary;
- the Bazaar `info`, `schema`, and optional `routeTemplate`;
- an observation timestamp supplied by the future catalog service.

## Validation profile

The adapter bounds both `schema` and `info` to 32 KiB, 32 levels, and 4,096
JSON nodes before cataloging. It requires Draft 2020-12, a required `input`
object, a matching `input.type` const, and the HTTP or MCP fields mandated by
the Bazaar extension. HTTP body methods require `bodyType` and `body`; MCP
requires `toolName` and `inputSchema`; external `$ref` and `$id` values are forbidden.

The dependency-free offline validator supports the keywords required by the
checked-in HTTP and MCP fixtures: object/array/scalar `type`, `const`, `enum`,
`properties`, `required`, `additionalProperties`, `items`, local JSON Pointer
`$ref`, string length, and numeric bounds. A valid Draft 2020-12 schema using another keyword is not
guessed at: cataloging fails closed with `schema_profile_unavailable` until the
future maintained TypeScript validator is connected.

Optional resource service metadata and `routeTemplate` still pass through the
existing catalog's x402-aligned sanitization and canonical-key rules. A schema
match never bypasses those catalog-integrity checks.

## Outcomes and `EXTENSION-RESPONSES`

Every internal outcome has a stable `code` and non-empty `reason`:

| Internal disposition | Typical code | x402 response |
| --- | --- | --- |
| `accepted` | `cataloged` | `success` |
| `dropped` | `bazaar_extension_missing` | no Bazaar response |
| `invalid` | `schema_info_mismatch` | `rejected` |
| `duplicate` | `duplicate_resource` | `rejected` |
| `unavailable` | `catalog_unavailable` | `rejected` |

The public x402 wire contract remains exactly `success | processing | rejected`.
The synchronous offline adapter emits `success` or `rejected`; it does not
claim asynchronous `processing`. Rejected responses place the stable code and
bounded explanation in `rejectedReason`. The helper serializes the wrapper as
base64 JSON ready for a later `EXTENSION-RESPONSES` header, but this milestone
does not register an HTTP route or append a real header.

## Deliberate limits

- No raw payment payload, signature, auth entry, verify, or settle processing.
- No HTTP/MCP runtime, database, persistence, update, deletion, or expiry.
- No external schema fetch or filesystem reference resolution.
- No wallet, RPC, signing, transaction, payment, pubnet operation, or submit.
- No new dependency and no claim of full Draft 2020-12 conformance.

## Primary sources

- SCF RFP: <https://github.com/stellar/scf-handbook/blob/main/scf-awards/build-award/rfp-track.md#x402-facilitator-with-bazaar-discovery-support>
- x402 Bazaar extension: <https://github.com/x402-foundation/x402/blob/main/specs/extensions/bazaar.md>
