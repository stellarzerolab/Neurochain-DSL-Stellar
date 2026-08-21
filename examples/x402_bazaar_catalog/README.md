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
- `list_response.json` locks the deterministic x402 v2 resources-list response,
  validated `accepts` fields, and default offset pagination.

No HTTP endpoint, search index, payment verification, settlement, wallet
signing, or ActionPlan submission is active in this package.
