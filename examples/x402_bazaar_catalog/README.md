# Offline x402 Bazaar catalog fixtures

These fixtures exercise the first offline Bazaar catalog milestone. They are
extracted discovery candidates, not payment payloads and not HTTP requests.

- `http_dynamic.json` proves a dynamic HTTP route maps to one canonical
  `routeTemplate` key and service tags are deduplicated case-insensitively.
- `hostile_soft_drop.json` proves percent-encoded traversal, invalid service
  metadata, and a percent-encoded loopback icon host are discarded without
  rejecting the otherwise valid resource.
- `mcp_tool.json` proves MCP identity is the tuple of resource URL and tool
  name.
- `market_data.json` adds a third deterministic HTTP candidate for ranking and
  pagination tests.
- `list_response.json` locks the deterministic x402 v2 resources-list response,
  validated `accepts` fields, and default offset pagination.
- `search_evaluation.json` declares the offline search candidates, expected
  top result for each query, and the minimum mean reciprocal rank gate.
- `search_pages.json` locks the full three-page `api` ranking, query-bound
  cursors, pagination flags, and x402 v2 response wire consumed by both Rust
  and the pure TypeScript resources/search adapter.

No HTTP endpoint, production search index, payment verification, settlement,
wallet signing, or ActionPlan submission is active in this package.
