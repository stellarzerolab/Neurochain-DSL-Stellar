# MCP External Host Validation

The NeuroChain MCP v0 release gate already validates the stdio server by
launching it through a generated host configuration. That is repeatable
internal launch evidence, but it is not a claim that a third-party MCP host or
MCP Inspector has completed an external session.

## Current Boundary

Run the dependency-free readiness check:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check_mcp_external_host_readiness.ps1
```

The check only discovers an existing Inspector executable. It does not install
Node.js packages, contact a registry, modify host configuration, or launch a
network or submit operation.

When no supported Inspector executable is available, the result is:

```text
status = host_unavailable
external_host_available = false
missing_port = external_mcp_host_or_inspector_executable
installation_attempted = false
runtime_dependency_added = false
submit_surface_added = false
```

This is the exact remaining host port. The MCP server, generated host
configuration, process-level conformance client, and five runtime-backed
read-only tools remain independently validated by:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify_mcp_v0_release.ps1 `
  -HostConfigOut .\target\release\neurochain-mcp-v0-host.json
```

## External Validation Acceptance Criteria

An external host or Inspector validation is complete only when that host:

1. launches `neurochain-mcp-v0-stdio` over stdio
2. completes `initialize` and `notifications/initialized`
3. discovers exactly the five default read-only tools
4. calls at least one runtime-backed tool successfully
5. observes `underlying_action_submit_allowed=false`
6. does not receive signing, broadcast, attestation-submit, nullifier-consume,
   or underlying ActionPlan submit tools

Selecting or installing an external host is a separate environment decision.
It must not become a NeuroChain runtime dependency.
