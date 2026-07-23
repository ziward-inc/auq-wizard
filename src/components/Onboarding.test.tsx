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
  codexHookTrust: "trusted",
  codexHookReviews: [],
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
        onTrustCodexHooks={vi.fn().mockResolvedValue(undefined)}
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
        onTrustCodexHooks={vi.fn().mockResolvedValue(undefined)}
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
        onTrustCodexHooks={vi.fn().mockResolvedValue(undefined)}
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
        onTrustCodexHooks={vi.fn().mockResolvedValue(undefined)}
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
        onTrustCodexHooks={vi.fn().mockResolvedValue(undefined)}
      />,
    )

    expect(screen.getByText(/native interaction/i)).toBeInTheDocument()
    await userEvent.click(screen.getByRole("button", { name: "Enable AUQ" }))

    expect(onSetEnabled).toHaveBeenCalledWith(true)
  })

  it("reviews the exact AUQ hooks before trusting them", async () => {
    const onTrustCodexHooks = vi.fn().mockResolvedValue(undefined)
    render(
      <Onboarding
        status={{
          ...STATUS,
          codexHookTrust: "untrusted",
          codexHookReviews: [
            {
              eventName: "PreToolUse",
              command: "'/Users/test/.local/bin/auq' codex-hook pre-tool-use",
            },
            {
              eventName: "PermissionRequest",
              command: "'/Users/test/.local/bin/auq' codex-hook permission-request",
            },
          ],
        }}
        onInstall={vi.fn().mockResolvedValue(undefined)}
        onSetEnabled={vi.fn().mockResolvedValue(undefined)}
        onTrustCodexHooks={onTrustCodexHooks}
      />,
    )

    expect(screen.getByText("Codex hooks need approval")).toBeInTheDocument()
    expect(screen.getByText("3/4 ready")).toBeInTheDocument()
    expect(screen.getAllByText("Needs approval")).toHaveLength(1)
    await userEvent.click(screen.getByRole("button", { name: "Review & trust" }))

    expect(screen.getByRole("alertdialog")).toHaveTextContent("Trust AUQ hooks?")
    expect(screen.getByRole("alertdialog")).toHaveTextContent(
      "'/Users/test/.local/bin/auq' codex-hook pre-tool-use",
    )
    expect(screen.getByRole("alertdialog")).toHaveTextContent(
      "'/Users/test/.local/bin/auq' codex-hook permission-request",
    )

    await userEvent.click(screen.getByRole("button", { name: "Trust hooks" }))
    expect(onTrustCodexHooks).toHaveBeenCalledOnce()
  })

  it("does not offer direct trust when Codex hook status is unavailable", () => {
    render(
      <Onboarding
        status={{ ...STATUS, codexHookTrust: "unavailable" }}
        onInstall={vi.fn().mockResolvedValue(undefined)}
        onSetEnabled={vi.fn().mockResolvedValue(undefined)}
        onTrustCodexHooks={vi.fn().mockResolvedValue(undefined)}
      />,
    )

    expect(screen.getByText("Codex hook status unavailable")).toBeInTheDocument()
    expect(screen.getByText("Status unavailable")).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Review & trust" })).not.toBeInTheDocument()
  })

  it("keeps a failed Codex trust error visible in the confirmation", async () => {
    render(
      <Onboarding
        status={{
          ...STATUS,
          codexHookTrust: "untrusted",
          codexHookReviews: [
            { eventName: "PreToolUse", command: "auq codex-hook pre-tool-use" },
            {
              eventName: "PermissionRequest",
              command: "auq codex-hook permission-request",
            },
          ],
        }}
        onInstall={vi.fn().mockResolvedValue(undefined)}
        onSetEnabled={vi.fn().mockResolvedValue(undefined)}
        onTrustCodexHooks={vi.fn().mockRejectedValue(new Error("Codex trust failed"))}
      />,
    )

    await userEvent.click(screen.getByRole("button", { name: "Review & trust" }))
    await userEvent.click(screen.getByRole("button", { name: "Trust hooks" }))

    expect(await screen.findByRole("alert")).toHaveTextContent("Codex trust failed")
    expect(screen.getByRole("alertdialog")).toBeInTheDocument()
  })
})
