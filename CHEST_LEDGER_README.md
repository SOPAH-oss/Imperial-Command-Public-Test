# Credits Chest Ledger Branch

This third branch is focused on one job: a connected bot walks to a configured chest, opens it, scans signed books, validates each book author against your allowed-player list, reads a Credits amount, credits that author's casino balance, and optionally shift-clicks the processed book into the bot inventory.

## Signed book format

The book must be a Minecraft written book (`minecraft:written_book`). The author must match one of the names entered in the GUI allowed-player list.

The Credits value can appear in the title or pages. These formats work:

- `Credits: 250`
- `Amount 250`
- `Value 250`
- Any first number in the book/title if no labeled value is found

## GUI flow

1. Start `rust_pearl_stasis_bot.exe`.
2. Open the GUI URL printed in the console, usually `http://0.0.0.0:8081` locally as `http://127.0.0.1:8081`.
3. Log in.
4. Connect the enabled bot account.
5. Enter chest coordinates, allowed signed-book players, min/max Credits limits, and run the processor.

## Important behavior

- If `Move processed books into bot inventory` is checked, credited books are shift-clicked out of the chest after processing.
- Balances are stored in `casino.json`.
- This branch still uses the same login/config system as the main bot so it can connect the Minecraft account normally, but the GUI is dedicated to the chest-ledger processor.
