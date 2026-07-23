---
name: auq
description: Ask the user structured clarification questions through the AUQ Wizard desktop app in both Default and Plan modes. Use whenever a Codex task has a material preference or tradeoff that cannot be discovered from the repository or environment and the answer changes the implementation.
---

# AUQ

This skill applies in both Default and Plan modes. Do not skip AUQ in Default mode merely because Codex can continue with an assumption. When the outcome depends on consequential user intent, preferences, or mutually exclusive tradeoffs that cannot be discovered from the repository or environment, pause and ask before implementation.

Resolve discoverable facts yourself before asking. Do not ask for confirmation when a safe, reversible default is clear.

Use an AUQ-specific adapter when the current Codex client exposes one in the active mode. Otherwise, use the AUQ CLI below before falling back to an unstructured question in the conversation. Default mode may not expose the native structured-input adapter; that is not a reason to skip AUQ.

Submit one to five questions. Give each question a header of at most 30 characters and two to five meaningful options. Put the recommended option first and add `(Recommended)` to its label. Do not add an `Other` option; the GUI provides free-text input.

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
