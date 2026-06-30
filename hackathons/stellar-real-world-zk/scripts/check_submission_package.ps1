param(
    [ValidateSet("Text", "Json")]
    [string]$Format = "Text",
    [switch]$RunTests
)

$ErrorActionPreference = "Stop"

$ProjectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$RepoRoot = (Resolve-Path (Join-Path $ProjectRoot "..\..")).Path
$Checks = [Collections.Generic.List[object]]::new()

function Add-Check {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][bool]$Passed,
        [Parameter(Mandatory = $true)][string]$Detail
    )

    $Checks.Add([pscustomobject]@{
            name = $Name
            passed = $Passed
            detail = $Detail
        })
}

function Test-SubmissionDocument {
    param([Parameter(Mandatory = $true)][string]$RelativePath)

    $path = Join-Path $ProjectRoot $RelativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        Add-Check "document:$RelativePath" $false "missing"
        return
    }

    $raw = Get-Content -Raw -LiteralPath $path
    $placeholderFree = $raw -notmatch '(?im)\b(TODO|TBD|PLACEHOLDER|LOREM)\b'
    Add-Check "placeholders:$RelativePath" $placeholderFree $(
        if ($placeholderFree) { "none" } else { "placeholder text found" }
    )

    $fenceCount = @(Get-Content -LiteralPath $path | Where-Object { $_ -match '^```' }).Count
    $fencesBalanced = ($fenceCount % 2) -eq 0
    Add-Check "fences:$RelativePath" $fencesBalanced "count=$fenceCount"

    $base = Split-Path -Parent $path
    $brokenLinks = [Collections.Generic.List[string]]::new()
    foreach ($match in [regex]::Matches($raw, '\[[^\]]+\]\(([^)]+)\)')) {
        $target = $match.Groups[1].Value
        if ($target -match '^(https?://|#)') {
            continue
        }
        $relativeTarget = $target.Split('#')[0].Replace('/', '\')
        if (-not (Test-Path -LiteralPath (Join-Path $base $relativeTarget))) {
            $brokenLinks.Add($target)
        }
    }
    Add-Check "links:$RelativePath" ($brokenLinks.Count -eq 0) $(
        if ($brokenLinks.Count -eq 0) { "all local links resolve" } else { $brokenLinks -join ", " }
    )
}

function Test-ProofFixture {
    param([Parameter(Mandatory = $true)][string]$Name)

    $path = Join-Path $ProjectRoot "fixtures\$Name"
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        Add-Check "fixture:$Name" $false "missing"
        return
    }

    try {
        $fixture = Get-Content -Raw -LiteralPath $path | ConvertFrom-Json
        $expectedFields = @(
            "schema_version",
            "seal_hex",
            "image_id_hex",
            "journal_hex",
            "journal_digest_hex"
        )
        $actualFields = @($fixture.PSObject.Properties.Name)
        $fieldsMatch = @(Compare-Object $expectedFields $actualFields).Count -eq 0
        $shapeValid = $fixture.schema_version -eq 1 -and
            $fixture.seal_hex -match '^[0-9a-f]+$' -and
            $fixture.seal_hex.Length -gt 8 -and
            $fixture.image_id_hex -match '^[0-9a-f]{64}$' -and
            $fixture.journal_hex -match '^(?:[0-9a-f]{2})+$' -and
            $fixture.journal_digest_hex -match '^[0-9a-f]{64}$'

        [byte[]]$journalBytes = for (
            $index = 0;
            $index -lt $fixture.journal_hex.Length;
            $index += 2
        ) {
            [Convert]::ToByte($fixture.journal_hex.Substring($index, 2), 16)
        }
        $sha256 = [Security.Cryptography.SHA256]::Create()
        try {
            $digest = -join (
                $sha256.ComputeHash($journalBytes) |
                    ForEach-Object { $_.ToString("x2") }
            )
        }
        finally {
            $sha256.Dispose()
        }
        $digestMatches = $digest -eq $fixture.journal_digest_hex
        Add-Check "fixture:$Name" ($fieldsMatch -and $shapeValid -and $digestMatches) $(
            "fields=$fieldsMatch shape=$shapeValid digest=$digestMatches"
        )
    }
    catch {
        Add-Check "fixture:$Name" $false $_.Exception.Message
    }
}

foreach ($document in @(
        "README.md",
        "SUBMISSION.md",
        "ARCHITECTURE.md",
        "DEMO_SCRIPT.md",
        "SUBMISSION_CHECKLIST.md"
    )) {
    Test-SubmissionDocument $document
}

foreach ($fixture in @(
        "groth16_approved.json",
        "groth16_requires_approval.json",
        "groth16_blocked_exit_3.json"
    )) {
    Test-ProofFixture $fixture
}

$risc0Runner = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot "run_risc0_e2e.ps1")
$localnetRunner = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot "run_soroban_localnet_e2e.ps1")
foreach ($scenario in @("approved", "requires_approval", "blocked_allowlist")) {
    $present = $risc0Runner.Contains($scenario) -and $localnetRunner.Contains($scenario)
    Add-Check "scenario:$scenario" $present "present in proof and localnet runners"
}

$demoRunnerPath = Join-Path $PSScriptRoot "run_demo_rehearsal.ps1"
$demoRunnerReady = Test-Path -LiteralPath $demoRunnerPath -PathType Leaf
if ($demoRunnerReady) {
    $demoRunner = Get-Content -Raw -LiteralPath $demoRunnerPath
    $demoRunnerReady = $demoRunner.Contains("check_submission_package.ps1") -and
        $demoRunner.Contains("run_soroban_localnet_e2e.ps1") -and
        $demoRunner.Contains('switch]$IncludeLocalnet') -and
        $demoRunner.Contains('switch]$OfflineLocalnet') -and
        $localnetRunner.Contains('switch]$Offline') -and
        $localnetRunner.Contains('CARGO_NET_OFFLINE') -and
        $localnetRunner.Contains('--pull=never')
}
Add-Check "runner:demo-rehearsal" $demoRunnerReady $(
    if ($demoRunnerReady) { "proof gate plus explicit offline localnet opt-in" } else { "missing or incomplete" }
)

if ($RunTests) {
    Push-Location $RepoRoot
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        # Windows PowerShell surfaces native stderr as non-terminating error records.
        # Cargo writes normal progress there, so capture it and trust the exit code.
        $ErrorActionPreference = "Continue"
        $fixtureOutput = (& cargo test --test zk_guardrail_contract 2>&1 | Out-String)
        $fixturePassed = $LASTEXITCODE -eq 0
        Add-Check "tests:canonical-fixtures" $fixturePassed $(
            if ($fixturePassed) { "passed" } else { $fixtureOutput.Trim() }
        )

        $sorobanManifest = Join-Path $ProjectRoot "soroban\Cargo.toml"
        $proofOutput = (& cargo test --manifest-path $sorobanManifest --test groth16_proof 2>&1 | Out-String)
        $proofPassed = $LASTEXITCODE -eq 0
        Add-Check "tests:genuine-groth16" $proofPassed $(
            if ($proofPassed) { "passed" } else { $proofOutput.Trim() }
        )
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
        Pop-Location
    }
}

$failed = @($Checks | Where-Object { -not $_.passed })
$result = [pscustomobject]@{
    schema_version = 1
    package_ready = $failed.Count -eq 0
    tests_requested = [bool]$RunTests
    checks = @($Checks)
}

if ($Format -eq "Json") {
    $result | ConvertTo-Json -Depth 5
}
else {
    foreach ($check in $Checks) {
        $status = if ($check.passed) { "PASS" } else { "FAIL" }
        Write-Output "[$status] $($check.name): $($check.detail)"
    }
    Write-Output "submission_package_ready=$($result.package_ready.ToString().ToLowerInvariant())"
}

if ($failed.Count -ne 0) {
    exit 1
}
