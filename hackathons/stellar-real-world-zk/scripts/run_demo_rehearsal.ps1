param(
    [ValidateSet("approved", "requires_approval", "blocked_allowlist")]
    [string]$Scenario = "blocked_allowlist",
    [string]$WslDistribution = "Ubuntu",
    [switch]$IncludeLocalnet
)

$ErrorActionPreference = "Stop"

$ProjectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$ReadinessScript = Join-Path $PSScriptRoot "check_submission_package.ps1"
$LocalnetScript = Join-Path $PSScriptRoot "run_soroban_localnet_e2e.ps1"

function Invoke-ChildPowerShell {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [string[]]$Arguments = @()
    )

    $commandArguments = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $Path
    ) + $Arguments

    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & powershell.exe @commandArguments
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }

    if ($exitCode -ne 0) {
        throw "Demo step '$Path' failed with exit code $exitCode."
    }
}

$fixtureName = switch ($Scenario) {
    "approved" { "groth16_approved.json" }
    "requires_approval" { "groth16_requires_approval.json" }
    "blocked_allowlist" { "groth16_blocked_exit_3.json" }
}
$decision = switch ($Scenario) {
    "approved" { "approved" }
    "requires_approval" { "requires_approval" }
    "blocked_allowlist" { "blocked" }
}
$exitCode = if ($Scenario -eq "blocked_allowlist") { 3 } else { 0 }
$requiresApproval = $Scenario -eq "requires_approval"
$fixturePath = Join-Path $ProjectRoot "fixtures\$fixtureName"

Write-Output "demo_stage=submission_readiness"
Invoke-ChildPowerShell -Path $ReadinessScript -Arguments @("-RunTests")

$fixture = Get-Content -Raw -LiteralPath $fixturePath | ConvertFrom-Json
$sealBytes = [int]($fixture.seal_hex.Length / 2)

Write-Output "demo_stage=public_proof_artifact"
Write-Output "scenario=$Scenario"
Write-Output "decision=$decision"
Write-Output "exit_code=$exitCode"
Write-Output "requires_approval=$($requiresApproval.ToString().ToLowerInvariant())"
Write-Output "artifact_schema=$($fixture.schema_version)"
Write-Output "seal_bytes=$sealBytes"
Write-Output "evaluator_image_id=$($fixture.image_id_hex)"
Write-Output "journal_digest=$($fixture.journal_digest_hex)"
Write-Output "private_policy_revealed=false"

if ($IncludeLocalnet) {
    Write-Output "demo_stage=protocol_26_localnet"
    Invoke-ChildPowerShell -Path $LocalnetScript -Arguments @(
        "-WslDistribution", $WslDistribution,
        "-Scenario", $Scenario
    )
    Write-Output "localnet_included=true"
}
else {
    Write-Output "localnet_included=false"
    Write-Output "localnet_hint=rerun_with_-IncludeLocalnet_for_the_full_protocol_26_demo"
}

Write-Output "demo_rehearsal_ready=true"
