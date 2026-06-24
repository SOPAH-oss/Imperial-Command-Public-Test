@echo off
setlocal

cd /d "%~dp0"

if not exist "build_version1.ps1" (
  echo Could not find build_version1.ps1 next to this BAT.
  pause
  exit /b 1
)

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%cd%\build_version1.ps1"

echo.
echo Build finished. Check the version1_package folder.
pause
