import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { userEvent } from "@testing-library/user-event"
import { describe, expect, it, vi } from "vitest"

import { QuestionWizard } from "@/components/QuestionWizard"
import type { StoredRequest } from "@/lib/auq"

const REQUEST: StoredRequest = {
  requestId: "019abcdef-test-request",
  sequence: 1,
  status: "pending",
  payload: {
    questions: [
      {
        question: "Which database should we use?",
        header: "Database",
        multiSelect: false,
        options: [
          { label: "SQLite", description: "Local and embedded." },
          { label: "Postgres", description: "Shared server database." },
        ],
      },
      {
        question: "Which checks should run?",
        header: "Checks",
        multiSelect: true,
        options: [
          { label: "Typecheck", description: "Check TypeScript types." },
          { label: "Tests", description: "Run automated tests." },
        ],
      },
    ],
  },
  origin: {
    agent: "codex",
    cwd: "/Volumes/t500/Projects/auq-wizard/src-tauri",
    projectRoot: "/Volumes/t500/Projects/auq-wizard",
    projectName: "auq-wizard",
    branch: "main",
    sessionId: "session-123",
  },
  context: {
    summary: "Make each clarification request identifiable at a glance.",
  },
  createdAt: Date.now(),
  updatedAt: 1,
}

describe("QuestionWizard", () => {
  it("shows project, agent, task summary, and request details before the question", async () => {
    render(
      <QuestionWizard
        request={REQUEST}
        pendingCount={1}
        onSubmit={vi.fn().mockResolvedValue(undefined)}
        onCancel={vi.fn().mockResolvedValue(undefined)}
      />,
    )

    expect(screen.getAllByText("auq-wizard")).toHaveLength(3)
    expect(screen.getAllByText("Codex")).toHaveLength(2)
    expect(
      screen.getByText("Make each clarification request identifiable at a glance."),
    ).toBeVisible()
    expect(screen.queryByText(REQUEST.requestId)).not.toBeVisible()

    await userEvent.click(screen.getByText("Details"))

    expect(screen.getByText("/Volumes/t500/Projects/auq-wizard/src-tauri")).toBeVisible()
    expect(screen.getByText(REQUEST.requestId)).toBeVisible()
  })

  it("keeps legacy requests without context usable", () => {
    render(
      <QuestionWizard
        request={{ ...REQUEST, origin: undefined, context: undefined }}
        pendingCount={1}
        onSubmit={vi.fn().mockResolvedValue(undefined)}
        onCancel={vi.fn().mockResolvedValue(undefined)}
      />,
    )

    expect(screen.getAllByText("Unknown project")).toHaveLength(3)
    expect(screen.getAllByText("Unknown source")).toHaveLength(2)
    expect(
      screen.getByText("No task summary was provided for this clarification request."),
    ).toBeVisible()
  })

  it("wraps a 30-character sidebar header without truncating it", () => {
    const header = "123456789012345678901234567890"
    const request = {
      ...REQUEST,
      payload: {
        questions: [{ ...REQUEST.payload.questions[0], header }, REQUEST.payload.questions[1]],
      },
    }

    render(
      <QuestionWizard
        request={request}
        pendingCount={1}
        onSubmit={vi.fn().mockResolvedValue(undefined)}
        onCancel={vi.fn().mockResolvedValue(undefined)}
      />,
    )

    expect(screen.getByText(header)).toHaveClass("break-words")
    expect(screen.getByText(header)).not.toHaveClass("truncate")
  })

  it("collects single and multi-select answers", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined)
    render(
      <QuestionWizard
        request={REQUEST}
        pendingCount={2}
        onSubmit={onSubmit}
        onCancel={vi.fn().mockResolvedValue(undefined)}
      />,
    )

    fireEvent.click(screen.getByRole("checkbox", { name: /^SQLite/ }))
    await userEvent.click(screen.getByRole("button", { name: /next/i }))
    fireEvent.click(screen.getByRole("checkbox", { name: /^Typecheck/ }))
    fireEvent.click(screen.getByRole("checkbox", { name: /^Tests/ }))
    await userEvent.click(screen.getByRole("button", { name: /^submit$/i }))

    await waitFor(() =>
      expect(onSubmit).toHaveBeenCalledWith({
        answers: {
          "Which database should we use?": "SQLite",
          "Which checks should run?": ["Typecheck", "Tests"],
        },
      }),
    )
  })

  it("submits a free response instead of structured answers", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined)
    render(
      <QuestionWizard
        request={REQUEST}
        pendingCount={1}
        onSubmit={onSubmit}
        onCancel={vi.fn().mockResolvedValue(undefined)}
      />,
    )

    await userEvent.click(screen.getByRole("button", { name: "Respond freely" }))
    await userEvent.type(screen.getByRole("textbox"), "Use the existing project defaults.")
    await userEvent.click(screen.getByRole("button", { name: /^submit$/i }))

    await waitFor(() =>
      expect(onSubmit).toHaveBeenCalledWith({ response: "Use the existing project defaults." }),
    )
  })

  it("passes cancel through without submitting", async () => {
    const onCancel = vi.fn().mockResolvedValue(undefined)
    const onSubmit = vi.fn().mockResolvedValue(undefined)
    render(
      <QuestionWizard request={REQUEST} pendingCount={1} onSubmit={onSubmit} onCancel={onCancel} />,
    )

    await userEvent.click(screen.getByRole("button", { name: /^cancel$/i }))

    expect(onCancel).toHaveBeenCalledOnce()
    expect(onSubmit).not.toHaveBeenCalled()
  })
})
