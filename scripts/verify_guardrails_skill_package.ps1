param(
    [string]$SkillDir = "skills/neurochain-stellar-guardrails"
)

$ErrorActionPreference = "Stop"

$requiredFiles = @(
    "SKILL.md",
    "PACKAGING.md",
    "INSTALL.md",
    "RELEASE_CANDIDATE.md",
    "agents/openai.yaml",
    "examples/README.md",
    "examples/approved.md",
    "examples/requires_approval.md",
    "examples/blocked.md",
    "examples/state_unavailable.md"
)

$defaultTools = @(
    "plan_stellar_action",
    "evaluate_guardrails",
    "prove_guardrail_decision",
    "verify_zk_on_stellar",
    "get_guardrail_status"
)

$excludedTools = @(
    "submit_testnet_attestation",
    "consume_nullifier",
    "submit_underlying_action",
    "sign_transaction",
    "configure_server"
)

$requiredPhrases = @(
    "Plan -> Evaluate -> Prove -> Verify -> Status",
    "underlying_action_submit_allowed=false",
    "Payment, proof, read-only verification, status, or attestation evidence must",
    "never imply underlying ActionPlan submit permission",
    "x402 is not production until settlement runtime integration",
    "ZK is beyond a lite demo"
)

$forbiddenPatterns = @(
    "(?i)seed phrase",
    "(?i)private key",
    "(?i)api key",
    "(?i)hosted service token",
    "(?i)submit permission is granted",
    "(?i)payment is submit permission",
    "(?i)proof is submit permission"
)

$root = Resolve-Path -LiteralPath "."
$skillPath = Join-Path $root $SkillDir
if (-not (Test-Path -LiteralPath $skillPath -PathType Container)) {
    throw "Skill directory not found: $SkillDir"
}

$raw = ""
foreach ($relative in $requiredFiles) {
    $path = Join-Path $skillPath $relative
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Missing required skill package file: $relative"
    }
    $raw += "`n" + (Get-Content -LiteralPath $path -Raw)
}

foreach ($tool in $defaultTools) {
    if ($raw -notmatch [regex]::Escape($tool)) {
        throw "Default MCP v0 tool missing from skill package: $tool"
    }
}

foreach ($tool in $excludedTools) {
    if ($raw -notmatch [regex]::Escape($tool)) {
        throw "Excluded submit/stateful tool is not named as excluded: $tool"
    }
}

foreach ($phrase in $requiredPhrases) {
    if ($raw -notmatch [regex]::Escape($phrase)) {
        throw "Required boundary phrase missing from skill package: $phrase"
    }
}

foreach ($pattern in $forbiddenPatterns) {
    $matches = [regex]::Matches($raw, $pattern)
    foreach ($match in $matches) {
        $line = ($raw.Substring(0, $match.Index) -split "`n").Count
        $excerpt = $match.Value

        $allowed = $false
        if ($excerpt -match "(?i)seed phrase|private key|api key|hosted service token") {
            $allowed = $true
        }
        if (-not $allowed) {
            throw "Forbidden publish-surface wording found at combined line ${line}: $excerpt"
        }
    }
}

$summary = [ordered]@{
    status = "passed"
    skill_dir = $SkillDir
    required_files = $requiredFiles.Count
    default_tools = $defaultTools
    excluded_tools_named = $excludedTools
    runtime_dependency = $false
    submit_surface = $false
    secrets_included = $false
}

$summary | ConvertTo-Json -Depth 4
