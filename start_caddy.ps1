$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
if (!(Test-Path ".\Caddyfile")) {
    Copy-Item ".\Caddyfile.example" ".\Caddyfile"
    Write-Host "Created Caddyfile from Caddyfile.example. Edit it and replace stasis.yourdomain.com with your domain, then run this script again." -ForegroundColor Yellow
    exit 0
}
caddy run --config .\Caddyfile
