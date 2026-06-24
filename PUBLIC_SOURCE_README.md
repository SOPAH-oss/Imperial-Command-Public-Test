# Minecraft Utility Bot - Public Source Pack

This is a sanitized public source package. Private server names, account emails, passwords, custom faction branding, and logo assets have been removed or replaced with placeholders.

## Before use

Edit these files:

- `config.json` or `config.example.json`
- `users.json`
- `discord_bot/.env.example` if you use the Discord bot
- `Caddyfile` if you expose the GUI over HTTPS

Default placeholder login:

```text
username: admin
password: change-me
```

Change this immediately before running on any public network.

## Build

```powershell
cargo build --release
```

Or run:

```powershell
powershell -ExecutionPolicy Bypass -File .\build_version1.ps1
```
