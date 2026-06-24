@echo off
setlocal
cd /d "%~dp0"

echo Starting Minecraft Utility Control Center HTTPS/Caddy package...

if not exist "rust_pearl_stasis_bot.exe" (
  echo ERROR: rust_pearl_stasis_bot.exe was not found in this folder.
  echo Put this .bat file in the same folder as rust_pearl_stasis_bot.exe.
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

if not exist "Caddyfile" (
  if exist "Caddyfile.example" (
    copy /Y "Caddyfile.example" "Caddyfile" >nul
    echo Created Caddyfile from Caddyfile.example.
    echo Edit Caddyfile and replace stasis.yourdomain.com with your real domain.
    echo Then run this file again.
    notepad "Caddyfile"
    pause
    exit /b 0
  )
)

where caddy >nul 2>nul
if errorlevel 1 (
  echo ERROR: Caddy was not found on PATH.
  echo Install it with: winget install CaddyServer.Caddy
  pause
  exit /b 1
)

echo Starting bot GUI/API on 127.0.0.1:8081...
echo Bot terminal output will be written to bot_output_log.txt
start "Minecraft Utility Control Center" powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0start_bot_logged.ps1"

timeout /t 4 /nobreak >nul

echo Starting Caddy HTTPS reverse proxy...
echo Caddy terminal output will be written to caddy_output_log.txt
start "Caddy HTTPS Proxy" powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0start_caddy_logged.ps1"

echo.
echo Started both windows. Logs are in bot_output_log.txt and caddy_output_log.txt.
echo Open your domain, for example: https://stasis.yourdomain.com
echo.
pause
