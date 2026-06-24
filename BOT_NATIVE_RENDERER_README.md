# Bot-Native 2-Chunk Renderer

This branch does **not** use a camera account.

The Viewport page has a `Bot-Native 2-Chunk World Render` panel. It asks the connected Azalea bot for a small snapshot of its own loaded world data and renders the first non-air block in each x/z column around the bot.

## What It Shows

- The selected bot's own known loaded blocks.
- A 2-chunk capped area around the bot.
- A top-surface colored render in the browser.
- The bot position marker.

## What It Does Not Yet Show

- Full Minecraft lighting.
- Resource-pack textures.
- First-person camera perspective.
- Animated models.

Those would require building a much larger renderer on top of the packet/chunk data. This is the first bot-native renderer step without a second account.
