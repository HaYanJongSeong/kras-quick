@echo off
chcp 65001 >nul
setlocal
cd /d "%~dp0"

set "NODE=node"
if exist "%~dp0node\node.exe" set "NODE=%~dp0node\node.exe"

set "SCRIPT=%~dp0scripts\property-auto-runner.ts"
if exist "%~dp0source\scripts\property-auto-runner.ts" set "SCRIPT=%~dp0source\scripts\property-auto-runner.ts"
"%NODE%" "%SCRIPT%" quick %*
set "EXIT_CODE=%errorlevel%"
if not "%EXIT_CODE%"=="0" pause
exit /b %EXIT_CODE%
