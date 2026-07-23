import { listen } from "@tauri-apps/api/event"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { useCallback, useEffect, useState } from "react"

import { Onboarding } from "@/components/Onboarding"
import { QuestionWizard } from "@/components/QuestionWizard"
import { TooltipProvider } from "@/components/ui/tooltip"
import {
  type AnswerPayload,
  auqApi,
  type InstallOptions,
  type IntegrationStatus,
  type QueueSummary,
  type StoredRequest,
} from "@/lib/auq"

const EMPTY_SUMMARY: QueueSummary = { pending: 0 }

export default function App() {
  const [request, setRequest] = useState<StoredRequest | null>(null)
  const [summary, setSummary] = useState<QueueSummary>(EMPTY_SUMMARY)
  const [integrationStatus, setIntegrationStatus] = useState<IntegrationStatus | null>(null)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      const [active, queue, integrations] = await Promise.all([
        auqApi.active(),
        auqApi.summary(),
        auqApi.integrationStatus(),
      ])
      setRequest(active)
      setSummary(queue)
      setIntegrationStatus(integrations)
      setError(null)
    } catch (refreshError) {
      setError(String(refreshError))
    }
  }, [])

  const refreshQueue = useCallback(async () => {
    try {
      const [active, queue] = await Promise.all([auqApi.active(), auqApi.summary()])
      setRequest(active)
      setSummary(queue)
      setError(null)
    } catch (refreshError) {
      setError(String(refreshError))
    }
  }, [])

  useEffect(() => {
    refresh()
    const unlistenPromise = listen<QueueSummary>("queue-changed", () => refreshQueue())
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") getCurrentWindow().hide()
    }
    window.addEventListener("keydown", onKeyDown)
    return () => {
      window.removeEventListener("keydown", onKeyDown)
      unlistenPromise.then((unlisten) => unlisten())
    }
  }, [refresh, refreshQueue])

  const answer = async (result: AnswerPayload) => {
    if (!request) return
    await auqApi.answer(request.requestId, result)
    await refreshQueue()
  }

  const cancel = async () => {
    if (!request) return
    await auqApi.cancel(request.requestId)
    await refreshQueue()
  }

  const install = async (options: InstallOptions) => {
    const status = await auqApi.install(options)
    setIntegrationStatus(status)
  }

  const setEnabled = async (enabled: boolean) => {
    await auqApi.setEnabled(enabled)
    await refresh()
  }

  const trustCodexHooks = async () => {
    const status = await auqApi.trustCodexHooks()
    setIntegrationStatus(status)
  }

  return (
    <TooltipProvider>
      <div className="flex h-screen min-h-0 flex-col overflow-hidden bg-background text-foreground">
        {error ? (
          <div
            role="alert"
            className="border-b border-destructive/30 bg-destructive/8 px-5 py-2.5 text-sm text-destructive"
          >
            {error}
          </div>
        ) : null}
        {request ? (
          <QuestionWizard
            key={request.requestId}
            request={request}
            pendingCount={summary.pending}
            onSubmit={answer}
            onCancel={cancel}
          />
        ) : (
          <Onboarding
            status={integrationStatus}
            onInstall={install}
            onSetEnabled={setEnabled}
            onTrustCodexHooks={trustCodexHooks}
          />
        )}
      </div>
    </TooltipProvider>
  )
}
