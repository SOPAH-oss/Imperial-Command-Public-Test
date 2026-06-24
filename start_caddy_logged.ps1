Set-Location -LiteralPath $PSScriptRoot

$logPath = Join-Path $PSScriptRoot "caddy_output_log.txt"
$caddyFile = Join-Path $PSScriptRoot "Caddyfile"

if (-not (Get-Command caddy -ErrorAction SilentlyContinue)) {
    Write-Host "ERROR: Caddy was not found on PATH."
    Write-Host "Install it with: winget install CaddyServer.Caddy"
    Read-Host "Press Enter to close"
    exit 1
}

if (-not (Test-Path -LiteralPath $caddyFile)) {
    Write-Host "ERROR: Caddyfile was not found in this folder."
    Read-Host "Press Enter to close"
    exit 1
}

"===== Caddy started $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') =====" | Tee-Object -FilePath $logPath -Append
$cmdLine = "caddy run --config `"$caddyFile`" 2>&1"
& cmd.exe /d /c $cmdLine | Tee-Object -FilePath $logPath -Append
"===== Caddy exited $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') =====" | Tee-Object -FilePath $logPath -Append

Write-Host ""
Write-Host "Caddy exited. Output was also saved to caddy_output_log.txt."
Read-Host "Press Enter to close"
