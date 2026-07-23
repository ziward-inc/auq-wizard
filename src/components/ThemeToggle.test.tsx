import { render, screen } from "@testing-library/react"
import { userEvent } from "@testing-library/user-event"
import { afterEach, describe, expect, it } from "vitest"

import { ThemeToggle } from "@/components/ThemeToggle"

afterEach(() => {
  window.localStorage.clear()
  document.documentElement.classList.remove("light", "dark")
})

describe("ThemeToggle", () => {
  it("switches themes and stores the selection", async () => {
    window.localStorage.setItem("auq-wizard-theme", "light")
    render(<ThemeToggle />)

    expect(document.documentElement).toHaveClass("light")

    await userEvent.click(screen.getByRole("button", { name: "Switch to dark mode" }))

    expect(document.documentElement).toHaveClass("dark")
    expect(document.documentElement).not.toHaveClass("light")
    expect(window.localStorage.getItem("auq-wizard-theme")).toBe("dark")
    expect(screen.getByRole("button", { name: "Switch to light mode" })).toBeInTheDocument()
  })
})
