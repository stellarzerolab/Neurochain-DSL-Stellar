param(
    [string]$InspectorCommand = ""
)

$ErrorActionPreference = "Stop"

function Resolve-Command([string]$Name) {
    $Command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($null -eq $Command) {
        return $null
    }
    return $Command.Source
}

$NodePath = Resolve-Command "node"
$NpxPath = Resolve-Command "npx"
$InspectorPath = $null

if ($InspectorCommand.Trim().Length -gt 0) {
    $InspectorPath = Resolve-Command $InspectorCommand
}
else {
    foreach ($Candidate in @("mcp-inspector", "modelcontextprotocol-inspector")) {
        $InspectorPath = Resolve-Command $Candidate
        if ($null -ne $InspectorPath) {
            break
        }
    }
}

$ExternalHostAvailable = $null -ne $InspectorPath
$MissingPort = if ($ExternalHostAvailable) {
    $null
}
else {
    "external_mcp_host_or_inspector_executable"
}

[ordered]@{
    status = if ($ExternalHostAvailable) { "ready" } else { "host_unavailable" }
    transport = "stdio"
    external_host_available = $ExternalHostAvailable
    inspector_command = $InspectorPath
    node_command = $NodePath
    npx_command = $NpxPath
    missing_port = $MissingPort
    internal_release_launch_evidence = "scripts/verify_mcp_v0_release.ps1"
    installation_attempted = $false
    runtime_dependency_added = $false
    submit_surface_added = $false
    next_action = if ($ExternalHostAvailable) {
        "Run the selected external host against the generated MCP configuration."
    }
    else {
        "Select or approve an external MCP host or Inspector before external validation."
    }
} | ConvertTo-Json -Depth 4
