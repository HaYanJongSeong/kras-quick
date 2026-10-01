@echo off
chcp 65001 >nul
setlocal
cd /d "%~dp0"

REM ===== node 찾기 (포터블 node 우선) =====
set "NODE=node"
if exist "%~dp0node\node.exe" set "NODE=%~dp0node\node.exe"

REM ===== Chrome CDP 확인 및 시작 =====
powershell -NoProfile -Command "try { Invoke-WebRequest -Uri 'http://127.0.0.1:9222/json/version' -TimeoutSec 2 -UseBasicParsing | Out-Null; exit 0 } catch { exit 1 }" >nul 2>&1
if errorlevel 1 (
  set "CHROME="
  for %%P in ("%ProgramFiles%\Google\Chrome\Application\chrome.exe" "%ProgramFiles(x86)%\Google\Chrome\Application\chrome.exe" "%LOCALAPPDATA%\Google\Chrome\Application\chrome.exe") do if exist "%%~P" if not defined CHROME set "CHROME=%%~P"
  if not defined CHROME (
    echo [오류] Google Chrome을 찾을 수 없습니다.
    echo Chrome 설치 후 다시 실행하세요.
    pause
    exit /b 2
  )
  echo [1/3] Chrome을 CDP 모드로 시작하는 중...
  start "" "%CHROME%" --remote-debugging-port=9222 --user-data-dir="%~dp0chrome-profile" --no-first-run
  timeout /t 5 /nobreak >nul
  powershell -NoProfile -Command "try { Invoke-WebRequest -Uri 'http://127.0.0.1:9222/json/version' -TimeoutSec 3 -UseBasicParsing | Out-Null; exit 0 } catch { exit 1 }" >nul 2>&1
  if errorlevel 1 (
    echo [오류] Chrome CDP 연결을 시작하지 못했습니다.
    pause
    exit /b 3
  )
) else (
  echo [1/3] Chrome CDP 연결 확인됨.
)

REM ===== 주소 입력 =====
if "%~1"=="" (
  set /p "ADDRESS=주소 입력 > "
) else (
  set "ADDRESS=%~1"
)

echo [2/3] KRAS에서 조회 중: %ADDRESS%
"%NODE%" scripts\property-auto-runner.ts quick "%ADDRESS%"
if errorlevel 1 (
  echo.
  echo [오류] KRAS quick 실행 실패. 오류 코드: %errorlevel%
  pause
  exit /b %errorlevel%
)

echo.
echo [3/3] 완료. 오즈뷰어에서 직접 확인하세요.
pause
endlocal
