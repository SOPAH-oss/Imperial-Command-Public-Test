param(
    [string]$OutputDir = (Join-Path $PSScriptRoot "version1_package")
)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$exe = Join-Path $root "target\release\minecraft_utility_bot.exe"

Push-Location $root
try {
    if (Get-Command rustup.exe -ErrorAction SilentlyContinue) {
        cargo +nightly build --release
    } else {
        cargo build --release
    }

    New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
    Copy-Item -LiteralPath $exe -Destination (Join-Path $OutputDir "minecraft_utility_bot.exe") -Force

    $items = @(
        "static",
        "config.json",
        "config.example.json",
        "users.json",
        "users.example.json",
        "pearls.json",
        "pearls.example.json",
        "waypoints.json",
        "waypoints.example.json",
        "ledger_chests.json",
        "butler_chests.json",
        "butler_waypoints.json",
        "host_control_gui.ps1",
        "host_control_gui.bat",
        "discord_bot",
        "START_HOST_GUI_HIDDEN.vbs",
        "host_control_gui_hidden.bat",
        "start_bot_logged.ps1",
        "start_bot_logged.bat",
        "pull_pearl.ps1",
        "pull_pearl.sh",
        "README.md",
        "CASINO_README.md",
        "CHEST_LEDGER_README.md",
        "BOOK_WRITER_README.md",
        "BOT_NATIVE_RENDERER_README.md",
        "VERSION.txt"
    )

    foreach ($item in $items) {
        $src = Join-Path $root $item
        if (Test-Path -LiteralPath $src) {
            Copy-Item -LiteralPath $src -Destination $OutputDir -Recurse -Force
        }
    }

    foreach ($caddyItem in @("Caddyfile", "Caddyfile.example", "Caddyfile.localhost.example", "Caddyfile_443_only.example", "install_caddy_windows.ps1", "README_HTTPS_CADDY.md", "start_caddy.ps1", "start_caddy_logged.ps1", "start_https_caddy_all.bat")) {
        $src = Join-Path $root $caddyItem
        if (Test-Path -LiteralPath $src) {
            Copy-Item -LiteralPath $src -Destination $OutputDir -Force
        }
    }

    Write-Host "Built Minecraft Utility Bot package:"
    Write-Host $OutputDir
} finally {
    Pop-Location
}

