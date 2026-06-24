@echo off
setlocal
cd /d "%~dp0"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0host_control_gui.ps1" 1>>"%~dp0host_control_gui_error.txt" 2>>&1
if errorlevel 1 (
  echo.
  echo Host Control GUI crashed or exited with an error.
  echo Send Codex host_control_gui_error.txt plus bot_output_log.txt and caddy_output_log.txt.
  pause
)
