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
Write-Host "${DIM}  version v1.0.0 · github.com/HaYanJongSeong/kras-quick${RST}"
Write-Host ''

$u = 'https://github.com/HaYanJongSeong/kras-quick/releases/download/v1.0.0/kras_quick_v1.0.0.exe'
$d = "$env:USERPROFILE\Downloads"
$tmp = "$d\.kras-quick.$([guid]::NewGuid().ToString('N')).tmp"
$exe = "$tmp.exe"
$sum = "$tmp.sha256"

step "Downloading kras-quick (~136 MB) ..."
Invoke-WebRequest $u -OutFile $exe
ok 'Downloaded'

step "Downloading SHA-256 checksum ..."
Invoke-WebRequest "$u.sha256" -OutFile $sum
ok 'Checksum fetched'

step 'Verifying SHA-256 ...'
$h = ((Get-Content $sum -Raw) -split '\s+')[0]
$a = (Get-FileHash $exe -Algorithm SHA256).Hash
if ($h -ne $a) {
    Remove-Item $exe, $sum -Force -ErrorAction SilentlyContinue
    Write-Host "${RED}${BOLD}  ✗ SHA-256 MISMATCH${RST}${RED} — download corrupted, aborting.${RST}"
    throw 'SHA-256 mismatch'
}
ok 'Checksum verified'

step 'Installing ...'
Move-Item $exe "$d\kras-quick.exe" -Force
Remove-Item $sum -Force -ErrorAction SilentlyContinue
done 'Installed to Downloads\kras-quick.exe'

Write-Host ''
Write-Host "${GRN}${BOLD}  ✔ KRAS-QUICK READY${RST}"
Write-Host "${DIM}  Launching ...${RST}"
Write-Host ''
Start-Process "$d\kras-quick.exe"