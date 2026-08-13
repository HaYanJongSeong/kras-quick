<#
.SYNOPSIS
    kras-quick one-line installer. Downloads from GitHub Releases, verifies SHA-256, installs atomically, and runs.
#>
$ErrorActionPreference = 'Stop'
$u='https://github.com/HaYanJongSeong/kras-quick/releases/download/v0.2.1/kras_quick_v0.2.1.exe'
$d="$env:USERPROFILE\Downloads"
Invoke-WebRequest $u -OutFile "$d\kras_quick_v0.2.1.exe"
Invoke-WebRequest "$u.sha256" -OutFile "$d\kras_quick_v0.2.1.exe.sha256"
$h=((Get-Content "$d\kras_quick_v0.2.1.exe.sha256" -Raw) -split '\s+')[0]
$a=(Get-FileHash "$d\kras_quick_v0.2.1.exe" -Algorithm SHA256).Hash
if($h -ne $a){throw 'SHA-256 mismatch'}
$t="$d\kras-quick.exe.$([guid]::NewGuid().ToString('N')).tmp"
Copy-Item "$d\kras_quick_v0.2.1.exe" $t
Move-Item $t "$d\kras-quick.exe" -Force
Start-Process "$d\kras-quick.exe"
