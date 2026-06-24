# Server Casino

The bot now has a Casino page and in-game casino commands using **Credits**, the currency of the Server Project.

## GUI

After login, open the **Casino** tab.

Available games:

- Roulette
- Slots
- Blackjack

The Settings page has a casino output mode:

- **Whisper players**: in-game command replies are sent with `/msg <player> ...`
- **Public chat**: in-game command replies are sent in normal chat

## In-Game Commands

```text
!credits
!slots <bet>
!roulette <bet> <red|black|green|odd|even|0-36>
!blackjack <bet>
!casino
```

Examples:

```text
!slots 50
!roulette 25 red
!blackjack 100
```

## Suggested Future Features

- Daily Credits reward
- Admin grant/take Credits controls
- Leaderboard
- Transaction history
- Cooldowns or max-bet limits per user
- Casino win/loss announcements channel
- More games: coinflip, dice duel, high-low, lottery
