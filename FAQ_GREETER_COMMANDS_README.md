# FAQ and Login Greeter Commands

## FAQ lookup

Players can ask FAQs in Minecraft chat/whisper:

```text
!faq list
!faq 1
!faq #1
!faq pearl
!faq treasury
```

The bot searches `public_faqs.json`. FAQs are numbered by enabled entry order.

## FAQ additions by allowed users

Users with the `faq_whisper` permission can add FAQs by whispering the bot:

```text
!faqadd question | answer
```

Example:

```text
!faqadd How do I pull a pearl? | Open Pearl Stasis and press Pull.
```

## Output mode

Owner Settings now contains:

- FAQ answer output: `Whisper` or `Public chat`
- Greeter output: `Whisper` or `Public chat`
- FAQ add cooldown seconds
- Login greeter cooldown seconds

## Login greeter

Greeters are loaded from `public_login_greeters.json`.

The bot now tries to greet players when it sees join mesadmins like:

```text
PlayerName joined the game
```

It also keeps the older whisper-trigger fallback. Use `{player}` in greeter mesadmins.
