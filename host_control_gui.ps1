Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$botExe = Join-Path $root "rust_pearl_stasis_bot.exe"
$caddyFile = Join-Path $root "Caddyfile"
$discordDir = Join-Path $root "discord_bot"
$discordScript = Join-Path $discordDir "discord_bot.js"

$botLog = Join-Path $root "bot_output_log.txt"
$caddyLog = Join-Path $root "caddy_output_log.txt"
$discordLog = Join-Path $root "discord_output_log.txt"
$chatLog = Join-Path $root "chat_output_log.txt"

$botStdout = Join-Path $root "_host_bot_stdout.txt"
$botStderr = Join-Path $root "_host_bot_stderr.txt"
$caddyStdout = Join-Path $root "_host_caddy_stdout.txt"
$caddyStderr = Join-Path $root "_host_caddy_stderr.txt"
$discordStdout = Join-Path $root "_host_discord_stdout.txt"
$discordStderr = Join-Path $root "_host_discord_stderr.txt"

$script:botProcess = $null
$script:caddyProcess = $null
$script:discordProcess = $null
$script:botOffset = 0L
$script:caddyOffset = 0L
$script:discordOffset = 0L
$script:chatOffset = 0L
$script:botStdoutOffset = 0L
$script:botStderrOffset = 0L
$script:caddyStdoutOffset = 0L
$script:caddyStderrOffset = 0L
$script:discordStdoutOffset = 0L
$script:discordStderrOffset = 0L
$script:utf8NoBom = New-Object System.Text.UTF8Encoding($false)

$colorBg = [System.Drawing.Color]::FromArgb(11, 12, 18)
$colorPanel = [System.Drawing.Color]::FromArgb(22, 24, 34)
$colorText = [System.Drawing.Color]::FromArgb(238, 241, 247)
$colorBorder = [System.Drawing.Color]::FromArgb(62, 68, 86)
$colorGood = [System.Drawing.Color]::FromArgb(36, 83, 52)
$colorDanger = [System.Drawing.Color]::FromArgb(94, 43, 48)
$colorButton = [System.Drawing.Color]::FromArgb(38, 42, 58)

function Ensure-Log { param([string]$Path) if (-not (Test-Path -LiteralPath $Path)) { New-Item -ItemType File -Path $Path -Force | Out-Null } }
function Clear-LogFile { param([string]$Path) Ensure-Log $Path; $stream=[System.IO.File]::Open($Path,[System.IO.FileMode]::OpenOrCreate,[System.IO.FileAccess]::Write,[System.IO.FileShare]::ReadWrite); try{$stream.SetLength(0)}finally{$stream.Dispose()} }
function Write-LogLine { param([string]$Path,[string]$Text) Ensure-Log $Path; Add-Content -LiteralPath $Path -Value $Text -Encoding UTF8 }
function Write-RawLogText { param([string]$Path,[string]$Text) if([string]::IsNullOrEmpty($Text)){return}; Ensure-Log $Path; Add-Content -LiteralPath $Path -Value $Text -NoNewline -Encoding UTF8 }
function Clean-TerminalText { param([string]$Text) if([string]::IsNullOrEmpty($Text)){return ""}; $esc=[regex]::Escape([string][char]27); return ([regex]::Replace(($Text -replace "`0", ""), "$esc\[[0-?]*[ -/]*[@-~]", "")) }
function Decode-LogBytes { param([byte[]]$Bytes) if($null -eq $Bytes -or $Bytes.Length -eq 0){return ""}; return Clean-TerminalText ([System.Text.Encoding]::UTF8.GetString($Bytes)) }
function Read-NewLogText { param([string]$Path,[ref]$Offset) Ensure-Log $Path; $stream=[System.IO.File]::Open($Path,[System.IO.FileMode]::Open,[System.IO.FileAccess]::Read,[System.IO.FileShare]::ReadWrite); try{ if($Offset.Value -gt $stream.Length){$Offset.Value=0L}; [void]$stream.Seek($Offset.Value,[System.IO.SeekOrigin]::Begin); $len=[int]($stream.Length-$stream.Position); if($len -le 0){return ""}; $bytes=New-Object byte[] $len; [void]$stream.Read($bytes,0,$len); $Offset.Value=$stream.Position; return Decode-LogBytes $bytes } finally { $stream.Dispose() } }
function Append-Box { param([System.Windows.Forms.TextBox]$Box,[string]$Text) if([string]::IsNullOrEmpty($Text)){return}; $Box.AppendText($Text); $Box.SelectionStart=$Box.TextLength; $Box.ScrollToCaret() }
function Style-Button { param([System.Windows.Forms.Button]$Button,[System.Drawing.Color]$BackColor) $Button.Width=145; $Button.Height=34; $Button.Margin=New-Object System.Windows.Forms.Padding(4,2,4,2); $Button.FlatStyle=[System.Windows.Forms.FlatStyle]::Flat; $Button.FlatAppearance.BorderColor=$colorBorder; $Button.BackColor=$BackColor; $Button.ForeColor=$colorText; $Button.UseVisualStyleBackColor=$false }
function New-TerminalBox { param([string]$Fore) $box=New-Object System.Windows.Forms.TextBox; $box.Multiline=$true; $box.ScrollBars="Both"; $box.WordWrap=$false; $box.ReadOnly=$true; $box.Dock="Fill"; $box.Font=New-Object System.Drawing.Font("Consolas",8.5); $box.BackColor=[System.Drawing.Color]::FromArgb(5,7,12); $box.ForeColor=[System.Drawing.ColorTranslator]::FromHtml($Fore); $box.BorderStyle=[System.Windows.Forms.BorderStyle]::FixedSingle; return $box }
function Add-LabeledPane { param([System.Windows.Forms.TableLayoutPanel]$Grid,[string]$Title,[System.Windows.Forms.TextBox]$Box,[int]$Col,[int]$Row) $panel=New-Object System.Windows.Forms.Panel; $panel.Dock="Fill"; $panel.BackColor=$colorPanel; $lbl=New-Object System.Windows.Forms.Label; $lbl.Text=$Title; $lbl.Dock="Top"; $lbl.Height=24; $lbl.ForeColor=$colorText; $lbl.BackColor=$colorPanel; $lbl.Padding=New-Object System.Windows.Forms.Padding(6,4,0,0); $panel.Controls.Add($Box); $panel.Controls.Add($lbl); $Grid.Controls.Add($panel,$Col,$Row) }

function Extract-ChatLines { param([string]$Text) if([string]::IsNullOrEmpty($Text)){return ""}; $out=New-Object System.Text.StringBuilder; foreach($line in ($Text -split "`r?`n")){ if($line -match "(?i)(\[chat\]|chat\[|<[^>]+>|whisper|tellraw|public chat|game chat|received chat|chat mesadmin|\bCHAT\b)"){ [void]$out.AppendLine($line) } }; return $out.ToString() }
function Drain-LiveProcessOutput { param([string]$OutPath,[ref]$OutOffset,[string]$ErrPath,[ref]$ErrOffset,[string]$LogPath,[bool]$ExtractChat) $outText=Read-NewLogText $OutPath $OutOffset; if(-not [string]::IsNullOrEmpty($outText)){ Write-RawLogText $LogPath $outText; if($ExtractChat){ Write-RawLogText $chatLog (Extract-ChatLines $outText) } }; $errText=Read-NewLogText $ErrPath $ErrOffset; if(-not [string]::IsNullOrEmpty($errText)){ Write-RawLogText $LogPath $errText; if($ExtractChat){ Write-RawLogText $chatLog (Extract-ChatLines $errText) } } }

function Start-LoggedProcess { param([string]$Name,[string]$FilePath,[string]$Arguments,[string]$WorkingDirectory,[string]$LogPath,[string]$OutPath,[string]$ErrPath) Ensure-Log $LogPath; Clear-LogFile $OutPath; Clear-LogFile $ErrPath; Write-LogLine $LogPath "===== $Name started $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') ====="; $args=@{FilePath=$FilePath; WorkingDirectory=$WorkingDirectory; WindowStyle="Hidden"; PassThru=$true; RedirectStandardOutput=$OutPath; RedirectStandardError=$ErrPath}; if(-not [string]::IsNullOrWhiteSpace($Arguments)){$args.ArgumentList=$Arguments}; $p=Start-Process @args; Write-LogLine $LogPath "Started $Name PID $($p.Id). Output is captured inside this Host GUI."; return $p }

function Start-BotIfNeeded { if($script:botProcess -and -not $script:botProcess.HasExited){Write-LogLine $botLog "Rust bot already running from this Host GUI."; return}; if(-not(Test-Path -LiteralPath $botExe)){Write-LogLine $botLog "ERROR: Bot executable not found: $botExe"; return}; $script:botStdoutOffset=0L; $script:botStderrOffset=0L; $script:botProcess=Start-LoggedProcess "Rust Bot" $botExe "" $root $botLog $botStdout $botStderr }
function Start-CaddyIfNeeded { if($script:caddyProcess -and -not $script:caddyProcess.HasExited){Write-LogLine $caddyLog "Caddy already running from this Host GUI."; return}; if(-not(Test-Path -LiteralPath $caddyFile)){Write-LogLine $caddyLog "Caddy skipped: no Caddyfile found."; return}; $caddyExe=(Get-Command caddy.exe -ErrorAction SilentlyContinue).Source; if(-not $caddyExe){$caddyExe=Join-Path $root "caddy.exe"}; if(-not(Test-Path -LiteralPath $caddyExe)){Write-LogLine $caddyLog "ERROR: caddy.exe not found in PATH or package folder."; return}; $script:caddyStdoutOffset=0L; $script:caddyStderrOffset=0L; $script:caddyProcess=Start-LoggedProcess "Caddy" $caddyExe "run --config `"$caddyFile`" --adapter caddyfile" $root $caddyLog $caddyStdout $caddyStderr }
function Start-DiscordIfNeeded { if($script:discordProcess -and -not $script:discordProcess.HasExited){Write-LogLine $discordLog "Discord bot already running from this Host GUI."; return}; if(-not(Test-Path -LiteralPath $discordScript)){Write-LogLine $discordLog "Discord bot skipped: discord_bot.js not found."; return}; $node=(Get-Command node.exe -ErrorAction SilentlyContinue).Source; if(-not $node){Write-LogLine $discordLog "ERROR: node.exe not found. Install Node.js LTS or add it to PATH."; return}; if(-not(Test-Path -LiteralPath (Join-Path $discordDir "node_modules"))){ $npm=(Get-Command npm.cmd -ErrorAction SilentlyContinue).Source; if($npm){ Write-LogLine $discordLog "Installing Discord bot dependencies..."; $install=Start-Process -FilePath $npm -ArgumentList "install" -WorkingDirectory $discordDir -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput $discordStdout -RedirectStandardError $discordStderr; Write-RawLogText $discordLog (Read-NewLogText $discordStdout ([ref]$script:discordStdoutOffset)); Write-RawLogText $discordLog (Read-NewLogText $discordStderr ([ref]$script:discordStderrOffset)); if($install.ExitCode -ne 0){Write-LogLine $discordLog "npm install failed with exit code $($install.ExitCode)."; return} } else { Write-LogLine $discordLog "ERROR: npm.cmd not found and node_modules is missing."; return } }; $script:discordStdoutOffset=0L; $script:discordStderrOffset=0L; $script:discordProcess=Start-LoggedProcess "Discord Bot" $node "discord_bot.js" $discordDir $discordLog $discordStdout $discordStderr }
function Stop-LoggedProcess { param([System.Diagnostics.Process]$Process,[string]$Name,[string]$LogPath,[string]$ImageName) $stopped=$false; if($null -ne $Process -and -not $Process.HasExited){try{Write-LogLine $LogPath "Stopping $Name PID $($Process.Id)..."; $r=& taskkill.exe /PID $Process.Id /T /F 2>&1; if($r){Write-LogLine $LogPath (($r|Out-String).TrimEnd())}; $stopped=$true}catch{Write-LogLine $LogPath "Stop failed for ${Name}: $($_.Exception.Mesadmin)"}}; $running=Get-Process -Name ([System.IO.Path]::GetFileNameWithoutExtension($ImageName)) -ErrorAction SilentlyContinue; foreach($p in $running){try{$r=& taskkill.exe /PID $p.Id /T /F 2>&1; if($r){Write-LogLine $LogPath (($r|Out-String).TrimEnd())}; $stopped=$true}catch{}}; if(-not $stopped){Write-LogLine $LogPath "$Name is not running."} else {Write-LogLine $LogPath "===== $Name stop requested $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') ====="} }
function Stop-All { Stop-LoggedProcess $script:botProcess "Rust Bot" $botLog "rust_pearl_stasis_bot.exe"; Stop-LoggedProcess $script:caddyProcess "Caddy" $caddyLog "caddy.exe"; Stop-LoggedProcess $script:discordProcess "Discord Bot" $discordLog "node.exe" }

foreach($f in @($botLog,$caddyLog,$discordLog,$chatLog,$botStdout,$botStderr,$caddyStdout,$caddyStderr,$discordStdout,$discordStderr)){Ensure-Log $f}
$script:botOffset=(Get-Item -LiteralPath $botLog).Length; $script:caddyOffset=(Get-Item -LiteralPath $caddyLog).Length; $script:discordOffset=(Get-Item -LiteralPath $discordLog).Length; $script:chatOffset=(Get-Item -LiteralPath $chatLog).Length

$form=New-Object System.Windows.Forms.Form; $form.Text="Minecraft Utility Control Center Host"; $form.Size=New-Object System.Drawing.Size(1280,860); $form.StartPosition="CenterScreen"; $form.BackColor=$colorBg; $form.ForeColor=$colorText; $form.Font=New-Object System.Drawing.Font("Segoe UI",9)
$buttons=New-Object System.Windows.Forms.FlowLayoutPanel; $buttons.Dock="Top"; $buttons.Height=56; $buttons.Padding=New-Object System.Windows.Forms.Padding(10,10,10,8); $buttons.BackColor=$colorPanel; $form.Controls.Add($buttons)
$startAll=New-Object System.Windows.Forms.Button; $startAll.Text="Start Bot+Caddy+Discord"; $startAll.Width=190; Style-Button $startAll $colorGood; $buttons.Controls.Add($startAll)
$startDiscord=New-Object System.Windows.Forms.Button; $startDiscord.Text="Start Discord Only"; $startDiscord.Width=160; Style-Button $startDiscord $colorGood; $buttons.Controls.Add($startDiscord)
$stopAll=New-Object System.Windows.Forms.Button; $stopAll.Text="Stop All"; Style-Button $stopAll $colorDanger; $buttons.Controls.Add($stopAll)
$openGui=New-Object System.Windows.Forms.Button; $openGui.Text="Open Web GUI"; Style-Button $openGui $colorButton; $buttons.Controls.Add($openGui)
$clearOutput=New-Object System.Windows.Forms.Button; $clearOutput.Text="Clear Output"; Style-Button $clearOutput $colorButton; $buttons.Controls.Add($clearOutput)

$grid=New-Object System.Windows.Forms.TableLayoutPanel; $grid.Dock="Fill"; $grid.BackColor=$colorBg; $grid.ColumnCount=3; $grid.RowCount=2; $grid.Padding=New-Object System.Windows.Forms.Padding(8); $grid.ColumnStyles.Add((New-Object System.Windows.Forms.ColumnStyle([System.Windows.Forms.SizeType]::Percent,33.34))); $grid.ColumnStyles.Add((New-Object System.Windows.Forms.ColumnStyle([System.Windows.Forms.SizeType]::Percent,33.33))); $grid.ColumnStyles.Add((New-Object System.Windows.Forms.ColumnStyle([System.Windows.Forms.SizeType]::Percent,33.33))); $grid.RowStyles.Add((New-Object System.Windows.Forms.RowStyle([System.Windows.Forms.SizeType]::Percent,58))); $grid.RowStyles.Add((New-Object System.Windows.Forms.RowStyle([System.Windows.Forms.SizeType]::Percent,42))); $form.Controls.Add($grid)
$botBox=New-TerminalBox "#bce8c5"; $caddyBox=New-TerminalBox "#f0d18a"; $discordBox=New-TerminalBox "#c7ceff"; $chatBox=New-TerminalBox "#e5eaff"
Add-LabeledPane $grid "Rust Bot Terminal" $botBox 0 0
Add-LabeledPane $grid "Caddy Terminal" $caddyBox 1 0
Add-LabeledPane $grid "Discord Bot Terminal" $discordBox 2 0
$chatPanel=New-Object System.Windows.Forms.Panel; $chatPanel.Dock="Fill"; $chatPanel.BackColor=$colorPanel; $chatLbl=New-Object System.Windows.Forms.Label; $chatLbl.Text="Minecraft Chat Output"; $chatLbl.Dock="Top"; $chatLbl.Height=24; $chatLbl.ForeColor=$colorText; $chatLbl.BackColor=$colorPanel; $chatLbl.Padding=New-Object System.Windows.Forms.Padding(6,4,0,0); $chatPanel.Controls.Add($chatBox); $chatPanel.Controls.Add($chatLbl); $grid.Controls.Add($chatPanel,0,1); $grid.SetColumnSpan($chatPanel,3)

$timer=New-Object System.Windows.Forms.Timer; $timer.Interval=500; $timer.Add_Tick({ try{ Drain-LiveProcessOutput $botStdout ([ref]$script:botStdoutOffset) $botStderr ([ref]$script:botStderrOffset) $botLog $true; Drain-LiveProcessOutput $caddyStdout ([ref]$script:caddyStdoutOffset) $caddyStderr ([ref]$script:caddyStderrOffset) $caddyLog $false; Drain-LiveProcessOutput $discordStdout ([ref]$script:discordStdoutOffset) $discordStderr ([ref]$script:discordStderrOffset) $discordLog $false; Append-Box $botBox (Read-NewLogText $botLog ([ref]$script:botOffset)); Append-Box $caddyBox (Read-NewLogText $caddyLog ([ref]$script:caddyOffset)); Append-Box $discordBox (Read-NewLogText $discordLog ([ref]$script:discordOffset)); Append-Box $chatBox (Read-NewLogText $chatLog ([ref]$script:chatOffset)) } catch { try{Append-Box $botBox ("Host GUI tail error: $($_.Exception.Mesadmin)"+[Environment]::NewLine)}catch{} } })
$timer.Start()
$startAll.Add_Click({ try{ Start-BotIfNeeded; Start-CaddyIfNeeded; Start-DiscordIfNeeded } catch { Write-LogLine $botLog "ERROR starting services: $($_.Exception.Mesadmin)" } })
$startDiscord.Add_Click({ try{ Start-DiscordIfNeeded } catch { Write-LogLine $discordLog "ERROR starting Discord bot: $($_.Exception.Mesadmin)" } })
$stopAll.Add_Click({ try{ Stop-All } catch { Write-LogLine $botLog "ERROR stopping services: $($_.Exception.Mesadmin)" } })
$openGui.Add_Click({ Start-Process "http://127.0.0.1:8081" })
$clearOutput.Add_Click({ foreach($b in @($botBox,$caddyBox,$discordBox,$chatBox)){$b.Clear()}; foreach($f in @($botLog,$caddyLog,$discordLog,$chatLog,$botStdout,$botStderr,$caddyStdout,$caddyStderr,$discordStdout,$discordStderr)){Clear-LogFile $f}; $script:botOffset=0L; $script:caddyOffset=0L; $script:discordOffset=0L; $script:chatOffset=0L; $script:botStdoutOffset=0L; $script:botStderrOffset=0L; $script:caddyStdoutOffset=0L; $script:caddyStderrOffset=0L; $script:discordStdoutOffset=0L; $script:discordStderrOffset=0L })
$form.Add_FormClosing({ $timer.Stop() })
[void]$form.ShowDialog()
