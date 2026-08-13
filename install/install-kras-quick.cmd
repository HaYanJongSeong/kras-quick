@echo off
rem Launcher for install-kras-quick.ps1 (PowerShell 5.1 compatible).
rem Usage: install-kras-quick.cmd [-Repo x] [-Version tag] [-InstallDir dir] [-StagingDir dir]
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install-kras-quick.ps1" %*
exit /b %ERRORLEVEL%
