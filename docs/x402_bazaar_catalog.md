# Offline x402 Bazaar catalog core

Date: 2026-08-21

Status: typed offline catalog and resources-list contract; no HTTP or MCP discovery runtime

## Scope

`src/x402_bazaar.rs` implements the first catalog integrity boundary for the
future TypeScript facilitator/Bazaar service. It accepts an already extracted
discovery candidate, validates hard envelope fields, soft-drops hostile
`routeTemplate` and optional service metadata, creates a deterministic catalog
key, and rejects duplicates without overwriting the first entry.

The hard envelope accepts x402 v2, `exact` or `upto`, the two Stellar CAIP-2
network identifiers, a positive integer-string `amount`, a contract-shaped
Stellar `asset`, a Stellar account/contract/muxed-account-shaped `payTo`, and
a positive `maxTimeoutSeconds`. Cryptographic asset and recipient
verification still belongs to the later payment-verified automatic-cataloging
adapter, not this offline type layer.

The module supports both resource identities required by the Bazaar spec:

- HTTP: origin plus a valid canonical `routeTemplate`, or the concrete URL
  path when the template is absent or invalid.
- MCP: the tuple of resource URL and `input.toolName`.

`routeTemplate` follows the current x402 rules: it must begin with `/`, contain
only the allowed path characters, and must not contain traversal or URL-scheme
sequences after strict percent-decoding. An invalid value is discarded and the
concrete path is used instead.

Optional `serviceName`, `tags`, and `iconUrl` follow the spec's soft-drop
model. Service names and tags are bounded printable ASCII, tags are
case-insensitively deduplicated with at most five valid values, and icon URLs
must be credential-free HTTP(S) URLs whose normalized and percent-decoded host
is not an IP literal, loopback name, decimal IP form, or hexadecimal IP form.

Duplicate canonical keys fail closed. This milestone intentionally does not
define update ownership or overwrite semantics; those need provenance and
persistent-store rules first.

## Offline resources-list contract

`BazaarCatalog::list` defines the data contract later used by
`GET /discovery/resources`; it does not register an HTTP route. The query
supports `type`, `payTo`, `scheme`, `network`, `extensions`, `limit`, and
`offset`. The response uses x402 v2 `items` and offset pagination, including
the total number of filtered resources before pagination.

- `limit=20` and `offset=0` are the defaults; `limit` must be 1-100.
- The local defensive offset ceiling is 1,000,000.
- Entries are listed in deterministic BTreeMap key order, independent of
  insertion order.
- The current catalog marks every accepted candidate as the `bazaar`
  extension. An unknown but well-formed extension filter returns no matches;
  malformed filters fail closed.
- A list item includes the concrete resource URL, HTTP/MCP type, x402 version,
  one validated accepted Stellar payment requirement, and observation time.
  Optional extension response metadata is omitted until its exact payload and
  provenance contract is implemented.

## Deliberate limits

This module does not receive a `PaymentPayload`, validate payment signatures,
resolve untrusted JSON Schema references, verify `info` against seller-supplied
schemas, expose an HTTP route for `GET /discovery/resources`, define
`GET /discovery/search`, perform natural-language ranking, make network calls,
or provide paid-call, signing, settlement, or ActionPlan-submit authority.
`stellar:pubnet` is a filterable catalog value only and does not enable a pubnet operation.

The future automatic-cataloging adapter must validate `info` against its
Draft 2020-12 schema without resolving external `$ref` or `$id` values before
creating this extracted candidate.

## Primary sources

- SCF RFP: <https://github.com/stellar/scf-handbook/blob/main/scf-awards/build-award/rfp-track.md#x402-facilitator-with-bazaar-discovery-support>
- x402 Bazaar extension: <https://github.com/x402-foundation/x402/blob/main/specs/extensions/bazaar.md>
- x402 v2 discovery API: <https://github.com/x402-foundation/x402/blob/main/specs/x402-specification-v2.md#8-discovery-api>
