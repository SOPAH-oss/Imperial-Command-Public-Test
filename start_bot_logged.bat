@echo off
setlocal
cd /d "%~dp0"

echo Starting Minecraft Utility Control Center with logging...
echo Bot terminal output will be written to:
echo   %~dp0bot_output_log.txt
echo.

if not exist "rust_pearl_stasis_bot.exe" (
  echo ERROR: rust_pearl_stasis_bot.exe was not found in this folder.
  pause
  exit /b 1
)

if not exist "config.json" (
  if exist "config.example.json" copy /Y "config.example.json" "config.json" >nul
)

if not exist "users.json" (
  if exist "users.example.json" copy /Y "users.example.json" "users.json" >nul
)

if not exist "pearls.json" (
  if exist "pearls.example.json" copy /Y "pearls.example.json" "pearls.json" >nul
)

if not exist "waypoints.json" (
  if exist "waypoints.example.json" copy /Y "waypoints.example.json" "waypoints.json" >nul
)

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0start_bot_logged.ps1"
