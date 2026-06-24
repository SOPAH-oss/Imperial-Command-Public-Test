Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path

$colorBg = [System.Drawing.Color]::FromArgb(11, 12, 18)
$colorPanel = [System.Drawing.Color]::FromArgb(22, 24, 34)
$colorInput = [System.Drawing.Color]::FromArgb(8, 10, 16)
$colorText = [System.Drawing.Color]::FromArgb(238, 241, 247)
$colorMuted = [System.Drawing.Color]::FromArgb(164, 172, 190)
$colorBorder = [System.Drawing.Color]::FromArgb(62, 68, 86)
$colorGood = [System.Drawing.Color]::FromArgb(36, 83, 52)
$colorDanger = [System.Drawing.Color]::FromArgb(94, 43, 48)
$colorButton = [System.Drawing.Color]::FromArgb(38, 42, 58)

$script:items = @()
$script:index = -1
$script:added = 0

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

function Parse-XaeroWaypoint {
    param([string]$Line)
    if ([string]::IsNullOrWhiteSpace($Line)) { return $null }
    $trimmed = $Line.Trim()
    if ($trimmed.StartsWith("#") -or -not $trimmed.StartsWith("waypoint:")) { return $null }
    $parts = $trimmed -split ":"
    if ($parts.Length -lt 6) { return $null }
    $x = 0; $y = 0; $z = 0
    if (-not [int]::TryParse($parts[3], [ref]$x)) { return $null }
    if (-not [int]::TryParse($parts[4], [ref]$y)) { return $null }
    if (-not [int]::TryParse($parts[5], [ref]$z)) { return $null }
    return [pscustomobject]@{ Name=$parts[1]; Initials=$parts[2]; X=$x; Y=$y; Z=$z; Raw=$trimmed }
}

function New-Id { [guid]::NewGuid().ToString() }
function Now-Iso { (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffffffZ") }
function Split-List { param([string]$Value) @($Value -split "," | ForEach-Object { $_.Trim().ToLowerInvariant() } | Where-Object { $_ }) }
function Optional-Int { param([string]$Value) if([string]::IsNullOrWhiteSpace($Value)){ return $null }; $n=0; if([int]::TryParse($Value,[ref]$n)){ return $n }; return $null }

function Style-Control {
    param([System.Windows.Forms.Control]$Control)
    $Control.BackColor = $colorInput
    $Control.ForeColor = $colorText
    if ($Control -is [System.Windows.Forms.TextBox] -or $Control -is [System.Windows.Forms.ComboBox] -or $Control -is [System.Windows.Forms.NumericUpDown]) {
        $Control.Font = New-Object System.Drawing.Font("Segoe UI", 9)
    }
}

function Style-Button {
    param([System.Windows.Forms.Button]$Button, [System.Drawing.Color]$BackColor)
    $Button.Height = 34
    $Button.FlatStyle = [System.Windows.Forms.FlatStyle]::Flat
    $Button.FlatAppearance.BorderColor = $colorBorder
    $Button.FlatAppearance.MouseOverBackColor = [System.Drawing.Color]::FromArgb(52, 58, 78)
    $Button.BackColor = $BackColor
    $Button.ForeColor = $colorText
    $Button.UseVisualStyleBackColor = $false
}

function Add-Label {
    param([System.Windows.Forms.TableLayoutPanel]$Panel, [string]$Text, [int]$Row)
    $label = New-Object System.Windows.Forms.Label
    $label.Text = $Text
    $label.ForeColor = $colorMuted
    $label.Dock = "Fill"
    $label.TextAlign = "MiddleLeft"
    $Panel.Controls.Add($label, 0, $Row)
    return $label
}

function Add-Input {
    param([System.Windows.Forms.TableLayoutPanel]$Panel, [string]$Text, [int]$Row)
    Add-Label $Panel $Text $Row | Out-Null
    $box = New-Object System.Windows.Forms.TextBox
    $box.Dock = "Fill"
    Style-Control $box
    $Panel.Controls.Add($box, 1, $Row)
    return $box
}

$form = New-Object System.Windows.Forms.Form
$form.Text = "Xaero Waypoint Importer"
$form.Size = New-Object System.Drawing.Size(920, 720)
$form.StartPosition = "CenterScreen"
$form.BackColor = $colorBg
$form.ForeColor = $colorText
$form.Font = New-Object System.Drawing.Font("Segoe UI", 9)

$main = New-Object System.Windows.Forms.TableLayoutPanel
$main.Dock = "Fill"
$main.Padding = New-Object System.Windows.Forms.Padding(14)
$main.ColumnCount = 1
$main.RowCount = 5
$main.RowStyles.Add((New-Object System.Windows.Forms.RowStyle([System.Windows.Forms.SizeType]::Absolute, 52)))
$main.RowStyles.Add((New-Object System.Windows.Forms.RowStyle([System.Windows.Forms.SizeType]::Absolute, 90)))
$main.RowStyles.Add((New-Object System.Windows.Forms.RowStyle([System.Windows.Forms.SizeType]::Percent, 100)))
$main.RowStyles.Add((New-Object System.Windows.Forms.RowStyle([System.Windows.Forms.SizeType]::Absolute, 48)))
$main.RowStyles.Add((New-Object System.Windows.Forms.RowStyle([System.Windows.Forms.SizeType]::Absolute, 38)))
$form.Controls.Add($main)

$top = New-Object System.Windows.Forms.FlowLayoutPanel
$top.Dock = "Fill"
$top.BackColor = $colorPanel
$top.Padding = New-Object System.Windows.Forms.Padding(8)
$main.Controls.Add($top, 0, 0)

$loadButton = New-Object System.Windows.Forms.Button
$loadButton.Text = "Load Xaero File"
$loadButton.Width = 140
Style-Button $loadButton $colorGood
$top.Controls.Add($loadButton)

$botLabel = New-Object System.Windows.Forms.Label
$botLabel.Text = "Default bot"
$botLabel.Width = 74
$botLabel.TextAlign = "MiddleLeft"
$botLabel.ForeColor = $colorMuted
$top.Controls.Add($botLabel)

$defaultBot = New-Object System.Windows.Forms.TextBox
$defaultBot.Text = "UtilityBot"
$defaultBot.Width = 150
Style-Control $defaultBot
$top.Controls.Add($defaultBot)

$ownerLabel = New-Object System.Windows.Forms.Label
$ownerLabel.Text = "Owner user"
$ownerLabel.Width = 78
$ownerLabel.TextAlign = "MiddleLeft"
$ownerLabel.ForeColor = $colorMuted
$top.Controls.Add($ownerLabel)

$ownerUser = New-Object System.Windows.Forms.TextBox
$ownerUser.Text = "admin"
$ownerUser.Width = 140
Style-Control $ownerUser
$top.Controls.Add($ownerUser)

$summary = New-Object System.Windows.Forms.Label
$summary.Text = "Load Xaero's waypoints.txt to begin."
$summary.Dock = "Fill"
$summary.ForeColor = $colorMuted
$summary.BackColor = $colorPanel
$summary.Padding = New-Object System.Windows.Forms.Padding(12)
$main.Controls.Add($summary, 0, 1)

$grid = New-Object System.Windows.Forms.TableLayoutPanel
$grid.Dock = "Fill"
$grid.BackColor = $colorPanel
$grid.Padding = New-Object System.Windows.Forms.Padding(12)
$grid.ColumnCount = 2
$grid.RowCount = 17
$grid.ColumnStyles.Add((New-Object System.Windows.Forms.ColumnStyle([System.Windows.Forms.SizeType]::Absolute, 190)))
$grid.ColumnStyles.Add((New-Object System.Windows.Forms.ColumnStyle([System.Windows.Forms.SizeType]::Percent, 100)))
for($i=0;$i -lt 17;$i++){ $grid.RowStyles.Add((New-Object System.Windows.Forms.RowStyle([System.Windows.Forms.SizeType]::Absolute, 34))) }
$main.Controls.Add($grid, 0, 2)

$labelBox = Add-Input $grid "Label" 0
$typeLabel = Add-Label $grid "Import as" 1
$typeBox = New-Object System.Windows.Forms.ComboBox
$typeBox.Dock = "Fill"
$typeBox.DropDownStyle = "DropDownList"
@("Walk waypoint","Stasis pearl","Book writer chest","Bank intake chest","Butler source chest","Butler destination chest") | ForEach-Object { [void]$typeBox.Items.Add($_) }
$typeBox.SelectedIndex = 0
Style-Control $typeBox
$grid.Controls.Add($typeBox, 1, 1)

$botBox = Add-Input $grid "Bot name" 2
$xBox = Add-Input $grid "X" 3
$yBox = Add-Input $grid "Y" 4
$zBox = Add-Input $grid "Z" 5
$notesBox = Add-Input $grid "Notes" 6

$playerBox = Add-Input $grid "Pearl player" 7
$stasisKindBox = Add-Input $grid "Stasis type" 8
$itemNameBox = Add-Input $grid "Pearl item name" 9
$inventorySlotBox = Add-Input $grid "Inventory slot 0-35" 10
$allowedUsersBox = Add-Input $grid "Allowed users" 11

$allowedWritersBox = Add-Input $grid "Allowed book writers" 12
$minBox = Add-Input $grid "Minimum Credits" 13
$maxBox = Add-Input $grid "Maximum Credits" 14
$trashXBox = Add-Input $grid "Trash chest X" 15
$trashYBox = Add-Input $grid "Trash chest Y" 16

$trashZLabel = New-Object System.Windows.Forms.Label
$trashZLabel.Text = "Trash chest Z"
$trashZLabel.ForeColor = $colorMuted
$trashZLabel.Dock = "Fill"
$trashZLabel.TextAlign = "MiddleLeft"
$grid.RowCount = 18
$grid.RowStyles.Add((New-Object System.Windows.Forms.RowStyle([System.Windows.Forms.SizeType]::Absolute, 34)))
$grid.Controls.Add($trashZLabel, 0, 17)
$trashZBox = New-Object System.Windows.Forms.TextBox
$trashZBox.Dock = "Fill"
Style-Control $trashZBox
$grid.Controls.Add($trashZBox, 1, 17)

$actions = New-Object System.Windows.Forms.FlowLayoutPanel
$actions.Dock = "Fill"
$actions.BackColor = $colorPanel
$actions.Padding = New-Object System.Windows.Forms.Padding(8)
$main.Controls.Add($actions, 0, 3)

$importButton = New-Object System.Windows.Forms.Button
$importButton.Text = "Import This"
$importButton.Width = 120
Style-Button $importButton $colorGood
$actions.Controls.Add($importButton)

$skipButton = New-Object System.Windows.Forms.Button
$skipButton.Text = "Skip"
$skipButton.Width = 90
Style-Button $skipButton $colorButton
$actions.Controls.Add($skipButton)

$prevButton = New-Object System.Windows.Forms.Button
$prevButton.Text = "Previous"
$prevButton.Width = 90
Style-Button $prevButton $colorButton
$actions.Controls.Add($prevButton)

$closeButton = New-Object System.Windows.Forms.Button
$closeButton.Text = "Close"
$closeButton.Width = 90
Style-Button $closeButton $colorDanger
$actions.Controls.Add($closeButton)

$status = New-Object System.Windows.Forms.Label
$status.Text = "Ready."
$status.Dock = "Fill"
$status.ForeColor = $colorMuted
$main.Controls.Add($status, 0, 4)

function Set-Current {
    if($script:index -lt 0 -or $script:index -ge $script:items.Count){
        $summary.Text = "No waypoint loaded."
        return
    }
    $wp = $script:items[$script:index]
    $summary.Text = "Waypoint $($script:index + 1) / $($script:items.Count): $($wp.Name) at $($wp.X), $($wp.Y), $($wp.Z)"
    $labelBox.Text = $wp.Name
    $botBox.Text = $defaultBot.Text
    $xBox.Text = [string]$wp.X
    $yBox.Text = [string]$wp.Y
    $zBox.Text = [string]$wp.Z
    $notesBox.Text = "imported from Xaero"
    $playerBox.Text = $wp.Name
    $stasisKindBox.Text = "block"
    $itemNameBox.Text = ""
    $inventorySlotBox.Text = "0"
    $allowedUsersBox.Text = ""
    $allowedWritersBox.Text = ""
    $minBox.Text = "1"
    $maxBox.Text = "100000000"
    $trashXBox.Text = ""
    $trashYBox.Text = ""
    $trashZBox.Text = ""
}

function Next-Item {
    $script:index++
    if($script:index -ge $script:items.Count){
        $summary.Text = "Done. Imported $script:added waypoint(s). Refresh the web GUI."
        $status.Text = "Import complete."
        return
    }
    Set-Current
}

function Import-Current {
    if($script:index -lt 0 -or $script:index -ge $script:items.Count){ return }
    $created = Now-Iso
    $label = if([string]::IsNullOrWhiteSpace($labelBox.Text)){ $script:items[$script:index].Name } else { $labelBox.Text.Trim() }
    $bot = if([string]::IsNullOrWhiteSpace($botBox.Text)){ $defaultBot.Text.Trim() } else { $botBox.Text.Trim() }
    $x = [int]$xBox.Text
    $y = [int]$yBox.Text
    $z = [int]$zBox.Text

    switch($typeBox.SelectedItem) {
        "Walk waypoint" {
            $path = Join-Path $root "waypoints.json"
            $rows = @(Load-JsonArray $path)
            $rows += [pscustomobject]@{ id=New-Id; label=$label; bot_name=$bot; x=$x; y=$y; z=$z; notes=$notesBox.Text; created_at=$created }
            Save-JsonArray $path $rows
        }
        "Stasis pearl" {
            $path = Join-Path $root "pearls.json"
            $rows = @(Load-JsonArray $path)
            $slot = 0
            [void][int]::TryParse($inventorySlotBox.Text, [ref]$slot)
            $rows += [pscustomobject]@{
                id=New-Id
                player=$playerBox.Text.Trim()
                label=$label
                stasis_kind=if([string]::IsNullOrWhiteSpace($stasisKindBox.Text)){"block"}else{$stasisKindBox.Text.Trim().ToLowerInvariant()}
                item_name=$itemNameBox.Text.Trim()
                inventory_slot=[Math]::Min([Math]::Max($slot,0),35)
                bot_name=$bot
                x=$x
                y=$y
                z=$z
                notes=$notesBox.Text
                created_at=$created
                owner_user=$ownerUser.Text.Trim().ToLowerInvariant()
                allowed_users=@(Split-List $allowedUsersBox.Text)
            }
            Save-JsonArray $path $rows
        }
        "Book writer chest" {
            $path = Join-Path $root "ledger_chests.json"
            $rows = @(Load-JsonArray $path)
            $rows += [pscustomobject]@{ id=New-Id; purpose="writing"; label=$label; bot_name=$bot; chest_x=$x; chest_y=$y; chest_z=$z; processed_chest_x=$null; processed_chest_y=$null; processed_chest_z=$null; allowed_players=@(); min_credits=1; max_credits=100000000; remove_processed_book=$true; created_at=$created }
            Save-JsonArray $path $rows
        }
        "Bank intake chest" {
            $path = Join-Path $root "ledger_chests.json"
            $rows = @(Load-JsonArray $path)
            $rows += [pscustomobject]@{ id=New-Id; purpose="banking"; label=$label; bot_name=$bot; chest_x=$x; chest_y=$y; chest_z=$z; processed_chest_x=(Optional-Int $trashXBox.Text); processed_chest_y=(Optional-Int $trashYBox.Text); processed_chest_z=(Optional-Int $trashZBox.Text); allowed_players=@(Split-List $allowedWritersBox.Text); min_credits=([int64]$minBox.Text); max_credits=([int64]$maxBox.Text); remove_processed_book=$true; created_at=$created }
            Save-JsonArray $path $rows
        }
        "Butler source chest" {
            $path = Join-Path $root "butler_chests.json"
            $rows = @(Load-JsonArray $path)
            $rows += [pscustomobject]@{ id=New-Id; label=$label; bot_name=$bot; chest_x=$x; chest_y=$y; chest_z=$z; created_at=$created }
            Save-JsonArray $path $rows
        }
        "Butler destination chest" {
            $path = Join-Path $root "butler_waypoints.json"
            $rows = @(Load-JsonArray $path)
            $rows += [pscustomobject]@{ id=New-Id; label=$label; bot_name=$bot; chest_x=$x; chest_y=$y; chest_z=$z; created_at=$created }
            Save-JsonArray $path $rows
        }
    }
    $script:added++
    $status.Text = "Imported '$label' as $($typeBox.SelectedItem)."
    Next-Item
}

$loadButton.Add_Click({
    $dialog = New-Object System.Windows.Forms.OpenFileDialog
    $dialog.Title = "Select Xaero waypoints.txt"
    $dialog.Filter = "Xaero waypoints.txt|waypoints.txt|Text files|*.txt|All files|*.*"
    if($dialog.ShowDialog() -ne [System.Windows.Forms.DialogResult]::OK){ return }
    $script:items = @(Get-Content -LiteralPath $dialog.FileName | ForEach-Object { Parse-XaeroWaypoint $_ } | Where-Object { $_ })
    $script:index = 0
    $script:added = 0
    if(-not $script:items.Count){
        $summary.Text = "No Xaero waypoint lines found."
        return
    }
    $status.Text = "Loaded $($script:items.Count) waypoint(s)."
    Set-Current
})

$importButton.Add_Click({
    try { Import-Current } catch { [System.Windows.Forms.MesadminBox]::Show($_.Exception.Mesadmin, "Import failed") | Out-Null }
})
$skipButton.Add_Click({ Next-Item })
$prevButton.Add_Click({ if($script:index -gt 0){ $script:index--; Set-Current } })
$closeButton.Add_Click({ $form.Close() })

[void]$form.ShowDialog()

