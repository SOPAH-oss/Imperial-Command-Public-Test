@echo off
setlocal
cd /d "%~dp0"

echo Starting Minecraft Utility Control Center V1...
echo.

if exist "discord_bot\start_discord_bot.bat" (
  start "Discord Integration" cmd /k ""%~dp0discord_bot\start_discord_bot.bat""
) else (
  echo Discord bot folder not found, skipping Discord integration.
)

if exist "start_https_caddy_all.bat" (
  call "%~dp0start_https_caddy_all.bat"
) else if exist "host_control_gui.bat" (
  call "%~dp0host_control_gui.bat"
) else (
  call "%~dp0start_bot_logged.bat"
)
