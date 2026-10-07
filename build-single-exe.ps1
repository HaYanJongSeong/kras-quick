# Creates a small kras-quick launcher plus an external runtime ZIP. Chrome remains external.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$source = Join-Path $root "source"
$runtime = Join-Path $root "dist\kras-quick"
$staging = Join-Path $env:TEMP "kras-quick-runtime"
$version = "2.0.3"
$payload = Join-Path $root "dist\kras-quick-runtime-v$version.zip"
$metaPath = Join-Path $root "kras-quick-runtime.meta.json"
$output = Join-Path $root "dist\kras_quick.exe"

if (-not (Test-Path (Join-Path $runtime "node\node.exe"))) {
    throw "dist\kras-quick runtime missing. Run build-portable.ps1 first."
}
if (Test-Path $staging) { Remove-Item -Recurse -Force $staging }
New-Item -ItemType Directory -Force -Path (Join-Path $staging "scripts") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $staging "node") | Out-Null
Copy-Item (Join-Path $runtime "node\node.exe") (Join-Path $staging "node\node.exe")
# EXE payload는 quick 런타임(property-auto-runner) import 체인만 포함.
# 데모(captcha-ansi-demo), MCP 브리지(kras-bridge/kras-runner/kras-lookup),
# 독립 도구(kras-excel-copy, scp-usage-watcher), 타입 선언은 payload에서 제외.
$payloadScripts = @(
    "kras-auto-stages.ts", "kras-dark.ts", "kras-domain.ts", "kras-excel-updater.ts",
    "kras-lawdong.ts", "kras-oz.ts", "kras-page.ts", "kras-pnu.ts", "kras-tesseract.ts",
    "kras-workflow-watcher.ts",
    "property-address.ts", "property-auto-output.ts", "property-auto-runner.ts",
    "property-progress.ts", "property-progress-windows.ts",
    "property-watcher.ts", "property-watcher-cdp.ts", "property-watcher-channel.ts",
    "property-watcher-channel-state.ts", "property-watcher-control.ts",
    "property-watcher-guidance.ts", "property-watcher-lifecycle.ts",
    "property-watcher-listener.ts", "property-watcher-page.ts",
    "property-watcher-payload.ts", "property-watcher-process.ts",
    "property-watcher-runner.ts", "property-watcher-runtime.ts",
    "property-watcher-start.ts", "property-watcher-state.ts", "terminal-reader.ts"
)
foreach ($name in $payloadScripts) {
    Copy-Item (Join-Path $source "scripts\$name") (Join-Path $staging "scripts")
}
$tesseractSource = "C:\Program Files\Tesseract-OCR"
$tesseractTarget = Join-Path $staging "tesseract"
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
$quickPackage = @{
    type = "module"
    dependencies = @{
        "pdf-lib" = "1.17.1"
        "playwright-core" = "1.61.1"
        zod = "4.1.8"
    }
} | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText((Join-Path $staging "package.json"), $quickPackage, [System.Text.UTF8Encoding]::new($false))
Push-Location $staging
$env:PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1"
cmd /c "npm install --omit=dev --no-audit --no-fund 2>&1" | Out-Host
if ($LASTEXITCODE -ne 0) {
    Pop-Location
    throw "quick runtime npm install failed (exit $LASTEXITCODE)"
}
Pop-Location
if (Test-Path $payload) { Remove-Item -Force $payload }
Compress-Archive -Path (Join-Path $staging "*") -DestinationPath $payload -CompressionLevel Optimal
$runtimeSha256 = (Get-FileHash -LiteralPath $payload -Algorithm SHA256).Hash
$meta = [ordered]@{
    "//" = "GENERATED FILE - overwritten by build-single-exe.ps1"
    version = $version
    url = "https://github.com/HaYanJongSeong/kras-quick/releases/download/v$version/kras-quick-runtime-v$version.zip"
    sha256 = $runtimeSha256
} | ConvertTo-Json
[System.IO.File]::WriteAllText($metaPath, $meta, [System.Text.UTF8Encoding]::new($false))
Write-Host "Runtime ZIP: $payload"
Write-Host "Runtime SHA-256: $runtimeSha256"

  bun build --compile --target=bun-windows-x64 (Join-Path $source "scripts\kras-quick-launcher.ts") --outfile $output --windows-icon (Join-Path $root "kras-quick.ico") --windows-title "KRAS Quick" --windows-publisher "HaYanJongSeong" --windows-version "2.0.3.0" --windows-description "KRAS certificate one-line installer and OZ viewer capture" --windows-copyright "Copyright (c) 2026 HaYanJongSeong"
if ($LASTEXITCODE -ne 0) { throw "single EXE build failed" }

Remove-Item -Recurse -Force $staging
Write-Host "완료: $output"
