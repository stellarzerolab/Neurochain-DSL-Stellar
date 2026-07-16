# MCP V0 Stdio Client Example

This directory shows how an MCP host can launch the offline NeuroChain MCP v0
stdio server and complete one no-submit session.

The example has no network, wallet, signing, broadcast, attestation, nullifier
consume, or underlying ActionPlan execution capability.

## Build And Verify

Build both executables:

```bash
cargo build --bin neurochain-mcp-v0-stdio --bin neurochain-mcp-v0-client-smoke
```

Run the smoke client after both binaries exist in the same target directory:

```bash
cargo run --bin neurochain-mcp-v0-client-smoke
```

The client owns the stdio server process, sends the host-neutral protocol cases
from `conformance_session.jsonl`, validates tool discovery, calls the
`evaluate_guardrails` `requires_approval` fixture, and exits only after all
no-submit and JSON-RPC conformance checks pass. The gate also confirms that
notifications stay silent and that excluded tools, secret-like arguments,
unsupported methods, and invalid parameters fail with standard protocol error
codes.

`session.jsonl` remains the smallest readable happy-path example for host
integrators.

To verify a separately built server, pass its absolute path:

```bash
cargo run --bin neurochain-mcp-v0-client-smoke -- \
  --server /absolute/path/to/neurochain-mcp-v0-stdio
```

PowerShell:

```powershell
cargo run --bin neurochain-mcp-v0-client-smoke -- `
  --server C:\absolute\path\to\neurochain-mcp-v0-stdio.exe
```

## MCP Host Configuration

Copy `mcp_servers.json.example` into the configuration shape expected by the
host. Replace `command` with the absolute path to the built stdio executable.
On Windows, use the `.exe` path.

The server needs no arguments and no environment secrets. An MCP host should:

1. Spawn the configured executable over stdio.
2. Send `initialize`.
3. Send `notifications/initialized`.
4. Discover the five read-only tools with `tools/list`.
5. Call a tool and preserve `underlying_action_submit_allowed=false`.
6. Close the transport when the session ends.

Do not add wallet secrets, API keys, seed phrases, transaction signing, or
submit tools to this default configuration. `submit_testnet_attestation` stays
outside MCP v0 and requires a separate explicit product boundary.
