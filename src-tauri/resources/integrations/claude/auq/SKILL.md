---
name: auq
description: Ask the user structured clarification questions through the AUQ Wizard desktop app. Use when a task has a material preference or tradeoff that cannot be discovered from the repository or environment, including from a subagent where Claude's built-in AskUserQuestion tool is unavailable.
---

# AUQ

Resolve discoverable facts yourself before asking. Use AUQ only when the answer materially changes the work.

In a main Claude Code session, call the built-in `AskUserQuestion` tool normally. When AUQ GUI routing is enabled, the installed hook presents it in the desktop app and returns the answers. When routing is paused, the hook yields to Claude's native interaction.

When the built-in tool is unavailable in the current agent or mode, fall back to the AUQ CLI with this exact standalone command:

```bash
auq ask <<'AUQ_JSON'
{
  "questions": [
    {
      "question": "Which implementation should I use?",
      "header": "Approach",
      "options": [
        {
          "label": "Recommended path (Recommended)",
          "description": "Use the simplest robust implementation."
        },
        {
          "label": "Alternative path",
          "description": "Accept the stated tradeoff."
        }
      ],
      "multiSelect": false
    }
  ]
}
AUQ_JSON
```

Use one to five questions, headers of at most 30 characters, and two to five meaningful options. Put the recommendation first. Do not add `Other`; the GUI supplies free-text input. Wait for the command and use its Markdown result as the user's answer. Resume an interrupted request with `auq wait <request-id>`.

If `auq` is unavailable, GUI routing is disabled, or the CLI says to use native interaction, ask through the current session's native user-input channel instead. If no structured input tool is available, ask directly in the conversation. Do not retry a disabled or unavailable CLI in a loop.
