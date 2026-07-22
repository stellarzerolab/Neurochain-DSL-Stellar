param(
    [string]$Cargo = "cargo",
    [string]$HostConfigOut = ""
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$TargetRoot = Join-Path $RepoRoot "target\release"
$ServerPath = Join-Path $TargetRoot "neurochain-mcp-v0-stdio.exe"
$ClientPath = Join-Path $TargetRoot "neurochain-mcp-v0-client-smoke.exe"
$ModelPath = Join-Path $RepoRoot "models\intent_stellar\model.onnx"

function Fail([string]$Message) {
    throw "MCP v0 release verification failed: $Message"
}

function Assert-Smoke([object]$Smoke) {
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
    if (!(Test-Path -LiteralPath $ModelPath)) {
        Fail "expected local intent model was not found at $ModelPath"
    }

    $SmokeJson = (& $ClientPath --server $ServerPath | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) {
        Fail "client smoke exited with $LASTEXITCODE"
    }
    $Smoke = $SmokeJson | ConvertFrom-Json
    Assert-Smoke $Smoke

    $Artifacts = foreach ($Path in @($ServerPath, $ClientPath)) {
        $File = Get-Item -LiteralPath $Path
        $Hash = Get-FileHash -LiteralPath $Path -Algorithm SHA256
        [ordered]@{
            name = $File.Name
            size_bytes = $File.Length
            sha256 = $Hash.Hash.ToLowerInvariant()
        }
    }

    $HostConfig = $null
    if ($HostConfigOut.Trim().Length -gt 0) {
        $ResolvedHostConfigOut = $HostConfigOut
        if (![System.IO.Path]::IsPathRooted($ResolvedHostConfigOut)) {
            $ResolvedHostConfigOut = Join-Path $RepoRoot $ResolvedHostConfigOut
        }
        $ResolvedHostConfigOut = [System.IO.Path]::GetFullPath($ResolvedHostConfigOut)
        $HostConfigDir = Split-Path -Parent $ResolvedHostConfigOut
        if ($HostConfigDir -and !(Test-Path -LiteralPath $HostConfigDir)) {
            New-Item -ItemType Directory -Path $HostConfigDir | Out-Null
        }

        [ordered]@{
            mcpServers = [ordered]@{
                "neurochain-stellar-guardrails" = [ordered]@{
                    command = $ServerPath
                    args = @()
                    env = [ordered]@{
                        NC_INTENT_STELLAR_MODEL = $ModelPath
                    }
                }
            }
        } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $ResolvedHostConfigOut -Encoding UTF8

        $HostConfigJson = Get-Content -LiteralPath $ResolvedHostConfigOut -Raw
        if ($HostConfigJson -match "submit_testnet_attestation|consume_nullifier|submit_underlying_action|sign_transaction|NC_STELLAR_SOURCE|NC_SOROBAN_SOURCE|SECRET|SEED|PRIVATE|API_KEY|TOKEN") {
            Fail "generated host config contains a forbidden submit, source, or secret-like value"
        }

        $ParsedHostConfig = $HostConfigJson | ConvertFrom-Json
        $ServerConfig = $ParsedHostConfig.mcpServers."neurochain-stellar-guardrails"
        if ($null -eq $ServerConfig) {
            Fail "generated host config is missing neurochain-stellar-guardrails"
        }
        if ($ServerConfig.command -ne $ServerPath) {
            Fail "generated host config command does not match release server path"
        }
        if ($ServerConfig.args.Count -ne 0) {
            Fail "generated host config must not add server args"
        }
        if ($ServerConfig.env.NC_INTENT_STELLAR_MODEL -ne $ModelPath) {
            Fail "generated host config model path does not match local release model path"
        }

        $PreviousModelEnv = $env:NC_INTENT_STELLAR_MODEL
        try {
            $env:NC_INTENT_STELLAR_MODEL = $ServerConfig.env.NC_INTENT_STELLAR_MODEL
            $HostConfigSmokeJson = (& $ClientPath --server $ServerConfig.command | Out-String).Trim()
            if ($LASTEXITCODE -ne 0) {
                Fail "host config smoke exited with $LASTEXITCODE"
            }
            $HostConfigSmoke = $HostConfigSmokeJson | ConvertFrom-Json
            Assert-Smoke $HostConfigSmoke
        }
        finally {
            $env:NC_INTENT_STELLAR_MODEL = $PreviousModelEnv
        }

        $HostConfig = [ordered]@{
            path = $ResolvedHostConfigOut
            command = $ServerPath
            model = $ModelPath
            validated_by_launch = $true
            secrets_included = $false
            submit_tools_included = $false
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
        host_config = $HostConfig
        artifacts = @($Artifacts)
    } | ConvertTo-Json -Depth 6
}
finally {
    Pop-Location
}
