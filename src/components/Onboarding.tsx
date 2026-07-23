import { Check, CircleAlert, Command, Download, Power, ShieldCheck } from "lucide-react"
import { useState } from "react"

import { Button } from "@/components/ui/button"
import type { InstallOptions, IntegrationStatus } from "@/lib/auq"

type OnboardingProps = {
  status: IntegrationStatus | null
  onInstall: (options: InstallOptions) => Promise<void>
  onSetEnabled: (enabled: boolean) => Promise<void>
}

const ITEMS = [
  { key: "cli", label: "CLI", description: "Link auq into ~/.local/bin.", icon: Command },
  {
    key: "claude",
    label: "Claude Code",
    description: "Install the AUQ skill and AskUserQuestion hook.",
    icon: ShieldCheck,
  },
  {
    key: "codex",
    label: "Codex",
    description: "Install the AUQ skill and command-validation hooks.",
    icon: ShieldCheck,
  },
  {
    key: "autostart",
    label: "Launch at login",
    description: "Keep AUQ Wizard ready in the menu bar.",
    icon: Download,
  },
] as const

function installed(status: IntegrationStatus | null, key: (typeof ITEMS)[number]["key"]) {
  if (!status) return false
  if (key === "claude") return status.claudeSkill && status.claudeHook
  if (key === "codex") return status.codexSkill && status.codexHooks
  return status[key]
}

export function Onboarding({ status, onInstall, onSetEnabled }: OnboardingProps) {
  const [installing, setInstalling] = useState(false)
  const [changingRouting, setChangingRouting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const allInstalled = ITEMS.every((item) => installed(status, item.key))

  return (
    <main className="mx-auto flex w-full max-w-3xl flex-1 flex-col justify-center px-8 py-12">
      <p className="text-xs font-semibold tracking-[0.2em] text-muted-foreground uppercase">
        Agent question bridge
      </p>
      <h1 className="mt-4 text-balance text-4xl font-semibold tracking-tight">
        Ask once. Answer in one place.
      </h1>
      <p className="mt-4 max-w-2xl text-pretty text-base leading-7 text-muted-foreground">
        AUQ Wizard connects agent clarification requests to a focused desktop wizard. It stays in
        the menu bar and returns your answer to the waiting agent.
      </p>

      <div className="mt-9 flex items-center gap-4 border bg-card p-4">
        <span className="grid size-9 place-items-center border bg-background">
          <Power className="size-4" />
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-sm font-semibold">
            GUI routing {status?.auqEnabled === false ? "paused" : "enabled"}
          </p>
          <p className="mt-1 text-sm text-muted-foreground">
            {status?.auqEnabled === false
              ? "Agents use their native interaction instead of opening this app."
              : "Pause before working from mobile or a remote session."}
          </p>
        </div>
        <Button
          type="button"
          variant="outline"
          disabled={!status || changingRouting}
          onClick={async () => {
            if (!status) return
            setChangingRouting(true)
            setError(null)
            try {
              await onSetEnabled(!status.auqEnabled)
            } catch (routingError) {
              setError(String(routingError))
            } finally {
              setChangingRouting(false)
            }
          }}
        >
          {changingRouting
            ? "Updating…"
            : status?.auqEnabled === false
              ? "Enable AUQ"
              : "Pause AUQ"}
        </Button>
      </div>

      <div className="mt-4 divide-y border bg-card">
        {ITEMS.map((item) => {
          const Icon = item.icon
          const done = installed(status, item.key)
          return (
            <div key={item.key} className="flex items-center gap-4 p-4">
              <span className="grid size-9 place-items-center border bg-background">
                <Icon className="size-4" />
              </span>
              <div className="min-w-0 flex-1">
                <p className="text-sm font-semibold">{item.label}</p>
                <p className="mt-1 text-sm text-muted-foreground">{item.description}</p>
              </div>
              <span className={done ? "text-foreground" : "text-muted-foreground"}>
                {done ? <Check className="size-4" /> : "Not installed"}
              </span>
            </div>
          )
        })}
      </div>

      {status?.warnings.map((warning) => (
        <div
          key={warning}
          className="mt-4 flex gap-3 border border-amber-400/50 bg-amber-50 p-4 text-sm text-amber-950"
        >
          <CircleAlert className="mt-0.5 size-4 shrink-0" />
          <span>{warning}</span>
        </div>
      ))}
      {error ? (
        <p role="alert" className="mt-4 text-sm text-destructive">
          {error}
        </p>
      ) : null}

      <div className="mt-7 flex items-center justify-between gap-4">
        <p className="text-xs leading-5 text-muted-foreground">
          Existing settings are merged and backed up. Codex asks you to trust the new hooks in
          <code>/hooks</code>.
        </p>
        <Button
          type="button"
          disabled={installing || allInstalled}
          onClick={async () => {
            setInstalling(true)
            setError(null)
            try {
              await onInstall({ cli: true, claude: true, codex: true, autostart: true })
            } catch (installError) {
              setError(String(installError))
            } finally {
              setInstalling(false)
            }
          }}
        >
          {allInstalled ? "Ready" : installing ? "Installing…" : "Install integrations"}
        </Button>
      </div>
    </main>
  )
}
