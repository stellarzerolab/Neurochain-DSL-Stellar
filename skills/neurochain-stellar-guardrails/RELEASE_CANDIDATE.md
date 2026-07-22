# NeuroChain Stellar Guardrails Skill Release Candidate

This is the internal release-candidate manifest for the
`neurochain-stellar-guardrails` skill package.

Status:

```text
internal_release_candidate = true
published = false
runtime_dependency = false
submit_surface = false
```

The package is ready for internal host testing when the MCP v0 release gate
passes with `validated_by_launch=true`. Public publication still requires a
separate final review for the chosen distribution channel.

## Included Files

The release candidate includes:

- `SKILL.md`
- `PACKAGING.md`
- `INSTALL.md`
- `RELEASE_CANDIDATE.md`
- `agents/openai.yaml`
- `examples/README.md`
- `examples/approved.md`
- `examples/requires_approval.md`
- `examples/blocked.md`
- `examples/state_unavailable.md`

## Required Release Candidate Evidence

Run the combined release candidate gate:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify_guardrails_skill_release_candidate.ps1
```

Required top-level summary fields:

```text
status = passed
published = false
release_candidate = true
runtime_dependency = false
submit_surface = false
```

The combined gate runs both checks below.

## Required MCP Host Evidence

MCP host evidence command:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify_mcp_v0_release.ps1 `
  -HostConfigOut .\target\release\neurochain-mcp-v0-host.json
```

Required summary fields:

```text
status = passed
mode = read_only_no_submit
validated_by_launch = true
secrets_included = false
submit_tools_included = false
```

Skill package evidence command:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify_guardrails_skill_package.ps1
```

Required summary fields:

```text
status = passed
runtime_dependency = false
submit_surface = false
secrets_included = false
```

## Included Runtime Surface

The skill may describe only the default MCP v0 read-only tools:

- `plan_stellar_action`
- `evaluate_guardrails`
- `prove_guardrail_decision`
- `verify_zk_on_stellar`
- `get_guardrail_status`

The flow remains:

```text
Plan -> Evaluate -> Prove -> Verify -> Status -> no automatic submit
```

## Excluded Product Surfaces

The release candidate must not include or advertise:

- `submit_testnet_attestation`
- `consume_nullifier`
- `submit_underlying_action`
- `sign_transaction`
- `configure_server`
- wallet sources
- seed phrases
- private keys
- API keys
- hosted service tokens
- real x402 facilitator verify/settle transport
- mainnet or testnet transaction submit

## Review Notes

- ZK is beyond a lite demo at the core level: the project has real RISC Zero
  guest logic, Groth16 fixture proofs, Soroban verifier/router integration,
  tamper rejection, replay rejection, and hosted CLI proof evidence.
- x402 is beyond a lite UI idea at the product-boundary level: the project has
  gateway responses, schema/types, a viewer, audit/replay boundaries, mock
  verifier fencing, and a fail-closed facilitator boundary.
- x402 is not production until real facilitator verify/settle transport is
  attached behind the existing boundary.
- Payment, proof, read-only verification, status, or attestation evidence must
  never imply underlying ActionPlan submit permission.
