import { useForm } from "@tanstack/react-form"
import { ArrowLeft, ArrowRight, Check, CircleX, Send } from "lucide-react"
import { useMemo, useState } from "react"
import ReactMarkdown from "react-markdown"
import remarkGfm from "remark-gfm"
import { z } from "zod"

import { ThemeToggle } from "@/components/ThemeToggle"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Field, FieldDescription, FieldError, FieldLabel, FieldTitle } from "@/components/ui/field"
import { Progress, ProgressLabel } from "@/components/ui/progress"
import { Textarea } from "@/components/ui/textarea"
import type { AnswerPayload, Question, StoredRequest } from "@/lib/auq"
import { cn } from "@/lib/utils"

type Selection = {
  values: string[]
  other: string
}

type WizardValues = {
  selections: Record<string, Selection>
  generalResponse: string
  useGeneralResponse: boolean
}

type QuestionWizardProps = {
  request: StoredRequest
  pendingCount: number
  onSubmit: (result: AnswerPayload) => Promise<void>
  onCancel: () => Promise<void>
}

const selectionSchema = z.object({
  values: z.array(z.string()),
  other: z.string(),
})

function validationSchema(questions: Question[]) {
  return z
    .object({
      selections: z.record(z.string(), selectionSchema),
      generalResponse: z.string(),
      useGeneralResponse: z.boolean(),
    })
    .superRefine((value, context) => {
      if (value.useGeneralResponse) {
        if (!value.generalResponse.trim()) {
          context.addIssue({
            code: "custom",
            message: "Enter a response before submitting.",
            path: ["generalResponse"],
          })
        }
        return
      }
      for (const question of questions) {
        const selection = value.selections[question.question]
        if (!selection || (selection.values.length === 0 && !selection.other.trim())) {
          context.addIssue({
            code: "custom",
            message: `Answer “${question.question}” before submitting.`,
            path: ["selections", question.question],
          })
        }
      }
    })
}

function initialValues(questions: Question[]): WizardValues {
  return {
    selections: Object.fromEntries(
      questions.map((question) => [question.question, { values: [], other: "" }]),
    ),
    generalResponse: "",
    useGeneralResponse: false,
  }
}

function toAnswerPayload(values: WizardValues, questions: Question[]): AnswerPayload {
  if (values.useGeneralResponse) {
    return { response: values.generalResponse.trim() }
  }
  return {
    answers: Object.fromEntries(
      questions.map((question) => {
        const selection = values.selections[question.question]
        const answers = [...selection.values]
        if (selection.other.trim()) answers.push(selection.other.trim())
        return [question.question, question.multiSelect ? answers : answers[0]]
      }),
    ),
  }
}

function isAnswered(values: WizardValues, question: Question) {
  const selection = values.selections[question.question]
  return Boolean(selection && (selection.values.length > 0 || selection.other.trim()))
}

export function QuestionWizard({ request, pendingCount, onSubmit, onCancel }: QuestionWizardProps) {
  const questions = request.payload.questions
  const [questionIndex, setQuestionIndex] = useState(0)
  const [canceling, setCanceling] = useState(false)
  const schema = useMemo(() => validationSchema(questions), [questions])
  const question = questions[questionIndex]
  const form = useForm({
    defaultValues: initialValues(questions),
    validators: {
      onSubmit: ({ value }) => {
        const result = schema.safeParse(value)
        return result.success ? undefined : result.error.issues[0]?.message
      },
    },
    onSubmit: async ({ value }) => onSubmit(toAnswerPayload(value, questions)),
  })

  return (
    <main className="flex min-h-0 flex-1 flex-col">
      <header className="flex h-14 shrink-0 items-center border-b bg-card/85 px-5 backdrop-blur-xl">
        <div className="mx-auto flex w-full max-w-6xl items-center justify-between gap-4">
          <div className="flex min-w-0 items-center gap-3">
            <span className="grid size-7 shrink-0 place-items-center rounded-md bg-primary font-mono text-[11px] font-bold text-primary-foreground shadow-xs">
              A/
            </span>
            <div className="flex min-w-0 items-baseline gap-2.5">
              <p className="text-sm font-semibold tracking-tight">AUQ Wizard</p>
              <span className="text-xs text-muted-foreground">Clarification request</span>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <span className="flex items-center gap-2 rounded-full border bg-background px-2.5 py-1 text-xs text-muted-foreground shadow-xs">
              <span className="size-1.5 rounded-full bg-primary" />
              {pendingCount} pending
            </span>
            <ThemeToggle />
          </div>
        </div>
      </header>

      <form
        className="mx-auto flex min-h-0 w-full max-w-6xl flex-1 overflow-hidden border-x bg-card/35"
        onSubmit={(event) => {
          event.preventDefault()
          form.handleSubmit()
        }}
      >
        <aside className="hidden w-52 shrink-0 flex-col border-r bg-sidebar/70 p-4 md:flex">
          <div className="border-b pb-4">
            <p className="text-[11px] font-medium tracking-[0.12em] text-muted-foreground uppercase">
              Request
            </p>
            <code className="mt-1.5 block w-fit bg-transparent px-0 text-xs text-foreground">
              {request.requestId.slice(0, 8)}
            </code>
          </div>

          <Progress
            value={questionIndex + 1}
            max={questions.length}
            aria-valuetext={`${questionIndex + 1} of ${questions.length}`}
            className="mt-4 gap-2"
          >
            <ProgressLabel className="text-[11px] tracking-[0.1em] text-muted-foreground">
              Progress
            </ProgressLabel>
            <span className="ml-auto font-mono text-[11px] text-muted-foreground tabular-nums">
              {questionIndex + 1}/{questions.length}
            </span>
          </Progress>

          <form.Subscribe selector={(state) => state.values}>
            {(values) => (
              <ol className="mt-5 flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto">
                {questions.map((item, index) => {
                  const current = index === questionIndex
                  const answered = !values.useGeneralResponse && isAnswered(values, item)
                  return (
                    <li
                      key={item.question}
                      aria-current={current ? "step" : undefined}
                      className={cn(
                        "flex items-start gap-2.5 rounded-md border border-transparent px-2.5 py-2 text-xs text-muted-foreground",
                        current && "border-border bg-card text-foreground shadow-xs",
                      )}
                    >
                      <span
                        className={cn(
                          "mt-px grid size-5 shrink-0 place-items-center rounded-full border bg-background font-mono text-[10px] tabular-nums",
                          current &&
                            "border-primary text-primary outline outline-2 outline-primary/10",
                          answered && "border-primary bg-primary text-primary-foreground",
                        )}
                      >
                        {answered ? <Check className="size-3" /> : index + 1}
                      </span>
                      <span className="min-w-0 break-words pt-0.5 font-medium leading-4">
                        {item.header}
                      </span>
                    </li>
                  )
                })}
              </ol>
            )}
          </form.Subscribe>

          <p className="mt-4 border-t pt-3 text-[11px] leading-4 text-muted-foreground">
            Answer each step to return a structured response to the agent.
          </p>
        </aside>

        <section className="flex min-w-0 flex-1 flex-col bg-background/60">
          <div className="min-h-0 flex-1 overflow-y-auto px-6 py-6 lg:px-8">
            <form.Field name="useGeneralResponse">
              {(modeField) =>
                modeField.state.value ? (
                  <form.Field name="generalResponse">
                    {(responseField) => (
                      <Field
                        className="mx-auto max-w-2xl gap-2"
                        data-invalid={!responseField.state.meta.isValid}
                      >
                        <div className="mb-3 flex items-center gap-2">
                          <span className="rounded-full bg-accent px-2 py-1 text-[11px] font-medium text-accent-foreground">
                            Free response
                          </span>
                          <span className="text-xs text-muted-foreground">
                            Replaces all structured answers
                          </span>
                        </div>
                        <FieldTitle className="text-balance text-2xl leading-tight tracking-tight normal-case">
                          Add the context the agent needs
                        </FieldTitle>
                        <FieldDescription className="mt-1">
                          Write one response for the complete clarification request.
                        </FieldDescription>
                        <Textarea
                          autoFocus
                          aria-invalid={!responseField.state.meta.isValid}
                          className="mt-4 min-h-44 bg-card p-4"
                          placeholder="Type a response…"
                          value={responseField.state.value}
                          onBlur={responseField.handleBlur}
                          onChange={(event) => responseField.handleChange(event.target.value)}
                        />
                        <FieldError
                          errors={responseField.state.meta.errors.map((message) => ({ message }))}
                        />
                      </Field>
                    )}
                  </form.Field>
                ) : (
                  <form.Field name="selections">
                    {(selectionField) => {
                      const selection = selectionField.state.value[question.question]
                      return (
                        <Field className="mx-auto max-w-2xl gap-0">
                          <div className="mb-5">
                            <div className="mb-3 flex items-center gap-2">
                              <span className="font-mono text-[11px] font-medium tracking-[0.1em] text-primary uppercase">
                                Question {String(questionIndex + 1).padStart(2, "0")}
                              </span>
                              <span className="rounded-full border bg-card px-2 py-0.5 text-[11px] text-muted-foreground">
                                {question.multiSelect ? "Multiple choice" : "Single choice"}
                              </span>
                            </div>
                            <FieldTitle className="max-w-2xl text-balance text-2xl leading-tight tracking-tight normal-case">
                              {question.question}
                            </FieldTitle>
                            <FieldDescription className="mt-2">
                              {question.multiSelect
                                ? "Select every option that applies."
                                : "Choose the best option, or add your own answer."}
                            </FieldDescription>
                          </div>

                          <div className="grid gap-2.5" data-slot="checkbox-group">
                            {question.options.map((option) => {
                              const selected = selection.values.includes(option.label)
                              const update = (checked: boolean) => {
                                const values = question.multiSelect
                                  ? checked
                                    ? [...selection.values, option.label]
                                    : selection.values.filter((label) => label !== option.label)
                                  : checked
                                    ? [option.label]
                                    : []
                                selectionField.handleChange({
                                  ...selectionField.state.value,
                                  [question.question]: {
                                    values,
                                    other: question.multiSelect ? selection.other : "",
                                  },
                                })
                              }
                              return (
                                <FieldLabel
                                  key={option.label}
                                  className={cn(
                                    "option-card rounded-lg border bg-card outline-offset-[-1px] transition-[background-color,box-shadow] hover:bg-muted/65",
                                    selected &&
                                      "bg-primary/[0.045] outline outline-1 outline-primary shadow-xs",
                                  )}
                                >
                                  <Field orientation="horizontal" className="gap-3">
                                    <Checkbox
                                      aria-label={option.label}
                                      checked={selected}
                                      onCheckedChange={update}
                                    />
                                    <div className="min-w-0 flex-1">
                                      <FieldTitle className="text-[13px] normal-case">
                                        {option.label}
                                      </FieldTitle>
                                      <FieldDescription className="mt-0.5 text-[13px] leading-5">
                                        {option.description}
                                      </FieldDescription>
                                      {option.preview ? (
                                        <div className="markdown-preview mt-3 rounded-md border bg-background p-3 font-mono text-xs">
                                          <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml>
                                            {option.preview}
                                          </ReactMarkdown>
                                        </div>
                                      ) : null}
                                    </div>
                                  </Field>
                                </FieldLabel>
                              )
                            })}
                          </div>

                          <Field className="mt-4 gap-2">
                            <div className="flex items-center justify-between gap-3">
                              <FieldLabel htmlFor="other-answer" className="text-xs font-medium">
                                Other answer
                              </FieldLabel>
                              <span className="text-[11px] text-muted-foreground">Optional</span>
                            </div>
                            <Textarea
                              id="other-answer"
                              aria-label="Other answer"
                              className="min-h-14 bg-card"
                              placeholder="Type another answer…"
                              value={selection.other}
                              onFocus={() => {
                                if (!question.multiSelect && selection.values.length > 0) {
                                  selectionField.handleChange({
                                    ...selectionField.state.value,
                                    [question.question]: { values: [], other: selection.other },
                                  })
                                }
                              }}
                              onChange={(event) =>
                                selectionField.handleChange({
                                  ...selectionField.state.value,
                                  [question.question]: {
                                    values: question.multiSelect ? selection.values : [],
                                    other: event.target.value,
                                  },
                                })
                              }
                            />
                          </Field>
                        </Field>
                      )
                    }}
                  </form.Field>
                )
              }
            </form.Field>
          </div>

          <form.Subscribe selector={(state) => state.values}>
            {(values) => {
              const selection = values.selections[question.question]
              const currentAnswered = values.useGeneralResponse
                ? Boolean(values.generalResponse.trim())
                : selection.values.length > 0 || Boolean(selection.other.trim())
              return (
                <footer className="shrink-0 border-t bg-card/90 px-5 py-3 backdrop-blur-xl">
                  <form.Subscribe selector={(state) => state.errors}>
                    {(errors) =>
                      errors.length > 0 ? (
                        <p role="alert" className="mb-2.5 text-xs text-destructive">
                          {String(errors[0])}
                        </p>
                      ) : null
                    }
                  </form.Subscribe>
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <div className="flex gap-1.5">
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={async () => {
                          setCanceling(true)
                          try {
                            await onCancel()
                          } finally {
                            setCanceling(false)
                          }
                        }}
                        disabled={canceling}
                      >
                        <CircleX data-icon="inline-start" />
                        Cancel
                      </Button>
                      <form.Field name="useGeneralResponse">
                        {(modeField) => (
                          <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={() => modeField.handleChange(!modeField.state.value)}
                          >
                            {modeField.state.value ? "Use choices" : "Respond freely"}
                          </Button>
                        )}
                      </form.Field>
                    </div>

                    <div className="flex gap-2">
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={questionIndex === 0 || values.useGeneralResponse}
                        onClick={() => setQuestionIndex((index) => index - 1)}
                      >
                        <ArrowLeft data-icon="inline-start" />
                        Back
                      </Button>
                      {questionIndex < questions.length - 1 && !values.useGeneralResponse ? (
                        <Button
                          type="button"
                          size="sm"
                          disabled={!currentAnswered}
                          onClick={() => setQuestionIndex((index) => index + 1)}
                        >
                          Next
                          <ArrowRight data-icon="inline-end" />
                        </Button>
                      ) : (
                        <form.Subscribe selector={(state) => state.isSubmitting}>
                          {(isSubmitting) => (
                            <Button
                              type="submit"
                              size="sm"
                              disabled={!currentAnswered || isSubmitting}
                            >
                              <Send data-icon="inline-start" />
                              {isSubmitting ? "Submitting…" : "Submit"}
                            </Button>
                          )}
                        </form.Subscribe>
                      )}
                    </div>
                  </div>
                </footer>
              )
            }}
          </form.Subscribe>
        </section>
      </form>
    </main>
  )
}
