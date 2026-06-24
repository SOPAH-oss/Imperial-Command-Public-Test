Set-Location -LiteralPath $PSScriptRoot

$logPath = Join-Path $PSScriptRoot "bot_output_log.txt"
$exePath = Join-Path $PSScriptRoot "rust_pearl_stasis_bot.exe"

if (-not (Test-Path -LiteralPath $exePath)) {
    Write-Host "ERROR: rust_pearl_stasis_bot.exe was not found in this folder."
    Read-Host "Press Enter to close"
    exit 1
}

Write-Host "Press Ctrl+C in this window to stop the restart watcher."
while ($true) {
    "===== Bot started $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') =====" | Tee-Object -FilePath $logPath -Append
    $cmdLine = "`"$exePath`" 2>&1"
    & cmd.exe /d /c $cmdLine | Tee-Object -FilePath $logPath -Append
    $exitCode = $LASTEXITCODE
    "===== Bot exited $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') with code $exitCode =====" | Tee-Object -FilePath $logPath -Append

    Write-Host ""
    Write-Host "Bot exited with code $exitCode. Restarting in 3 seconds..."
    Write-Host "Output was also saved to bot_output_log.txt."
    Start-Sleep -Seconds 3
}
