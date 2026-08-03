# NeuroChain Stellar Guardrails Skill Install And Use

This note shows how to connect the `neurochain-stellar-guardrails` skill to the
current MCP v0 stdio host examples. It is a package/use note only; it does not
add a new runtime, dependency, wallet, signer, broadcaster, or submit path.

## Prerequisite

Run the MCP v0 release gate and generate a host config:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify_mcp_v0_release.ps1 `
  -HostConfigOut .\target\release\neurochain-mcp-v0-host.json
```

The gate must report:

```text
status = passed
mode = read_only_no_submit
validated_by_launch = true
secrets_included = false
submit_tools_included = false
```

## Host Configuration Source

Use the host-ready config produced by the release gate, or copy one of these
examples and replace the paths with absolute local paths:

```text
examples/mcp_v0_stdio_client/mcp_servers.json.example
examples/mcp_v0_stdio_client/mcp_servers.windows.json.example
```

The host config should contain only:

- the `neurochain-mcp-v0-stdio` command path
- no args
- `NC_INTENT_STELLAR_MODEL`

Do not add wallet sources, seed phrases, private keys, API keys, signing
material, submit tools, attestation tools, nullifier-consume tools, or hosted
service tokens.

## Skill Flow

After the MCP host can launch the stdio server, the skill should guide agents
through:

```text
Plan -> Evaluate -> Prove -> Verify -> Status -> no automatic submit
```

Default tool order:

1. `plan_stellar_action`
2. `evaluate_guardrails`
3. `prove_guardrail_decision`
4. `verify_zk_on_stellar`
5. `get_guardrail_status`

Stop on `blocked` or `requires_approval`. In those states, call
`get_guardrail_status` with the latest MCP `structuredContent` as
`latest_result`, report the boundary, and stop.

## Verification

Use the release gate as the local package check:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify_mcp_v0_release.ps1 `
  -HostConfigOut .\target\release\neurochain-mcp-v0-host.json
```

Use the skill examples for expected agent-facing wording:

```text
skills/neurochain-stellar-guardrails/examples/
```

## Non-Goals

This install/use note does not enable:

- `submit_testnet_attestation`
- `consume_nullifier`
- `submit_underlying_action`
- `sign_transaction`
- `configure_server`
- x402 settlement runtime integration or live settlement
- mainnet or testnet transaction submit

Those remain separate product surfaces with their own explicit approval and
security review.
