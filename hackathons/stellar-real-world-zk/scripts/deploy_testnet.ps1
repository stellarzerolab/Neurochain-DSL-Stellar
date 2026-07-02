param(
    [Parameter(Mandatory = $true)][string]$Source,
    [Parameter(Mandatory = $true)][string]$VerifierWasm,
    [Parameter(Mandatory = $true)][string]$RouterWasm,
    [string]$Network = "testnet",
    [string]$DeploymentManifest,
    [switch]$Execute
)

$ErrorActionPreference = "Stop"

$Selector = "73c457ba"
$VerifierCommit = "e8ff6ea202db195352c0141ecc533ff649393fe4"
$VerifierWasmSha256 = "f6a1f928de93db9b1e4176ef247d4a8c5d45a07a16cafd0bce9e641d7eaa03d8"
$RouterWasmSha256 = "03f6e4c26ac5d662b60b6230a1a498b8d69d726b4291d17f56812ecb81797659"
$ApplicationWasmSha256 = "dbe89ddc76717b50b66934af39cfaf3153e3cef218267c0a01ab3d4bc0a1a70f"

$ProjectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$RepoRoot = (Resolve-Path (Join-Path $ProjectRoot "..\..")).Path
$OutDir = Join-Path $ProjectRoot "target\testnet"
$AppManifest = Join-Path $ProjectRoot "soroban\Cargo.toml"
$AppWasm = Join-Path $OutDir "neurochain_zk_guardrail_soroban.wasm"
if ([string]::IsNullOrWhiteSpace($DeploymentManifest)) {
    $DeploymentManifest = Join-Path $ProjectRoot "deployments\testnet.json"
}

function Invoke-Native {
    param(
        [Parameter(Mandatory = $true)][string]$Command,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    $output = & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Command failed with exit code $LASTEXITCODE"
    }
    return $output
}

function Invoke-Stellar {
    param([Parameter(Mandatory = $true)][string[]]$Arguments)
    return Invoke-Native -Command "stellar.exe" -Arguments $Arguments
}

function Last-OutputLine {
    param([Parameter(Mandatory = $true)]$Output)
    return [string](@($Output)[-1]).Trim()
}

function Assert-Sha256 {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Expected
    )

    $resolved = (Resolve-Path -LiteralPath $Path).Path
    $actual = (Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $Expected) {
        throw "Unexpected SHA-256 for ${resolved}: $actual"
    }
}

function Read-JournalFields {
    param([Parameter(Mandatory = $true)][string]$JournalHex)

    if ($JournalHex.Length -ne (164 * 2)) {
        throw "Unexpected public journal length: $($JournalHex.Length / 2) bytes"
    }
    return [PSCustomObject]@{
        ActionPlanHash = $JournalHex.Substring(60 * 2, 64)
        PolicyCommitment = $JournalHex.Substring(92 * 2, 64)
        PolicyVersion = [Convert]::ToUInt32($JournalHex.Substring(124 * 2, 8), 16)
        AuditNullifier = $JournalHex.Substring(132 * 2, 64)
    }
}

if ($Network -ne "testnet") {
    throw "This deployment script is intentionally restricted to testnet."
}
if (-not $Execute) {
    throw "Refusing network deployment without the explicit -Execute switch."
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $DeploymentManifest) | Out-Null

Assert-Sha256 -Path $VerifierWasm -Expected $VerifierWasmSha256
Assert-Sha256 -Path $RouterWasm -Expected $RouterWasmSha256
Invoke-Stellar -Arguments @(
    "contract", "build", "--manifest-path", $AppManifest, "--out-dir", $OutDir
) | Out-Null
Assert-Sha256 -Path $AppWasm -Expected $ApplicationWasmSha256

$owner = Last-OutputLine (Invoke-Stellar -Arguments @("keys", "public-key", $Source))
$verifierId = Last-OutputLine (Invoke-Stellar -Arguments @(
    "contract", "deploy", "--wasm", (Resolve-Path $VerifierWasm).Path,
    "--source", $Source, "--network", $Network
))
$routerId = Last-OutputLine (Invoke-Stellar -Arguments @(
    "contract", "deploy", "--wasm", (Resolve-Path $RouterWasm).Path,
    "--source", $Source, "--network", $Network, "--", "--owner", $owner
))
Invoke-Stellar -Arguments @(
    "contract", "invoke", "--id", $routerId, "--source", $Source,
    "--network", $Network, "--send", "yes", "--", "add_verifier",
    "--selector", $Selector, "--verifier", $verifierId
) | Out-Null

$fixtureDefinitions = @(
    @{ Scenario = "approved"; File = "groth16_approved.json" },
    @{ Scenario = "requires_approval"; File = "groth16_requires_approval.json" },
    @{ Scenario = "blocked"; File = "groth16_blocked_exit_3.json" }
)
$fixtures = foreach ($definition in $fixtureDefinitions) {
    $artifact = Get-Content -Raw (Join-Path $ProjectRoot "fixtures\$($definition.File)") |
        ConvertFrom-Json
    if ($artifact.seal_hex.Substring(0, 8) -ne $Selector) {
        throw "Fixture selector mismatch for $($definition.Scenario)"
    }
    [PSCustomObject]@{
        Scenario = $definition.Scenario
        Artifact = $artifact
        Journal = Read-JournalFields -JournalHex $artifact.journal_hex
    }
}

$initial = $fixtures[0]
$appId = Last-OutputLine (Invoke-Stellar -Arguments @(
    "contract", "deploy", "--wasm", $AppWasm, "--source", $Source,
    "--network", $Network, "--", "--owner", $owner,
    "--verifier_router", $routerId,
    "--evaluator_image_id", $initial.Artifact.image_id_hex,
    "--initial_policy_commitment", $initial.Journal.PolicyCommitment,
    "--initial_policy_version", "$($initial.Journal.PolicyVersion)"
))

foreach ($fixture in $fixtures | Select-Object -Skip 1) {
    Invoke-Stellar -Arguments @(
        "contract", "invoke", "--id", $appId, "--source", $Source,
        "--network", $Network, "--send", "yes", "--", "authorize_policy",
        "--policy_commitment", $fixture.Journal.PolicyCommitment,
        "--policy_version", "$($fixture.Journal.PolicyVersion)"
    ) | Out-Null
}

foreach ($fixture in $fixtures) {
    $accepted = Last-OutputLine (Invoke-Stellar -Arguments @(
        "contract", "invoke", "--id", $appId, "--source", $Source,
        "--network", $Network, "--send", "no", "--instruction-leeway", "10000000",
        "--", "verify", "--seal", $fixture.Artifact.seal_hex,
        "--journal_bytes", $fixture.Artifact.journal_hex
    )) | ConvertFrom-Json
    if ($accepted.action_plan_hash -ne $fixture.Journal.ActionPlanHash -or
        $accepted.policy_commitment -ne $fixture.Journal.PolicyCommitment -or
        $accepted.policy_version -ne $fixture.Journal.PolicyVersion -or
        $accepted.audit_nullifier -ne $fixture.Journal.AuditNullifier) {
        throw "Read-only verification binding mismatch for $($fixture.Scenario)"
    }
}

$repoCommit = Last-OutputLine (Invoke-Native -Command "git.exe" -Arguments @(
    "-C", $RepoRoot, "rev-parse", "HEAD"
))
$policies = @($fixtures | ForEach-Object {
    [ordered]@{
        scenario = $_.Scenario
        commitment = $_.Journal.PolicyCommitment
        version = $_.Journal.PolicyVersion
        action_plan_hash = $_.Journal.ActionPlanHash
        audit_nullifier = $_.Journal.AuditNullifier
        read_only_verified = $true
    }
})
$manifest = [ordered]@{
    schema_version = 1
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    network = $Network
    repository_commit = $repoCommit
    verifier_upstream_commit = $VerifierCommit
    selector = $Selector
    evaluator_image_id = $initial.Artifact.image_id_hex
    contracts = [ordered]@{
        groth16_verifier = $verifierId
        risc0_router = $routerId
        neurochain_zk_guardrail = $appId
    }
    wasm_sha256 = [ordered]@{
        groth16_verifier = $VerifierWasmSha256
        risc0_router = $RouterWasmSha256
        neurochain_zk_guardrail = $ApplicationWasmSha256
    }
    authorized_policies = $policies
    safety = [ordered]@{
        read_only_verify_is_repeatable = $true
        consume_requires_owner_auth = $true
        underlying_action_submit_allowed = $false
    }
}
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $DeploymentManifest -Encoding UTF8

Write-Output "verifier_contract=$verifierId"
Write-Output "router_contract=$routerId"
Write-Output "application_contract=$appId"
Write-Output "deployment_manifest=$DeploymentManifest"
Write-Output "read_only_scenarios_verified=$($fixtures.Count)"
