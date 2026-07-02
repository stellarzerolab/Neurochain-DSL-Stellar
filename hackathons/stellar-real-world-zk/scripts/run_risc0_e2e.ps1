param(
    [string]$WslDistribution = "Ubuntu",
    [ValidateSet("approved", "requires_approval", "blocked_allowlist")]
    [string]$Scenario = "approved",
    [string]$InputPath,
    [string]$OutputPath,
    [switch]$CheckInput
)

$ErrorActionPreference = "Stop"

$risc0WindowsPath = (Resolve-Path (Join-Path $PSScriptRoot "..\risc0")).Path
$wslHome = (& wsl.exe -d $WslDistribution -- printenv HOME).Trim()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($wslHome)) {
    throw "Could not resolve HOME in WSL distribution '$WslDistribution'."
}

if ($risc0WindowsPath -notmatch '^([A-Za-z]):\\(.*)$') {
    throw "RISC Zero workspace is not on a Windows drive: $risc0WindowsPath"
}
$drive = $Matches[1].ToLowerInvariant()
$relativePath = $Matches[2].Replace('\', '/')
$risc0WslPath = "/mnt/$drive/$relativePath"

function Convert-WindowsPathToWsl {
    param([Parameter(Mandatory = $true)][string]$Path)

    $fullPath = [IO.Path]::GetFullPath($Path)
    if ($fullPath -notmatch '^([A-Za-z]):\(.*)$') {
        throw "Path is not on a Windows drive: $fullPath"
    }
    $pathDrive = $Matches[1].ToLowerInvariant()
    $pathRelative = $Matches[2].Replace('\', '/')
    return "/mnt/$pathDrive/$pathRelative"
}

$toolPath = "$wslHome/.risc0/bin:$wslHome/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
$hostArguments = @($Scenario)
$artifactName = switch ($Scenario) {
    "approved" { "neurochain-zk-stellar-proof.json" }
    "requires_approval" { "neurochain-zk-stellar-proof-requires-approval.json" }
    "blocked_allowlist" { "neurochain-zk-stellar-proof-blocked-allowlist.json" }
}
$artifactPath = Join-Path $risc0WindowsPath "target\$artifactName"
if (-not [string]::IsNullOrWhiteSpace($InputPath)) {
    $resolvedInput = (Resolve-Path -LiteralPath $InputPath).Path
    if ([string]::IsNullOrWhiteSpace($OutputPath)) {
        $artifactPath = Join-Path $risc0WindowsPath "target\neurochain-zk-stellar-proof-custom.json"
    }
    else {
        $artifactPath = [IO.Path]::GetFullPath($OutputPath)
    }
    $hostArguments = @(
        "--input", (Convert-WindowsPathToWsl -Path $resolvedInput),
        "--output", (Convert-WindowsPathToWsl -Path $artifactPath)
    )
    if ($CheckInput) {
        $hostArguments = @("--check-input", (Convert-WindowsPathToWsl -Path $resolvedInput))
    }
}
elseif (-not [string]::IsNullOrWhiteSpace($OutputPath)) {
    throw "-OutputPath requires -InputPath."
}
elseif ($CheckInput) {
    throw "-CheckInput requires -InputPath."
}

& wsl.exe -d $WslDistribution --cd $risc0WslPath -- env -u RISC0_DEV_MODE "PATH=$toolPath" cargo run --release -p neurochain-zk-risc0-host -- @hostArguments
if ($LASTEXITCODE -ne 0) {
    throw "RISC Zero end-to-end run failed with exit code $LASTEXITCODE."
}
if ($CheckInput) {
    Write-Output "input_mode=private_file_check"
    return
}

$artifact = Get-Content -Raw -LiteralPath $artifactPath | ConvertFrom-Json
$expectedFields = @(
    "schema_version",
    "seal_hex",
    "image_id_hex",
    "journal_hex",
    "journal_digest_hex"
)
$actualFields = @($artifact.PSObject.Properties.Name)
if (@(Compare-Object $expectedFields $actualFields).Count -ne 0) {
    throw "Stellar proof artifact contains an unexpected or missing field."
}
if ($artifact.schema_version -ne 1) {
    throw "Unsupported Stellar proof artifact schema version."
}
if ($artifact.seal_hex -notmatch '^[0-9a-f]+$' -or $artifact.seal_hex.Length -le 8) {
    throw "Groth16 seal must be lowercase hex with a four-byte routing selector."
}
if ($artifact.image_id_hex -notmatch '^[0-9a-f]{64}$') {
    throw "Evaluator image id must be exactly 32 bytes of lowercase hex."
}
if ($artifact.journal_digest_hex -notmatch '^[0-9a-f]{64}$') {
    throw "Journal digest must be exactly 32 bytes of lowercase hex."
}
if ($artifact.journal_hex -notmatch '^(?:[0-9a-f]{2})+$') {
    throw "Public journal must be non-empty lowercase byte-aligned hex."
}

[byte[]]$journalBytes = for ($index = 0; $index -lt $artifact.journal_hex.Length; $index += 2) {
    [Convert]::ToByte($artifact.journal_hex.Substring($index, 2), 16)
}
$sha256 = [Security.Cryptography.SHA256]::Create()
try {
    $computedDigest = -join ($sha256.ComputeHash($journalBytes) | ForEach-Object { $_.ToString("x2") })
}
finally {
    $sha256.Dispose()
}
if ($computedDigest -ne $artifact.journal_digest_hex) {
    throw "Journal digest does not match the public journal bytes."
}

Write-Output "stellar_artifact_valid=true"
Write-Output "input_mode=$(if ([string]::IsNullOrWhiteSpace($InputPath)) { 'scenario' } else { 'private_file' })"
Write-Output "scenario=$(if ([string]::IsNullOrWhiteSpace($InputPath)) { $Scenario } else { 'custom' })"
