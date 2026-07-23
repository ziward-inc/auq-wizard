import { Check, CircleAlert, Command, Download, Power, ShieldCheck } from "lucide-react"
import { useState } from "react"

import { ThemeToggle } from "@/components/ThemeToggle"
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

type IntegrationKey = (typeof ITEMS)[number]["key"]
type InstallTarget = IntegrationKey | "missing"

function missingInstallOptions(
  status: IntegrationStatus | null,
  replaceCli: boolean,
): InstallOptions {
  return {
    cli: !installed(status, "cli"),
    claude: !installed(status, "claude"),
    codex: !installed(status, "codex"),
    autostart: !installed(status, "autostart"),
    replaceCli,
  }
}

function reinstallOptions(key: IntegrationKey): InstallOptions {
  return {
    cli: key === "cli",
    claude: key === "claude",
    codex: key === "codex",
    autostart: key === "autostart",
    replaceCli: false,
  }
}

export function Onboarding({ status, onInstall, onSetEnabled }: OnboardingProps) {
  const [installingTarget, setInstallingTarget] = useState<InstallTarget | null>(null)
  const [confirmingReplace, setConfirmingReplace] = useState(false)
  const [changingRouting, setChangingRouting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const installing = installingTarget !== null
  const installedCount = ITEMS.filter((item) => installed(status, item.key)).length
  const allInstalled = installedCount === ITEMS.length

  const runInstall = async (options: InstallOptions, target: InstallTarget) => {
    setInstallingTarget(target)
    setError(null)
    try {
      await onInstall(options)
      setConfirmingReplace(false)
    } catch (installError) {
      setError(String(installError))
    } finally {
      setInstallingTarget(null)
    }
  }

  return (
    <main className="flex min-h-0 flex-1 flex-col overflow-y-auto">
      <header className="flex h-14 shrink-0 items-center border-b bg-card/85 px-5 backdrop-blur-xl">
        <div className="mx-auto flex w-full max-w-5xl items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <span className="grid size-7 place-items-center rounded-md bg-primary font-mono text-[11px] font-bold text-primary-foreground shadow-xs">
              A/
            </span>
            <div>
              <p className="text-sm font-semibold tracking-tight">AUQ Wizard</p>
              <p className="text-[11px] text-muted-foreground">Agent question bridge</p>
            </div>
          </div>
          <ThemeToggle />
        </div>
      </header>

      <div className="mx-auto flex w-full max-w-5xl flex-1 flex-col px-6 py-7">
        <section className="grid items-end gap-6 border-b pb-6 sm:grid-cols-[minmax(0,1fr)_auto]">
          <div>
            <p className="font-mono text-[11px] font-medium tracking-[0.12em] text-primary uppercase">
              Setup & integrations
            </p>
            <h1 className="mt-2 max-w-2xl text-balance text-3xl font-semibold tracking-[-0.035em]">
              Answer agent questions without leaving your desktop.
            </h1>
            <p className="mt-2.5 max-w-2xl text-sm leading-6 text-muted-foreground">
              AUQ routes structured clarification requests from your coding agents into one focused
              desktop workflow.
            </p>
          </div>
          {!allInstalled ? (
            <Button
              type="button"
              size="lg"
              disabled={installing}
              onClick={() => {
                if (status?.cliConflict) {
                  setConfirmingReplace(true)
                  setError(null)
                  return
                }
                void runInstall(missingInstallOptions(status, false), "missing")
              }}
            >
              {installingTarget === "missing" ? "Installing…" : "Install integrations"}
            </Button>
          ) : null}
        </section>

        {confirmingReplace ? (
          <div className="mt-4 flex items-center gap-3 rounded-lg border border-amber-400/50 bg-amber-50 p-3.5 text-sm text-amber-950 shadow-xs dark:border-amber-300/20 dark:bg-amber-300/10 dark:text-amber-100">
            <CircleAlert className="size-4 shrink-0" />
            <p className="min-w-0 flex-1">
              An existing <code className="bg-amber-950 text-amber-50">auq</code> command will be
              backed up and replaced. Continue?
            </p>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={installing}
              onClick={() => setConfirmingReplace(false)}
            >
              Cancel
            </Button>
            <Button
              type="button"
              size="sm"
              disabled={installing}
              onClick={() => void runInstall(missingInstallOptions(status, true), "missing")}
            >
              {installing ? "Replacing…" : "Replace and install"}
            </Button>
          </div>
        ) : null}

        <div className="mt-5 grid items-start gap-4 md:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)]">
          <section className="rounded-lg border bg-card p-4 shadow-xs">
            <div className="flex items-start gap-3">
              <span className="grid size-9 shrink-0 place-items-center rounded-md border bg-background text-muted-foreground">
                <Power className="size-4" />
              </span>
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <h2 className="text-sm font-semibold">
                    GUI routing {status?.auqEnabled === false ? "paused" : "enabled"}
                  </h2>
                  <span className="flex items-center gap-1.5 rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">
                    <span
                      className={`size-1.5 rounded-full ${
                        status?.auqEnabled === false ? "bg-amber-500" : "bg-emerald-500"
                      }`}
                    />
                    {status?.auqEnabled === false ? "Paused" : "Active"}
                  </span>
                </div>
                <p className="mt-1.5 text-xs leading-5 text-muted-foreground">
                  {status?.auqEnabled === false
                    ? "Agents use their native interaction instead of opening this app."
                    : "New clarification requests open here automatically."}
                </p>
              </div>
            </div>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="mt-4 w-full"
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
            <p className="mt-3 border-t pt-3 text-[11px] leading-4 text-muted-foreground">
              Pause routing before working from mobile or a remote session.
            </p>
          </section>

          <section className="overflow-hidden rounded-lg border bg-card shadow-xs">
            <div className="flex items-center justify-between gap-4 border-b px-4 py-3">
              <div>
                <h2 className="text-sm font-semibold">Integrations</h2>
                <p className="mt-0.5 text-xs text-muted-foreground">
                  CLI, agents, and background launch
                </p>
              </div>
              <span className="rounded-full border bg-background px-2.5 py-1 font-mono text-[11px] text-muted-foreground tabular-nums">
                {installedCount}/{ITEMS.length} ready
              </span>
            </div>

            <div className="divide-y">
              {ITEMS.map((item) => {
                const Icon = item.icon
                const done = installed(status, item.key)
                return (
                  <div key={item.key} className="flex items-center gap-3 px-4 py-3">
                    <span className="grid size-8 shrink-0 place-items-center rounded-md bg-muted text-muted-foreground">
                      <Icon className="size-3.5" />
                    </span>
                    <div className="min-w-0 flex-1">
                      <p className="text-[13px] font-medium">{item.label}</p>
                      <p className="mt-0.5 truncate text-xs text-muted-foreground">
                        {item.description}
                      </p>
                    </div>
                    <div className="flex shrink-0 items-center gap-2.5">
                      <span
                        className={
                          done
                            ? "flex items-center gap-1.5 text-xs font-medium text-emerald-600 dark:text-emerald-400"
                            : "text-xs text-muted-foreground"
                        }
                      >
                        {done ? (
                          <>
                            <Check className="size-3.5" />
                            Ready
                          </>
                        ) : item.key === "cli" && status?.cliConflict ? (
                          "Needs approval"
                        ) : (
                          "Not installed"
                        )}
                      </span>
                      {done ? (
                        <Button
                          type="button"
                          variant="outline"
                          size="xs"
                          aria-label={`Reinstall ${item.label}`}
                          disabled={installing}
                          onClick={() => void runInstall(reinstallOptions(item.key), item.key)}
                        >
                          {installingTarget === item.key ? "Reinstalling…" : "Reinstall"}
                        </Button>
                      ) : null}
                    </div>
                  </div>
                )
              })}
            </div>
          </section>
        </div>

        <p className="mt-4 text-xs leading-5 text-muted-foreground">
          Existing settings are merged and backed up. Codex asks you to trust the new hooks in
          <code>/hooks</code>.
        </p>

        {status?.warnings.map((warning) => (
          <div
            key={warning}
            className="mt-3 flex gap-3 rounded-lg border border-amber-400/50 bg-amber-50 p-3.5 text-sm text-amber-950 dark:border-amber-300/20 dark:bg-amber-300/10 dark:text-amber-100"
          >
            <CircleAlert className="mt-0.5 size-4 shrink-0" />
            <span>{warning}</span>
          </div>
        ))}
        {error ? (
          <p role="alert" className="mt-3 text-sm text-destructive">
            {error}
          </p>
        ) : null}
      </div>
    </main>
  )
}
