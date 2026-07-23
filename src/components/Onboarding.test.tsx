import { render, screen } from "@testing-library/react"
import { userEvent } from "@testing-library/user-event"
import { describe, expect, it, vi } from "vitest"

import { Onboarding } from "@/components/Onboarding"
import type { IntegrationStatus } from "@/lib/auq"

const STATUS: IntegrationStatus = {
  auqEnabled: true,
  cli: true,
  cliConflict: false,
  claudeSkill: true,
  claudeHook: true,
  codexSkill: true,
  codexHooks: true,
  autostart: true,
  pathReady: true,
  warnings: [],
}

describe("Onboarding", () => {
  it("places the install action before integration settings", () => {
    render(
      <Onboarding
        status={{ ...STATUS, cli: false }}
        onInstall={vi.fn().mockResolvedValue(undefined)}
        onSetEnabled={vi.fn().mockResolvedValue(undefined)}
      />,
    )

    const installButton = screen.getByRole("button", { name: "Install integrations" })
    const routingHeading = screen.getByText(/GUI routing/)

    expect(installButton.compareDocumentPosition(routingHeading)).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING,
    )
  })

  it("requires approval before replacing an existing CLI", async () => {
    const onInstall = vi.fn().mockResolvedValue(undefined)
    render(
      <Onboarding
        status={{ ...STATUS, cli: false, cliConflict: true }}
        onInstall={onInstall}
        onSetEnabled={vi.fn().mockResolvedValue(undefined)}
      />,
    )

    await userEvent.click(screen.getByRole("button", { name: "Install integrations" }))

    expect(onInstall).not.toHaveBeenCalled()
    expect(screen.getByText(/will be backed up and replaced/i)).toBeInTheDocument()

    await userEvent.click(screen.getByRole("button", { name: "Replace and install" }))

    expect(onInstall).toHaveBeenCalledWith({
      cli: true,
      claude: true,
      codex: true,
      autostart: true,
      replaceCli: true,
    })
  })

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
