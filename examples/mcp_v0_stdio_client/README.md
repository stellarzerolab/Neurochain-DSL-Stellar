# MCP V0 Stdio Client Example

This directory shows how an MCP host can launch the read-only NeuroChain MCP v0
stdio server and complete one no-submit session. `plan_stellar_action` uses the
real local intent model and deterministic ActionPlan builder. The later tools
remain explicit fixtures while their runtime adapters are added one at a time.

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

## Release Gate On Windows

Build the locked release binaries, run the same host-neutral conformance
session against the absolute release server path, and emit artifact SHA-256
hashes:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify_mcp_v0_release.ps1
```

The command exits non-zero unless all seven conformance cases pass and every
submit, attestation, transaction, and nullifier state remains disabled. It does
not connect to Stellar or require wallet credentials.

## MCP Host Configuration

Copy `mcp_servers.json.example` into the configuration shape expected by the
host. Replace `command` with the absolute path to the built stdio executable and
set `NC_INTENT_STELLAR_MODEL` to the absolute local ONNX model path. On Windows,
use the `.exe` path.

The server needs no arguments and no environment secrets. The model path is
local runtime configuration, not a credential. An MCP host should:

1. Spawn the configured executable over stdio.
2. Send `initialize`.
3. Send `notifications/initialized`.
4. Discover the five read-only tools with `tools/list`.
5. Call `plan_stellar_action` with `intent_text`, or use an explicit fixture for
   conformance testing.
6. Preserve `underlying_action_submit_allowed=false`.
7. Close the transport when the session ends.

Do not add wallet secrets, API keys, seed phrases, transaction signing, or
submit tools to this default configuration. `submit_testnet_attestation` stays
outside MCP v0 and requires a separate explicit product boundary.
