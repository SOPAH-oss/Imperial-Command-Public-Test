# Book Writer

The Book Writer page lets an owner command a connected bot to write, sign, and optionally deposit a book into a chest.

Requirements:

- The bot must already be connected.
- The bot must already have a writable book / book and quill in the selected inventory slot.
- Inventory slots use the same numbering as the pearl setup: `0-8` hotbar, `9-35` main inventory.
- Book titles must be 32 characters or fewer.
- Separate GUI page text into multiple book pages with a line containing only `---`.

Flow:

1. Choose the bot name.
2. Enter the inventory slot containing the writable book.
3. Enter the chest coordinates.
4. Enter the title and pages.
5. Press `Write, Sign, and Place Book`.

If `Place signed book into chest after signing` is enabled, the bot signs the book, walks to the chest, opens it, and shift-clicks the selected inventory slot into the chest.
