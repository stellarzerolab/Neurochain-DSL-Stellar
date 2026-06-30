param(
    [string]$WslDistribution = "Ubuntu",
    [ValidateSet("approved", "requires_approval", "blocked_allowlist")]
    [string]$Scenario = "approved"
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

$toolPath = "$wslHome/.risc0/bin:$wslHome/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
& wsl.exe -d $WslDistribution --cd $risc0WslPath -- env -u RISC0_DEV_MODE "PATH=$toolPath" cargo run --release -p neurochain-zk-risc0-host -- $Scenario
if ($LASTEXITCODE -ne 0) {
    throw "RISC Zero end-to-end run failed with exit code $LASTEXITCODE."
}

$artifactName = switch ($Scenario) {
    "approved" { "neurochain-zk-stellar-proof.json" }
    "requires_approval" { "neurochain-zk-stellar-proof-requires-approval.json" }
    "blocked_allowlist" { "neurochain-zk-stellar-proof-blocked-allowlist.json" }
}
$artifactPath = Join-Path $risc0WindowsPath "target\$artifactName"
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
Write-Output "scenario=$Scenario"
