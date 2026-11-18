param([Parameter(Mandatory=$true)][ValidateSet('baseline','candidate')][string]$Phase)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$manifest = Join-Path $root ('test8-' + $Phase + '-compile.jsonl')
$artifact = Get-Content -Encoding UTF8 -LiteralPath $manifest |
    ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.reason -eq 'compiler-artifact' -and $_.executable -and $_.target.kind -contains 'lib' } |
    Select-Object -Last 1
if (!$artifact -or !(Test-Path -LiteralPath $artifact.executable)) { throw 'BENCHMARK_EXECUTABLE_NOT_FOUND' }
$directory = Join-Path $root ('test8-bench-' + $Phase)
if (Test-Path -LiteralPath $directory) { throw 'BENCHMARK_DIRECTORY_ALREADY_EXISTS' }
[IO.Directory]::CreateDirectory($directory) | Out-Null
$results = @()
foreach ($fixture in @('rgb','rgba','document')) {
    foreach ($run in 1..3) {
        $process = [Diagnostics.Process]::new()
        try {
            $process.StartInfo.FileName = $artifact.executable
            $process.StartInfo.Arguments = '--exact cdp_surface::capture_memory_tests::synthetic_capture_benchmark --ignored --nocapture'
            $process.StartInfo.WorkingDirectory = $root
            $process.StartInfo.EnvironmentVariables['KRAS_BENCH_FIXTURE'] = $fixture
            $process.StartInfo.UseShellExecute = $false
            $process.StartInfo.CreateNoWindow = $true
            $process.StartInfo.RedirectStandardOutput = $true
            $process.StartInfo.RedirectStandardError = $true
            $process.Start() | Out-Null
            $peak = 0L
            do {
                $process.Refresh()
                $peak = [Math]::Max($peak, $process.PeakWorkingSet64)
                $ended = $process.WaitForExit(20)
            } while (!$ended)
            $text = $process.StandardOutput.ReadToEnd()
            $errorText = $process.StandardError.ReadToEnd()
            [IO.File]::WriteAllText((Join-Path $directory ($fixture + '-' + $run + '.txt')), $text)
            [IO.File]::WriteAllText((Join-Path $directory ($fixture + '-' + $run + '-stderr.txt')), $errorText)
            if ($process.ExitCode -ne 0) { throw ('BENCHMARK_FAILED: ' + $errorText) }
            $line = ($text -split "`n" | Where-Object { $_ -like 'BENCH_*' }) -join ' '
            if (!$line) { throw 'BENCHMARK_RESULT_MISSING' }
            $results += @{ fixture=$fixture; run=$run; peakWorkingSetBytes=$peak; result=$line }
        } finally { $process.Dispose() }
    }
}
$executable = if ($Phase -eq 'baseline') {
    Join-Path $env:USERPROFILE 'Downloads\kras-quick-3.0.2-test.7.exe'
} else { Join-Path $root 'rust\target\release\kras-quick.exe' }
$summary = @{
    executableBytes = (Get-Item -LiteralPath $executable).Length
    executableSHA256 = (Get-FileHash -LiteralPath $executable).Hash
    benchmarks = $results
}
$json = $summary | ConvertTo-Json -Depth 5
[IO.File]::WriteAllText((Join-Path $directory 'metrics.json'), $json)
Write-Output $json
