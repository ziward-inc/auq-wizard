import { render, screen } from "@testing-library/react"
import { userEvent } from "@testing-library/user-event"
import { describe, expect, it, vi } from "vitest"

import { Onboarding } from "@/components/Onboarding"
import type { IntegrationStatus } from "@/lib/auq"

const STATUS: IntegrationStatus = {
  auqEnabled: true,
  cli: true,
  claudeSkill: true,
  claudeHook: true,
  codexSkill: true,
  codexHooks: true,
  autostart: true,
  pathReady: true,
  warnings: [],
}

describe("Onboarding", () => {
  it("pauses GUI routing", async () => {
    const onSetEnabled = vi.fn().mockResolvedValue(undefined)
    render(
      <Onboarding
        status={STATUS}
        onInstall={vi.fn().mockResolvedValue(undefined)}
        onSetEnabled={onSetEnabled}
      />,
    )

    await userEvent.click(screen.getByRole("button", { name: "Pause AUQ" }))

    expect(onSetEnabled).toHaveBeenCalledWith(false)
  })

  it("resumes paused GUI routing", async () => {
    const onSetEnabled = vi.fn().mockResolvedValue(undefined)
    render(
      <Onboarding
        status={{ ...STATUS, auqEnabled: false }}
        onInstall={vi.fn().mockResolvedValue(undefined)}
        onSetEnabled={onSetEnabled}
      />,
    )

    expect(screen.getByText(/native interaction/i)).toBeInTheDocument()
    await userEvent.click(screen.getByRole("button", { name: "Enable AUQ" }))

    expect(onSetEnabled).toHaveBeenCalledWith(true)
  })
})
