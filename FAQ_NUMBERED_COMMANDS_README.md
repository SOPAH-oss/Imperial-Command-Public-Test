# Numbered FAQ Commands

This branch uses `?` for FAQ commands.

## Public lookup

```text
?faq list
?faq #1
?faq 1
```

FAQ lookup is number-based. Free-text search is intentionally disabled so users pick a number from the list.

## Editor commands

Owners and users with the `faq_whisper` permission can edit FAQ entries from Minecraft chat/whisper:

```text
?faqadd Full FAQ text here
?faqset #1 | Replacement FAQ text here
?faqdel #1
```

FAQ entries are stored as simple text entries, not question/answer pairs.

## Output mode

FAQ replies follow the configured FAQ output mode in Owner Settings:

- Whisper
- Public chat
