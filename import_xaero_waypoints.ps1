param(
    [string]$XaeroFile = "",
    [string]$BotName = "UtilityBot"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path

function Load-JsonArray {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) { return @() }
    $raw = Get-Content -Raw -LiteralPath $Path
    if ([string]::IsNullOrWhiteSpace($raw)) { return @() }
    $value = $raw | ConvertFrom-Json
    if ($null -eq $value) { return @() }
    if ($value -is [array]) { return @($value) }
    return @($value)
}

function Save-JsonArray {
    param([string]$Path, [array]$Rows)
    $Rows | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $Path -Encoding UTF8
}

function New-Id {
    return [guid]::NewGuid().ToString()
}

function Now-Iso {
    return (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffffffZ")
}

function Read-IntOrDefault {
    param([string]$Prompt, [int]$Default)
    $value = Read-Host "$Prompt [$Default]"
    if ([string]::IsNullOrWhiteSpace($value)) { return $Default }
    $parsed = 0
    if ([int]::TryParse($value, [ref]$parsed)) { return $parsed }
    Write-Host "Invalid number; using $Default." -ForegroundColor Yellow
    return $Default
}

function Read-OptionalInt {
    param([string]$Prompt)
    $value = Read-Host "$Prompt [blank = none]"
    if ([string]::IsNullOrWhiteSpace($value)) { return $null }
    $parsed = 0
    if ([int]::TryParse($value, [ref]$parsed)) { return $parsed }
    Write-Host "Invalid number; leaving blank." -ForegroundColor Yellow
    return $null
}

function Read-AllowedPlayers {
    $value = Read-Host "Allowed book writers/signers, comma separated"
    if ([string]::IsNullOrWhiteSpace($value)) { return @() }
    return @($value -split "," | ForEach-Object { $_.Trim() } | Where-Object { $_ })
}

function Parse-XaeroWaypoint {
    param([string]$Line)
    if ([string]::IsNullOrWhiteSpace($Line)) { return $null }
    $trimmed = $Line.Trim()
    if ($trimmed.StartsWith("#")) { return $null }
    if (-not $trimmed.StartsWith("waypoint:")) { return $null }
    $parts = $trimmed -split ":"
    if ($parts.Length -lt 6) { return $null }

    $x = 0
    $y = 0
    $z = 0
    if (-not [int]::TryParse($parts[3], [ref]$x)) { return $null }
    if (-not [int]::TryParse($parts[4], [ref]$y)) { return $null }
    if (-not [int]::TryParse($parts[5], [ref]$z)) { return $null }

    return [pscustomobject]@{
        Name = $parts[1]
        Initials = $parts[2]
        X = $x
        Y = $y
        Z = $z
        Raw = $trimmed
    }
}

if ([string]::IsNullOrWhiteSpace($XaeroFile)) {
    Write-Host "Paste the full path to Xaero's waypoints.txt."
    Write-Host "Typical path: .minecraft\XaeroWaypoints\<server>\<dimension>\waypoints.txt"
    $XaeroFile = Read-Host "Xaero waypoints.txt path"
}

if (-not (Test-Path -LiteralPath $XaeroFile)) {
    throw "File not found: $XaeroFile"
}

$waypointsPath = Join-Path $root "waypoints.json"
$ledgerPath = Join-Path $root "ledger_chests.json"
$butlerSourcePath = Join-Path $root "butler_chests.json"
$butlerDestinationPath = Join-Path $root "butler_waypoints.json"

$waypoints = @(Load-JsonArray $waypointsPath)
$ledger = @(Load-JsonArray $ledgerPath)
$butlerSources = @(Load-JsonArray $butlerSourcePath)
$butlerDestinations = @(Load-JsonArray $butlerDestinationPath)

$parsed = @(Get-Content -LiteralPath $XaeroFile | ForEach-Object { Parse-XaeroWaypoint $_ } | Where-Object { $_ })
if (-not $parsed.Count) {
    Write-Host "No Xaero waypoint lines found." -ForegroundColor Yellow
    pause
    exit 0
}

Write-Host ""
Write-Host "Found $($parsed.Count) Xaero waypoint(s)." -ForegroundColor Green
Write-Host "Types: [1] Walk  [2] Book writer chest  [3] Bank intake chest  [4] Butler source chest  [5] Butler destination chest  [s] Skip  [q] Quit"
Write-Host ""

$added = 0
foreach ($wp in $parsed) {
    Write-Host "Xaero: $($wp.Name) at $($wp.X), $($wp.Y), $($wp.Z)" -ForegroundColor Cyan
    $label = Read-Host "Label [$($wp.Name)]"
    if ([string]::IsNullOrWhiteSpace($label)) { $label = $wp.Name }
    $bot = Read-Host "Bot name [$BotName]"
    if ([string]::IsNullOrWhiteSpace($bot)) { $bot = $BotName }
    $choice = Read-Host "Choose type"

    if ($choice -eq "q") { break }
    if ($choice -eq "s" -or [string]::IsNullOrWhiteSpace($choice)) {
        Write-Host "Skipped." -ForegroundColor DarkYellow
        continue
    }

    $created = Now-Iso
    switch ($choice) {
        "1" {
            $notes = Read-Host "Notes [imported from Xaero]"
            if ([string]::IsNullOrWhiteSpace($notes)) { $notes = "imported from Xaero" }
            $waypoints += [pscustomobject]@{
                id = New-Id
                label = $label
                bot_name = $bot
                x = $wp.X
                y = $wp.Y
                z = $wp.Z
                notes = $notes
                created_at = $created
            }
            $added++
        }
        "2" {
            $ledger += [pscustomobject]@{
                id = New-Id
                purpose = "writing"
                label = $label
                bot_name = $bot
                chest_x = $wp.X
                chest_y = $wp.Y
                chest_z = $wp.Z
                processed_chest_x = $null
                processed_chest_y = $null
                processed_chest_z = $null
                allowed_players = @()
                min_credits = 1
                max_credits = 100000000
                remove_processed_book = $true
                created_at = $created
            }
            $added++
        }
        "3" {
            Write-Host "Configure bank intake security and trash/archive chest."
            $allowed = @(Read-AllowedPlayers)
            $min = Read-IntOrDefault "Minimum Credits" 1
            $max = Read-IntOrDefault "Maximum Credits" 100000000
            $trashX = Read-OptionalInt "Trash/archive chest X"
            $trashY = Read-OptionalInt "Trash/archive chest Y"
            $trashZ = Read-OptionalInt "Trash/archive chest Z"
            $ledger += [pscustomobject]@{
                id = New-Id
                purpose = "banking"
                label = $label
                bot_name = $bot
                chest_x = $wp.X
                chest_y = $wp.Y
                chest_z = $wp.Z
                processed_chest_x = $trashX
                processed_chest_y = $trashY
                processed_chest_z = $trashZ
                allowed_players = $allowed
                min_credits = $min
                max_credits = $max
                remove_processed_book = $true
                created_at = $created
            }
            $added++
        }
        "4" {
            $butlerSources += [pscustomobject]@{
                id = New-Id
                label = $label
                bot_name = $bot
                chest_x = $wp.X
                chest_y = $wp.Y
                chest_z = $wp.Z
                created_at = $created
            }
            $added++
        }
        "5" {
            $butlerDestinations += [pscustomobject]@{
                id = New-Id
                label = $label
                bot_name = $bot
                chest_x = $wp.X
                chest_y = $wp.Y
                chest_z = $wp.Z
                created_at = $created
            }
            $added++
        }
        default {
            Write-Host "Unknown choice; skipped." -ForegroundColor Yellow
        }
    }
    Write-Host ""
}

Save-JsonArray $waypointsPath $waypoints
Save-JsonArray $ledgerPath $ledger
Save-JsonArray $butlerSourcePath $butlerSources
Save-JsonArray $butlerDestinationPath $butlerDestinations

Write-Host "Imported $added waypoint(s)." -ForegroundColor Green
Write-Host "Restart/refresh the web GUI if it was already open."
pause

