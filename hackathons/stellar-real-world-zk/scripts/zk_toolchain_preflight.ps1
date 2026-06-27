param(
    [ValidateSet("Text", "Json")]
    [string]$Format = "Text",
    [switch]$RequireReady
)

$ErrorActionPreference = "Stop"

function Get-VersionText {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Executable,
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    try {
        $output = & $Executable @Arguments 2>&1
        if ($LASTEXITCODE -ne 0) {
            return $null
        }
        return (($output | ForEach-Object { "$_" }) -join " ").Trim()
    }
    catch {
        return $null
    }
}

function Get-ToolStatus {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [string]$VersionExecutable = $Name,
        [string[]]$VersionArguments = @("--version")
    )

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($null -eq $command) {
        return [pscustomobject][ordered]@{
            name      = $Name
            available = $false
            path      = $null
            version   = $null
        }
    }

    return [pscustomobject][ordered]@{
        name      = $Name
        available = $true
        path      = $command.Source
        version   = Get-VersionText -Executable $VersionExecutable -Arguments $VersionArguments
    }
}

$rustc = Get-ToolStatus -Name "rustc"
$cargo = Get-ToolStatus -Name "cargo"
$rustup = Get-ToolStatus -Name "rustup"
$stellar = Get-ToolStatus -Name "stellar"
$rzup = Get-ToolStatus -Name "rzup"
$cargoRiscZero = Get-ToolStatus `
    -Name "cargo-risczero" `
    -VersionExecutable "cargo" `
    -VersionArguments @("risczero", "--version")

$installedTargets = @()
if ($rustup.available) {
    try {
        $installedTargets = @(& rustup target list --installed 2>$null)
        if ($LASTEXITCODE -ne 0) {
            $installedTargets = @()
        }
    }
    catch {
        $installedTargets = @()
    }
}

$rustReady = $rustc.available -and $cargo.available -and $rustup.available
$stellarReady = $stellar.available -and ($installedTargets -contains "wasm32v1-none")
$riscZeroReady = $rzup.available -and $cargoRiscZero.available
$ready = $rustReady -and $stellarReady -and $riscZeroReady

$missing = [System.Collections.Generic.List[string]]::new()
if (-not $rustc.available) { $missing.Add("rustc") }
if (-not $cargo.available) { $missing.Add("cargo") }
if (-not $rustup.available) { $missing.Add("rustup") }
if (-not $stellar.available) { $missing.Add("stellar") }
if ($installedTargets -notcontains "wasm32v1-none") { $missing.Add("rust target wasm32v1-none") }
if (-not $rzup.available) { $missing.Add("rzup") }
if (-not $cargoRiscZero.available) { $missing.Add("cargo risczero") }

$result = [pscustomobject][ordered]@{
    schema_version = 1
    ready = $ready
    rust_ready = $rustReady
    stellar_ready = $stellarReady
    risc_zero_ready = $riscZeroReady
    missing = @($missing)
    installed_targets = $installedTargets
    tools = [pscustomobject][ordered]@{
        rustc = $rustc
        cargo = $cargo
        rustup = $rustup
        stellar = $stellar
        rzup = $rzup
        cargo_risczero = $cargoRiscZero
    }
}

if ($Format -eq "Json") {
    $result | ConvertTo-Json -Depth 5
}
else {
    Write-Output "NeuroChain ZK toolchain preflight"
    Write-Output "Rust:      $(if ($rustReady) { 'READY' } else { 'BLOCKED' })"
    Write-Output "Stellar:   $(if ($stellarReady) { 'READY' } else { 'BLOCKED' })"
    Write-Output "RISC Zero: $(if ($riscZeroReady) { 'READY' } else { 'BLOCKED' })"
    Write-Output "Overall:   $(if ($ready) { 'READY' } else { 'BLOCKED' })"
    if ($missing.Count -gt 0) {
        Write-Output "Missing:"
        foreach ($item in $missing) {
            Write-Output "- $item"
        }
    }
}

if ($RequireReady -and -not $ready) {
    exit 2
}
