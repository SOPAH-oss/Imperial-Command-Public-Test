# Discord Shop Role Rewards

This version lets Server Shop items grant Discord roles through the Discord bot.

## Setup

1. In Discord Developer Portal, enable the bot intent for Server Members if required by your bot settings.
2. Invite the bot with Manage Roles permission.
3. In your Discord server, place the bot role above every role it should grant.
4. Edit `discord_bot/discord_config.json`:

```json
{
  "token": "...",
  "client_id": "...",
  "guild_id": "...",
  "currency_name": "Credits"
}
```

`shop_role_mappings` remains supported for old shop items, but the preferred method is now setting the role ID directly on each shop item in the GUI.

## Create a role reward item

Open the web GUI → Server Shop → Add Shop Item.

Set:

- Reward Type: `Discord Role`
- Discord Role ID: the numeric Discord role ID
- Discord Role Name: display name for your own reference
- Price: cost in Credits

Users buy with:

```text
/shop buy
```

The bot deducts Credits, grants the Discord role, and writes an audit entry to:

```text
shop_role_audit.json
```

If the role grant fails, the Discord command returns an error. Make sure the bot has Manage Roles and that its highest role is above the target role.
