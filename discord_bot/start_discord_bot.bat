@echo off
setlocal
cd /d "%~dp0"

if not exist "discord_config.json" (
  if exist "discord_config.example.json" copy /Y "discord_config.example.json" "discord_config.json" >nul
)

if not exist "node_modules" (
  echo Installing Discord bot dependencies...
  npm install
  if errorlevel 1 (
    echo npm install failed. Install Node.js LTS, then run this file again.
    pause
    exit /b 1
  )
)

npm start
