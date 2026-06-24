FAQ + Login Greeter Fix
=======================

This package fixes the greeter not firing when join lines are logged as:

    chat[UtilityBot]: admin joined the server.

The bot now strips the host-log prefix before parsing and supports:

- Player joined the server.
- Player joined the server
- Player joined the game
- Player joined

Greeter behavior:

- Controlled by config.json: login_greeter_enabled
- Output mode: greeter_output_mode = "whisper" or "chat"
- Default storage: public_login_greeters.json
- Mesadmins support {player}
- If no greeter entries exist but the greeter is enabled, it uses a safe fallback welcome instead of silently doing nothing.

FAQ behavior:

- !faq list
- !faq 1
- !faq #1
- !faq search text
- !faqadd question | answer

FAQ output mode:

- faq_output_mode = "whisper" or "chat"
