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
  it("offers a separate reinstall action for every ready integration", async () => {
    const onInstall = vi.fn().mockResolvedValue(undefined)
    render(
      <Onboarding
        status={STATUS}
        onInstall={onInstall}
        onSetEnabled={vi.fn().mockResolvedValue(undefined)}
      />,
    )

    expect(screen.queryByRole("button", { name: "Install integrations" })).not.toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Reinstall integrations" })).not.toBeInTheDocument()

    await userEvent.click(screen.getByRole("button", { name: "Reinstall CLI" }))
    expect(onInstall).toHaveBeenLastCalledWith({
      cli: true,
      claude: false,
      codex: false,
      autostart: false,
      replaceCli: false,
    })

    await userEvent.click(screen.getByRole("button", { name: "Reinstall Claude Code" }))
    expect(onInstall).toHaveBeenLastCalledWith({
      cli: false,
      claude: true,
      codex: false,
      autostart: false,
      replaceCli: false,
    })

    await userEvent.click(screen.getByRole("button", { name: "Reinstall Codex" }))
    expect(onInstall).toHaveBeenLastCalledWith({
      cli: false,
      claude: false,
      codex: true,
      autostart: false,
      replaceCli: false,
    })

    await userEvent.click(screen.getByRole("button", { name: "Reinstall Launch at login" }))
    expect(onInstall).toHaveBeenLastCalledWith({
      cli: false,
      claude: false,
      codex: false,
      autostart: true,
      replaceCli: false,
    })
  })

  it("places the missing-integration install action before integration settings", async () => {
    const onInstall = vi.fn().mockResolvedValue(undefined)
    render(
      <Onboarding
        status={{ ...STATUS, cli: false }}
        onInstall={onInstall}
        onSetEnabled={vi.fn().mockResolvedValue(undefined)}
      />,
    )

    const installButton = screen.getByRole("button", { name: "Install integrations" })
    const routingHeading = screen.getByText(/GUI routing/)

    expect(installButton.compareDocumentPosition(routingHeading)).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING,
    )

    await userEvent.click(installButton)
    expect(onInstall).toHaveBeenCalledWith({
      cli: true,
      claude: false,
      codex: false,
      autostart: false,
      replaceCli: false,
    })
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
    expect(screen.getByText("auq")).toHaveClass("bg-amber-950", "text-amber-50")

    await userEvent.click(screen.getByRole("button", { name: "Replace and install" }))

    expect(onInstall).toHaveBeenCalledWith({
      cli: true,
      claude: false,
      codex: false,
      autostart: false,
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
