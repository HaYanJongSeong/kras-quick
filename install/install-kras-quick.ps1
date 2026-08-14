<#
.SYNOPSIS
    Installs kras-quick.exe from a private GitHub release (HaYanJongSeong/kras-quick).
.DESCRIPTION
    Downloads the EXE + .sha256 assets (or reads them from -StagingDir for
    offline testing), verifies each SHA-256 checksum, then installs atomically
    as kras-quick.exe in -InstallDir. When the release also ships a runtime zip
    (kras-quick-runtime-*.zip + .sha256) it is verified and installed as
    kras-quick-runtime.zip the same way. Only the target files are replaced;
    every
    other file in the install directory (.kras-chrome-profile, KRAS output, ...)
    is left untouched so profiles survive updates.
.PARAMETER Repo
    GitHub repo, default HaYanJongSeong/kras-quick.
.PARAMETER Version
    Release tag, default latest.
.PARAMETER InstallDir
    Directory that receives kras-quick.exe. Default: user Downloads (where the
    current EXE and its executable-relative profile live). Override for other setups.
.PARAMETER StagingDir
    Local directory containing the exe + .sha256 assets. Offline testing only;
    skips all network calls.
.EXAMPLE
    install\install-kras-quick.ps1
.EXAMPLE
    install\install-kras-quick.ps1 -StagingDir .\stage -InstallDir .\app
#>
[CmdletBinding()]
param(
    [string]$Repo = "HaYanJongSeong/kras-quick",
    [string]$Version = "latest",
    [string]$InstallDir = (Join-Path $env:USERPROFILE "Downloads"),
    [string]$StagingDir = ""
)

$ErrorActionPreference = "Stop"

function Get-GhExe {
    $cmd = Get-Command gh -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $known = "C:\Program Files\GitHub CLI\gh.exe"
    if (Test-Path -LiteralPath $known) { return $known }
    throw "gh CLI not found. Install from https://cli.github.com/"
}

try {
    $tmp = Join-Path $env:TEMP ("kras-quick-install-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $tmp | Out-Null

    # --- 1. Obtain assets (staging = offline; otherwise gh release download) ---
    if ($StagingDir) {
        $src = (Resolve-Path -LiteralPath $StagingDir).Path
        $exe = Get-ChildItem -LiteralPath $src -Filter *.exe -File | Select-Object -First 1
        if (-not $exe) { throw "Staging dir needs one .exe asset: $src" }
        # Pair checksums by exact asset name so a runtime .sha256 can never be
        # picked (wildcard-first) to verify the EXE.
        $exeSha = Join-Path $src ($exe.Name + ".sha256")
        if (-not (Test-Path -LiteralPath $exeSha)) { throw "Staging dir needs $($exe.Name).sha256: $src" }
        # Runtime zip is optional: handled only when it (and its checksum) exist.
        $zip = Get-ChildItem -LiteralPath $src -Filter *.zip -File | Select-Object -First 1
        $zipSha = $null
        if ($zip) {
            $zipSha = Join-Path $src ($zip.Name + ".sha256")
            if (-not (Test-Path -LiteralPath $zipSha)) { throw "Staging dir needs $($zip.Name).sha256: $src" }
        }
    } else {
        $gh = Get-GhExe
        & $gh auth status *> $null
        if ($LASTEXITCODE -ne 0) { throw "gh is not authenticated. Run 'gh auth login' first (token stays in gh's own store, never in args/env)." }
        & $gh release download $Version --repo $Repo --pattern "*.exe" --pattern "*.sha256" --dir $tmp
        if ($LASTEXITCODE -ne 0) { throw "gh release download failed (exit $LASTEXITCODE)." }
        $exe = Get-ChildItem -LiteralPath $tmp -Filter *.exe -File | Select-Object -First 1
        if (-not $exe) { throw "Release $Version has no exe asset." }
        $exeSha = Join-Path $tmp ($exe.Name + ".sha256")
        if (-not (Test-Path -LiteralPath $exeSha)) { throw "Release $Version has no $($exe.Name).sha256 asset." }
        # Runtime zip is optional: fetch it only when the release ships its checksum.
        $zip = $null; $zipSha = $null
        if (Get-ChildItem -LiteralPath $tmp -Filter *.zip.sha256 -File | Select-Object -First 1) {
            & $gh release download $Version --repo $Repo --pattern "*.zip" --dir $tmp
            if ($LASTEXITCODE -ne 0) { throw "gh release download (runtime zip) failed (exit $LASTEXITCODE)." }
            $zip = Get-ChildItem -LiteralPath $tmp -Filter *.zip -File | Select-Object -First 1
            if (-not $zip) { throw "Release $Version runtime zip missing." }
            $zipSha = Join-Path $tmp ($zip.Name + ".sha256")
            if (-not (Test-Path -LiteralPath $zipSha)) { throw "Release $Version has no $($zip.Name).sha256 asset." }
        }
    }

    # --- 2. Verify SHA-256 (each asset against its own checksum) ---
    $expected = ((Get-Content -LiteralPath $exeSha -TotalCount 1).Trim() -split "\s+")[0]
    $actual = (Get-FileHash -LiteralPath $exe.FullName -Algorithm SHA256).Hash
    if ($expected -ne $actual) {
        throw "SHA-256 mismatch. expected=$expected actual=$actual asset=$($exe.Name)"
    }
    Write-Host "Checksum OK: $($exe.Name) ($actual)"
    if ($zip) {
        $expected = ((Get-Content -LiteralPath $zipSha -TotalCount 1).Trim() -split "\s+")[0]
        $actual = (Get-FileHash -LiteralPath $zip.FullName -Algorithm SHA256).Hash
        if ($expected -ne $actual) {
            throw "SHA-256 mismatch. expected=$expected actual=$actual asset=$($zip.Name)"
        }
        Write-Host "Checksum OK: $($zip.Name) ($actual)"
    }

    # --- 3. Atomic install: copy to temp name, rename over target ---
    # Only the target files are touched; sibling profile/output folders are preserved.
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $target = Join-Path $InstallDir "kras-quick.exe"
    $tmpTarget = Join-Path $InstallDir ("kras-quick.exe." + [guid]::NewGuid().ToString("N") + ".tmp")
    Copy-Item -LiteralPath $exe.FullName -Destination $tmpTarget -Force
    Move-Item -LiteralPath $tmpTarget -Destination $target -Force

    Write-Host "Installed kras-quick.exe -> $target"
    if ($zip) {
        $zipTarget = Join-Path $InstallDir "kras-quick-runtime.zip"
        $tmpZipTarget = Join-Path $InstallDir ("kras-quick-runtime.zip." + [guid]::NewGuid().ToString("N") + ".tmp")
        Copy-Item -LiteralPath $zip.FullName -Destination $tmpZipTarget -Force
        Move-Item -LiteralPath $tmpZipTarget -Destination $zipTarget -Force
        Write-Host "Installed kras-quick-runtime.zip -> $zipTarget"
    }
    exit 0
} catch {
    Write-Host "ERROR: $_" -ForegroundColor Red
    exit 1
} finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
