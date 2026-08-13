<#
.SYNOPSIS
    Offline tests for the kras-quick release/install scripts.
.DESCRIPTION
    Fully self-contained: stages fake EXE + checksum assets in %TEMP%, runs the
    installer in StagingDir mode, exercises the release script's StageOnly
    checksum path, and checks the .cmd launcher. No network, no gh, no GitHub
    mutations.
.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File tests\test-installer.ps1
#>
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$installer = Join-Path $root "install\install-kras-quick.ps1"
$launcher = Join-Path $root "install\install-kras-quick.cmd"
$releaser = Join-Path $root "scripts\release-kras-quick.ps1"
foreach ($f in @($installer, $launcher, $releaser)) {
    if (-not (Test-Path -LiteralPath $f)) { throw "Missing: $f" }
}

$work = Join-Path $env:TEMP ("kras-quick-test-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $work | Out-Null

$script:failures = 0
function Assert-True([bool]$Cond, [string]$Msg) {
    if ($Cond) { Write-Host "PASS: $Msg" }
    else { Write-Host "FAIL: $Msg" -ForegroundColor Red; $script:failures++ }
}

function New-FakeAsset([string]$Dir, [string]$Name, [string]$Body, [switch]$WithGoodSha) {
    New-Item -ItemType Directory -Path $Dir -Force | Out-Null
    $exe = Join-Path $Dir $Name
    Set-Content -LiteralPath $exe -Value $Body -Encoding Ascii
    if ($WithGoodSha) {
        $hash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
        Set-Content -LiteralPath "$exe.sha256" -Value ("$hash  $Name") -Encoding Ascii
    }
    return $exe
}

try {
    # --- 1. happy path: staged install, profile preserved ---
    $stage = Join-Path $work "stage"
    $fakeExe = New-FakeAsset $stage "kras_quick_v9.9.9.exe" "fake-exe-bytes" -WithGoodSha

    $app = Join-Path $work "app"
    New-Item -ItemType Directory -Path (Join-Path $app ".kras-chrome-profile") -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $app ".kras-chrome-profile\state.json") -Value '{"keep":true}' -Encoding Ascii

    & powershell -NoProfile -ExecutionPolicy Bypass -File $installer -StagingDir $stage -InstallDir $app
    Assert-True ($LASTEXITCODE -eq 0) "staged install exits 0"

    $installed = Join-Path $app "kras-quick.exe"
    Assert-True (Test-Path -LiteralPath $installed) "kras-quick.exe installed"
    Assert-True ((Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash -eq (Get-FileHash -LiteralPath $fakeExe -Algorithm SHA256).Hash) "installed bytes match asset"
    Assert-True (Test-Path -LiteralPath (Join-Path $app ".kras-chrome-profile\state.json")) "profile folder untouched by update"

    # --- 2. tampered checksum is rejected, nothing installed ---
    $badStage = Join-Path $work "badstage"
    New-FakeAsset $badStage "kras_quick_v9.9.9.exe" "other-bytes" | Out-Null
    Set-Content -LiteralPath (Join-Path $badStage "kras_quick_v9.9.9.exe.sha256") -Value ("00" * 32 + "  kras_quick_v9.9.9.exe") -Encoding Ascii

    $badApp = Join-Path $work "badapp"
    & powershell -NoProfile -ExecutionPolicy Bypass -File $installer -StagingDir $badStage -InstallDir $badApp
    Assert-True ($LASTEXITCODE -ne 0) "bad checksum exits non-zero"
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $badApp "kras-quick.exe"))) "no install on bad checksum"

    # --- 3. .cmd wrapper works and propagates exit code ---
    $stage3 = Join-Path $work "stage3"
    New-FakeAsset $stage3 "kras_quick_v9.9.9.exe" "cmd-wrapper-bytes" -WithGoodSha | Out-Null
    $app3 = Join-Path $work "app3"
    cmd /c "$launcher -StagingDir $stage3 -InstallDir $app3"
    Assert-True ($LASTEXITCODE -eq 0) "cmd wrapper exits 0"
    Assert-True (Test-Path -LiteralPath (Join-Path $app3 "kras-quick.exe")) "cmd wrapper installs"

    # --- 4. release script StageOnly writes correct checksum ---
    $relExe = Join-Path $work "kras_quick_v1.0.0.exe"
    Set-Content -LiteralPath $relExe -Value "release-payload" -Encoding Ascii
    & powershell -NoProfile -ExecutionPolicy Bypass -File $releaser -ExePath $relExe -Tag v1.0.0 -StageOnly
    Assert-True ($LASTEXITCODE -eq 0) "release StageOnly exits 0"

    $shaLine = Get-Content -LiteralPath "$relExe.sha256" -TotalCount 1
    $expectHash = (Get-FileHash -LiteralPath $relExe -Algorithm SHA256).Hash
    Assert-True ($shaLine.StartsWith($expectHash)) "checksum file contains correct SHA-256"
    Assert-True ($shaLine.TrimEnd() -match "kras_quick_v1\.0\.0\.exe$") "checksum file names the asset"

    # --- 5. release script rejects non-exe / missing file before gh ---
    $notExe = Join-Path $work "notes.txt"
    Set-Content -LiteralPath $notExe -Value "nope" -Encoding Ascii
    & powershell -NoProfile -ExecutionPolicy Bypass -File $releaser -ExePath $notExe -Tag v1.0.0 -StageOnly
    Assert-True ($LASTEXITCODE -ne 0) "non-exe rejected"
    & powershell -NoProfile -ExecutionPolicy Bypass -File $releaser -ExePath (Join-Path $work "missing.exe") -Tag v1.0.0 -StageOnly
    Assert-True ($LASTEXITCODE -ne 0) "missing exe rejected"
}
finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

$resultFile = "$env:TEMP\kras-quick-test-result.txt"
if ($script:failures -gt 0) {
    [System.IO.File]::WriteAllText($resultFile, "FAILED: $($script:failures) assertion(s)`r`n")
    Write-Host ("FAILED: {0} assertion(s)" -f $script:failures) -ForegroundColor Red
    exit 1
}
[System.IO.File]::WriteAllText($resultFile, "PASS - all assertions OK`r`n")
Write-Host "All tests passed." -ForegroundColor Green
exit 0
