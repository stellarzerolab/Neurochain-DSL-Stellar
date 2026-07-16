param(
    [string]$Cargo = "cargo"
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$TargetRoot = Join-Path $RepoRoot "target\release"
$ServerPath = Join-Path $TargetRoot "neurochain-mcp-v0-stdio.exe"
$ClientPath = Join-Path $TargetRoot "neurochain-mcp-v0-client-smoke.exe"

function Fail([string]$Message) {
    throw "MCP v0 release verification failed: $Message"
}

Push-Location $RepoRoot
try {
    $env:CARGO_INCREMENTAL = "0"
    & $Cargo build --release --locked `
        --bin neurochain-mcp-v0-stdio `
        --bin neurochain-mcp-v0-client-smoke
    if ($LASTEXITCODE -ne 0) {
        Fail "cargo build exited with $LASTEXITCODE"
    }

    if (!(Test-Path -LiteralPath $ServerPath) -or !(Test-Path -LiteralPath $ClientPath)) {
        Fail "expected release binaries were not created"
    }

    $SmokeJson = (& $ClientPath --server $ServerPath | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) {
        Fail "client smoke exited with $LASTEXITCODE"
    }
    $Smoke = $SmokeJson | ConvertFrom-Json

    if ($Smoke.status -ne "passed") {
        Fail "client smoke status was not passed"
    }
    if ($Smoke.transport -ne "stdio" -or [int]$Smoke.conformance_cases -ne 7) {
        Fail "unexpected transport or conformance case count"
    }

    foreach ($Field in @(
        "underlying_action_submit_allowed",
        "attestation_submitted",
        "verification_transaction_submitted",
        "nullifier_consumed"
    )) {
        if ([bool]$Smoke.$Field) {
            Fail "$Field must remain false"
        }
    }

    $Artifacts = foreach ($Path in @($ServerPath, $ClientPath)) {
        $File = Get-Item -LiteralPath $Path
        $Hash = Get-FileHash -LiteralPath $Path -Algorithm SHA256
        [ordered]@{
            name = $File.Name
            size_bytes = $File.Length
            sha256 = $Hash.Hash.ToLowerInvariant()
        }
    }

    [ordered]@{
        status = "passed"
        mode = "read_only_no_submit"
        protocol_version = $Smoke.protocol_version
        conformance_cases = [int]$Smoke.conformance_cases
        tools = @($Smoke.tools)
        safety = [ordered]@{
            underlying_action_submit_allowed = $false
            attestation_submitted = $false
            verification_transaction_submitted = $false
            nullifier_consumed = $false
        }
        artifacts = @($Artifacts)
    } | ConvertTo-Json -Depth 6
}
finally {
    Pop-Location
}
