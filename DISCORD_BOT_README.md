# Discord Sidecar Bot

This private V1 package includes a Node.js Discord sidecar in `discord_bot`.

## Setup

1. Install Node.js LTS.
2. Open `discord_bot\discord_config.json` after first run, or copy `discord_config.example.json`.
3. Fill in:
   - `token`
   - `client_id`
   - `guild_id`
   - optional `admin_role_id`
   - optional `shop_role_mappings`
4. Run `discord_bot\start_discord_bot.bat`.

The sidecar uses the same `bank.json`, `shop_items.json`, and `users.json` in the main bot folder.

## Linking Discord to Credits Accounts

Open the web GUI, go to Settings, and have an owner edit the account under `Owner: User Logins`.

Set `Discord name` to one of:

- the Discord user ID
- the Discord username
- the Discord global name
- the server display name
- the full Discord tag

When a linked Discord user runs casino/shop commands, the sidecar credits and charges the account's Minecraft name in `bank.json`.

## Slash Commands

- `/help`
- `/credits balance`
- `/casino slots bet:<amount>`
- `/casino roulette bet:<amount> choice:<red|black|green|odd|even|0-36>`
- `/casino blackjack bet:<amount>`
- `/casino blackjack-hit`
- `/casino blackjack-stand`
- `/shop list`
- `/shop buy item:<exact item name>`
- `/admin sync-commands`

Shop role grants use exact shop item names mapped to Discord role IDs in `discord_config.json`.
