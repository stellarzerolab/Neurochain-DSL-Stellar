# Offline x402 Bazaar automatic cataloging

Date: 2026-08-22

Status: bounded Rust adapter plus TypeScript handoff/wire parity; no facilitator
or HTTP runtime

## Purpose and authority boundary

`src/x402_bazaar_cataloging.rs` connects an already verified facilitator-side
discovery handoff to the existing offline Bazaar catalog. The adapter does not receive a raw `PaymentPayload`, payment signature, credential, wallet handle,
or transaction. It does not verify or settle a payment and does not grant payment, settlement, signing, execution, or ActionPlan-submit authority.

The TypeScript `@x402/stellar` workspace remains responsible for the standard
facilitator protocol and for ensuring that the handoff is invoked only after
the intended upstream verify/settle result. Its pure
`bazaar-automatic-cataloging.ts` adapter reads only that result's `extensions`
field, rejects authority-shaped context, and delegates the schema/catalog
decision to a narrow Rust port. Rust receives only:

- the resource descriptor and already extracted payment summary;
- the Bazaar `info`, `schema`, and optional `routeTemplate`;
- an observation timestamp supplied by the eventual catalog service.

The TypeScript adapter cannot infer successful verification from extension
presence and does not re-run payment validation. It only locks the v1 handoff,
all-false authority boundary, stable outcome envelope, and base64
`EXTENSION-RESPONSES` representation.

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
bounded explanation in `rejectedReason`. Rust and TypeScript consume the same
HTTP, MCP, and outcome fixtures and serialize the same wrapper as base64 JSON
ready for a later `EXTENSION-RESPONSES` header. Neither adapter registers an
HTTP route or appends a real header.

## Deliberate limits

- No raw payment payload, signature, auth entry, verify, or settle processing.
- No HTTP/MCP runtime, database, persistence, update, deletion, or expiry.
- No service dispatch: the TypeScript catalog port is only an injected offline
  interface and has no production implementation in this milestone.
- No external schema fetch or filesystem reference resolution.
- No wallet, RPC, signing, transaction, payment, pubnet operation, or submit.
- No new dependency and no claim of full Draft 2020-12 conformance.

## Primary sources

- SCF RFP: <https://github.com/stellar/scf-handbook/blob/main/scf-awards/build-award/rfp-track.md#x402-facilitator-with-bazaar-discovery-support>
- x402 Bazaar extension: <https://github.com/x402-foundation/x402/blob/main/specs/extensions/bazaar.md>
