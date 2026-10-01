# 포터블 KRAS-quick 배포 생성 스크립트
# 사용법: powershell -File build-portable.ps1
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$dist = Join-Path $root "dist\kras-quick"
$nodeZip = Join-Path $env:TEMP "node-portable.zip"
$nodeDir = Join-Path $env:TEMP "node-portable-extract"

# 1. dist 폴더 초기화
if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
New-Item -ItemType Directory -Force -Path (Join-Path $dist "scripts") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $dist "node") | Out-Null

# 2. 파일 복사
Copy-Item (Join-Path $root "quick.bat") (Join-Path $dist "quick.bat")
Copy-Item (Join-Path $root "package.json") (Join-Path $dist "package.json")
Get-ChildItem (Join-Path $root "scripts\*.ts") | Copy-Item -Destination (Join-Path $dist "scripts")
$tesseractSource = "C:\Program Files\Tesseract-OCR"
$tesseractTarget = Join-Path $dist "tesseract"
if (-not (Test-Path (Join-Path $tesseractSource "tesseract.exe"))) {
    throw "Tesseract build source missing: $tesseractSource"
}
New-Item -ItemType Directory -Force -Path (Join-Path $tesseractTarget "tessdata") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $tesseractTarget "licenses") | Out-Null
Copy-Item (Join-Path $tesseractSource "tesseract.exe") $tesseractTarget
Get-ChildItem (Join-Path $tesseractSource "*.dll") | Copy-Item -Destination $tesseractTarget
Copy-Item (Join-Path $tesseractSource "tessdata\eng.traineddata") (Join-Path $tesseractTarget "tessdata\eng.traineddata")
Copy-Item (Join-Path $tesseractSource "doc\LICENSE") (Join-Path $tesseractTarget "licenses\TESSERACT-LICENSE")
Copy-Item (Join-Path $tesseractSource "doc\README.md") (Join-Path $tesseractTarget "licenses\TESSERACT-NOTICE.md")

# 3. 포터블 node.exe 다운로드 (없으면)
$nodeExe = Join-Path $dist "node\node.exe"
if (-not (Test-Path $nodeExe)) {
    Write-Host "포터블 node.exe 다운로드 중..."
    $url = "https://nodejs.org/dist/v24.16.0/node-v24.16.0-win-x64.zip"
    Invoke-WebRequest -Uri $url -OutFile $nodeZip
    Expand-Archive -Path $nodeZip -DestinationPath $nodeDir -Force
    Copy-Item (Join-Path $nodeDir "node-v24.16.0-win-x64\node.exe") $nodeExe
}

# 4. 의존성 설치 (cmd로 실행해 native stderr와 PowerShell의 충돌 회피)
Push-Location $dist
$env:PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1"
cmd /c "npm install --omit=dev --no-audit --no-fund 2>&1" | Out-Host
if ($LASTEXITCODE -ne 0) {
    Pop-Location
    throw "npm install 실패 (exit $LASTEXITCODE)"
}
Pop-Location

# 5. 잔여물 정리
Remove-Item -Recurse -Force $nodeDir -ErrorAction SilentlyContinue
Write-Host "완료: $dist"
Write-Host "사용법: dist\kras-quick\quick.bat"