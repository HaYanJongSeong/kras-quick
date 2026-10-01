<#
.SYNOPSIS
    kras-quick v2.0.2 one-line installer. Downloads EXE and runtime, verifies SHA-256, installs, and runs.
    Run: irm https://raw.githubusercontent.com/HaYanJongSeong/kras-quick/main/install.ps1 | iex
#>
$ErrorActionPreference = 'Stop'
$version = '2.0.2'
$base = "https://github.com/HaYanJongSeong/kras-quick/releases/download/v$version"
$d = Join-Path $env:USERPROFILE 'Downloads'
$tmp = Join-Path $d ('.kras-quick.' + [guid]::NewGuid().ToString('N'))
$exe = "$tmp.exe"
$sum = "$tmp.exe.sha256"
$runtime = "$tmp.runtime.zip"
$runtimeSum = "$tmp.runtime.zip.sha256"
New-Item -ItemType Directory -Force -Path $d | Out-Null
try {
    Write-Host "Downloading kras-quick v$version..."
    Invoke-WebRequest "$base/kras-quick.exe" -OutFile $exe
    Invoke-WebRequest "$base/kras-quick.exe.sha256" -OutFile $sum
    $expected = ((Get-Content $sum -Raw) -split '\s+')[0].ToUpperInvariant()
    $actual = (Get-FileHash $exe -Algorithm SHA256).Hash.ToUpperInvariant()
    if ($expected -ne $actual) { throw "SHA-256 mismatch for kras-quick.exe" }

    Write-Host 'Downloading runtime...'
    Invoke-WebRequest "$base/kras-quick-runtime-v$version.zip" -OutFile $runtime
    Invoke-WebRequest "$base/kras-quick-runtime-v$version.zip.sha256" -OutFile $runtimeSum
    $expected = ((Get-Content $runtimeSum -Raw) -split '\s+')[0].ToUpperInvariant()
    $actual = (Get-FileHash $runtime -Algorithm SHA256).Hash.ToUpperInvariant()
    if ($expected -ne $actual) { throw "SHA-256 mismatch for runtime" }

    Move-Item $exe (Join-Path $d 'kras-quick.exe') -Force
    Move-Item $runtime (Join-Path $d "kras-quick-runtime-v$version.zip") -Force
    Write-Host "Installed to $d"
    Start-Process (Join-Path $d 'kras-quick.exe')
} finally {
    Remove-Item $exe, $sum, $runtime, $runtimeSum -Force -ErrorAction SilentlyContinue
}
