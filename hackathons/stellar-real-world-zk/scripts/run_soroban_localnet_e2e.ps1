param(
    [string]$WslDistribution = "Ubuntu",
    [string]$QuickstartImage = "stellar/quickstart:testing",
    [int]$ProtocolVersion = 26,
    [ValidateSet("approved", "requires_approval", "blocked_allowlist")]
    [string]$Scenario = "approved",
    [switch]$Offline
)

$ErrorActionPreference = "Stop"

$VerifierCommit = "e8ff6ea202db195352c0141ecc533ff649393fe4"
$VerifierRepository = "https://github.com/NethermindEth/stellar-risc0-verifier"
$RunId = [Guid]::NewGuid().ToString("N").Substring(0, 8)
$ContainerName = "neurochain-zk-localnet-$RunId"
$IdentityName = "nc-zk-localnet-$RunId"
$Selector = "73c457ba"
$VerifierWasmSha256 = "f6a1f928de93db9b1e4176ef247d4a8c5d45a07a16cafd0bce9e641d7eaa03d8"
$RouterWasmSha256 = "03f6e4c26ac5d662b60b6230a1a498b8d69d726b4291d17f56812ecb81797659"
$ApplicationWasmSha256 = "dbe89ddc76717b50b66934af39cfaf3153e3cef218267c0a01ab3d4bc0a1a70f"

$ProjectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
$OutDir = Join-Path $ProjectRoot "target\localnet"
$UpstreamDir = Join-Path $OutDir "stellar-risc0-verifier"
$FixtureName = switch ($Scenario) {
    "approved" { "groth16_approved.json" }
    "requires_approval" { "groth16_requires_approval.json" }
    "blocked_allowlist" { "groth16_blocked_exit_3.json" }
}
$FixturePath = Join-Path $ProjectRoot "fixtures\$FixtureName"
$AppManifest = Join-Path $ProjectRoot "soroban\Cargo.toml"
$ExpectedDecisionStatus = switch ($Scenario) {
    "approved" { 0 }
    "requires_approval" { 2 }
    "blocked_allowlist" { 1 }
}
$ExpectedExitCode = if ($Scenario -eq "blocked_allowlist") { 3 } else { 0 }
$ExpectedNextStep = switch ($Scenario) {
    "approved" { "EligibleForSeparateApprovalFlow" }
    "requires_approval" { "RequiresApproval" }
    "blocked_allowlist" { "Blocked" }
}
$ExpectedNextStepOutput = switch ($Scenario) {
    "approved" { "eligible_for_separate_approval_flow" }
    "requires_approval" { "requires_approval" }
    "blocked_allowlist" { "blocked" }
}
$ExpectedRequiresApproval = $Scenario -eq "requires_approval"

function Invoke-Native {
    param(
        [Parameter(Mandatory = $true)][string]$Command,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    $output = & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Command failed with exit code $LASTEXITCODE"
    }
    return $output
}

function Invoke-WslRoot {
    param([Parameter(Mandatory = $true)][string[]]$Arguments)

    $wslArguments = @("-d", $WslDistribution, "-u", "root", "--") + $Arguments
    return Invoke-Native -Command "wsl.exe" -Arguments $wslArguments
}

function Invoke-Stellar {
    param([Parameter(Mandatory = $true)][string[]]$Arguments)

    return Invoke-Native -Command "stellar.exe" -Arguments $Arguments
}

function Last-OutputLine {
    param([Parameter(Mandatory = $true)]$Output)

    return [string](@($Output)[-1]).Trim()
}

function Read-JournalFields {
    param([Parameter(Mandatory = $true)][string]$JournalHex)

    $journalLengthBytes = 164
    if ($JournalHex.Length -ne ($journalLengthBytes * 2)) {
        throw "Unexpected public journal length: $($JournalHex.Length / 2) bytes"
    }

    return [PSCustomObject]@{
        ActionPlanHash = $JournalHex.Substring(60 * 2, 64)
        PolicyCommitment = $JournalHex.Substring(92 * 2, 64)
        PolicyVersion = [Convert]::ToUInt32($JournalHex.Substring(124 * 2, 8), 16)
        AuditNullifier = $JournalHex.Substring(132 * 2, 64)
    }
}

function Assert-Sha256 {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Expected
    )

    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $Expected) {
        throw "Unexpected SHA-256 for ${Path}: $actual"
    }
}

function Assert-StellarFailure {
    param(
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$Pattern,
        [Parameter(Mandatory = $true)][string]$Label
    )

    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = (& stellar.exe @Arguments 2>&1 | Out-String)
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }

    if ($exitCode -eq 0 -or $output -notmatch $Pattern) {
        throw "$Label did not fail with the expected contract error. Output: $output"
    }
}

function Wait-Localnet {
    $consecutiveHealthyChecks = 0
    for ($attempt = 0; $attempt -lt 90; $attempt++) {
        $previousPreference = $ErrorActionPreference
        $ErrorActionPreference = "SilentlyContinue"
        try {
            & stellar.exe network health --network local --quiet *> $null
            $healthSucceeded = $LASTEXITCODE -eq 0
            $networkInfo = (& stellar.exe network info --network local --output json --quiet 2>$null | Out-String)
            $infoSucceeded = $LASTEXITCODE -eq 0
        }
        finally {
            $ErrorActionPreference = $previousPreference
        }

        $protocolMatches = $networkInfo -match "`"protocol_version`":$ProtocolVersion"
        if ($healthSucceeded -and $infoSucceeded -and $protocolMatches) {
            $consecutiveHealthyChecks++
            if ($consecutiveHealthyChecks -ge 3) {
                return
            }
        }
        else {
            $consecutiveHealthyChecks = 0
        }
        Start-Sleep -Seconds 2
    }
    throw "Protocol $ProtocolVersion localnet did not become stably healthy"
}

$keeper = $null
$containerStarted = $false
$identityCreated = $false
$quickstartImageId = "not-recorded"
$previousCargoNetOffline = $env:CARGO_NET_OFFLINE

try {
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

    if ($Offline) {
        if (-not (Test-Path (Join-Path $UpstreamDir ".git"))) {
            throw "Offline mode requires the pinned verifier checkout at '$UpstreamDir'."
        }
        Invoke-Native -Command "git.exe" -Arguments @(
            "-C", $UpstreamDir, "cat-file", "-e", "$VerifierCommit`^{commit}"
        ) | Out-Null
        $env:CARGO_NET_OFFLINE = "true"
    }
    else {
        if (-not (Test-Path (Join-Path $UpstreamDir ".git"))) {
            Invoke-Native -Command "git.exe" -Arguments @(
                "clone", "--filter=blob:none", $VerifierRepository, $UpstreamDir
            ) | Out-Null
        }
        Invoke-Native -Command "git.exe" -Arguments @(
            "-C", $UpstreamDir, "fetch", "origin", $VerifierCommit, "--depth", "1"
        ) | Out-Null
    }
    Invoke-Native -Command "git.exe" -Arguments @(
        "-C", $UpstreamDir, "checkout", "--detach", $VerifierCommit
    ) | Out-Null
    $resolvedVerifierCommit = Last-OutputLine (Invoke-Native -Command "git.exe" -Arguments @(
            "-C", $UpstreamDir, "rev-parse", "HEAD"
        ))
    if ($resolvedVerifierCommit -ne $VerifierCommit) {
        throw "Verifier checkout does not match the pinned commit."
    }

    $UpstreamManifest = Join-Path $UpstreamDir "Cargo.toml"
    Invoke-Stellar -Arguments @(
        "contract", "build", "--manifest-path", $UpstreamManifest,
        "--package", "groth16-verifier", "--out-dir", $OutDir
    ) | Out-Null
    Invoke-Stellar -Arguments @(
        "contract", "build", "--manifest-path", $UpstreamManifest,
        "--package", "risc0-router", "--out-dir", $OutDir
    ) | Out-Null
    Invoke-Stellar -Arguments @(
        "contract", "build", "--manifest-path", $AppManifest,
        "--out-dir", $OutDir
    ) | Out-Null

    Assert-Sha256 -Path (Join-Path $OutDir "groth16_verifier.wasm") -Expected $VerifierWasmSha256
    Assert-Sha256 -Path (Join-Path $OutDir "risc0_router.wasm") -Expected $RouterWasmSha256
    Assert-Sha256 -Path (Join-Path $OutDir "neurochain_zk_guardrail_soroban.wasm") `
        -Expected $ApplicationWasmSha256

    $keeper = Start-Process wsl.exe -ArgumentList @(
        "-d", $WslDistribution, "--", "sleep", "infinity"
    ) -WindowStyle Hidden -PassThru
    Start-Sleep -Seconds 3
    if ($keeper.HasExited) {
        throw "WSL keeper process exited before localnet startup"
    }

    Invoke-WslRoot -Arguments @("service", "docker", "start") | Out-Null
    $dockerRunArguments = @("docker", "run")
    if ($Offline) {
        $quickstartImageId = Last-OutputLine (Invoke-WslRoot -Arguments @(
                "docker", "image", "inspect", "--format", "{{.Id}}", $QuickstartImage
            ))
        $dockerRunArguments += "--pull=never"
    }
    $dockerRunArguments += @(
        "-d", "-p", "8000:8000", "--name", $ContainerName,
        $QuickstartImage, "--local", "--protocol-version", "$ProtocolVersion",
        "--limits", "unlimited", "--enable", "rpc,horizon"
    )
    Invoke-WslRoot -Arguments $dockerRunArguments | Out-Null
    $containerStarted = $true
    if (-not $Offline) {
        $quickstartImageId = Last-OutputLine (Invoke-WslRoot -Arguments @(
                "docker", "image", "inspect", "--format", "{{.Id}}", $QuickstartImage
            ))
    }
    Wait-Localnet

    $identityCreated = $true
    Invoke-Stellar -Arguments @(
        "keys", "generate", $IdentityName, "--network", "local", "--fund", "--overwrite"
    ) | Out-Null
    $owner = Last-OutputLine (Invoke-Stellar -Arguments @("keys", "public-key", $IdentityName))

    $verifierId = Last-OutputLine (Invoke-Stellar -Arguments @(
        "contract", "deploy", "--wasm", (Join-Path $OutDir "groth16_verifier.wasm"),
        "--source", $IdentityName, "--network", "local"
    ))
    $routerId = Last-OutputLine (Invoke-Stellar -Arguments @(
        "contract", "deploy", "--wasm", (Join-Path $OutDir "risc0_router.wasm"),
        "--source", $IdentityName, "--network", "local", "--",
        "--owner", $owner
    ))
    Invoke-Stellar -Arguments @(
        "contract", "invoke", "--id", $routerId, "--source", $IdentityName,
        "--network", "local", "--send", "yes", "--", "add_verifier",
        "--selector", $Selector, "--verifier", $verifierId
    ) | Out-Null

    $fixture = Get-Content -Raw $FixturePath | ConvertFrom-Json
    $journal = Read-JournalFields -JournalHex $fixture.journal_hex
    if ($fixture.seal_hex.Substring(0, 8) -ne $Selector) {
        throw "Proof fixture selector does not match the pinned verifier selector"
    }

    $appId = Last-OutputLine (Invoke-Stellar -Arguments @(
        "contract", "deploy", "--wasm", (Join-Path $OutDir "neurochain_zk_guardrail_soroban.wasm"),
        "--source", $IdentityName, "--network", "local", "--",
        "--owner", $owner, "--verifier_router", $routerId,
        "--evaluator_image_id", $fixture.image_id_hex,
        "--initial_policy_commitment", $journal.PolicyCommitment,
        "--initial_policy_version", "$($journal.PolicyVersion)"
    ))

    $readOnlyVerifyArguments = @(
        "contract", "invoke", "--id", $appId, "--source", $IdentityName,
        "--network", "local", "--send", "no", "--instruction-leeway", "10000000",
        "--", "verify", "--seal", $fixture.seal_hex,
        "--journal_bytes", $fixture.journal_hex
    )
    $readOnlyAccepted = Last-OutputLine (Invoke-Stellar -Arguments $readOnlyVerifyArguments) |
        ConvertFrom-Json
    if ($readOnlyAccepted.action_plan_hash -ne $journal.ActionPlanHash -or
        $readOnlyAccepted.policy_commitment -ne $journal.PolicyCommitment -or
        $readOnlyAccepted.policy_version -ne $journal.PolicyVersion -or
        $readOnlyAccepted.audit_nullifier -ne $journal.AuditNullifier -or
        $readOnlyAccepted.decision_status -ne $ExpectedDecisionStatus -or
        $readOnlyAccepted.exit_code -ne $ExpectedExitCode -or
        $readOnlyAccepted.requires_approval -ne $ExpectedRequiresApproval -or
        $readOnlyAccepted.next_step -ne $ExpectedNextStep) {
        throw "Localnet read-only verification returned an unexpected attestation result"
    }

    $consumedBefore = Last-OutputLine (Invoke-Stellar -Arguments @(
        "contract", "invoke", "--id", $appId, "--source", $IdentityName,
        "--network", "local", "--send", "no", "--", "is_consumed",
        "--audit_nullifier", $journal.AuditNullifier
    ))
    if ($consumedBefore -ne "false") {
        throw "Read-only verification consumed the audit nullifier"
    }

    $verifyArguments = @(
        "contract", "invoke", "--id", $appId, "--source", $IdentityName,
        "--network", "local", "--send", "yes", "--instruction-leeway", "10000000",
        "--", "verify_and_consume", "--seal", $fixture.seal_hex,
        "--journal_bytes", $fixture.journal_hex
    )
    $accepted = Last-OutputLine (Invoke-Stellar -Arguments $verifyArguments) | ConvertFrom-Json
    if ($accepted.action_plan_hash -ne $journal.ActionPlanHash -or
        $accepted.policy_commitment -ne $journal.PolicyCommitment -or
        $accepted.policy_version -ne $journal.PolicyVersion -or
        $accepted.audit_nullifier -ne $journal.AuditNullifier -or
        $accepted.decision_status -ne $ExpectedDecisionStatus -or
        $accepted.exit_code -ne $ExpectedExitCode -or
        $accepted.requires_approval -ne $ExpectedRequiresApproval -or
        $accepted.next_step -ne $ExpectedNextStep) {
        throw "Localnet accepted an unexpected attestation result"
    }

    $consumed = Last-OutputLine (Invoke-Stellar -Arguments @(
        "contract", "invoke", "--id", $appId, "--source", $IdentityName,
        "--network", "local", "--send", "no", "--", "is_consumed",
        "--audit_nullifier", $accepted.audit_nullifier
    ))
    if ($consumed -ne "true") {
        throw "Localnet did not persist the audit nullifier"
    }

    Assert-StellarFailure -Arguments $verifyArguments `
        -Pattern "Error\(Contract, #3\)" -Label "Replay"

    $proofByte = [Convert]::ToByte($fixture.seal_hex.Substring(8, 2), 16)
    $mutatedProofByte = $proofByte -bxor 1
    $invalidSeal = $Selector + ("{0:x2}" -f $mutatedProofByte) + $fixture.seal_hex.Substring(10)
    $invalidArguments = @(
        "contract", "invoke", "--id", $appId, "--source", $IdentityName,
        "--network", "local", "--send", "yes", "--instruction-leeway", "10000000",
        "--", "verify_and_consume", "--seal", $invalidSeal,
        "--journal_bytes", $fixture.journal_hex
    )
    Assert-StellarFailure -Arguments $invalidArguments `
        -Pattern "Error\(Contract, #2\)" -Label "Invalid proof"

    Write-Output "localnet_protocol=$ProtocolVersion"
    Write-Output "offline_mode=$($Offline.ToString().ToLowerInvariant())"
    Write-Output "verifier_commit=$resolvedVerifierCommit"
    Write-Output "quickstart_image_id=$quickstartImageId"
    Write-Output "verifier_contract=$verifierId"
    Write-Output "router_contract=$routerId"
    Write-Output "application_contract=$appId"
    Write-Output "decision=$Scenario"
    Write-Output "read_only_verified=true"
    Write-Output "authorized_policy_version=$($journal.PolicyVersion)"
    Write-Output "exit_code=$ExpectedExitCode"
    Write-Output "next_step=$ExpectedNextStepOutput"
    Write-Output "nullifier_consumed=true"
    Write-Output "replay=contract_error_3"
    Write-Output "invalid_proof=contract_error_2"
    Write-Output "soroban_localnet_e2e=true"
}
finally {
    if ($identityCreated) {
        $previousPreference = $ErrorActionPreference
        $ErrorActionPreference = "SilentlyContinue"
        try {
            & stellar.exe keys rm $IdentityName *> $null
        }
        finally {
            $ErrorActionPreference = $previousPreference
        }
    }
    if ($containerStarted) {
        $previousPreference = $ErrorActionPreference
        $ErrorActionPreference = "SilentlyContinue"
        try {
            & wsl.exe -d $WslDistribution -u root -- docker rm -f $ContainerName *> $null
        }
        finally {
            $ErrorActionPreference = $previousPreference
        }
    }
    if ($null -ne $keeper -and -not $keeper.HasExited) {
        Stop-Process -Id $keeper.Id -Force
    }
    if ($null -eq $previousCargoNetOffline) {
        Remove-Item Env:CARGO_NET_OFFLINE -ErrorAction SilentlyContinue
    }
    else {
        $env:CARGO_NET_OFFLINE = $previousCargoNetOffline
    }
}
