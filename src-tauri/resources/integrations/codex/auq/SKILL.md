---
name: auq
description: Ask the user structured clarification questions through the AUQ Wizard desktop app. Use when a Codex task has a material preference or tradeoff that cannot be discovered from the repository or environment and the answer changes the implementation.
---

# AUQ

Resolve discoverable facts yourself before asking. Use AUQ only for consequential user intent, preferences, or mutually exclusive tradeoffs. Do not ask for confirmation when a safe, reversible default is clear.

Use an AUQ-specific adapter when the current Codex client exposes one. Otherwise, fall back to the AUQ CLI below.

Submit one to four questions. Give each question a header of at most 12 characters and two to four meaningful options. Put the recommended option first and add `(Recommended)` to its label. Do not add an `Other` option; the GUI provides free-text input.

Run exactly one standalone command in this form, with no pipes, chaining, substitutions, environment prefixes, or additional redirects:

```bash
auq ask <<'AUQ_JSON'
{
  "questions": [
    {
      "question": "Which persistence strategy should this use?",
      "header": "Storage",
      "options": [
        {
          "label": "SQLite (Recommended)",
          "description": "Use transactions and durable recovery."
        },
        {
          "label": "JSON file",
          "description": "Use a smaller implementation with weaker recovery."
        }
      ],
      "multiSelect": false
    }
  ]
}
AUQ_JSON
```

Wait for the command to finish, then treat its Markdown output as the user's answer. If the process is interrupted after it prints a request ID, resume it with `auq wait <request-id>`.

If `auq` is unavailable, GUI routing is disabled, or the CLI says to use native interaction, ask through the current client's native user-input channel instead. If no structured tool is available, ask directly in the conversation. Do not retry a disabled or unavailable CLI in a loop.
