param(
    [string]$Cargo = "cargo",
    [string]$HostConfigOut = "target\release\neurochain-mcp-v0-host.json",
    [string]$SkillDir = "skills/neurochain-stellar-guardrails"
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

function Fail([string]$Message) {
    throw "Guardrails skill release candidate verification failed: $Message"
}

function Read-JsonOutput([scriptblock]$Command, [string]$Label) {
    $Raw = (& $Command | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) {
        Fail "$Label exited with $LASTEXITCODE"
    }
    try {
        return $Raw | ConvertFrom-Json
    }
    catch {
        Fail "$Label did not return valid JSON: $Raw"
    }
}

Push-Location $RepoRoot
try {
    $Release = Read-JsonOutput {
        & (Join-Path $PSScriptRoot "verify_mcp_v0_release.ps1") `
            -Cargo $Cargo `
            -HostConfigOut $HostConfigOut
    } "MCP v0 release gate"

    if ($Release.status -ne "passed") {
        Fail "MCP release status was not passed"
    }
    if ($Release.mode -ne "read_only_no_submit") {
        Fail "MCP release mode must be read_only_no_submit"
    }
    if ($null -eq $Release.host_config -or -not [bool]$Release.host_config.validated_by_launch) {
        Fail "MCP host config must be validated by launch"
    }
    if ([bool]$Release.host_config.secrets_included) {
        Fail "MCP host config must not include secrets"
    }
    if ([bool]$Release.host_config.submit_tools_included) {
        Fail "MCP host config must not include submit tools"
    }

    $Skill = Read-JsonOutput {
        & (Join-Path $PSScriptRoot "verify_guardrails_skill_package.ps1") `
            -SkillDir $SkillDir
    } "Guardrails skill package check"

    if ($Skill.status -ne "passed") {
        Fail "skill package status was not passed"
    }
    if ([bool]$Skill.runtime_dependency) {
        Fail "skill package must not be a runtime dependency"
    }
    if ([bool]$Skill.submit_surface) {
        Fail "skill package must not be a submit surface"
    }
    if ([bool]$Skill.secrets_included) {
        Fail "skill package must not include secrets"
    }

    [ordered]@{
        status = "passed"
        package = "neurochain-stellar-guardrails"
        published = $false
        release_candidate = $true
        runtime_dependency = $false
        submit_surface = $false
        mcp = [ordered]@{
            status = $Release.status
            mode = $Release.mode
            protocol_version = $Release.protocol_version
            conformance_cases = $Release.conformance_cases
            validated_by_launch = [bool]$Release.host_config.validated_by_launch
            secrets_included = [bool]$Release.host_config.secrets_included
            submit_tools_included = [bool]$Release.host_config.submit_tools_included
            tools = @($Release.tools)
            artifacts = @($Release.artifacts)
        }
        skill = [ordered]@{
            status = $Skill.status
            skill_dir = $Skill.skill_dir
            required_files = $Skill.required_files
            default_tools = @($Skill.default_tools)
            excluded_tools_named = @($Skill.excluded_tools_named)
            runtime_dependency = [bool]$Skill.runtime_dependency
            submit_surface = [bool]$Skill.submit_surface
            secrets_included = [bool]$Skill.secrets_included
        }
    } | ConvertTo-Json -Depth 8
}
finally {
    Pop-Location
}
