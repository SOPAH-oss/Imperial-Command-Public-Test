# Minecraft Utility Control Center - Version 1

Defaults in this pack:

- GUI host: `0.0.0.0`
- GUI port: `8081`

Version 1 combines the pearl puller, viewport, casino, book writer, chest ledger
banking, and Bank Butler tools into one runnable package.

## Build

```powershell
cargo clean
cargo build --release
```

## Run

```powershell
.\target\release\rust_pearl_stasis_bot.exe
```

Open:

```text
http://127.0.0.1:8081
```

From another PC, use:

```text
http://YOUR_SERVER_IP:8081
```

Make sure Windows Firewall and your VPS/provider firewall allow inbound TCP `8081`.

## Multi bot accounts

Log in as owner, then use **Owner: Bot Accounts** in the GUI.

Each bot account has:

- Enabled checkbox
- Bot name, e.g. `UtilityBot`, `backup`, `utilitybot2`
- Server host and port
- Microsoft email/offline name
- Auth mode

Press **Connect Enabled Bots** to connect all enabled accounts, or use the per-account Connect button.
Per-bot disconnect buttons were removed from the bot listing because they were unreliable on some Azalea runtimes.

## Unified waypoints

Use the **Waypoints** page as the main place to add, edit, delete, and walk to:

- normal viewport/walking waypoints
- book writer chest waypoints
- chest ledger banking chest waypoints
- Bank Butler source chest waypoints
- Bank Butler destination chest waypoints

The chest waypoint **Walk To** buttons use the same walking code as viewport waypoints.

## Pearls and bots

When adding a pearl, you can assign it to:

- `Any connected bot`, or
- a specific bot account name.

ThrowStasis still uses inventory slots `0-35`:

- `0-8` = hotbar
- `9-35` = main inventory

The entry also stores the named item display name so each named stasis item is tied to a player.


## Host Control GUI

Run `host_control_gui.bat` to open the host PC launcher. It can start/stop the bot,
start/stop Caddy when a Caddyfile is present, open the web GUI, and show live bot/Caddy log output.

## Hard Stop / disconnect note

Azalea can keep internal runtime tasks alive after a normal `client.disconnect()`.
If a bot disconnects and then rejoins, use **Hard Stop All Bots** in the GUI.
That endpoint first requests disconnect, then exits the process. Because the whole process is gone,
there is no remaining runtime/task that can reconnect. Restart `rust_pearl_stasis_bot.exe` when you want the GUI/bots back.

Soft disconnect all is still present for testing, but Hard Stop is the definite no-rejoin option.
