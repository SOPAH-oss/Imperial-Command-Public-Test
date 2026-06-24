param(
  [string]$Player,
  [string]$Target
)

# This is the REAL wiring point called by the Rust GUI when you press Pull.
# Replace the body with the exact click/command you use to pull stasis.
# Examples:
# 1. Call an AutoHotkey clicker:
#    & "C:\Program Files\AutoHotkey\v2\AutoHotkey64.exe" ".\click_stasis.ahk" $Player $Target
# 2. Call a Mineflayer/Node bot command bridge:
#    node .\pull-stasis.js --player $Player --target $Target
# 3. Send a redstone-control HTTP request to another local service:
#    Invoke-RestMethod -Method POST "http://127.0.0.1:3001/pull" -Body (@{ player=$Player; target=$Target } | ConvertTo-Json) -ContentType "application/json"

Write-Host "Pull command received for player '$Player' at target '$Target'. Edit pull_pearl.ps1 to perform the actual Minecraft click."
exit 0
