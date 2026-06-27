param(
    [ValidateSet("Text", "Json")]
    [string]$Format = "Text",
    [string]$WslDistribution = "Ubuntu",
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

function Get-WslText {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Distribution,
        [Parameter(Mandatory = $true)]
        [string]$Executable,
        [string[]]$Arguments = @()
    )

    try {
        $output = & wsl -d $Distribution -- $Executable @Arguments 2>&1
        if ($LASTEXITCODE -ne 0) {
            return $null
        }
        return (($output | ForEach-Object { "$_" }) -join " ").Trim()
    }
    catch {
        return $null
    }
}

$rustc = Get-ToolStatus -Name "rustc"
$cargo = Get-ToolStatus -Name "cargo"
$rustup = Get-ToolStatus -Name "rustup"
$stellar = Get-ToolStatus -Name "stellar"
$rzup = Get-ToolStatus -Name "rzup"
$r0vm = Get-ToolStatus -Name "r0vm"
$cargoRiscZero = Get-ToolStatus `
    -Name "cargo-risczero" `
    -VersionExecutable "cargo" `
    -VersionArguments @("risczero", "--version")
$wsl = Get-ToolStatus -Name "wsl"
$wsl.version = $null

$wslRiscZero = [pscustomobject][ordered]@{
    distribution = $WslDistribution
    available = $false
    rzup_path = $null
    cargo_risczero_path = $null
    r0vm_path = $null
    cargo_risczero_version = $null
}
if ($wsl.available) {
    $wslHome = Get-WslText `
        -Distribution $WslDistribution `
        -Executable "printenv" `
        -Arguments @("HOME")
    if (-not [string]::IsNullOrWhiteSpace($wslHome)) {
        $candidateRzup = "$wslHome/.risc0/bin/rzup"
        $candidateCargo = "$wslHome/.cargo/bin/cargo"
        $candidateCargoRiscZero = "$wslHome/.cargo/bin/cargo-risczero"
        $candidateR0vm = "$wslHome/.cargo/bin/r0vm"
        $wslRzupVersion = Get-WslText `
            -Distribution $WslDistribution `
            -Executable $candidateRzup `
            -Arguments @("--version")
        $wslCargoRiscZeroVersion = Get-WslText `
            -Distribution $WslDistribution `
            -Executable $candidateCargo `
            -Arguments @("risczero", "--version")
        $wslR0vmVersion = Get-WslText `
            -Distribution $WslDistribution `
            -Executable $candidateR0vm `
            -Arguments @("--version")
        if (-not [string]::IsNullOrWhiteSpace($wslRzupVersion)) {
            $wslRzupPath = $candidateRzup
        }
        if (-not [string]::IsNullOrWhiteSpace($wslCargoRiscZeroVersion)) {
            $wslCargoRiscZeroPath = $candidateCargoRiscZero
        }
        if (-not [string]::IsNullOrWhiteSpace($wslR0vmVersion)) {
            $wslR0vmPath = $candidateR0vm
        }
    }
    $wslRiscZero = [pscustomobject][ordered]@{
        distribution = $WslDistribution
        available = (
            -not [string]::IsNullOrWhiteSpace($wslRzupPath) -and
            -not [string]::IsNullOrWhiteSpace($wslCargoRiscZeroPath) -and
            -not [string]::IsNullOrWhiteSpace($wslR0vmPath) -and
            -not [string]::IsNullOrWhiteSpace($wslCargoRiscZeroVersion)
        )
        rzup_path = $wslRzupPath
        cargo_risczero_path = $wslCargoRiscZeroPath
        r0vm_path = $wslR0vmPath
        cargo_risczero_version = $wslCargoRiscZeroVersion
    }
}

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
$nativeRiscZeroReady = $rzup.available -and $cargoRiscZero.available -and $r0vm.available
$riscZeroReady = $nativeRiscZeroReady -or $wslRiscZero.available
$riscZeroEnvironment = if ($nativeRiscZeroReady) {
    "native"
}
elseif ($wslRiscZero.available) {
    "wsl:$WslDistribution"
}
else {
    "missing"
}
$ready = $rustReady -and $stellarReady -and $riscZeroReady

$missing = [System.Collections.Generic.List[string]]::new()
if (-not $rustc.available) { $missing.Add("rustc") }
if (-not $cargo.available) { $missing.Add("cargo") }
if (-not $rustup.available) { $missing.Add("rustup") }
if (-not $stellar.available) { $missing.Add("stellar") }
if ($installedTargets -notcontains "wasm32v1-none") { $missing.Add("rust target wasm32v1-none") }
if (-not $riscZeroReady) {
    $missing.Add("RISC Zero toolchain (rzup, cargo risczero, r0vm)")
}

$result = [pscustomobject][ordered]@{
    schema_version = 1
    ready = $ready
    rust_ready = $rustReady
    stellar_ready = $stellarReady
    risc_zero_ready = $riscZeroReady
    risc_zero_environment = $riscZeroEnvironment
    missing = @($missing)
    installed_targets = $installedTargets
    tools = [pscustomobject][ordered]@{
        rustc = $rustc
        cargo = $cargo
        rustup = $rustup
        stellar = $stellar
        rzup = $rzup
        r0vm = $r0vm
        cargo_risczero = $cargoRiscZero
        wsl = $wsl
        wsl_risc_zero = $wslRiscZero
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
    Write-Output "RISC env:  $riscZeroEnvironment"
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
