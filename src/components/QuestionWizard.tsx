import { useForm } from "@tanstack/react-form"
import { ArrowLeft, ArrowRight, CircleX, Send } from "lucide-react"
import { useMemo, useState } from "react"
import ReactMarkdown from "react-markdown"
import remarkGfm from "remark-gfm"
import { z } from "zod"

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
      <header className="border-b bg-background/95 px-8 py-5 backdrop-blur">
        <div className="mx-auto flex max-w-3xl items-center justify-between gap-6">
          <div>
            <p className="text-xs font-semibold tracking-[0.18em] text-muted-foreground uppercase">
              AUQ Wizard
            </p>
            <p className="mt-1 text-sm text-muted-foreground">
              Request <code>{request.requestId.slice(0, 8)}</code>
            </p>
          </div>
          <span className="border bg-muted px-3 py-1 text-xs font-medium text-muted-foreground">
            {pendingCount} pending
          </span>
        </div>
      </header>

      <form
        className="mx-auto flex w-full max-w-3xl flex-1 flex-col px-8 py-8"
        onSubmit={(event) => {
          event.preventDefault()
          form.handleSubmit()
        }}
      >
        <Progress
          value={questionIndex + 1}
          max={questions.length}
          aria-valuetext={`${questionIndex + 1} of ${questions.length}`}
          className="mb-9"
        >
          <ProgressLabel>{question.header}</ProgressLabel>
          <span className="ml-auto text-sm text-muted-foreground tabular-nums">
            {questionIndex + 1} / {questions.length}
          </span>
        </Progress>

        <form.Field name="useGeneralResponse">
          {(modeField) =>
            modeField.state.value ? (
              <form.Field name="generalResponse">
                {(responseField) => (
                  <Field className="flex-1" data-invalid={!responseField.state.meta.isValid}>
                    <FieldTitle className="text-2xl normal-case">Respond freely</FieldTitle>
                    <FieldDescription>
                      This response replaces all structured answers for this request.
                    </FieldDescription>
                    <Textarea
                      autoFocus
                      aria-invalid={!responseField.state.meta.isValid}
                      className="mt-6 min-h-44 border bg-card p-4"
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
                    <Field className="flex-1">
                      <div className="mb-7">
                        <FieldTitle className="max-w-2xl text-balance text-2xl leading-tight normal-case">
                          {question.question}
                        </FieldTitle>
                        <FieldDescription className="mt-2">
                          {question.multiSelect
                            ? "Select one or more options."
                            : "Select one option."}
                        </FieldDescription>
                      </div>

                      <div className="grid gap-3" data-slot="checkbox-group">
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
                                "option-card border bg-card p-4 outline-2 outline-offset-[-2px] transition-colors hover:bg-muted/60",
                                selected && "bg-muted outline outline-primary",
                              )}
                            >
                              <Field orientation="horizontal">
                                <Checkbox
                                  aria-label={option.label}
                                  checked={selected}
                                  onCheckedChange={update}
                                />
                                <div className="min-w-0 flex-1">
                                  <FieldTitle className="normal-case">{option.label}</FieldTitle>
                                  <FieldDescription className="mt-1">
                                    {option.description}
                                  </FieldDescription>
                                  {option.preview ? (
                                    <div className="markdown-preview mt-4 border bg-background p-4 text-sm">
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

                      <Field className="mt-5">
                        <FieldLabel htmlFor="other-answer">Other</FieldLabel>
                        <Textarea
                          id="other-answer"
                          aria-label="Other answer"
                          className="border bg-card px-4"
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

        <form.Subscribe selector={(state) => state.values}>
          {(values) => {
            const selection = values.selections[question.question]
            const currentAnswered = values.useGeneralResponse
              ? Boolean(values.generalResponse.trim())
              : selection.values.length > 0 || Boolean(selection.other.trim())
            return (
              <footer className="mt-9 border-t pt-6">
                <form.Subscribe selector={(state) => state.errors}>
                  {(errors) =>
                    errors.length > 0 ? (
                      <p role="alert" className="mb-4 text-sm text-destructive">
                        {String(errors[0])}
                      </p>
                    ) : null
                  }
                </form.Subscribe>
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="flex gap-2">
                    <Button
                      type="button"
                      variant="ghost"
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
                      disabled={questionIndex === 0 || values.useGeneralResponse}
                      onClick={() => setQuestionIndex((index) => index - 1)}
                    >
                      <ArrowLeft data-icon="inline-start" />
                      Back
                    </Button>
                    {questionIndex < questions.length - 1 && !values.useGeneralResponse ? (
                      <Button
                        type="button"
                        disabled={!currentAnswered}
                        onClick={() => setQuestionIndex((index) => index + 1)}
                      >
                        Next
                        <ArrowRight data-icon="inline-end" />
                      </Button>
                    ) : (
                      <form.Subscribe selector={(state) => state.isSubmitting}>
                        {(isSubmitting) => (
                          <Button type="submit" disabled={!currentAnswered || isSubmitting}>
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
      </form>
    </main>
  )
}
