<#
.SYNOPSIS
    kras-quick one-line installer. Downloads from GitHub Releases, verifies SHA-256, installs atomically, and runs.
    Run: irm https://raw.githubusercontent.com/HaYanJongSeong/kras-quick/main/install.ps1 | iex
#>
$ErrorActionPreference = 'Stop'
$e = [char]27
function c([string]$code) { "$e[$($code)m" }
$BOLD = c '1'; $DIM = c '2'; $RST = c '0'
$CYAN = c '36'; $GRN = c '32'; $YEL = c '33'; $RED = c '31'; $BLU = c '34'
function ok($m) { "${GRN}  OK${RST}  $m" }
function step($m) { "${CYAN}  >>${RST}  $m" }
function done($m) { "${GRN}  DONE${RST}  $m" }

Write-Host ''
Write-Host "${CYAN}${BOLD}  ╭──────────────────────────────────────╮${RST}"
Write-Host "${CYAN}${BOLD}  │   KRAS-QUICK  ·  ONE-LINE INSTALLER   │${RST}"
Write-Host "${CYAN}${BOLD}  ╰──────────────────────────────────────╯${RST}"
Write-Host "${DIM}  version v0.2.2 · github.com/HaYanJongSeong/kras-quick${RST}"
Write-Host ''

$u = 'https://github.com/HaYanJongSeong/kras-quick/releases/download/v0.2.2/kras_quick_v0.2.2.exe'
$d = "$env:USERPROFILE\Downloads"
$exe = "$d\kras_quick_v0.2.2.exe"
$sum = "$d\kras_quick_v0.2.2.exe.sha256"

step "Downloading kras-quick (~136 MB) ..."
Invoke-WebRequest $u -OutFile $exe
ok "Downloaded: $(Split-Path $exe -Leaf)"

step "Downloading SHA-256 checksum ..."
Invoke-WebRequest "$u.sha256" -OutFile $sum
ok 'Checksum fetched'

step 'Verifying SHA-256 ...'
$h = ((Get-Content $sum -Raw) -split '\s+')[0]
$a = (Get-FileHash $exe -Algorithm SHA256).Hash
if ($h -ne $a) {
    Write-Host "${RED}${BOLD}  ✗ SHA-256 MISMATCH${RST}${RED} — download corrupted, aborting.${RST}"
    throw 'SHA-256 mismatch'
}
ok 'Checksum verified'

step 'Installing ...'
$t = "$d\kras-quick.exe.$([guid]::NewGuid().ToString('N')).tmp"
Copy-Item $exe $t
Move-Item $t "$d\kras-quick.exe" -Force
done 'Installed to Downloads\kras-quick.exe'

Write-Host ''
Write-Host "${GRN}${BOLD}  ✔ KRAS-QUICK READY${RST}"
Write-Host "${DIM}  Launching ...${RST}"
Write-Host ''
Start-Process "$d\kras-quick.exe"