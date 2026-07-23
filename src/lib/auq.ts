import { invoke } from "@tauri-apps/api/core"

export type QuestionOption = {
  label: string
  description: string
  preview?: string
}

export type Question = {
  question: string
  header: string
  options: QuestionOption[]
  multiSelect: boolean
}

export type AskPayload = {
  questions: Question[]
}

export type AnswerValue = string | string[]

export type AnswerPayload = {
  answers?: Record<string, AnswerValue>
  response?: string
}

export type RequestStatus = "pending" | "answered" | "canceled"

export type StoredRequest = {
  requestId: string
  sequence: number
  status: RequestStatus
  payload: AskPayload
  result?: AnswerPayload
  createdAt: number
  updatedAt: number
  completedAt?: number
}

export type QueueSummary = {
  pending: number
  activeRequestId?: string
}

export type InstallOptions = {
  cli: boolean
  claude: boolean
  codex: boolean
  autostart: boolean
  replaceCli: boolean
}

export type IntegrationStatus = {
  auqEnabled: boolean
  cli: boolean
  cliConflict: boolean
  claudeSkill: boolean
  claudeHook: boolean
  codexSkill: boolean
  codexHooks: boolean
  autostart: boolean
  pathReady: boolean
  warnings: string[]
}

export const auqApi = {
  active: () => invoke<StoredRequest | null>("get_active_request"),
  summary: () => invoke<QueueSummary>("get_queue_summary"),
  answer: (requestId: string, result: AnswerPayload) =>
    invoke<StoredRequest>("submit_answer", { requestId, result }),
  cancel: (requestId: string) => invoke<StoredRequest>("cancel_request", { requestId }),
  integrationStatus: () => invoke<IntegrationStatus>("get_integration_status"),
  enabled: () => invoke<boolean>("get_auq_enabled"),
  setEnabled: (enabled: boolean) => invoke<boolean>("set_auq_enabled", { enabled }),
  install: (options: InstallOptions) =>
    invoke<IntegrationStatus>("install_integrations", { options }),
}
