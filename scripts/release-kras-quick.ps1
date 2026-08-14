<#
.SYNOPSIS
    Publishes kras-quick.exe to a private draft release on GitHub.
.DESCRIPTION
    Computes SHA-256 checksums, writes <exe>.sha256 (and, when
    kras-quick-runtime-<tag>.zip sits next to the EXE, its .sha256 too), then
    creates a DRAFT release on HaYanJongSeong/kras-quick (private repo =>
    private release) and uploads the assets. Use -StageOnly to write only the
    checksums (offline test seam). Auth comes from gh's own stored credentials -
    never pass a token as an argument or env var.
.PARAMETER ExePath
    Path to the built EXE, e.g. C:\Users\admin\Downloads\kras_quick_v0.2.1.exe.
.PARAMETER Tag
    Release tag, e.g. v0.2.1.
.PARAMETER Repo
    GitHub repo, default HaYanJongSeong/kras-quick.
.PARAMETER Title
    Release title; defaults to the tag.
.PARAMETER Notes
    Release notes text.
.PARAMETER Publish
    Skip --draft (release becomes immediately visible). Default is draft.
.PARAMETER StageOnly
    Only write the .sha256 file next to the EXE; no gh calls. For offline tests.
.EXAMPLE
    scripts\release-kras-quick.ps1 -ExePath C:\Users\admin\Downloads\kras_quick_v0.2.1.exe -Tag v0.2.1
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ExePath,
    [Parameter(Mandatory = $true)][string]$Tag,
    [string]$Repo = "HaYanJongSeong/kras-quick",
    [string]$Title = "",
    [string]$Notes = "",
    [switch]$Publish,
    [switch]$StageOnly
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
    if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) { throw "EXE not found: $ExePath" }
    if ([System.IO.Path]::GetExtension($ExePath) -ne ".exe") { throw "Not an .exe: $ExePath" }

    # --- 1. Checksums (EXE + optional runtime zip) ---
    $hash = (Get-FileHash -LiteralPath $ExePath -Algorithm SHA256).Hash
    $exeLeaf = Split-Path -Leaf $ExePath
    $shaFile = "$ExePath.sha256"
    Set-Content -LiteralPath $shaFile -Value ("$hash  $exeLeaf") -Encoding Ascii
    Write-Host "Checksum: $hash"
    Write-Host "Wrote:    $shaFile"

    # Runtime zip asset named kras-quick-runtime-<tag>.zip next to the EXE.
    # Handled only when the file actually exists (exe-only releases stay valid).
    $zipPath = Join-Path (Split-Path -Parent $ExePath) ("kras-quick-runtime-$Tag.zip")
    $zipShaFile = "$zipPath.sha256"
    $hasZip = Test-Path -LiteralPath $zipPath
    if ($hasZip) {
        $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash
        $zipLeaf = Split-Path -Leaf $zipPath
        Set-Content -LiteralPath $zipShaFile -Value ("$zipHash  $zipLeaf") -Encoding Ascii
        Write-Host "Checksum: $zipHash"
        Write-Host "Wrote:    $zipShaFile"
    }

    if ($StageOnly) { Write-Host "StageOnly - no gh calls."; exit 0 }

    # --- 2. Create draft release + upload assets ---
    $gh = Get-GhExe
    & $gh auth status *> $null
    if ($LASTEXITCODE -ne 0) { throw "gh is not authenticated. Run 'gh auth login' first (token stays in gh's own store, never in args/env)." }

    $ghArgs = @($Tag, $ExePath, $shaFile, "--repo", $Repo)
    if ($hasZip) { $ghArgs += @($zipPath, $zipShaFile) }
    if (-not $Publish) { $ghArgs += "--draft" }
    if ($Title) { $ghArgs += @("--title", $Title) }
    if ($Notes) { $ghArgs += @("--notes", $Notes) }
    & $gh release create @ghArgs
    if ($LASTEXITCODE -ne 0) { throw "gh release create failed (exit $LASTEXITCODE)." }

    # --- 3. Verify assets landed ---
    $assets = & $gh release view $Tag --repo $Repo --json assets -q ".assets[].name"
    if ($LASTEXITCODE -ne 0) { throw "gh release view failed (exit $LASTEXITCODE)." }
    $assetsText = $assets -join "`n"
    $expectedAssets = @($exeLeaf, "$exeLeaf.sha256")
    if ($hasZip) { $expectedAssets += @($zipLeaf, "$zipLeaf.sha256") }
    foreach ($leaf in $expectedAssets) {
        if (-not ($assetsText -match [regex]::Escape($leaf))) { throw "Asset missing from release: $leaf" }
    }
    Write-Host "Draft release $Tag ready: https://github.com/$Repo/releases"
    exit 0
} catch {
    Write-Host "ERROR: $_" -ForegroundColor Red
    exit 1
}
