param(
    [string]$WslDistribution = "Ubuntu"
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
& wsl.exe -d $WslDistribution --cd $risc0WslPath -- env -u RISC0_DEV_MODE "PATH=$toolPath" cargo run --release -p neurochain-zk-risc0-host
if ($LASTEXITCODE -ne 0) {
    throw "RISC Zero end-to-end run failed with exit code $LASTEXITCODE."
}
