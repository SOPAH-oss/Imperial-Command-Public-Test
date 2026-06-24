# HTTPS Caddy Branch - Main Branch Base

This package is based on the canonical main branch:

- Multi-account bot manager
- Admin owner login
- Per-bot Connect / Disconnect / Hard Stop
- Hard Stop All Bots
- ThrowStasis / named item entries
- Inventory slots `0-35`

This branch adds an easier HTTPS deployment using **Caddy** as a reverse proxy.

## Important default change

For safer HTTPS use, the bot GUI/API default is now:

```text
127.0.0.1:8081
```

That means the Rust bot is only directly reachable on the VPS itself. Caddy exposes it publicly over HTTPS.

## Files added

```text
Caddyfile.example
Caddyfile.localhost.example
install_caddy_windows.ps1
start_bot.ps1
start_caddy.ps1
README_HTTPS_CADDY.md
```

## Step-by-step setup

### 1. Extract the package

Example location:

```text
C:\Users\Administrator\Desktop\rust_pearl_stasis_bot_main_https_caddy
```

### 2. Create normal config files from examples

In PowerShell:

```powershell
cd C:\Users\Administrator\Desktop\rust_pearl_stasis_bot_main_https_caddy
if (Test-Path "config.example.json") { Copy-Item "config.example.json" "config.json" -Force }
if (Test-Path "users.example.json") { Copy-Item "users.example.json" "users.json" -Force }
if (Test-Path "pearls.example.json") { Copy-Item "pearls.example.json" "pearls.json" -Force }
```

### 3. Start the bot

If using the included executable:

```powershell
.\rust_pearl_stasis_bot.exe
```

Or:

```powershell
.\start_bot.ps1
```

The terminal should show something like:

```text
GUI running at http://127.0.0.1:8081
```

Local test on the VPS:

```text
http://127.0.0.1:8081
```

### 4. Point a domain/subdomain to the VPS

In your DNS panel, create an A-record:

```text
stasis.yourdomain.com -> YOUR_VPS_PUBLIC_IP
```

Example:

```text
stasis.publicserver.nl -> 149.210.244.130
```

Wait until DNS resolves.

### 5. Install Caddy

PowerShell as Administrator:

```powershell
winget install CaddyServer.Caddy
```

Or run:

```powershell
.\install_caddy_windows.ps1
```

### 6. Create your Caddyfile

Copy the example:

```powershell
Copy-Item .\Caddyfile.example .\Caddyfile -Force
notepad .\Caddyfile
```

Change:

```text
stasis.yourdomain.com {
    reverse_proxy 127.0.0.1:8081
}
```

to your real domain:

```text
stasis.publicserver.nl {
    reverse_proxy 127.0.0.1:8081
}
```

### 7. Open firewall ports for HTTPS

Caddy needs public ports 80 and 443.

```powershell
New-NetFirewallRule -DisplayName "Caddy HTTP 80" -Direction Inbound -Protocol TCP -LocalPort 80 -Action Allow
New-NetFirewallRule -DisplayName "Caddy HTTPS 443" -Direction Inbound -Protocol TCP -LocalPort 443 -Action Allow
```

Also open TCP `80` and `443` in your VPS provider firewall/control panel if enabled.

You do **not** need to publicly open port `8081` when using Caddy.

### 8. Start Caddy

From the package folder:

```powershell
caddy run --config .\Caddyfile
```

Or:

```powershell
.\start_caddy.ps1
```

### 9. Open the HTTPS GUI

```text
https://stasis.yourdomain.com
```

Login:

```text
Admin
change-me
```

### 10. Connect bots

In the GUI:

1. Go to Settings.
2. Add/edit bot accounts.
3. Use Microsoft auth for real accounts.
4. Click Connect for the bot account.
5. Complete Microsoft device login in the terminal if prompted.

## Running both services

You need two processes running:

1. Rust bot GUI/API:

```powershell
.\rust_pearl_stasis_bot.exe
```

2. Caddy HTTPS proxy:

```powershell
caddy run --config .\Caddyfile
```

## If HTTPS does not work

Check locally first:

```powershell
curl.exe http://127.0.0.1:8081
```

Check Caddy ports:

```powershell
netstat -ano | findstr ":80"
netstat -ano | findstr ":443"
```

If Caddy cannot get a certificate, usually one of these is wrong:

- Domain does not point to the VPS IP.
- Port 80 is blocked.
- Port 443 is blocked.
- Another service such as IIS is already using 80/443.

## If IIS is using port 80/443

Stop IIS temporarily:

```powershell
iisreset /stop
```

Or configure Caddy on another domain/host after freeing the ports.
